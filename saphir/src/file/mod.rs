use std::{collections::HashMap, fs::Metadata, path::PathBuf, sync::Arc};

use futures_util::{
    io::{Error, SeekFrom},
    stream::Stream,
    task::{Context, Poll},
};
use hyper::body::Bytes;
use tokio::{fs::File as TokioFile, io::AsyncSeek, macros::support::Pin, prelude::*, sync::RwLock};

use crate::{file::middleware::PathExt, http_context::HttpContext, responder::Responder, response::Builder};
use futures::{Future, FutureExt};

mod conditional_request;
mod content_range;
mod etag;
pub mod middleware;
mod range;
mod range_requests;

const MAX_BUFFER: usize = 65534;

pub struct File<T: AsyncRead + AsyncSeek = TokioFile> {
    inner: Pin<Box<T>>,
    buffer: Vec<u8>,
    path: PathBuf,
    mime: Option<mime::Mime>,
    end_of_file: bool,
}

impl File {
    pub async fn open(path_str: &str) -> tokio::io::Result<File> {
        let path = path_str.to_string();
        match TokioFile::open(path_str).await {
            Ok(file) => Ok(File {
                inner: Box::pin(file),
                buffer: Vec::with_capacity(MAX_BUFFER),
                path: PathBuf::from(path),
                end_of_file: false,
                mime: None,
            }),

            Err(e) => Err(e),
        }
    }

    pub fn set_mime(&mut self, mime: mime::Mime) {
        self.mime = Some(mime);
    }

    pub async fn seek(&mut self, pos: SeekFrom) -> Result<u64, tokio::io::Error> {
        self.inner.seek(pos).await
    }
}

impl Responder for File {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        let mime = if let Some(mime) = &self.mime {
            mime.as_ref().to_string()
        } else {
            self.path
                .mime()
                .unwrap_or_else(|| if self.path.is_dir() { mime::TEXT_HTML_UTF_8 } else { mime::TEXT_PLAIN_UTF_8 })
                .as_ref()
                .to_string()
        };

        let len = self.path.size();

        let b = match builder.file(self) {
            Ok(b) => b,
            Err((b, _e)) => b.status(500).body("Unable to read file"),
        };

        b.header(http::header::CONTENT_TYPE, mime).header(http::header::CONTENT_LENGTH, len)
    }
}

impl Stream for File {
    type Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.end_of_file {
            return Poll::Ready(None);
        }

        let mut buffer = [0u8; MAX_BUFFER];
        while self.buffer.len() < MAX_BUFFER && !self.end_of_file {
            match self.inner.as_mut().poll_read(cx, &mut buffer) {
                Poll::Ready(Ok(s)) => {
                    self.buffer.extend_from_slice(&buffer[0..s]);
                    self.end_of_file = s == 0;
                }

                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(Box::new(e)))),

                Poll::Pending => return Poll::Pending,
            }
        }

        Poll::Ready(Some(Ok(Bytes::from(std::mem::take(&mut self.buffer)))))
    }
}

#[derive(PartialEq, Eq, Hash)]
pub enum Compression {
    Gzip,
    Deflate,
    Brotli,
    Raw,
}

impl Default for Compression {
    fn default() -> Self {
        Compression::Raw
    }
}

pub struct FileCache {
    inner: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
}

impl FileCache {
    pub fn new() -> Self {
        FileCache {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, key: (String, Compression)) -> Option<CachedFile> {
        if self.inner.read().await.contains_key(&key) {
            Some(CachedFile {
                key: key,
                inner: self.inner.clone(),
                position: 0,
                seek_from: None,
                file_seek_future: None,
                file_read_future: None,
                range: None,
                range_index: None,
            })
        } else {
            None
        }
    }
}

type FileSeekFuture = Pin<Box<dyn Future<Output = io::Result<u64>> + Send + Sync>>;
type FileReadFuture = Pin<Box<dyn Future<Output = io::Result<usize>> + Send + Sync>>;
pub struct CachedFile {
    key: (String, Compression),
    inner: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
    position: u64,
    seek_from: Option<SeekFrom>,
    file_seek_future: Option<FileSeekFuture>,
    file_read_future: Option<FileReadFuture>,
    range: Option<(u64, u64)>,
    range_index: Option<u64>,
}

impl CachedFile {
    async fn async_seek_owned(
        map: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
        seek_from: Option<SeekFrom>,
        key: (String, Compression),
        position: u64,
    ) -> io::Result<u64> {
        if let Some(seek_from) = seek_from {
            let file_size = map
                .read()
                .await
                .get(&key)
                .expect("CachedFile should not exist if file not present in cache")
                .len();
            match seek_from {
                SeekFrom::Start(i) => {
                    if i < file_size as u64 {
                        Ok(i)
                    } else {
                        Err(io::Error::from(io::ErrorKind::InvalidInput))
                    }
                }

                SeekFrom::Current(i) => {
                    if (i + position as i64) < file_size as i64 && (i + position as i64) >= 0 {
                        Ok(i as u64 + position)
                    } else {
                        Err(io::Error::from(io::ErrorKind::InvalidInput))
                    }
                }

                SeekFrom::End(i) => {
                    if file_size as i64 - i >= 0 {
                        Ok((file_size as i64 - i) as u64)
                    } else {
                        Err(io::Error::from(io::ErrorKind::InvalidInput))
                    }
                }
            }
        } else {
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        }
    }

    async fn async_read_owned(
        map: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
        key: (String, Compression),
        position: u64,
        range: Option<(u64, u64)>,
        range_index: Option<u64>,
    ) -> io::Result<usize> {
        unimplemented!()
    }
}

impl AsyncSeek for CachedFile {
    fn start_seek(mut self: Pin<&mut Self>, cx: &mut Context<'_>, position: SeekFrom) -> Poll<io::Result<()>> {
        self.seek_from = Some(position);
        Poll::Ready(Ok(()))
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let mut current_fut = self.file_seek_future.take();

        let res = if let Some(current) = current_fut.as_mut() {
            current.as_mut().poll(cx)
        } else {
            let mut current = Box::pin(Self::async_seek_owned(
                std::mem::take(&mut self.inner),
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
                    Poll::Ready(Ok(res))
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

impl AsyncRead for CachedFile {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let mut current_fut = self.file_read_future.take();

        let res = if let Some(current) = current_fut.as_mut() {
            current.as_mut().poll(cx)
        } else {
            let mut current = Box::pin(Self::async_read_owned(
                std::mem::take(&mut self.inner),
                std::mem::take(&mut self.key),
                std::mem::take(&mut self.position),
                self.range.take(),
                self.range_index.take(),
            ));
            let res = current.as_mut().poll(cx);
            current_fut = Some(current);
            res
        };

        match res {
            Poll::Ready(res) => Poll::Ready(res),
            Poll::Pending => {
                self.file_seek_future = current_fut;
                Poll::Pending
            }
        }
    }
}
