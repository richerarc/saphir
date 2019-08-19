use std::any::Any;
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};

use cookie::Cookie;
use cookie::CookieJar;
use futures::Future;
use futures::Stream;
use hashbrown::HashMap;
pub use hyper::Body;
pub use hyper::body::Payload;
pub use hyper::Method;
pub use hyper::Request;
pub use hyper::Response;
pub use hyper::StatusCode;
pub use hyper::Uri;
pub use hyper::Version;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub use crate::http_types::Extensions;
use crate::http_types::HttpTryFrom;
use crate::http_types::request::Parts as ReqParts;
use crate::http_types::response::Builder as ResponseBuilder;
use crate::utils::UriPathMatcher;

/// Headers types re-export
pub mod header {
    pub use hyperx::header::*;
    pub use hyperx::mime;

    pub use crate::http_types::header::*;
}

/// A Wrapper around the CookieJar Struct
pub struct Cookies<'a> {
    inner: RwLockReadGuard<'a, Option<CookieJar>>
}

/// A Mutable Wrapper around the CookieJar Struct
pub struct CookiesMut<'a> {
    inner: RwLockWriteGuard<'a, Option<CookieJar>>
}

impl<'a> Deref for Cookies<'a> {
    type Target = CookieJar;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().expect("This struct can not exists if The CookieJar is not initialized")
    }
}

impl<'a> Deref for CookiesMut<'a> {
    type Target = CookieJar;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().expect("This struct can not exists if The CookieJar is not initialized")
    }
}

impl<'a> DerefMut for CookiesMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().expect("This struct can not exists if The CookieJar is not initialized")
    }
}

static EMPTY_BODY: &[u8] = b"";

/// A Structure which represent an http request with a fully loaded body
#[derive(Debug)]
pub struct SyncRequest {
    /// Method
    head: ReqParts,
    /// Body
    body: Vec<u8>,
    /// Request Params
    current_path: VecDeque<String>,
    captures: HashMap<String, String>,
    cookies: RwLock<Option<CookieJar>>,
}

impl SyncRequest {
    /// Construct a new Request.
    #[inline]
    pub fn new(head: ReqParts,
               body: Vec<u8>,
    ) -> SyncRequest {
        let mut cp = head.uri.path().to_owned().split('/').map(|s| s.to_owned()).collect::<VecDeque<String>>();
        cp.pop_front();
        if cp.back().map(|s| s.len()).unwrap_or(0) < 1 {
            cp.pop_back();
        }
        SyncRequest {
            head,
            body,
            current_path: cp,
            captures: HashMap::new(),
            cookies: RwLock::new(None),
        }
    }

    /// Returns a reference to the associated HTTP method.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
    /// let mut request: Request<()> = Request::default();
    /// *request.uri_mut() = "/hello".parse().unwrap();
    /// assert_eq!(*request.uri(), *"/hello");
    /// ```
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.head.uri
    }

    ///
    pub(crate) fn current_path_match(&mut self, path: &UriPathMatcher) -> bool {
        let mut current_path = self.current_path.iter();
        // validate path
        for seg in path.iter() {
            if let Some(current) = current_path.next() {
                if !seg.matches(current) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Alter current path and capture path variable
        {
            for seg in path.iter() {
                if let Some(current) = self.current_path.pop_front() {
                    if let Some(name) = seg.name() {
                        self.captures.insert(name.to_string(), current);
                    }
                }
            }
        }

        true
    }

    ///
    pub(crate) fn current_path_match_all(&mut self, path: &UriPathMatcher) -> bool {
        if path.len() != self.current_path.len() {
            return false;
        }

        let mut current_path = self.current_path.iter();
        // validate path
        for seg in path.iter() {
            if let Some(current) = current_path.next() {
                if !seg.matches(current) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Alter current path and capture path variable
        {
            for seg in path.iter() {
                if let Some(current) = self.current_path.pop_front() {
                    if let Some(name) = seg.name() {
                        self.captures.insert(name.to_string(), current);
                    }
                }
            }
        }

        true
    }

    ///
    pub fn captures(&self) -> &HashMap<String, String> {
        &self.captures
    }

    /// Returns the associated version.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
    /// let request: Request<()> = Request::default();
    /// assert!(request.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers_map(&self) -> &header::HeaderMap<header::HeaderValue> {
        &self.head.headers
    }

    /// Returns a mutable reference to the associated header field map.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use saphir::*;
    /// # use saphir::headers::*;
    /// let mut request: Request<()> = Request::default();
    /// request.headers_mut().insert(HOST, HeaderValue::from_static("world"));
    /// assert!(!request.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers_map_mut(&mut self) -> &mut header::HeaderMap<header::HeaderValue> {
        &mut self.head.headers
    }

    /// Clone the HeaderMap and convert it to a more dev-friendly Headers struct
    ///
    pub fn parsed_header(&self) -> header::Headers {
        self.head.headers.clone().into()
    }

    /// Insert a dev-friendly Headers to the HeaderMap
    ///
    pub fn insert_parsed_headers(&mut self, headers: header::Headers) {
        let map: header::HeaderMap = headers.into();

        for header in map {
            if let (Some(name), value) = header {
                self.head.headers.insert(name, value);
            }
        }
    }

    /// Returns a reference to the associated extensions.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
    /// let mut request: Request<String> = Request::default();
    /// request.body_mut().push_str("hello world");
    /// assert!(!request.body().is_empty());
    /// ```
    #[inline]
    pub fn body_mut(&mut self) -> &mut Vec<u8> {
        &mut self.body
    }

    /// Get the cookies sent by the browsers
    pub fn cookies(&self) -> Cookies<'_> {
        {
            let read = self.cookies.read();

            if read.is_some() {
                return Cookies {
                    inner: read
                };
            }
        }

        self.init_cookies_jar(&mut self.cookies.write());

        self.cookies()
    }

    /// Get the cookies sent by the browsers in a mutable way
    pub fn cookies_mut(&mut self) -> CookiesMut<'_> {
        let mut write = self.cookies.write();

        if write.is_some() {
            return CookiesMut {
                inner: write
            };
        }

        self.init_cookies_jar(&mut write);

        CookiesMut {
            inner: write
        }
    }

    fn init_cookies_jar(&self, jar_guard: &mut RwLockWriteGuard<Option<CookieJar>>) {
        let mut jar = CookieJar::new();

        self.head.headers.get("Cookie")
            .and_then(|cookies| cookies.to_str().ok())
            .map(|cookies_str| cookies_str.split("; "))
            .map(|cookie_iter| cookie_iter.filter_map(|cookie_s| Cookie::parse(cookie_s.to_string()).ok()))
            .map(|cookie_iter| cookie_iter.for_each(|c| jar.add_original(c)));

        **jar_guard = Some(jar);
    }
}

/// A trait allowing the implicit conversion of a Hyper::Request into a SyncRequest
pub trait LoadBody {
    ///
    fn load_body(self) -> Box<dyn Future<Item=SyncRequest, Error=::hyper::Error> + Send>;
}

impl LoadBody for Request<Body> {
    fn load_body(self) -> Box<dyn Future<Item=SyncRequest, Error=::hyper::Error> + Send> {
        let (parts, body) = self.into_parts();
        Box::new(body.concat2().map(move |b| {
            let body_vec: Vec<u8> = b.to_vec();
            SyncRequest::new(parts, body_vec)
        }))
    }
}

/// A Structure which represent a fully mutable http response
pub struct SyncResponse {
    builder: ResponseBuilder,
    cookies: Option<CookieJar>,
    body: Box<dyn ToBody>,
}

impl SyncResponse {
    ///
    pub fn new() -> Self {
        SyncResponse {
            builder: ResponseBuilder::new(),
            cookies: None,
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    /// # use saphir::*;
    /// # use saphir::headers::*;
    ///
    /// let response = SyncResponse::new()
    ///     .header("Content-Type", "text/html")
    ///     .header("X-Custom-Foo", "bar")
    ///     .build_response()
    ///     .unwrap();
    /// ```
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut SyncResponse
        where header::HeaderName: HttpTryFrom<K>,
              header::HeaderValue: HttpTryFrom<V>
    {
        self.builder.header(key, value);
        self
    }

    /// A convinient function to constuct the response headers from a Headers struct
    pub fn parsed_header(&mut self, headers: header::Headers) -> &mut SyncResponse {
        let map: header::HeaderMap = headers.into();

        for header in map {
            if let (Some(name), value) = header {
                self.builder.header(name, value);
            }
        }

        self
    }

    /// Adds an extension to this builder
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use saphir::*;
    /// # use saphir::headers::*;
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

    /// Set a cookie with the Set-Cookie header
    pub fn cookie(&mut self, cookie: Cookie<'static>) -> &mut SyncResponse {
        if self.cookies.is_none() {
            self.cookies = Some(CookieJar::new());
        }

        self.cookies.as_mut().expect("Should not happens").add(cookie);

        self
    }

    /// Adds a body to a response
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use saphir::*;
    /// # use saphir::headers::*;
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
    pub fn build_response(self) -> Result<Response<Body>, crate::http_types::Error> {
        let SyncResponse { mut builder, cookies, body } = self;
        if let Some(cookies) = cookies {
            cookies.iter().for_each(|c| {
                builder.header(header::SET_COOKIE, c.to_string());
            })
        }
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