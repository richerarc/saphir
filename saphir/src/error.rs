use crate::{responder::Responder, response::Builder};
use http::{
    header::{InvalidHeaderValue, ToStrError},
    Error as HttpCrateError,
};
use hyper::error::Error as HyperError;
use std::{
    error::Error as StdError,
    fmt::{Display, Error as FmtError, Formatter},
    io::Error as IoError,
};

/// Type representing an internal error inerrant to the underlining logic behind
/// saphir
#[derive(Debug)]
pub enum InternalError {
    Http(HttpCrateError),
    Hyper(HyperError),
    ToStr(ToStrError),
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
    ///
    MissingParameter(String, bool),
    ///
    InvalidParameter(String, bool),
    ///
    #[cfg(feature = "multipart")]
    Multipart(crate::multipart::MultipartError),
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

impl From<ToStrError> for SaphirError {
    fn from(e: ToStrError) -> Self {
        SaphirError::Internal(InternalError::ToStr(e))
    }
}

impl From<crate::multipart::MultipartError> for SaphirError {
    fn from(e: crate::multipart::MultipartError) -> Self {
        SaphirError::Multipart(e)
    }
}

impl Display for SaphirError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        f.write_str("saphirError")
    }
}

impl StdError for SaphirError {}

impl Responder for SaphirError {
    fn respond_with_builder(self, builder: Builder) -> Builder {
        match self {
            SaphirError::Internal(e) => {
                warn!("Saphir encountered an internal error that was returned as a responder: {:?}", e);
                builder.status(500)
            }
            SaphirError::Io(e) => {
                warn!("Saphir encountered an Io error that was returned as a responder: {:?}", e);
                builder.status(500)
            }
            SaphirError::BodyAlreadyTaken => {
                warn!("A controller handler attempted to take the request body more thant one time");
                builder.status(500)
            }
            SaphirError::Custom(e) => {
                warn!("A custom error was returned as a responder: {:?}", e);
                builder.status(500)
            }
            SaphirError::Other(e) => {
                warn!("Saphir encountered an Unknown error that was returned as a responder: {:?}", e);
                builder.status(500)
            }
            #[cfg(feature = "json")]
            SaphirError::SerdeJson(e) => {
                debug!("Unable to de/serialize json type: {:?}", e);
                builder.status(400)
            }
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlDe(e) => {
                debug!("Unable to deserialize form type: {:?}", e);
                builder.status(400)
            }
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlSer(e) => {
                debug!("Unable to serialize form type: {:?}", e);
                builder.status(400)
            }
            SaphirError::MissingParameter(name, is_query) => {
                if is_query {
                    debug!("Missing query parameter {}", name);
                } else {
                    debug!("Missing path parameter {}", name);
                }

                builder.status(400)
            }
            SaphirError::InvalidParameter(name, is_query) => {
                if is_query {
                    debug!("Unable to parse query parameter {}", name);
                } else {
                    debug!("Unable to parse path parameter {}", name);
                }

                builder.status(400)
            }
            #[cfg(feature = "multipart")]
            SaphirError::Multipart(e) => {
                debug!("Unable to parse multipart data: {:?}", e);
                builder.status(400)
            }
        }
    }
}
