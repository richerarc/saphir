use std::{
    any::Any,
    convert::TryFrom,
    ops::{Deref, DerefMut},
};

use cookie::{Cookie, CookieJar};
use http::{header::HeaderName, response::Builder as RawBuilder, HeaderMap, HeaderValue, Response as RawResponse, StatusCode, Version};
use hyper::body::Body as RawBody;

use crate::{
    body::{Body, TransmuteBody},
    error::SaphirError,
};

/// Struct that wraps a hyper response + some magic
pub struct Response<T = Body> {
    #[doc(hidden)]
    inner: RawResponse<T>,
    #[doc(hidden)]
    cookies: CookieJar,
}

impl<T> Response<T> {
    /// Creates an instance of a response builder
    pub fn builder() -> Builder {
        Builder::new()
    }

    /// Create a new response with T as body
    pub fn new(body: T) -> Self {
        Response {
            inner: RawResponse::new(body),
            cookies: Default::default(),
        }
    }

    /// Get the cookies sent by the browsers
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }

    /// Get the cookies sent by the browsers in a mutable way
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }

    /// Convert a response of T in a response of U
    ///
    /// ```rust
    /// # use saphir::prelude::*;
    /// # use hyper::Response as RawResponse;
    /// # let mut res = Response::new(());
    ///
    /// // res is Response<()>
    /// let res: Response<String> = res.map(|_ignored_body| "New body".to_string());
    /// ```
    #[inline]
    pub fn map<F, U>(self, f: F) -> Response<U>
    where
        F: FnOnce(T) -> U,
    {
        let Response { inner, cookies } = self;
        Response { inner: inner.map(f), cookies }
    }

    pub(crate) fn into_raw(self) -> Result<RawResponse<T>, SaphirError> {
        let Response { mut inner, cookies } = self;
        for c in cookies.iter() {
            inner
                .headers_mut()
                .append(http::header::SET_COOKIE, http::HeaderValue::from_str(c.to_string().as_str())?);
        }

        Ok(inner)
    }
}

impl<T> Deref for Response<T> {
    type Target = RawResponse<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Response<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Struct used to conveniently build a response
pub struct Builder {
    #[doc(hidden)]
    inner: RawBuilder,
    #[doc(hidden)]
    cookies: Option<CookieJar>,
    #[doc(hidden)]
    body: Box<dyn TransmuteBody + Send + Sync>,
    #[doc(hidden)]
    status_set: bool,
}

impl Builder {
    /// Creates a new default instance of `Builder` to construct either a
    /// `Head` or a `Response`.
    /// ```
    /// # use saphir::prelude::*;
    ///
    /// let response = Builder::new()
    ///     .status(200)
    ///     .build()
    ///     .unwrap();
    /// ```
    #[inline]
    pub fn new() -> Self {
        Builder {
            inner: RawBuilder::new(),
            cookies: None,
            body: Box::new(Option::<String>::None),
            status_set: false,
        }
    }

    #[inline]
    pub(crate) fn status_if_not_set<T>(self, status: T) -> Builder
    where
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<http::Error>,
    {
        if !self.status_set {
            self.status(status)
        } else {
            self
        }
    }

    /// Set the HTTP status for this response.
    ///
    /// This function will configure the HTTP status code of the `Response` that
    /// will be returned from `Builder::build`.
    ///
    /// By default this is `200`.
    /// ```
    /// # use saphir::prelude::*;
    ///
    /// let response = Builder::new()
    ///     .status(200)
    ///     .build()
    ///     .unwrap();
    /// ```
    #[inline]
    pub fn status<T>(mut self, status: T) -> Builder
    where
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<http::Error>,
    {
        self.status_set = true;
        self.inner = self.inner.status(status);
        self
    }

    /// Set the HTTP version for this response.
    ///
    /// This function will configure the HTTP version of the `Response` that
    /// will be returned from `Builder::build`.
    ///
    /// By default this is HTTP/1.1
    ///
    /// ```
    /// # use saphir::prelude::*;
    ///
    /// let response = Builder::new()
    ///     .version(Version::HTTP_2)
    ///     .build()
    ///     .unwrap();
    /// ```
    #[inline]
    pub fn version(mut self, version: Version) -> Builder {
        self.inner = self.inner.version(version);
        self
    }

    /// Appends a header to this response builder.
    ///
    /// This function will append the provided key/value as a header to the
    /// internal `HeaderMap` being constructed. Essentially this is equivalent
    /// to calling `HeaderMap::append`.
    ///
    /// ```
    /// # use saphir::prelude::*;
    /// # use http::header::HeaderValue;
    ///
    /// let response = Builder::new()
    ///     .header("Content-Type", "text/html")
    ///     .header("X-Custom-Foo", "bar")
    ///     .header("content-length", 0)
    ///     .build()
    ///     .unwrap();
    /// ```
    #[inline]
    pub fn header<K, V>(mut self, key: K, value: V) -> Builder
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.inner = self.inner.header(key, value);
        self
    }

    /// Get header on this response builder.
    ///
    /// When builder has error returns None.
    /// ```
    /// # use saphir::prelude::*;
    /// # use http::header::HeaderValue;
    /// let res = Builder::new()
    ///     .header("Accept", "text/html")
    ///     .header("X-Custom-Foo", "bar");
    /// let headers = res.headers_ref().unwrap();
    /// assert_eq!( headers["Accept"], "text/html" );
    /// assert_eq!( headers["X-Custom-Foo"], "bar" );
    /// ```
    #[inline]
    pub fn headers_ref(&self) -> Option<&HeaderMap<HeaderValue>> {
        self.inner.headers_ref()
    }

    /// Get header on this response builder.
    /// when builder has error returns None
    /// ```
    /// # use http::*;
    /// # use http::header::HeaderValue;
    /// # use saphir::prelude::*;;
    /// let mut res = Builder::new();
    /// {
    ///   let headers = res.headers_mut().unwrap();
    ///   headers.insert("Accept", HeaderValue::from_static("text/html"));
    ///   headers.insert("X-Custom-Foo", HeaderValue::from_static("bar"));
    /// }
    /// let headers = res.headers_ref().unwrap();
    /// assert_eq!( headers["Accept"], "text/html" );
    /// assert_eq!( headers["X-Custom-Foo"], "bar" );
    /// ```
    #[inline]
    pub fn headers_mut(&mut self) -> Option<&mut HeaderMap<HeaderValue>> {
        self.inner.headers_mut()
    }

    /// Adds an extension to this builder
    /// ```
    /// # use saphir::prelude::*;
    ///
    /// let response = Builder::new()
    ///     .extension("My Extension")
    ///     .build()
    ///     .unwrap();
    ///
    /// assert_eq!(response.extensions().get::<&'static str>(),
    ///            Some(&"My Extension"));
    /// ```
    #[inline]
    pub fn extension<T>(mut self, extension: T) -> Builder
    where
        T: Any + Send + Sync + 'static,
    {
        self.inner = self.inner.extension(extension);
        self
    }

    /// Adds an extension to this builder
    /// ```
    /// # use saphir::prelude::*;
    ///
    /// let cookie = Cookie::new("MyCookie", "MyCookieValue");
    ///
    /// let response = Builder::new()
    ///     .cookie(cookie)
    ///     .build()
    ///     .unwrap();
    ///
    /// assert_eq!(response.cookies().get("MyCookie").map(|c| c.value()), Some("MyCookieValue"))
    /// ```
    #[inline]
    pub fn cookie(mut self, cookie: Cookie<'static>) -> Builder {
        if self.cookies.is_none() {
            self.cookies = Some(CookieJar::new());
        }

        self.cookies.as_mut().expect("Should not happens").add(cookie);

        self
    }

    #[inline]
    pub fn body<B: 'static + Into<RawBody> + Send + Sync>(mut self, body: B) -> Builder {
        self.body = Box::new(Some(body));
        self
    }

    /// Finish the builder into Response<Body>
    #[inline]
    pub fn build(self) -> Result<Response<Body>, SaphirError> {
        let Builder {
            inner,
            cookies,
            mut body,
            status_set: _,
        } = self;
        let b = body.transmute();
        let raw = inner.body(b)?;

        Ok(Response {
            inner: raw,
            cookies: cookies.unwrap_or_default(),
        })
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "json")]
mod json {
    use serde::Serialize;

    use super::*;

    impl Builder {
        pub fn json<T: Serialize>(self, t: &T) -> Result<Builder, (Builder, SaphirError)> {
            match serde_json::to_vec(t) {
                Ok(v) => Ok(self.body(v)),
                Err(e) => Err((self, e.into())),
            }
        }
    }
}

#[cfg(feature = "form")]
mod form {
    use serde::Serialize;

    use super::*;

    impl Builder {
        pub fn form<T: Serialize>(self, t: &T) -> Result<Builder, (Builder, SaphirError)> {
            match serde_urlencoded::to_string(t) {
                Ok(v) => Ok(self.body(v)),
                Err(e) => Err((self, e.into())),
            }
        }
    }
}
