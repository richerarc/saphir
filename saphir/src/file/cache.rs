use crate::{
    error::SaphirError,
    file::{compress_file, middleware::PathExt, Compression, Encoder, File, FileCursor, FileInfo, FileStream, SaphirFile, MAX_BUFFER},
};
use futures::{
    io::{AsyncRead, AsyncSeek, Cursor},
    AsyncReadExt, AsyncSeekExt, Future,
};
use mime::Mime;
use std::{
    collections::HashMap,
    io,
    io::SeekFrom,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::sync::RwLock;

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
        if let Some(file) = self.inner.read().await.get(&key) {
            let path = PathBuf::from(&key.0);
            Some(CachedFile {
                key,
                inner: self.inner.clone(),
                path,
                mime: None,
                position: 0,
                file_seek_future: None,
                get_file_future: None,
                size: file.len() as u64,
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
            if file_size + self.get_size().await <= self.max_capacity && file_size <= self.max_file_size {
                let mime = cached_raw_file.get_mime().cloned();
                let compressed_file = compress_file(Box::pin(cached_raw_file), Encoder::None, compression).await?;
                Ok(FileStream::new(FileCacher::new(
                    (path_str.to_string(), compression),
                    Box::pin(FileCursor::new(compressed_file, mime, path.clone())) as Pin<Box<dyn SaphirFile>>,
                    self.clone(),
                )))
            } else {
                let mime = cached_raw_file.get_mime().cloned();
                let compressed_file = compress_file(Box::pin(cached_raw_file), Encoder::None, compression).await?;
                Ok(FileStream::new(FileCursor::new(compressed_file, mime, path.clone())))
            }
        } else {
            let file = File::open(path_str).await?;
            let file_size = file.get_path().size();
            if file_size + self.get_size().await <= self.max_capacity && file_size <= self.max_file_size {
                let mime = file.get_mime().cloned();
                let compressed_file = compress_file(Box::pin(file), Encoder::None, compression).await?;
                Ok(FileStream::new(FileCacher::new(
                    (path_str.to_string(), compression),
                    Box::pin(FileCursor::new(compressed_file, mime, path.clone())) as Pin<Box<dyn SaphirFile>>,
                    self.clone(),
                )))
            } else {
                let mime = file.get_mime().cloned();
                let compressed_file = compress_file(Box::pin(file), Encoder::None, compression).await?;
                Ok(FileStream::new(FileCursor::new(compressed_file, mime, path.clone())))
            }
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
    file_seek_future: Option<FileSeekFuture>,
    get_file_future: Option<ReadFileFuture>,
    size: u64,
}

impl CachedFile {
    async fn async_seek_owned(
        map: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
        seek_from: SeekFrom,
        key: (String, Compression),
        position: usize,
    ) -> io::Result<usize> {
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
    }

    async fn read_async(
        key: (String, Compression),
        inner: Arc<RwLock<HashMap<(String, Compression), Vec<u8>>>>,
        position: usize,
        len: usize,
    ) -> io::Result<Vec<u8>> {
        match inner.read().await.get(&key) {
            Some(bytes) => {
                let mut vec = vec![0; len];
                let mut cursor = Cursor::new(bytes);
                cursor.seek(SeekFrom::Start(position as u64)).await?;
                match cursor.read(vec.as_mut_slice()).await {
                    Ok(size) => Ok(vec[..size].to_vec()),
                    Err(e) => Err(e),
                }
            }

            None => Err(io::Error::from(io::ErrorKind::BrokenPipe)),
        }
    }
}

impl AsyncSeek for CachedFile {
    fn poll_seek(mut self: Pin<&mut Self>, cx: &mut Context<'_>, position: SeekFrom) -> Poll<io::Result<u64>> {
        let mut current_fut = self.file_seek_future.take();

        let res = if let Some(current) = current_fut.as_mut() {
            current.as_mut().poll(cx)
        } else {
            let mut current = Box::pin(Self::async_seek_owned(
                self.inner.clone(),
                position,
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

    fn get_size(&self) -> u64 {
        self.size
    }
}

impl AsyncRead for CachedFile {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let mut current_fut = self.get_file_future.take();

        let res = if let Some(current) = current_fut.as_mut() {
            current.as_mut().poll(cx)
        } else {
            let mut current = Box::pin(Self::read_async(self.key.clone(), self.inner.clone(), self.position, buf.len()));
            let res = current.as_mut().poll(cx);
            current_fut = Some(current);
            res
        };

        match res {
            Poll::Ready(res) => Poll::Ready(res.and_then(|bytes| {
                let len = bytes.len();
                if len > 0 {
                    self.position += len;
                    let mut b = bytes.as_slice();
                    std::io::Read::read(&mut b, buf)
                } else {
                    Ok(0)
                }
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
            buff: Vec::with_capacity(MAX_BUFFER),
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
                    self.buff.extend_from_slice(&buf[0..bytes]);
                }
                Poll::Ready(Ok(bytes))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncSeek for FileCacher {
    fn poll_seek(mut self: Pin<&mut Self>, cx: &mut Context<'_>, pos: SeekFrom) -> Poll<io::Result<u64>> {
        self.inner.as_mut().poll_seek(cx, pos)
    }
}

impl FileInfo for FileCacher {
    fn get_path(&self) -> &PathBuf {
        self.inner.get_path()
    }

    fn get_mime(&self) -> Option<&Mime> {
        self.inner.get_mime()
    }

    fn get_size(&self) -> u64 {
        self.inner.get_size()
    }
}
