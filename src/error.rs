use std::fmt;

/// Potential server errors
#[derive(Debug)]
pub enum ServerError {
    /// An Hyper Error
    HyperError(::hyper::Error),
    /// A cancellation Error
    FutureCancelledError(::futures::Canceled),
    /// A parsing error of addr
    ParseError(::std::net::AddrParseError),
    /// An invalid URI
    InvalidUri(::http_types::uri::InvalidUri),
}

impl From<::std::net::AddrParseError> for ServerError {
    fn from(e: ::std::net::AddrParseError) -> Self {
        ServerError::ParseError(e)
    }
}

impl From<::hyper::Error> for ServerError {
    fn from(e: ::hyper::Error) -> Self {
        ServerError::HyperError(e)
    }
}

impl From<::futures::Canceled> for ServerError {
    fn from(e: ::futures::Canceled) -> Self {
        ServerError::FutureCancelledError(e)
    }
}

impl From<::http_types::uri::InvalidUri> for ServerError {
    fn from(e: ::http_types::uri::InvalidUri) -> Self {
        ServerError::InvalidUri(e)
    }
}

impl ::std::error::Error for ServerError {
    fn description(&self) -> &str {
        use ServerError::*;
        match self {
            HyperError(ref e) => e.description(),
            FutureCancelledError(ref e) => e.description(),
            ParseError(ref e) => e.description(),
            InvalidUri(ref e) => e.description(),
        }
    }
}

impl ::std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result<> {
        use ServerError::*;
        match self {
            HyperError(ref e) => e.fmt(f),
            FutureCancelledError(ref e) => e.fmt(f),
            ParseError(ref e) => e.fmt(f),
            InvalidUri(ref e) => e.fmt(f),
        }
    }
}