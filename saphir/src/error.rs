use crate::{
    http_context::HttpContext,
    responder::{DynResponder, Responder},
    response::Builder,
};
use http::{
    header::{InvalidHeaderValue, ToStrError},
    Error as HttpCrateError,
};
use hyper::error::Error as HyperError;
use std::{
    error::Error as StdError,
    fmt::{Debug, Display, Error as FmtError, Formatter},
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
pub enum SaphirError {
    ///
    Internal(InternalError),
    ///
    Io(IoError),
    /// Body was taken and cannot be polled
    BodyAlreadyTaken,
    /// The request was moved by a middleware without ending the request
    /// processing
    RequestMovedBeforeHandler,
    /// The response was moved before being sent to the client
    ResponseMoved,
    /// Custom error type to map any other error
    Custom(Box<dyn StdError + Send + Sync + 'static>),
    /// Custom error type to map any other error
    Responder(Box<dyn DynResponder + Send + Sync + 'static>),
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
}

impl Debug for SaphirError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SaphirError::Internal(d) => std::fmt::Debug::fmt(d, f),
            SaphirError::Io(d) => std::fmt::Debug::fmt(d, f),
            SaphirError::BodyAlreadyTaken => f.write_str("BodyAlreadyTaken"),
            SaphirError::RequestMovedBeforeHandler => f.write_str("RequestMovedBeforeHandler"),
            SaphirError::ResponseMoved => f.write_str("ResponseMoved"),
            SaphirError::Custom(d) => std::fmt::Debug::fmt(d, f),
            SaphirError::Responder(_) => f.write_str("Responder"),
            SaphirError::Other(d) => std::fmt::Debug::fmt(d, f),
            #[cfg(feature = "json")]
            SaphirError::SerdeJson(d) => std::fmt::Debug::fmt(d, f),
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlDe(d) => std::fmt::Debug::fmt(d, f),
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlSer(d) => std::fmt::Debug::fmt(d, f),
            SaphirError::MissingParameter(d, _) => std::fmt::Debug::fmt(d, f),
            SaphirError::InvalidParameter(d, _) => std::fmt::Debug::fmt(d, f),
        }
    }
}

impl SaphirError {
    pub fn responder<T: Responder + Send + Sync + 'static>(e: T) -> Self {
        SaphirError::Responder(Box::new(Some(e)))
    }
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

impl Display for SaphirError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        f.write_str("saphirError")
    }
}

impl StdError for SaphirError {}

impl Responder for SaphirError {
    #[allow(unused_variables)]
    fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
        let op_id = {
            #[cfg(not(feature = "operation"))]
            {
                String::new()
            }

            #[cfg(feature = "operation")]
            {
                format!("[Operation id: {}] ", ctx.operation_id)
            }
        };

        match self {
            SaphirError::Internal(e) => {
                warn!("{}Saphir encountered an internal error that was returned as a responder: {:?}", op_id, e);
                builder.status(500)
            }
            SaphirError::Io(e) => {
                warn!("{}Saphir encountered an Io error that was returned as a responder: {:?}", op_id, e);
                builder.status(500)
            }
            SaphirError::BodyAlreadyTaken => {
                warn!("{}A controller handler attempted to take the request body more thant one time", op_id);
                builder.status(500)
            }
            SaphirError::Custom(e) => {
                warn!("{}A custom error was returned as a responder: {:?}", op_id, e);
                builder.status(500)
            }
            SaphirError::Other(e) => {
                warn!("{}Saphir encountered an Unknown error that was returned as a responder: {:?}", op_id, e);
                builder.status(500)
            }
            #[cfg(feature = "json")]
            SaphirError::SerdeJson(e) => {
                debug!("{}Unable to de/serialize json type: {:?}", op_id, e);
                builder.status(400)
            }
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlDe(e) => {
                debug!("{}Unable to deserialize form type: {:?}", op_id, e);
                builder.status(400)
            }
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlSer(e) => {
                debug!("{}Unable to serialize form type: {:?}", op_id, e);
                builder.status(400)
            }
            SaphirError::MissingParameter(name, is_query) => {
                if is_query {
                    debug!("{}Missing query parameter {}", op_id, name);
                } else {
                    debug!("{}Missing path parameter {}", op_id, name);
                }

                builder.status(400)
            }
            SaphirError::InvalidParameter(name, is_query) => {
                if is_query {
                    debug!("{}Unable to parse query parameter {}", op_id, name);
                } else {
                    debug!("{}Unable to parse path parameter {}", op_id, name);
                }

                builder.status(400)
            }
            SaphirError::RequestMovedBeforeHandler => {
                warn!(
                    "{}A request was moved out of its context by a middleware, but the middleware did not stop request processing",
                    op_id
                );
                builder.status(500)
            }
            SaphirError::ResponseMoved => {
                warn!("{}A response was moved before being sent to the client", op_id);
                builder.status(500)
            }
            SaphirError::Responder(mut r) => r.dyn_respond(builder, ctx),
        }
    }
}
