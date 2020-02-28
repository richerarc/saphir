use std::{
    collections::HashMap,
    net::SocketAddr,
    ops::{Deref, DerefMut},
};

use cookie::{Cookie, CookieJar};
use futures_util::future::Future;
use http::Request as RawRequest;
use hyper::body::Bytes;

use crate::{
    body::{Body, FromBytes},
    error::SaphirError,
};

/// Struct that wraps a hyper request + some magic
pub struct Request<T = Body<Bytes>> {
    #[doc(hidden)]
    inner: RawRequest<T>,
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
        Request {
            inner: raw,
            captures: Default::default(),
            cookies: Default::default(),
            peer_addr,
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
        let Request {
            inner,
            captures,
            cookies,
            peer_addr,
        } = self;
        Request {
            inner: inner.map(f),
            captures,
            cookies,
            peer_addr,
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
        Fut: Future<Output = U>,
    {
        let Request {
            inner,
            captures,
            cookies,
            peer_addr,
        } = self;
        let (head, body) = inner.into_parts();
        let mapped = f(body).await;
        let mapped_r = RawRequest::from_parts(head, mapped);

        Request {
            inner: mapped_r,
            captures,
            cookies,
            peer_addr,
        }
    }

    /// Parse cookies from the Cookie header
    pub fn parse_cookies(&mut self) {
        let jar = &mut self.cookies;
        if let Some(cookie_iter) = self
            .inner
            .headers()
            .get("Cookie")
            .and_then(|cookies| cookies.to_str().ok())
            .map(|cookies_str| cookies_str.split("; "))
            .map(|cookie_iter| cookie_iter.filter_map(|cookie_s| Cookie::parse(cookie_s.to_string()).ok()))
        {
            cookie_iter.for_each(|c| jar.add_original(c));
        }
    }
}

impl<T: FromBytes + Unpin + 'static> Request<Body<T>> {
    /// Convert a request of T in a request of U through a future
    ///
    /// ```rust
    ///# use saphir::prelude::*;
    ///# use hyper::Request as RawRequest;
    ///# async {
    ///# let mut req = Request::new(RawRequest::builder().method("GET").uri("https://www.rust-lang.org/").body(Body::empty()).unwrap(), None);
    /// // req is Request<Body<Bytes>>
    /// let req = req.load_body().await.unwrap();
    /// // req is now Request<Bytes>
    ///# };
    /// ```
    #[inline]
    pub async fn load_body(self) -> Result<Request<T::Out>, SaphirError> {
        let Request {
            inner,
            captures,
            cookies,
            peer_addr,
        } = self;
        let (head, body) = inner.into_parts();

        let t = body.await?;

        let mapped_r = RawRequest::from_parts(head, t);

        Ok(Request {
            inner: mapped_r,
            captures,
            cookies,
            peer_addr,
        })
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
        let Request {
            inner,
            captures,
            cookies,
            peer_addr,
        } = self;
        let (head, body) = inner.into_parts();

        body.map(move |b| Request {
            inner: RawRequest::from_parts(head, b),
            captures,
            cookies,
            peer_addr,
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
        let Request {
            inner,
            captures,
            cookies,
            peer_addr,
        } = self;
        let (head, body) = inner.into_parts();

        body.map(move |b| Request {
            inner: RawRequest::from_parts(head, b),
            captures,
            cookies,
            peer_addr,
        })
    }
}

#[cfg(feature = "json")]
mod json {
    use serde::Deserialize;

    use crate::body::Json;

    use super::*;

    impl Request<Body<Bytes>> {
        pub async fn json<T>(&mut self) -> Result<T, SaphirError>
        where
            T: for<'a> Deserialize<'a> + Unpin + 'static,
        {
            self.body_mut().take_as::<Json<T>>().await
        }
    }
}

#[cfg(feature = "form")]
mod form {
    use serde::Deserialize;

    use crate::body::Form;

    use super::*;

    impl Request<Body<Bytes>> {
        pub async fn form<T>(&mut self) -> Result<T, SaphirError>
        where
            T: for<'a> Deserialize<'a> + Unpin + 'static,
        {
            self.body_mut().take_as::<Form<T>>().await
        }
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
