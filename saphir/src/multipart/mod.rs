use std::{fmt::Debug, str::FromStr, sync::Arc};

use futures::TryStreamExt;
use futures_util::stream::Stream;
use mime::Mime;
use nom::lib::std::fmt::Formatter;
use parking_lot::Mutex;

use crate::{body::Bytes, multipart::parser::ParseFieldError, request::Request};
use std::path::Path;

mod parser;

#[derive(Debug)]
pub enum MultipartError {
    Parsing(ParseFieldError),
    /// Error type returned when the current field was not consumed before tying
    /// to get the next one
    NotConsumed,
    AlreadyConsumed,
    MissingBoundary,
    Finished,
    Hyper(hyper::error::Error),
    Io(std::io::Error),
    #[cfg(feature = "json")]
    Json(serde_json::error::Error),
    #[cfg(feature = "form")]
    Form(serde_urlencoded::de::Error),
}

impl From<ParseFieldError> for MultipartError {
    fn from(e: ParseFieldError) -> Self {
        if let ParseFieldError::Finished = e {
            MultipartError::Finished
        } else {
            MultipartError::Parsing(e)
        }
    }
}

pub struct FieldStream {
    inner: Option<ParseStream>,
    paren: DataStream,
}

impl FieldStream {
    pub(crate) fn stream(&mut self) -> &mut ParseStream {
        self.inner.as_mut().expect("STREAM: Field stream should not exists with a uninitialized stream")
    }
}

impl Drop for FieldStream {
    fn drop(&mut self) {
        let stream = self.inner.take().expect("RETURN: Field stream should not exists with a uninitialized stream");
        self.paren.return_stream(stream)
    }
}

pub(crate) struct ParseStream {
    pub buf: Vec<u8>,
    pub exhausted: bool,
    pub stream: Box<dyn Stream<Item = Result<Bytes, MultipartError>> + Unpin + Send>,
}

#[derive(Clone)]
struct DataStream {
    inner: Arc<Mutex<Option<ParseStream>>>,
}

impl DataStream {
    pub fn new<S>(s: S) -> Self
    where
        S: Stream<Item = Result<Bytes, MultipartError>> + Send + Unpin + 'static,
    {
        DataStream {
            inner: Arc::new(Mutex::new(Some(ParseStream {
                buf: vec![],
                exhausted: false,
                stream: Box::new(s),
            }))),
        }
    }

    pub fn take(&self) -> Option<FieldStream> {
        if let Some(stream) = self.inner.lock().take() {
            let paren = self.clone();
            Some(FieldStream { inner: Some(stream), paren })
        } else {
            None
        }
    }

    pub fn return_stream(&self, s: ParseStream) {
        *self.inner.lock() = Some(s);
    }
}

/// Represent a from-data field
/// Only the data needed to construct the field will be read into memory
pub struct Field {
    name: String,
    filename: Option<String>,
    content_type: Mime,
    content_transfer_encoding: Option<String>,
    boundary: String,
    stream: Option<FieldStream>,
}

impl Field {
    /// Returns the `name` param of the `Content-Disposition` header
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the optional `filename` param of the `Content-Disposition`
    /// header
    pub fn filename(&self) -> Option<&str> {
        self.filename.as_ref().map(|s| &**s)
    }

    /// Returns the optional `Content-Type` Mime and is defaulted to
    /// `text/plain` as specified by the spec
    pub fn content_type(&self) -> &Mime {
        &self.content_type
    }

    /// Returns the next available chunk of data for this field
    /// Calling this function repeatedly until the field is completely read will
    /// result in Ok(None) being returned
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, MultipartError> {
        if self
            .stream
            .as_ref()
            .and_then(|s| s.inner.as_ref())
            .filter(|s| !s.exhausted || !s.buf.is_empty())
            .is_none()
        {
            return Ok(None);
        }

        parser::parse_next_field_chunk(self.stream.as_mut().ok_or_else(|| MultipartError::AlreadyConsumed)?, self.boundary.as_str())
            .await
            .map(|b| if b.is_empty() { None } else { Some(b) })
    }

    /// Loads the entire field into memory and returns it as raw bytes
    pub async fn as_raw(&mut self) -> Result<Vec<u8>, MultipartError> {
        self.read_all().await
    }

    /// Loads the entire field into memory and returns it as plain text
    pub async fn as_text(&mut self) -> Result<String, MultipartError> {
        self.read_all().await.map(|b| String::from_utf8_lossy(b.as_slice()).to_string())
    }

    /// Loads the entire field into memory and parses it as JSON data *IF* the
    /// content-type is `application/json`
    #[cfg(feature = "json")]
    pub async fn as_json<T>(&mut self) -> Result<Option<T>, MultipartError>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        if self.content_type == mime::APPLICATION_JSON {
            let bytes = self.read_all().await?;
            serde_json::from_slice::<T>(bytes.as_slice()).map_err(MultipartError::Json).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Loads the entire field into memory and parses it as Form urlencoded data
    /// *IF* the content-type is `application/x-www-form-urlencoded`
    #[cfg(feature = "form")]
    pub async fn as_form<T>(&mut self) -> Result<Option<T>, MultipartError>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        if self.content_type == mime::APPLICATION_JSON {
            let bytes = self.read_all().await?;
            serde_urlencoded::from_bytes(bytes.as_slice()).map_err(MultipartError::Form).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Saves the field into a file on disk.
    pub async fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, MultipartError> {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::File::create(path).await.map_err(MultipartError::Io)?;
        let mut bytes_writen = 0;
        while let Some(bytes) = self.next_chunk().await? {
            file.write_all(bytes.as_slice()).await.map_err(MultipartError::Io)?;
            bytes_writen += bytes.len();
        }

        Ok(bytes_writen)
    }

    async fn read_all(&mut self) -> Result<Vec<u8>, MultipartError> {
        parser::parse_field_data(self.stream.take().ok_or_else(|| MultipartError::AlreadyConsumed)?, self.boundary.as_str())
            .await
            .map(|mut b| {
                if b[(b.len() - 2)..b.len()] == [0x0d, 0x0a] {
                    b.truncate(b.len() - 2)
                }

                b
            })
    }
}

impl Debug for Field {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Field")
            .field("name", &self.name)
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .field("content_transfer_encoding", &self.content_transfer_encoding)
            .field("boundary", &self.boundary)
            .finish()
    }
}

/// Struct used to parse a multipart body into fields
pub struct Multipart {
    boundary: String,
    inner: DataStream,
}

impl Multipart {
    /// Initialize a Multipart from a given request
    /// This will consume the body
    pub fn from_request(req: &mut Request) -> Result<Multipart, MultipartError> {
        let boundary = req
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|c_t| c_t.to_str().ok())
            .and_then(|c_t_str| Mime::from_str(c_t_str).ok())
            .filter(|mime| mime.type_() == mime::MULTIPART && mime.subtype() == mime::FORM_DATA)
            .as_ref()
            .and_then(|mime| mime.get_param(mime::BOUNDARY))
            .map(|name| name.to_string())
            .ok_or_else(|| MultipartError::MissingBoundary)?;

        let stream = req.body_mut().take().into_raw().map_err(MultipartError::Hyper);

        Ok(Self::from_part(boundary, stream))
    }

    /// Initialize the Multipart from raw parts
    pub fn from_part<S>(boundary: String, stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, MultipartError>> + Send + Unpin + 'static,
    {
        Multipart {
            boundary,
            inner: DataStream::new(stream),
        }
    }

    /// Parse the next field inside the body
    /// This will partially load the content, until the field "metadata" is
    /// parsed
    pub async fn next_field(&self) -> Result<Option<Field>, MultipartError> {
        if let Some(s) = self.inner.take() {
            match parser::parse_field(s, self.boundary.as_str()).await {
                Ok(f) => Ok(Some(f)),
                Err(MultipartError::Finished) => Ok(None),
                Err(e) => Err(e),
            }
        } else {
            Err(MultipartError::NotConsumed)
        }
    }
}
