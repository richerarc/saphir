use std::collections::{HashMap, VecDeque};
use std::ops::{Deref, DerefMut};

use cookie::Cookie;
use cookie::CookieJar;
use http::Request as RawRequest;

use crate::utils::UriPathMatcher;
use std::net::SocketAddr;
use futures_util::future::Future;
use hyper::body::Bytes;
use crate::body::Body;

/// Struct that wraps a hyper request + some magic
pub struct Request<T = Body<Bytes>> {
    #[doc(hidden)]
    inner: RawRequest<T>,
    #[doc(hidden)]
    current_path: VecDeque<String>,
    #[doc(hidden)]
    captures: HashMap<String, String>,
    #[doc(hidden)]
    cookies: CookieJar,
    #[doc(hidden)]
    peer_addr: Option<SocketAddr>,
}

impl<T> Request<T> {
    #[doc(hidden)]
    pub fn new(raw: RawRequest<T>, peer_addr: Option<SocketAddr>) -> Self {
        let mut cp = raw.uri().path().to_owned().split('/').map(|s| s.to_owned()).collect::<VecDeque<String>>();
        cp.pop_front();
        if cp.back().map(|s| s.len()).unwrap_or(0) < 1 {
            cp.pop_back();
        }
        Request {
            inner: raw,
            current_path: cp,
            captures: Default::default(),
            cookies: Default::default(),
            peer_addr
        }
    }

    /// Return the Peer SocketAddr if one was available when receiving the request
    #[inline]
    pub fn peer_addr(&self) -> Option<&SocketAddr> {
        self.peer_addr.as_ref()
    }

    ///
    #[inline]
    pub fn peer_addr_mut(&mut self) -> Option<&mut SocketAddr> {
        self.peer_addr.as_mut()
    }

    /// Get the cookies sent by the browsers.
    ///
    /// Before accessing cookies, you will need to parse them, it is done with the
    /// [`parse_cookies`](#method.parse_cookies) method
    ///
    /// ```rust
    ///# use saphir::prelude::*;
    ///# use hyper::Request as RawRequest;
    ///# let mut req = Request::new(RawRequest::builder().method("GET").uri("https://www.rust-lang.org/").body(()).unwrap(), None);
    /// // Parse cookies
    /// req.parse_cookies();
    /// // then use cookies
    /// let cookie = req.cookies().get("MyCookie");
    /// ```
    #[inline]
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }

    /// Get the cookies sent by the browsers in a mutable way
    ///
    /// Before accessing cookies, you will need to parse them, it is done with the
    /// [`parse_cookies`](#method.parse_cookies) method
    ///
    /// ```rust
    ///# use saphir::prelude::*;
    ///# use hyper::Request as RawRequest;
    ///# let mut req = Request::new(RawRequest::builder().method("GET").uri("https://www.rust-lang.org/").body(()).unwrap(), None);
    /// // Parse cookies
    /// req.parse_cookies();
    /// // then use cookies
    /// let mut_cookie = req.cookies_mut().get("MyCookie");
    /// ```
    #[inline]
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }

    /// Access the captured variables from the request path. E.g. a path composed as
    /// `/user/{user_id}/profile` will store a capture named `"user_id"`.
    ///
    /// ```rust
    ///# use saphir::prelude::*;
    ///# use hyper::Request as RawRequest;
    ///# let mut req = Request::new(RawRequest::builder().method("GET").uri("https://www.rust-lang.org/").body(()).unwrap(), None);
    /// let user_id = req.captures().get("user_id");
    /// // retrieve user by id
    /// ```
    #[inline]
    pub fn captures(&self) -> &HashMap<String, String> {
        &self.captures
    }

    /// Access the captured variables from the request path, in a mutable way.
    #[inline]
    pub fn captures_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.captures
    }

    /// Convert a request of T in a request of U
    ///
    /// ```rust
    ///# use saphir::prelude::*;
    ///# use hyper::Request as RawRequest;
    ///# let mut req = Request::new(RawRequest::builder().method("GET").uri("https://www.rust-lang.org/").body(()).unwrap(), None);
    /// // req is Request<Body>
    /// let req: Request<String> = req.map(|_ignored_body| "New body".to_string());
    /// ```
    #[inline]
    pub fn map<F, U>(self, f: F) -> Request<U>
        where
            F: FnOnce(T) -> U,
    {
        let Request { inner, current_path, captures, cookies, peer_addr } = self;
        Request {
            inner: inner.map(f),
            current_path,
            captures,
            cookies,
            peer_addr
        }
    }

    /// Convert a request of T in a request of U through a future
    ///
    /// ```rust
    ///# use saphir::prelude::*;
    ///# use hyper::Request as RawRequest;
    ///# let mut req = Request::new(RawRequest::builder().method("GET").uri("https://www.rust-lang.org/").body(Body::empty()).unwrap(), None);
    /// // req is Request<Body>
    /// let req = req.async_map(|b| async {hyper::body::to_bytes(b).await});
    /// ```
    #[inline]
    pub async fn async_map<F, Fut, U>(self, f: F) -> Request<U>
        where
            F: FnOnce(T) -> Fut,
            Fut: Future<Output=U>
    {
        let Request { inner, current_path, captures, cookies, peer_addr } = self;
        let (head, body) = inner.into_parts();
        let mapped = f(body).await;
        let mapped_r = RawRequest::from_parts(head, mapped);

        Request {
            inner: mapped_r,
            current_path,
            captures,
            cookies,
            peer_addr
        }
    }

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

    /// Parse cookies from the Cookie header
    pub fn parse_cookies(&mut self) {
        let jar = &mut self.cookies;
        self.inner.headers().get("Cookie")
            .and_then(|cookies| cookies.to_str().ok())
            .map(|cookies_str| cookies_str.split("; "))
            .map(|cookie_iter| cookie_iter.filter_map(|cookie_s| Cookie::parse(cookie_s.to_string()).ok()))
            .map(|cookie_iter| cookie_iter.for_each(|c| jar.add_original(c)));
    }
}

impl<T, E> Request<Result<T, E>> {
    /// Convert a request of Result<T, E> in a Result<Request<T>, E>
    ///
    /// ```rust
    ///# use saphir::prelude::*;
    ///# use hyper::Request as RawRequest;
    ///# let r: Result<String, String> = Ok("Body".to_string());
    ///# let mut req = Request::new(RawRequest::builder().method("GET").uri("https://www.rust-lang.org/").body(r).unwrap(), None);
    /// // req is Request<Result<String, String>>
    /// let res = req.transpose();
    /// assert!(res.is_ok());
    /// ```
    pub fn transpose(self) -> Result<Request<T>, E> {
        let Request { inner, current_path, captures, cookies, peer_addr } = self;
        let (head, body) = inner.into_parts();

        body.map(move |b| {
            Request {
                inner: RawRequest::from_parts(head, b),
                current_path,
                captures,
                cookies,
                peer_addr
            }
        })
    }
}

impl<T> Request<Option<T>> {
    /// Convert a request of Option<T> in a Option<Request<T>, E>
    ///
    /// ```rust
    ///# use saphir::prelude::*;
    ///# use hyper::Request as RawRequest;
    ///# let mut req = Request::new(RawRequest::builder().method("GET").uri("https://www.rust-lang.org/").body(Some("Body".to_string())).unwrap(), None);
    /// // req is Request<Option<String>>
    /// let opt = req.transpose();
    /// assert!(opt.is_some());
    /// ```
    pub fn transpose(self) -> Option<Request<T>> {
        let Request { inner, current_path, captures, cookies, peer_addr } = self;
        let (head, body) = inner.into_parts();

        body.map(move |b| {
            Request {
                inner: RawRequest::from_parts(head, b),
                current_path,
                captures,
                cookies,
                peer_addr
            }
        })
    }
}

impl<T> Deref for Request<T> {
    type Target = RawRequest<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Request<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

