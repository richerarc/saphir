use crate::{
    http_context::HttpContext,
    responder::{DynResponder, Responder},
    response::Builder,
};
use http::{
    header::{InvalidHeaderValue, ToStrError},
    Error as HttpCrateError,
};
use hyper::Error as HyperError;
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
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    SerdeJson(serde_json::error::Error),
    /// Error from deserializing form data
    #[cfg(feature = "form")]
    #[cfg_attr(docsrs, doc(cfg(feature = "form")))]
    SerdeUrlDe(serde_urlencoded::de::Error),
    /// Error from serializing form data
    #[cfg(feature = "form")]
    #[cfg_attr(docsrs, doc(cfg(feature = "form")))]
    SerdeUrlSer(serde_urlencoded::ser::Error),
    ///
    MissingParameter(String, bool),
    ///
    InvalidParameter(String, bool),
    ///
    RequestTimeout,
    /// Attempted to build stack twice
    StackAlreadyInitialized,
    ///
    TooManyRequests,
    /// Validator error
    #[cfg(feature = "validate-requests")]
    #[cfg_attr(docsrs, doc(cfg(feature = "validate-requests")))]
    ValidationErrors(validator::ValidationErrors),
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
            SaphirError::RequestTimeout => f.write_str("RequestTimeout"),
            SaphirError::StackAlreadyInitialized => f.write_str("StackAlreadyInitialized"),
            SaphirError::TooManyRequests => f.write_str("TooManyRequests"),
            #[cfg(feature = "validate-requests")]
            SaphirError::ValidationErrors(d) => std::fmt::Debug::fmt(d, f),
        }
    }
}

impl SaphirError {
    pub fn responder<T: Responder + Send + Sync + 'static>(e: T) -> Self {
        SaphirError::Responder(Box::new(Some(e)))
    }

    pub(crate) fn response_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
        match self {
            SaphirError::Internal(_) => builder.status(500),
            SaphirError::Io(_) => builder.status(500),
            SaphirError::BodyAlreadyTaken => builder.status(500),
            SaphirError::Custom(_) => builder.status(500),
            SaphirError::Other(_) => builder.status(500),
            #[cfg(feature = "json")]
            SaphirError::SerdeJson(_) => builder.status(400),
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlDe(_) => builder.status(400),
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlSer(_) => builder.status(400),
            SaphirError::MissingParameter(..) => builder.status(400),
            SaphirError::InvalidParameter(..) => builder.status(400),
            SaphirError::RequestMovedBeforeHandler => builder.status(500),
            SaphirError::ResponseMoved => builder.status(500),
            SaphirError::Responder(mut r) => r.dyn_respond(builder, ctx),
            SaphirError::RequestTimeout => builder.status(408),
            SaphirError::StackAlreadyInitialized => builder.status(500),
            SaphirError::TooManyRequests => builder.status(429),
            #[cfg(feature = "validate-requests")]
            SaphirError::ValidationErrors(_) => builder.status(400),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn log(&self, ctx: &HttpContext) {
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
            }
            SaphirError::Io(e) => {
                warn!("{}Saphir encountered an Io error that was returned as a responder: {:?}", op_id, e);
            }
            SaphirError::BodyAlreadyTaken => {
                warn!("{}A controller handler attempted to take the request body more thant one time", op_id);
            }
            SaphirError::Custom(e) => {
                warn!("{}A custom error was returned as a responder: {:?}", op_id, e);
            }
            SaphirError::Other(e) => {
                warn!("{}Saphir encountered an Unknown error that was returned as a responder: {:?}", op_id, e);
            }
            #[cfg(feature = "json")]
            SaphirError::SerdeJson(e) => {
                debug!("{}Unable to de/serialize json type: {:?}", op_id, e);
            }
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlDe(e) => {
                debug!("{}Unable to deserialize form type: {:?}", op_id, e);
            }
            #[cfg(feature = "form")]
            SaphirError::SerdeUrlSer(e) => {
                debug!("{}Unable to serialize form type: {:?}", op_id, e);
            }
            SaphirError::MissingParameter(name, is_query) => {
                if *is_query {
                    debug!("{}Missing query parameter {}", op_id, name);
                } else {
                    debug!("{}Missing path parameter {}", op_id, name);
                }
            }
            SaphirError::InvalidParameter(name, is_query) => {
                if *is_query {
                    debug!("{}Unable to parse query parameter {}", op_id, name);
                } else {
                    debug!("{}Unable to parse path parameter {}", op_id, name);
                }
            }
            SaphirError::RequestMovedBeforeHandler => {
                warn!(
                    "{}A request was moved out of its context by a middleware, but the middleware did not stop request processing",
                    op_id
                );
            }
            SaphirError::ResponseMoved => {
                warn!("{}A response was moved before being sent to the client", op_id);
            }
            SaphirError::RequestTimeout => {
                warn!("{}Request timed out", op_id);
            }
            SaphirError::Responder(_) => {}
            SaphirError::StackAlreadyInitialized => {
                warn!("{}Attempted to initialize stack twice", op_id);
            }
            SaphirError::TooManyRequests => {
                warn!("{}Made too many requests", op_id);
            }
            #[cfg(feature = "validate-requests")]
            SaphirError::ValidationErrors(e) => {
                debug!("{}Validation error: {:?}", op_id, e);
            }
        }
    }
}

#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
impl From<serde_json::error::Error> for SaphirError {
    fn from(e: serde_json::error::Error) -> Self {
        SaphirError::SerdeJson(e)
    }
}

#[cfg(feature = "form")]
#[cfg_attr(docsrs, doc(cfg(feature = "form")))]
impl From<serde_urlencoded::de::Error> for SaphirError {
    fn from(e: serde_urlencoded::de::Error) -> Self {
        SaphirError::SerdeUrlDe(e)
    }
}

#[cfg(feature = "form")]
#[cfg_attr(docsrs, doc(cfg(feature = "form")))]
impl From<serde_urlencoded::ser::Error> for SaphirError {
    fn from(e: serde_urlencoded::ser::Error) -> Self {
        SaphirError::SerdeUrlSer(e)
    }
}

#[cfg(feature = "validate-requests")]
#[cfg_attr(docsrs, doc(cfg(feature = "validate-requests")))]
impl From<::validator::ValidationErrors> for SaphirError {
    fn from(e: ::validator::ValidationErrors) -> Self {
        SaphirError::ValidationErrors(e)
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
        self.log(ctx);
        self.response_builder(builder, ctx)
    }
}
