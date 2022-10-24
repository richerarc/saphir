use std::path::PathBuf;

use futures_util::{
    io::SeekFrom,
    stream::Stream,
    task::{Context, Poll},
};
use hyper::body::Bytes;
use tokio::{
    fs::File as TokioFile,
    io,
    io::{AsyncRead as TokioAsyncRead, AsyncSeek as TokioAsyncSeek},
    macros::support::Pin,
};

use crate::{error::SaphirError, file::middleware::PathExt, http_context::HttpContext, responder::Responder, response::Builder};
use flate2::write::{DeflateEncoder, GzEncoder};
use futures::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, Cursor};
use mime::Mime;
use std::{
    io::{Cursor as CursorSync, Write},
    str::FromStr,
};
use tokio::io::ReadBuf;

mod cache;
pub mod conditional_request;
pub mod content_range;
pub mod etag;
pub mod middleware;
pub mod range;
pub mod range_requests;

pub const MAX_BUFFER: usize = 65534;

pub trait SaphirFile: AsyncRead + AsyncSeek + FileInfo + Sync + Send {}

impl<T: AsyncRead + AsyncSeek + FileInfo + Sync + Send> SaphirFile for T {}

pub trait FileInfo {
    fn get_path(&self) -> &PathBuf;
    fn get_mime(&self) -> Option<&mime::Mime>;
    fn get_size(&self) -> u64;
}

pub struct File {
    inner: Pin<Box<TokioFile>>,
    path: PathBuf,
    mime: Option<mime::Mime>,
    seek_has_started: bool,
}

impl FileInfo for File {
    fn get_path(&self) -> &PathBuf {
        &self.path
    }

    fn get_mime(&self) -> Option<&mime::Mime> {
        self.mime.as_ref()
    }

    fn get_size(&self) -> u64 {
        self.path.size()
    }
}

impl File {
    pub async fn open(path_str: &str) -> tokio::io::Result<File> {
        let path = path_str.to_string();
        match TokioFile::open(path_str).await {
            Ok(file) => Ok(File {
                inner: Box::pin(file),
                path: PathBuf::from(path),
                mime: None,
                seek_has_started: false,
            }),

            Err(e) => Err(e),
        }
    }
}

impl AsyncRead for File {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let mut read_buf = ReadBuf::new(buf);
        let remaining_before = read_buf.remaining();
        match self.inner.as_mut().poll_read(cx, &mut read_buf) {
            Poll::Ready(r) => Poll::Ready(r.map(|_| remaining_before - read_buf.remaining())),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncSeek for File {
    fn poll_seek(mut self: Pin<&mut Self>, cx: &mut Context<'_>, pos: SeekFrom) -> Poll<io::Result<u64>> {
        if !self.seek_has_started {
            match self.inner.as_mut().start_seek(pos) {
                Ok(()) => {
                    self.seek_has_started = true;
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }

        match self.inner.as_mut().poll_complete(cx) {
            Poll::Ready(Ok(res)) => {
                self.seek_has_started = false;
                Poll::Ready(Ok(res))
            }
            Poll::Ready(Err(e)) => {
                self.seek_has_started = false;
                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Responder for File {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        let mime = if let Some(mime) = &self.get_mime() {
            mime.as_ref().to_string()
        } else {
            self.get_path()
                .mime()
                .unwrap_or_else(|| {
                    if self.get_path().is_dir() {
                        mime::TEXT_HTML_UTF_8
                    } else {
                        mime::TEXT_PLAIN_UTF_8
                    }
                })
                .as_ref()
                .to_string()
        };

        let len = self.get_size();
        builder
            .file(self)
            .header(http::header::ACCEPT_RANGES, "bytes")
            .header(http::header::CONTENT_TYPE, mime)
            .header(http::header::CONTENT_LENGTH, len)
    }
}

pub struct FileCursor {
    inner: Pin<Box<Cursor<Vec<u8>>>>,
    mime: Option<Mime>,
    path: PathBuf,
    size: u64,
}

impl FileCursor {
    pub fn new(inner: Vec<u8>, mime: Option<Mime>, path: PathBuf) -> Self {
        let size = inner.len() as u64;
        FileCursor {
            inner: Box::pin(Cursor::new(inner)),
            mime,
            path,
            size,
        }
    }
}

impl FileInfo for FileCursor {
    fn get_path(&self) -> &PathBuf {
        &self.path
    }

    fn get_mime(&self) -> Option<&Mime> {
        self.mime.as_ref()
    }

    fn get_size(&self) -> u64 {
        self.size
    }
}

impl AsyncRead for FileCursor {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        self.inner.as_mut().poll_read(cx, buf)
    }
}

impl AsyncSeek for FileCursor {
    fn poll_seek(mut self: Pin<&mut Self>, cx: &mut Context<'_>, pos: SeekFrom) -> Poll<io::Result<u64>> {
        self.inner.as_mut().poll_seek(cx, pos)
    }
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum Compression {
    Raw,
    Deflate,
    Gzip,
    Brotli,
}

impl Default for Compression {
    fn default() -> Self {
        Compression::Raw
    }
}

impl FromStr for Compression {
    type Err = SaphirError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "deflate" => Ok(Compression::Deflate),
            "gzip" => Ok(Compression::Gzip),
            "br" => Ok(Compression::Brotli),
            "identity" => Ok(Compression::Raw),
            "*" => Ok(Compression::Raw),
            _ => Err(SaphirError::Other("Encoding not supported".to_string())),
        }
    }
}

impl ToString for Compression {
    fn to_string(&self) -> String {
        match self {
            Compression::Deflate => "deflate".to_string(),
            Compression::Gzip => "gzip".to_string(),
            Compression::Brotli => "br".to_string(),
            _ => "".to_string(),
        }
    }
}

pub enum Encoder {
    Brotli(Box<brotli::CompressorWriter<Vec<u8>>>),
    Gzip(GzEncoder<Vec<u8>>),
    Deflate(DeflateEncoder<Vec<u8>>),
    Raw(CursorSync<Vec<u8>>),
    None,
}

impl Encoder {
    pub fn is_none(&self) -> bool {
        match self {
            Encoder::None => true,
            _ => false,
        }
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Encoder::None
    }
}

impl std::io::Write for Encoder {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Encoder::Brotli(e) => e.write(buf),
            Encoder::Gzip(e) => e.write(buf),
            Encoder::Deflate(e) => e.write(buf),
            Encoder::Raw(e) => std::io::Write::write(e, buf),
            Encoder::None => Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Encoder::Brotli(e) => e.flush(),
            Encoder::Gzip(e) => e.flush(),
            Encoder::Deflate(e) => e.flush(),
            Encoder::Raw(e) => std::io::Write::flush(e),
            Encoder::None => Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
        }
    }
}

pub async fn compress_file(mut file: Pin<Box<dyn SaphirFile>>, mut encoder: Encoder, compression: Compression) -> io::Result<Vec<u8>> {
    if encoder.is_none() {
        encoder = match compression {
            Compression::Gzip => Encoder::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default())),
            Compression::Deflate => Encoder::Deflate(DeflateEncoder::new(Vec::new(), flate2::Compression::default())),
            Compression::Brotli => Encoder::Brotli(Box::new(brotli::CompressorWriter::new(Vec::new(), MAX_BUFFER, 6, 22))),
            Compression::Raw => Encoder::Raw(CursorSync::new(Vec::new())),
        }
    }

    loop {
        let mut buffer = vec![0; MAX_BUFFER];
        match file.read(buffer.as_mut_slice()).await {
            Ok(size) => {
                if size > 0 {
                    encoder.write_all(&buffer[..size])?;
                } else {
                    break;
                }
            }
            Err(e) => return Err(e),
        }
    }

    match encoder {
        Encoder::Gzip(e) => e.finish(),
        Encoder::Deflate(e) => e.finish(),
        Encoder::Brotli(mut e) => match e.flush() {
            Ok(()) => Ok(e.into_inner()),
            Err(e) => Err(e),
        },
        Encoder::Raw(mut e) => match std::io::Write::flush(&mut e) {
            Ok(()) => Ok(e.into_inner()),
            Err(e) => Err(e),
        },
        Encoder::None => Err(io::Error::from(io::ErrorKind::Interrupted)),
    }
}

pub struct FileStream {
    inner: Pin<Box<dyn SaphirFile>>,
    buffer: Vec<u8>,
    end_of_file: bool,
    range_len: Option<u64>,
    amount_read: usize,
}

impl FileStream {
    pub fn new<T: SaphirFile + 'static>(inner: T) -> Self {
        FileStream {
            inner: Box::pin(inner),
            buffer: Vec::with_capacity(MAX_BUFFER),
            end_of_file: false,
            range_len: None,
            amount_read: 0,
        }
    }

    pub async fn set_range(&mut self, range: (u64, u64)) -> io::Result<()> {
        let (start, end) = range;
        self.inner.seek(SeekFrom::Start(start)).await?;
        self.range_len = Some((end - start) + 1);
        Ok(())
    }

    pub fn get_size(&self) -> u64 {
        self.inner.get_size()
    }
}

impl Stream for FileStream {
    type Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.end_of_file {
            return Poll::Ready(None);
        }

        if let Some(range_len) = self.range_len {
            let usize_range = range_len as usize;
            let mut buffer = vec![0; usize_range];
            while self.amount_read < usize_range && !self.end_of_file {
                match self.inner.as_mut().poll_read(cx, &mut buffer) {
                    Poll::Ready(Ok(s)) => {
                        if s + self.amount_read <= usize_range {
                            self.buffer.extend_from_slice(&buffer[0..s]);
                            self.amount_read += s;
                            self.end_of_file = s == 0 || self.amount_read == usize_range;
                        } else {
                            let amount_to_read = usize_range - self.amount_read;
                            self.buffer.extend_from_slice(&buffer[0..amount_to_read]);
                            self.end_of_file = true;
                        }
                    }

                    Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(Box::new(e)))),

                    Poll::Pending => return Poll::Pending,
                }
            }
        } else {
            let mut buffer = vec![0; MAX_BUFFER];
            while self.buffer.len() < MAX_BUFFER && !self.end_of_file {
                match self.inner.as_mut().poll_read(cx, &mut buffer) {
                    Poll::Ready(Ok(s)) => {
                        if s > 0 {
                            self.buffer.extend_from_slice(&buffer[0..s]);
                        }
                        self.end_of_file = s == 0;
                    }

                    Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(Box::new(e)))),

                    Poll::Pending => return Poll::Pending,
                }
            }
        }

        Poll::Ready(Some(Ok(Bytes::from(std::mem::take(&mut self.buffer)))))
    }
}

impl From<File> for FileStream {
    fn from(other: File) -> Self {
        FileStream::new(other)
    }
}

impl Responder for FileStream {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        let mime = if let Some(mime) = &self.inner.get_mime() {
            mime.as_ref().to_string()
        } else {
            self.inner
                .get_path()
                .mime()
                .unwrap_or_else(|| {
                    if self.inner.get_path().is_dir() {
                        mime::TEXT_HTML_UTF_8
                    } else {
                        mime::TEXT_PLAIN_UTF_8
                    }
                })
                .as_ref()
                .to_string()
        };

        let len = self.inner.get_size();

        builder
            .file(self)
            .header(http::header::ACCEPT_RANGES, "bytes")
            .header(http::header::CONTENT_TYPE, mime)
            .header(http::header::CONTENT_LENGTH, len)
    }
}
