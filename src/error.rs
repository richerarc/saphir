use std::fmt::{Display, Formatter, Error as FmtError};
use std::error::Error as StdError;
use http::Error as HttpCrateError;
use http::header::InvalidHeaderValue;
use std::io::Error as IoError;

/// Type representing an internal error inerrant to the underlining logic behind saphir
#[derive(Debug)]
pub enum InternalError {
    Http(HttpCrateError),
    Stack,
}

/// Error type throughout the saphir stack
#[derive(Debug)]
pub enum SaphirError {
    ///
    Internal(InternalError),
    ///
    Io(IoError),
    /// Custom error type to map any other error
    Custom(Box<dyn StdError + Send + Sync + 'static>),
    ///
    Other(String),
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