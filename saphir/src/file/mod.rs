use crate::file::etag::{EntityTag, SystemTimeExt};
use crate::http_context::HttpContext;
use crate::responder::Responder;
use crate::response::Builder;
use futures_util::stream::Stream;
use futures_util::task::{Context, Poll};
use hyper::body::Bytes;
use std::fs::Metadata;
use std::path::PathBuf;
use tokio::fs::File as TokioFile;
use tokio::macros::support::Pin;
use tokio::prelude::*;

mod conditional_request;
mod etag;
pub mod middleware;
mod range_utils;

const MAX_BUFFER: usize = 65534;

pub struct File {
    inner: Pin<Box<TokioFile>>,
    mime: mime::Mime,
    metadata: Metadata,
    buffer: Vec<u8>,
    path: PathBuf,
    end_of_file: bool,
}

impl File {
    pub async fn open(path_str: &str, mime: mime::Mime) -> tokio::io::Result<File> {
        let path = path_str.to_string();
        match TokioFile::open(path_str).await {
            Ok(file) => file.metadata().await.map(|metadata| File {
                inner: Box::pin(file),
                buffer: Vec::with_capacity(MAX_BUFFER),
                path: PathBuf::from(path),
                end_of_file: false,
                mime,
                metadata,
            }),

            Err(e) => Err(e),
        }
    }
}

impl Responder for File {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        let mime = &self.mime;
        let len = self.metadata.len();
        let last_modified = self.metadata.modified();

        let mut b = match builder.file(self) {
            Ok(b) => b,
            Err((b, _e)) => b.status(500).body("Unable to read file"),
        };

        if let Ok(time) = last_modified {
            b = b.header(
                http::header::ETAG,
                EntityTag::new(false, format!("{}-{}", time.timestamp(), len).as_str()).get_tag(),
            )
        }

        b.header(http::header::CONTENT_TYPE, mime.to_string()).header(http::header::CONTENT_LENGTH, len)
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
