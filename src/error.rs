use std::fmt::{Display, Formatter, Error as FmtError};
use std::error::Error as StdError;
use http::Error as HttpCrateError;
use http::header::InvalidHeaderValue;
use std::io::Error as IoError;
use hyper::error::Error as HyperError;

/// Type representing an internal error inerrant to the underlining logic behind saphir
#[derive(Debug)]
pub enum InternalError {
    Http(HttpCrateError),
    Hyper(HyperError),
    Stack,
}

/// Error type throughout the saphir stack
#[derive(Debug)]
pub enum SaphirError {
    ///
    Internal(InternalError),
    ///
    Io(IoError),
    /// Body was taken and cannot be polled
    BodyAlreadyTaken,
    /// Custom error type to map any other error
    Custom(Box<dyn StdError + Send + Sync + 'static>),
    ///
    Other(String),
    /// Error from (de)serializing json data
    #[cfg(feature = "json")]
    SerdeJson(serde_json::error::Error),
    /// Error from deserializing form data
    #[cfg(feature = "form")]
    SerdeUrlDe(serde_urlencoded::de::Error),
    /// Error from serializing form data
    #[cfg(feature = "form")]
    SerdeUrlSer(serde_urlencoded::ser::Error),
}

#[cfg(feature = "json")]
impl From<serde_json::error::Error> for SaphirError {
    fn from(e: serde_json::error::Error) -> Self {
        SaphirError::SerdeJson(e)
    }
}

#[cfg(feature = "form")]
impl From<serde_urlencoded::de::Error> for SaphirError {
    fn from(e: serde_urlencoded::de::Error) -> Self {
        SaphirError::SerdeUrlDe(e)
    }
}

#[cfg(feature = "form")]
impl From<serde_urlencoded::ser::Error> for SaphirError {
    fn from(e: serde_urlencoded::ser::Error) -> Self {
        SaphirError::SerdeUrlSer(e)
    }
}

impl From<HttpCrateError> for SaphirError {
    fn from(e: HttpCrateError) -> Self {
        SaphirError::Internal(InternalError::Http(e))
    }
}

impl From<InvalidHeaderValue> for SaphirError {
    fn from(e: InvalidHeaderValue) -> Self {
        SaphirError::Internal(InternalError::Http(HttpCrateError::from(e)))
    }
}

impl From<HyperError> for SaphirError {
    fn from(e: HyperError) -> Self {
        SaphirError::Internal(InternalError::Hyper(e))
    }
}

impl From<IoError> for SaphirError {
    fn from(e: IoError) -> Self {
        SaphirError::Io(e)
    }
}

impl Display for SaphirError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        f.write_str("saphirError")
    }
}

impl StdError for SaphirError {}