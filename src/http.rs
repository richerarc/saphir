pub use hyper::Method;
pub use hyper::Uri;
pub use hyper::HttpVersion;
pub use hyper::Headers;
pub use hyper::Response;
pub use hyper::Body;
pub use hyper::StatusCode;
pub use hyper::server::Http;
pub use hyper::server::Service;
pub use hyper::Request;
use futures::Future;
use futures::Stream;

/// A Structure which represent an http request with a fully loaded body
#[derive(Debug)]
pub struct SyncRequest {
    /// Method
    method: Method,
    /// Uri
    uri: Uri,
    /// Version
    version: HttpVersion,
    /// Headers
    headers: Headers,
    /// Body
    body: Vec<u8>,
}

impl SyncRequest {
    /// Construct a new Request.
    #[inline]
    pub fn new(method: Method,
               uri: Uri,
               version: HttpVersion,
               headers: Headers,
               body: Vec<u8>,
    ) -> SyncRequest {
        SyncRequest {
            method,
            uri,
            version,
            headers,
            body,
        }
    }

    /// Read the Request Uri.
    #[inline]
    pub fn uri(&self) -> &Uri { &self.uri }

    /// Read the Request Version.
    #[inline]
    pub fn version(&self) -> HttpVersion { self.version }

    /// Read the Request headers.
    #[inline]
    pub fn headers(&self) -> &Headers { &self.headers }

    /// Read the Request method.
    #[inline]
    pub fn method(&self) -> &Method { &self.method }

    /// Read the Request body.
    #[inline]
    pub fn body_ref(&self) -> &Vec<u8> { self.body.as_ref() }

    /// Get a mutable reference to the Request body.
    #[inline]
    pub fn body_mut(&mut self) -> &mut Vec<u8> { &mut self.body }

    /// The target path of this Request.
    #[inline]
    pub fn path(&self) -> &str {
        self.uri.path()
    }

    /// The query string of this Request.
    #[inline]
    pub fn query(&self) -> Option<&str> {
        self.uri.query()
    }

    /// Get a mutable reference to the Request headers.
    #[inline]
    pub fn headers_mut(&mut self) -> &mut Headers { &mut self.headers }
}

/// A trait allowing the implicit conversion of a Hyper::Request into a SyncRequest
pub trait LoadBody {
    ///
    fn load_body(self) -> Box<Future<Item=SyncRequest, Error=::hyper::Error>>;
}

impl LoadBody for Request<Body> {
    fn load_body(self) -> Box<Future<Item=SyncRequest, Error=::hyper::Error>> {
        let (method, uri, version, headers, body) = self.deconstruct();
        Box::new(body.concat2().map(move |b| {
            let body_vec: Vec<u8> = b.to_vec();
            SyncRequest::new(method, uri, version, headers, body_vec)
        }))
    }
}