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
    fmt::{Debug, Formatter},
    io::Error as IoError,
};
use thiserror::Error;

/// Type representing an internal error inerrant to the underlining logic behind
/// saphir
#[derive(Error, Debug)]
pub enum InternalError {
    #[error("Http: {0}")]
    Http(HttpCrateError),
    #[error("Hyper: {0}")]
    Hyper(HyperError),
    #[error("ToStr: {0}")]
    ToStr(ToStrError),
    #[error("Stack")]
    Stack,
}

/// Error type throughout the saphir stack
#[derive(Error)]
pub enum SaphirError {
    ///
    #[error("Internal: {0}")]
    Internal(#[from] InternalError),
    ///
    #[error("Io: {0}")]
    Io(#[from] IoError),
    /// Body was taken and cannot be polled
    #[error("Body already taken")]
    BodyAlreadyTaken,
    /// The request was moved by a middleware without ending the request
    /// processing
    #[error("Request moved before handler")]
    RequestMovedBeforeHandler,
    /// The response was moved before being sent to the client
    #[error("Response moved")]
    ResponseMoved,
    /// Custom error type to map any other error
    #[error("Custom: {0}")]
    Custom(Box<dyn StdError + Send + Sync + 'static>),
    /// Custom error type to map any other error
    #[error("Responder")]
    Responder(Box<dyn DynResponder + Send + Sync + 'static>),
    ///
    #[error("Other: {0}")]
    Other(String),
    /// Error from (de)serializing json data
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    #[error("SerdeJson: {0}")]
    SerdeJson(#[from] serde_json::error::Error),
    /// Error from deserializing form data
    #[cfg(feature = "form")]
    #[cfg_attr(docsrs, doc(cfg(feature = "form")))]
    #[error("SerdeUrlDe: {0}")]
    SerdeUrlDe(#[from] serde_urlencoded::de::Error),
    /// Error from serializing form data
    #[cfg(feature = "form")]
    #[cfg_attr(docsrs, doc(cfg(feature = "form")))]
    #[error("SerdeUrlSer: {0}")]
    SerdeUrlSer(#[from] serde_urlencoded::ser::Error),
    ///
    #[error("Missing parameter `{0}` (is_query: {1})")]
    MissingParameter(String, bool),
    ///
    #[error("Invalid parameter `{0}` (is_query: {1})")]
    InvalidParameter(String, bool),
    ///
    #[error("Request timed out")]
    RequestTimeout,
    /// Attempted to build stack twice
    #[error("Stack alrealy initialized")]
    StackAlreadyInitialized,
    ///
    #[error("Too many requests")]
    TooManyRequests,
    /// Validator error
    #[cfg(feature = "validate-requests")]
    #[cfg_attr(docsrs, doc(cfg(feature = "validate-requests")))]
    #[error("ValidationErrors: {0}")]
    ValidationErrors(#[from] validator::ValidationErrors),
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

impl From<ToStrError> for SaphirError {
    fn from(e: ToStrError) -> Self {
        SaphirError::Internal(InternalError::ToStr(e))
    }
}

impl Responder for SaphirError {
    #[allow(unused_variables)]
    fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
        self.log(ctx);
        self.response_builder(builder, ctx)
    }
}
