use crate::{
    body::{Body, Bytes},
    http_context::HttpContext,
    request::{FromRequest, Request},
    responder::Responder,
    response::Builder,
};
use futures::TryStreamExt;
use futures_util::stream::Stream;
use mime::Mime;
use multer::{Field as RawField, Multipart as RawMultipart};
use std::{
    fmt::{Debug, Formatter},
    path::Path,
    str::FromStr,
    sync::Arc,
};
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Error, Debug)]
pub enum MultipartError {
    #[error("Multer: {0}")]
    Multer(multer::Error),
    /// Error type returned when the current field was not consumed before tying
    /// to get the next one
    #[deprecated(since = "3.1.0", note = "This error cannot occur anymore")]
    #[error("Field not consumed")]
    NotConsumed,
    #[error("Field already consumed")]
    AlreadyConsumed,
    #[error("Missing boundary")]
    MissingBoundary,
    #[error("Finished")]
    Finished,
    #[error("Hyper: {0}")]
    Hyper(hyper::Error),
    #[error("Io: {0}")]
    Io(std::io::Error),
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    #[error("Json: {0}")]
    Json(serde_json::error::Error),
    #[cfg(feature = "form")]
    #[cfg_attr(docsrs, doc(cfg(feature = "form")))]
    #[error("Form: {0}")]
    Form(serde_urlencoded::de::Error),
}

impl From<multer::Error> for MultipartError {
    fn from(e: multer::Error) -> Self {
        Self::Multer(e)
    }
}

impl Responder for MultipartError {
    fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
        let op_id = {
            #[cfg(not(feature = "operation"))]
            {
                let _ = ctx;
                String::new()
            }

            #[cfg(feature = "operation")]
            {
                format!("[Operation id: {}] ", ctx.operation_id)
            }
        };

        debug!("{}Unable to parse multipart data: {:?}", op_id, &self);
        builder.status(400)
    }
}

/// Represent a form-data field
/// Only the data needed to construct the field will be read into memory
pub struct Field<'f> {
    // Note: this is only an Option to keep `as_raw()` and `as_text()` for backward compatibility.
    // FIXME: Remove the option in saphir 4.0.0
    raw: Option<RawField<'f>>,
}

impl<'f> From<RawField<'f>> for Field<'f> {
    fn from(raw: RawField<'f>) -> Self {
        Self { raw: Some(raw) }
    }
}

impl Field<'_> {
    /// Returns the `name` param of the `Content-Disposition` header.
    ///
    /// Currently return `""` if the name is missing.
    /// <br><br>
    /// *This will return an `Option<*str>` in saphir 4.0.0*
    pub fn name(&self) -> &str {
        self.raw.as_ref().and_then(|r| r.name()).unwrap_or_default()
    }

    /// Returns the optional `filename` param of the `Content-Disposition`
    /// header
    pub fn filename(&self) -> Option<&str> {
        self.raw.as_ref().and_then(|r| r.file_name())
    }

    /// Returns the optional `Content-Type` Mime and is defaulted to
    /// `text/plain` as specified by the spec
    pub fn content_type(&self) -> &Mime {
        self.raw.as_ref().and_then(|r| r.content_type()).unwrap_or(&mime::TEXT_PLAIN)
    }

    /// **Deprecated**;
    /// **Use `to_raw()` instead.**
    /// <br><br>
    /// Loads the entire field into memory and returns it as raw bytes
    /// <br><br>
    /// *This will be removed in saphir 4.0.0*
    #[deprecated(since = "3.1.0", note = "Use `to_raw()` instead.")]
    pub async fn as_raw(&mut self) -> Result<Vec<u8>, MultipartError> {
        let raw = std::mem::take(&mut self.raw).ok_or(MultipartError::AlreadyConsumed)?;
        raw.bytes().await.map(|b| b.to_vec()).map_err(MultipartError::from)
    }

    /// Loads the entire field into memory and returns it as raw bytes
    pub async fn to_raw(self) -> Result<Vec<u8>, MultipartError> {
        let raw = self.raw.ok_or(MultipartError::AlreadyConsumed)?;
        raw.bytes().await.map(|b| b.to_vec()).map_err(MultipartError::from)
    }

    /// **Deprecated**;
    /// **Use `to_raw()` instead.**
    /// <br><br>
    /// Loads the entire field into memory and returns it as plain text
    /// <br><br>
    /// *This will be removed in saphir 4.0.0*
    #[deprecated(since = "3.1.0", note = "use `to_text()` instead")]
    pub async fn as_text(&mut self) -> Result<String, MultipartError> {
        let raw = std::mem::take(&mut self.raw).ok_or(MultipartError::AlreadyConsumed)?;
        raw.text().await.map_err(MultipartError::from)
    }

    /// Loads the entire field into memory and returns it as plain text
    pub async fn to_text(self) -> Result<String, MultipartError> {
        let raw = self.raw.ok_or(MultipartError::AlreadyConsumed)?;
        raw.text().await.map_err(MultipartError::from)
    }

    /// **Deprecated**;
    /// **Use `to_raw()` instead.**
    /// <br><br>
    /// Loads the entire field into memory and parses it as JSON data
    /// *IF* the content-type is `application/json`.
    ///
    /// Return `Ok(None)` if the content-type was incorrect.
    /// <br><br>
    /// *This will be removed in saphir 4.0.0*
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    #[deprecated(since = "3.1.0", note = "use `to_json()` instead")]
    pub async fn as_json<T>(&mut self) -> Result<Option<T>, MultipartError>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        if *self.content_type() == mime::APPLICATION_JSON {
            let raw = std::mem::take(&mut self.raw).ok_or(MultipartError::AlreadyConsumed)?;
            let bytes = raw.bytes().await?;
            serde_json::from_slice::<T>(bytes.as_ref()).map_err(MultipartError::Json).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Loads the entire field into memory and parses it as JSON data
    /// *IF* the content-type is `application/json`.
    ///
    /// Return `Ok(None)` if the content-type was incorrect.
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    #[deprecated(since = "3.1.0", note = "use `to_json()` instead")]
    pub async fn to_json<T>(self) -> Result<Option<T>, MultipartError>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        if *self.content_type() == mime::APPLICATION_JSON {
            let raw = self.raw.ok_or(MultipartError::AlreadyConsumed)?;
            let bytes = raw.bytes().await?;
            serde_json::from_slice::<T>(bytes.as_ref()).map_err(MultipartError::Json).map(Some)
        } else {
            Ok(None)
        }
    }

    /// **Deprecated**;
    /// **Use `to_raw()` instead.**
    /// <br><br>
    /// Loads the entire field into memory and parses it as Form urlencoded data
    /// *IF* the content-type is `application/x-www-form-urlencoded`.
    ///
    /// Return `Ok(None)` if the content-type was incorrect.
    /// <br><br>
    /// *This will be removed in saphir 4.0.0*
    #[cfg(feature = "form")]
    #[cfg_attr(docsrs, doc(cfg(feature = "form")))]
    #[deprecated(since = "3.1.0", note = "use `to_form()` instead")]
    pub async fn as_form<T>(&mut self) -> Result<Option<T>, MultipartError>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        if *self.content_type() == mime::APPLICATION_WWW_FORM_URLENCODED {
            let raw = std::mem::take(&mut self.raw).ok_or(MultipartError::AlreadyConsumed)?;
            let bytes = raw.bytes().await?;
            serde_urlencoded::from_bytes(bytes.as_ref()).map_err(MultipartError::Form).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Loads the entire field into memory and parses it as Form urlencoded data
    /// *IF* the content-type is `application/x-www-form-urlencoded`.
    ///
    /// Return `Ok(None)` if the content-type was incorrect.
    #[cfg(feature = "form")]
    #[cfg_attr(docsrs, doc(cfg(feature = "form")))]
    pub async fn to_form<T>(self) -> Result<Option<T>, MultipartError>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        if *self.content_type() == mime::APPLICATION_WWW_FORM_URLENCODED {
            let raw = self.raw.ok_or(MultipartError::AlreadyConsumed)?;
            let bytes = raw.bytes().await?;
            serde_urlencoded::from_bytes(bytes.as_ref()).map_err(MultipartError::Form).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Saves the field into a file on disk.
    /// <br><br>
    /// *This function will consume the field in saphir 4.0.0*
    pub async fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, MultipartError> {
        let mut raw = std::mem::take(&mut self.raw).ok_or(MultipartError::AlreadyConsumed)?;
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::File::create(path).await.map_err(MultipartError::Io)?;
        let mut bytes_writen = 0;
        while let Some(bytes) = raw.chunk().await? {
            file.write_all(bytes.as_ref()).await.map_err(MultipartError::Io)?;
            bytes_writen += bytes.len();
        }

        Ok(bytes_writen)
    }
}

impl Debug for Field<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.raw.fmt(f)
    }
}

/// Struct used to parse a multipart body into fields.
/// *In Saphir 4.0.0, this will have a lifetime*
pub struct Multipart {
    raw: Arc<Mutex<multer::Multipart<'static>>>,
}

impl FromRequest for Multipart {
    type Err = MultipartError;
    type Fut = futures::future::Ready<Result<Self, Self::Err>>;

    fn from_request(req: &mut Request<Body<Bytes>>) -> Self::Fut {
        let boundary = req
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|c_t| c_t.to_str().ok())
            .and_then(|c_t_str| Mime::from_str(c_t_str).ok())
            .filter(|mime| mime.type_() == mime::MULTIPART && mime.subtype() == mime::FORM_DATA)
            .as_ref()
            .and_then(|mime| mime.get_param(mime::BOUNDARY))
            .map(|name| name.to_string())
            .ok_or(MultipartError::MissingBoundary);

        let stream = req.body_mut().take().into_raw().map_err(MultipartError::Hyper);

        futures::future::ready(boundary.map(|boundary| Self::from_part(boundary, stream)))
    }
}

impl Multipart {
    /// Initialize the Multipart from raw parts, for convenience, Multipart
    /// implement the FromRequest trait, from_request() should be used
    /// instead
    pub fn from_part<S>(boundary: String, stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, MultipartError>> + Send + Sync + Unpin + 'static,
    {
        Multipart {
            raw: Arc::from(Mutex::new(RawMultipart::new(stream, &boundary))),
        }
    }

    /// Yields the next [`MultipartField`] if available.
    ///
    /// Any previous `Field` returned by this method must be dropped before
    /// calling this method or [`Multipart::next_field_with_idx()`] again. See
    /// [field-exclusivity](#field-exclusivity) for details.
    pub async fn next_field(&self) -> Result<Option<Field<'static>>, MultipartError> {
        let cloned_raw = self.raw.clone();
        let mut raw_locked = cloned_raw.lock().await;
        let next_field = raw_locked.next_field().await?;
        Ok(next_field.map(Field::from))
    }
}
