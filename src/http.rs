pub use http_types::header::HeaderName;
pub use http_types::header::HeaderValue;
pub use http_types::request::Parts as ReqParts;
pub use http_types::response::Parts as ResParts;
pub use http_types::response::Builder as ResponseBuilder;
pub use http_types::Extensions;
pub use hyper::Method;
pub use hyper::Uri;
pub use hyper::Version;
pub use hyper::HeaderMap;
pub use hyper::Body;
pub use hyper::body::Payload;
pub use hyper::StatusCode;
pub use hyper::service::Service;
pub use hyper::service::service_fn;
pub use hyper::Request;
pub use hyper::Response;
pub use hyper::Server as HyperServer;
use futures::Future;
use futures::Stream;
use http_types::HttpTryFrom;
use std::any::Any;

static EMPTY_BODY: &[u8] = b"";

/// A Structure which represent an http request with a fully loaded body
#[derive(Debug)]
pub struct SyncRequest {
    /// Method
    head: ReqParts,
    /// Body
    body: Vec<u8>,
}

impl SyncRequest {
    /// Construct a new Request.
    #[inline]
    pub fn new(head: ReqParts,
               body: Vec<u8>,
    ) -> SyncRequest {
        SyncRequest {
            head,
            body,
        }
    }

    /// Returns a reference to the associated HTTP method.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert_eq!(*request.method(), Method::GET);
    /// ```
    #[inline]
    pub fn method(&self) -> &Method {
        &self.head.method
    }

    /// Returns a mutable reference to the associated HTTP method.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let mut request: Request<()> = Request::default();
    /// *request.method_mut() = Method::PUT;
    /// assert_eq!(*request.method(), Method::PUT);
    /// ```
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.head.method
    }

    /// Returns a reference to the associated URI.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert_eq!(*request.uri(), *"/");
    /// ```
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.head.uri
    }

    /// Returns a mutable reference to the associated URI.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let mut request: Request<()> = Request::default();
    /// *request.uri_mut() = "/hello".parse().unwrap();
    /// assert_eq!(*request.uri(), *"/hello");
    /// ```
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.head.uri
    }

    /// Returns the associated version.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert_eq!(request.version(), Version::HTTP_11);
    /// ```
    #[inline]
    pub fn version(&self) -> Version {
        self.head.version
    }

    /// Returns a mutable reference to the associated version.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let mut request: Request<()> = Request::default();
    /// *request.version_mut() = Version::HTTP_2;
    /// assert_eq!(request.version(), Version::HTTP_2);
    /// ```
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.head.version
    }

    /// Returns a reference to the associated header field map.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert!(request.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers(&self) -> &HeaderMap<HeaderValue> {
        &self.head.headers
    }

    /// Returns a mutable reference to the associated header field map.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// # use http::header::*;
    /// let mut request: Request<()> = Request::default();
    /// request.headers_mut().insert(HOST, HeaderValue::from_static("world"));
    /// assert!(!request.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.head.headers
    }


    /// Returns a reference to the associated extensions.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert!(request.extensions().get::<i32>().is_none());
    /// ```
    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.head.extensions
    }

    /// Returns a mutable reference to the associated extensions.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// # use http::header::*;
    /// let mut request: Request<()> = Request::default();
    /// request.extensions_mut().insert("hello");
    /// assert_eq!(request.extensions().get(), Some(&"hello"));
    /// ```
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.head.extensions
    }

    /// Returns a reference to the associated HTTP body.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let request: Request<String> = Request::default();
    /// assert!(request.body().is_empty());
    /// ```
    #[inline]
    pub fn body(&self) -> &Vec<u8> {
        &self.body
    }

    /// Returns a mutable reference to the associated HTTP body.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// let mut request: Request<String> = Request::default();
    /// request.body_mut().push_str("hello world");
    /// assert!(!request.body().is_empty());
    /// ```
    #[inline]
    pub fn body_mut(&mut self) -> &mut Vec<u8> {
        &mut self.body
    }
}

/// A trait allowing the implicit conversion of a Hyper::Request into a SyncRequest
pub trait LoadBody {
    ///
    fn load_body(self) -> Box<Future<Item=SyncRequest, Error=::hyper::Error> + Send>;
}

impl LoadBody for Request<Body> {
    fn load_body(self) -> Box<Future<Item=SyncRequest, Error=::hyper::Error> + Send> {
        let (parts, body) = self.into_parts();
        if body.content_length().unwrap_or_else(|| {0}) > 0 {
            Box::new(body.concat2().map(move |b| {
                let body_vec: Vec<u8> = b.to_vec();
                SyncRequest::new(parts, body_vec)
            }))
        } else {
            Box::new(::futures::future::ok( SyncRequest::new(parts, Vec::with_capacity(0))))
        }
    }
}

/// A Structure which represent a fully mutable http response
pub struct SyncResponse {
    builder: ResponseBuilder,
    body: Box<ToBody>,
}

impl SyncResponse {

    ///
    pub fn new() -> Self{
        SyncResponse {
            builder: ResponseBuilder::new(),
            body: Box::new(EMPTY_BODY),
        }
    }

    /// Set the HTTP status for this response.
    ///
    /// This function will configure the HTTP status code of the `Response` that
    /// will be returned from `Builder::build`.
    ///
    /// By default this is `200`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    ///
    /// let response = SyncResponse::new()
    ///     .status(200)
    ///     .build_response()
    ///     .unwrap();
    /// ```
    pub fn status<T>(&mut self, status: T) -> &mut SyncResponse
        where StatusCode: HttpTryFrom<T>,
    {
        self.builder.status(status);
        self
    }

    /// Set the HTTP version for this response.
    ///
    /// This function will configure the HTTP version of the `Response` that
    /// will be returned from `Builder::build`.
    ///
    /// By default this is HTTP/1.1
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    ///
    /// let response = SyncResponse::new()
    ///     .version(Version::HTTP_2)
    ///     .build_response()
    ///     .unwrap();
    /// ```
    pub fn version(&mut self, version: Version) -> &mut SyncResponse {
        self.builder.version(version);
        self
    }

    /// Appends a header to this response builder.
    ///
    /// This function will append the provided key/value as a header to the
    /// internal `HeaderMap` being constructed. Essentially this is equivalent
    /// to calling `HeaderMap::append`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    /// # use http::header::HeaderValue;
    ///
    /// let response = SyncResponse::new()
    ///     .header("Content-Type", "text/html")
    ///     .header("X-Custom-Foo", "bar")
    ///     .build_response()
    ///     .unwrap();
    /// ```
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut SyncResponse
        where HeaderName: HttpTryFrom<K>,
              HeaderValue: HttpTryFrom<V>
    {
        self.builder.header(key, value);
        self
    }

    /// Adds an extension to this builder
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    ///
    /// let response = SyncResponse::new()
    ///     .extension("My Extension")
    ///     .build_response()
    ///     .unwrap();
    ///
    /// assert_eq!(response.extensions().get::<&'static str>(),
    ///            Some(&"My Extension"));
    /// ```
    pub fn extension<T>(&mut self, extension: T) -> &mut SyncResponse
        where T: Any + Send + Sync + 'static,
    {
        self.builder.extension(extension);
        self
    }

    /// Adds a body to a response
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use http::*;
    ///
    /// let response = SyncResponse::new()
    ///     .body(b"this is a payload")
    ///     .build_response()
    ///     .unwrap();
    /// ```
    pub fn body<B: 'static + ToBody>(&mut self, body: B) -> &mut SyncResponse {
        self.body = Box::new(body);
        self
    }

    ///
    pub fn build_response(self) -> Result<Response<Body>, ::http_types::Error> {
        let SyncResponse { mut builder, body } = self;
        let b: Body = body.to_body();
        builder.body(b)
    }
}

///
pub trait ToBody {
    ///
    fn to_body(&self) -> Body;
}

impl<I> ToBody for I where I: Into<Body> + Clone {
    fn to_body(&self) -> Body {
        self.clone().into()
    }
}