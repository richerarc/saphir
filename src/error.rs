#[derive(Debug)]
pub enum ServerError {
    HyperError(::hyper::Error),
    ParseError(::std::net::AddrParseError)
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