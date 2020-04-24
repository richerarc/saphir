use std::{collections::HashMap, path::PathBuf, sync::Arc};

use futures_util::{
    io::SeekFrom,
    stream::Stream,
    task::{Context, Poll},
};
use hyper::body::Bytes;
use tokio::{fs::File as TokioFile, io::AsyncSeek, macros::support::Pin, prelude::*, sync::RwLock};

use crate::{error::SaphirError, file::middleware::PathExt, http_context::HttpContext, responder::Responder, response::Builder};
use flate2::write::{DeflateEncoder, GzEncoder};
use futures::{
    io::{AsyncRead, AsyncReadExt, Cursor},
    Future,
};
use mime::Mime;
use nom::lib::std::str::FromStr;
use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::io::AsyncRead as TokioAsyncRead;

mod conditional_request;
mod content_range;
mod etag;
pub mod middleware;
mod range;
mod range_requests;

pub const MAX_BUFFER: usize = 65534;

pub trait SaphirFile: AsyncRead + AsyncSeek + FileInfo + Sync + Send {}
impl<T: AsyncRead + AsyncSeek + FileInfo + Sync + Send> SaphirFile for T {}

pub trait FileInfo {
    fn get_path(&self) -> &PathBuf;
    fn get_mime(&self) -> Option<&mime::Mime>;
}

pub struct File {
    inner: Pin<Box<TokioFile>>,
    path: PathBuf,
    mime: Option<mime::Mime>,
}

impl FileInfo for File {
    fn get_path(&self) -> &PathBuf {
        &self.path
    }

    fn get_mime(&self) -> Option<&mime::Mime> {
        self.mime.as_ref()
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
            }),

            Err(e) => Err(e),
        }
    }
}

impl AsyncRead for File {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        dbg!(self.inner.as_mut().poll_read(cx, buf))
    }
}

impl AsyncSeek for File {
    fn start_seek(mut self: Pin<&mut Self>, cx: &mut Context<'_>, position: SeekFrom) -> Poll<io::Result<()>> {
        self.inner.as_mut().start_seek(cx, position)
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.inner.as_mut().poll_complete(cx)
    }
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy)]
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

#[derive(Clone)]
pub struct FileCache {
    inner: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
    max_file_size: u64,
    max_capacity: u64,
    current_size: Arc<AtomicU64>,
}

impl FileCache {
    pub fn new(max_file_size: u64, max_capacity: u64) -> Self {
        FileCache {
            inner: Arc::new(RwLock::new(HashMap::new())),
            max_file_size,
            max_capacity,
            current_size: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn get(&self, key: (String, Compression)) -> Option<CachedFile> {
        if self.inner.read().await.contains_key(&key) {
            let path = PathBuf::from(&key.0);
            Some(CachedFile {
                key,
                inner: self.inner.clone(),
                path,
                mime: None,
                position: 0,
                seek_from: None,
                file_seek_future: None,
                get_file_future: None,
            })
        } else {
            None
        }
    }

    pub async fn insert(&mut self, key: (String, Compression), value: Vec<u8>) {
        self.current_size.fetch_add(value.len() as u64, Ordering::Relaxed);
        self.inner.write().await.insert(key, value);
    }

    pub async fn get_size(&self) -> u64 {
        self.current_size.load(Ordering::Relaxed)
    }

    pub async fn open_file(&mut self, path: &PathBuf, compression: Compression) -> Result<FileStream, SaphirError> {
        let path_str = path.to_str().unwrap_or_default();
        if let Some(cached_file) = self.get((path_str.to_string(), compression)).await {
            Ok(FileStream::new(cached_file))
        } else if let Some(cached_raw_file) = self.get((path_str.to_string(), Compression::Raw)).await {
            let file_size = cached_raw_file.get_path().size();
            let inner = if file_size + self.get_size().await <= self.max_capacity && file_size <= self.max_file_size {
                Box::pin(FileCompressor::new(compression, Box::pin(cached_raw_file))) as Pin<Box<dyn SaphirFile>>
            } else {
                Box::pin(cached_raw_file) as Pin<Box<dyn SaphirFile>>
            };
            Ok(FileStream::new(FileCacher::new((path_str.to_string(), compression), inner, self.clone())))
        } else {
            let file = File::open(path_str).await?;
            let file_size = file.get_path().size();
            let inner = if file_size + self.get_size().await <= self.max_capacity && file_size <= self.max_file_size {
                Box::pin(FileCompressor::new(compression, Box::pin(file))) as Pin<Box<dyn SaphirFile>>
            } else {
                Box::pin(file) as Pin<Box<dyn SaphirFile>>
            };
            Ok(FileStream::new(FileCacher::new((path_str.to_string(), compression), inner, self.clone())))
        }
    }

    pub async fn open_file_with_range(&mut self, path: &PathBuf, range: (u64, u64)) -> Result<FileStream, SaphirError> {
        let path_str = path.to_str().unwrap_or_default();
        if let Some(cached_file) = self.get((path_str.to_string(), Compression::Raw)).await {
            let mut file_stream = FileStream::new(cached_file);
            file_stream.set_range(range).await?;
            Ok(file_stream)
        } else {
            let mut file_stream = FileStream::new(FileCacher::new(
                (path_str.to_string(), Compression::Raw),
                Box::pin(File::open(path_str).await?),
                self.clone(),
            ));
            file_stream.set_range(range).await?;
            Ok(file_stream)
        }
    }
}

type FileSeekFuture = Pin<Box<dyn Future<Output = io::Result<usize>> + Send + Sync>>;
type ReadFileFuture = Pin<Box<dyn Future<Output = io::Result<Vec<u8>>> + Send + Sync>>;

pub struct CachedFile {
    key: (String, Compression),
    inner: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
    path: PathBuf,
    mime: Option<mime::Mime>,
    position: usize,
    seek_from: Option<SeekFrom>,
    file_seek_future: Option<FileSeekFuture>,
    get_file_future: Option<ReadFileFuture>,
}

impl CachedFile {
    async fn async_seek_owned(
        map: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
        seek_from: Option<SeekFrom>,
        key: (String, Compression),
        position: usize,
    ) -> io::Result<usize> {
        if let Some(seek_from) = seek_from {
            let file_size = map.read().await.get(&key).ok_or(io::Error::from(io::ErrorKind::BrokenPipe))?.len();
            match seek_from {
                SeekFrom::Start(i) => {
                    if (i as usize) < file_size {
                        Ok(i as usize)
                    } else {
                        Err(io::Error::from(io::ErrorKind::InvalidInput))
                    }
                }

                SeekFrom::Current(i) => {
                    if (i + position as i64) >= 0 {
                        Ok((i + position as i64) as usize)
                    } else {
                        Err(io::Error::from(io::ErrorKind::InvalidInput))
                    }
                }

                SeekFrom::End(i) => {
                    if file_size as i64 + i >= 0 {
                        Ok((file_size as i64 + i) as usize)
                    } else {
                        Err(io::Error::from(io::ErrorKind::InvalidInput))
                    }
                }
            }
        } else {
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        }
    }

    async fn read_async(
        key: (String, Compression),
        inner: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
        position: usize,
        len: usize,
    ) -> io::Result<Vec<u8>> {
        match inner.read().await.get(&key) {
            Some(bytes) => {
                let len = if len > bytes.len() { bytes.len() } else { len };
                Ok(bytes.get(position..len).map(|bytes| bytes.to_vec()).unwrap_or_else(Vec::new))
            }

            None => Err(io::Error::from(io::ErrorKind::BrokenPipe)),
        }
    }
}

impl AsyncSeek for CachedFile {
    fn start_seek(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, position: SeekFrom) -> Poll<io::Result<()>> {
        self.seek_from = Some(position);
        Poll::Ready(Ok(()))
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let mut current_fut = self.file_seek_future.take();

        let res = if let Some(current) = current_fut.as_mut() {
            current.as_mut().poll(cx)
        } else {
            let mut current = Box::pin(Self::async_seek_owned(
                self.inner.clone(),
                self.seek_from.take(),
                std::mem::take(&mut self.key),
                std::mem::take(&mut self.position),
            ));
            let res = current.as_mut().poll(cx);
            current_fut = Some(current);
            res
        };

        match res {
            Poll::Ready(res) => match res {
                Ok(res) => {
                    self.position = res;
                    Poll::Ready(Ok(res as u64))
                }

                Err(e) => Poll::Ready(Err(e)),
            },
            Poll::Pending => {
                self.file_seek_future = current_fut;
                Poll::Pending
            }
        }
    }
}

impl FileInfo for CachedFile {
    fn get_path(&self) -> &PathBuf {
        &self.path
    }

    fn get_mime(&self) -> Option<&Mime> {
        self.mime.as_ref()
    }
}

impl AsyncRead for CachedFile {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let mut current_fut = self.get_file_future.take();

        let res = if let Some(current) = current_fut.as_mut() {
            current.as_mut().poll(cx)
        } else {
            let mut current = Box::pin(Self::read_async(
                std::mem::take(&mut self.key),
                self.inner.clone(),
                std::mem::take(&mut self.position),
                buf.len(),
            ));
            let res = current.as_mut().poll(cx);
            current_fut = Some(current);
            res
        };

        match res {
            Poll::Ready(res) => Poll::Ready(res.map(|bytes| {
                buf.copy_from_slice(bytes.as_slice());
                bytes.len()
            })),

            Poll::Pending => {
                self.get_file_future = current_fut;
                Poll::Pending
            }
        }
    }
}

pub struct FileCacher {
    key: (String, Compression),
    inner: Pin<Box<dyn SaphirFile>>,
    buff: Vec<u8>,
    cache: FileCache,
}

impl FileCacher {
    pub fn new(key: (String, Compression), inner: Pin<Box<dyn SaphirFile>>, cache: FileCache) -> Self {
        FileCacher {
            key,
            inner,
            buff: vec![],
            cache,
        }
    }

    fn save_file_to_cache(&mut self) {
        let key = std::mem::take(&mut self.key);
        let buff = std::mem::take(&mut self.buff);
        let mut cache = self.cache.clone();
        tokio::spawn(async move {
            cache.insert(key, buff).await;
        });
    }
}

impl Drop for FileCacher {
    fn drop(&mut self) {
        self.save_file_to_cache();
    }
}

impl AsyncRead for FileCacher {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        match self.inner.as_mut().poll_read(cx, buf) {
            Poll::Ready(Ok(bytes)) => {
                if bytes > 0 {
                    self.buff.extend_from_slice(dbg!(buf));
                }
                Poll::Ready(Ok(bytes))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncSeek for FileCacher {
    fn start_seek(mut self: Pin<&mut Self>, cx: &mut Context<'_>, position: SeekFrom) -> Poll<io::Result<()>> {
        self.inner.as_mut().start_seek(cx, position)
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.inner.as_mut().poll_complete(cx)
    }
}

impl FileInfo for FileCacher {
    fn get_path(&self) -> &PathBuf {
        self.inner.get_path()
    }

    fn get_mime(&self) -> Option<&Mime> {
        self.inner.get_mime()
    }
}

pub enum Encoder {
    Brotli(brotli::CompressorWriter<Vec<u8>>),
    Gzip(GzEncoder<Vec<u8>>),
    Deflate(DeflateEncoder<Vec<u8>>),
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
            Encoder::None => Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Encoder::Brotli(e) => e.flush(),
            Encoder::Gzip(e) => e.flush(),
            Encoder::Deflate(e) => e.flush(),
            Encoder::None => Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
        }
    }
}

type CompressFileFuture = Pin<Box<dyn Future<Output = io::Result<Vec<u8>>> + Send + Sync>>;
pub struct FileCompressor {
    inner: Option<Pin<Box<dyn SaphirFile>>>,
    encoder: Encoder,
    compression: Compression,
    compress_file_fut: Option<CompressFileFuture>,
    compressed_file: Option<Pin<Box<Cursor<Vec<u8>>>>>,
}

impl FileCompressor {
    pub fn new(compression: Compression, inner: Pin<Box<dyn SaphirFile>>) -> Self {
        FileCompressor {
            inner: Some(inner),
            encoder: Encoder::None,
            compression,
            compress_file_fut: None,
            compressed_file: None,
        }
    }

    async fn compress_file(mut inner: Pin<Box<dyn SaphirFile>>, mut encoder: Encoder, compression: Compression) -> io::Result<Vec<u8>> {
        if encoder.is_none() && compression != Compression::Raw {
            encoder = match compression {
                Compression::Gzip => Encoder::Gzip(GzEncoder::new(Vec::new(), flate2::Compression::default())),
                Compression::Deflate => Encoder::Deflate(DeflateEncoder::new(Vec::new(), flate2::Compression::default())),
                Compression::Brotli => Encoder::Brotli(brotli::CompressorWriter::new(Vec::new(), MAX_BUFFER, 6, 22)),
                Compression::Raw => Encoder::None,
            }
        } else if compression == Compression::Raw {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }

        loop {
            let mut buffer = Vec::with_capacity(MAX_BUFFER);
            match inner.read(buffer.as_mut_slice()).await {
                Ok(size) => {
                    if size > 0 {
                        encoder.write(&buffer)?;
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
                Ok(()) => Ok(dbg!(e.into_inner())),
                Err(e) => Err(e),
            },
            Encoder::None => Err(io::Error::from(io::ErrorKind::Interrupted)),
        }
    }
}

impl AsyncRead for FileCompressor {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        if let Some(compressed_file) = &mut self.compressed_file {
            compressed_file.as_mut().poll_read(cx, buf)
        } else {
            let mut current_fut = self.compress_file_fut.take();

            let res = if let Some(current) = current_fut.as_mut() {
                current.as_mut().poll(cx)
            } else {
                let mut current = Box::pin(Self::compress_file(
                    self.inner.take().expect("should be ok"),
                    std::mem::take(&mut self.encoder),
                    std::mem::take(&mut self.compression),
                ));
                let res = current.as_mut().poll(cx);
                current_fut = Some(current);
                res
            };

            match res {
                Poll::Ready(res) => match res {
                    Ok(file) => {
                        self.compressed_file = Some(Box::pin(Cursor::new(file)));
                        self.compressed_file.as_mut().expect("should be ok").as_mut().poll_read(cx, buf)
                    }

                    Err(e) => Poll::Ready(Err(e)),
                },

                Poll::Pending => {
                    self.compress_file_fut = current_fut;
                    Poll::Pending
                }
            }
        }
    }
}

impl AsyncSeek for FileCompressor {
    fn start_seek(mut self: Pin<&mut Self>, cx: &mut Context<'_>, position: SeekFrom) -> Poll<io::Result<()>> {
        if let Some(inner) = self.inner.as_mut() {
            inner.as_mut().start_seek(cx, position)
        } else {
            Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)))
        }
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        if let Some(inner) = self.inner.as_mut() {
            inner.as_mut().poll_complete(cx)
        } else {
            Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)))
        }
    }
}

impl FileInfo for FileCompressor {
    fn get_path(&self) -> &PathBuf {
        self.inner.as_ref().expect("should be ok").get_path()
    }

    fn get_mime(&self) -> Option<&Mime> {
        self.inner.as_ref().and_then(|inner| inner.get_mime())
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
        self.range_len = Some(end - start);
        Ok(())
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
            let mut buffer = Vec::with_capacity(usize_range);
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
            let mut buffer = Vec::with_capacity(MAX_BUFFER);
            while self.buffer.len() < MAX_BUFFER && !self.end_of_file {
                match self.inner.as_mut().poll_read(cx, &mut buffer) {
                    Poll::Ready(Ok(s)) => {
                        self.buffer.extend_from_slice(dbg!(&buffer[0..s]));
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

        let len = self.inner.get_path().size();

        let b = match builder.file(self) {
            Ok(b) => b,
            Err((b, _e)) => b.status(500).body("Unable to read file"),
        };

        b.header(http::header::ACCEPT_RANGES, "bytes")
            .header(http::header::CONTENT_TYPE, mime)
            .header(http::header::CONTENT_LENGTH, len)
    }
}
