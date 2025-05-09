use std::{
    any::Any,
    convert::TryFrom,
    ops::{Deref, DerefMut},
};

use crate::cookie::{Cookie, CookieJar};
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
    #[cfg(feature = "tracing-instrument")]
    #[doc(hidden)]
    pub(crate) span: Option<tracing::span::Span>,
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
            #[cfg(feature = "tracing-instrument")]
            span: None,
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
        #[cfg(feature = "tracing-instrument")]
        {
            let Response { inner, cookies, span } = self;
            Response {
                inner: inner.map(f),
                cookies,
                span,
            }
        }
        #[cfg(not(feature = "tracing-instrument"))]
        {
            let Response { inner, cookies } = self;
            Response { inner: inner.map(f), cookies }
        }
    }

    pub(crate) fn into_raw(self) -> Result<RawResponse<T>, SaphirError> {
        #[cfg(feature = "tracing-instrument")]
        let Response { mut inner, cookies, span: _ } = self;
        #[cfg(not(feature = "tracing-instrument"))]
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
    body: Box<dyn TransmuteBody + Send>,
    #[doc(hidden)]
    status_set: bool,
    #[cfg(feature = "tracing-instrument")]
    #[doc(hidden)]
    span: Option<tracing::span::Span>,
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
            #[cfg(feature = "tracing-instrument")]
            span: None,
        }
    }

    #[cfg(feature = "tracing-instrument")]
    #[inline]
    pub(crate) fn span(mut self, span: tracing::span::Span) -> Builder {
        self.span = Some(span);
        self
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
        self.cookies_mut().add(cookie);
        self
    }

    ///
    #[inline]
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        if self.cookies.is_none() {
            self.cookies = Some(CookieJar::new());
        }

        self.cookies.as_mut().expect("Checked above")
    }

    #[inline]
    pub fn cookies(mut self, cookies: CookieJar) -> Builder {
        self.cookies = Some(cookies);
        self
    }

    #[inline]
    pub fn body<B: 'static + Into<RawBody> + Send>(mut self, body: B) -> Builder {
        self.body = Box::new(Some(body));
        self
    }

    #[cfg(any(feature = "form", feature = "json"))]
    #[inline]
    pub(crate) fn content_type_if_not_set(mut self, content_type: &str) -> Builder {
        if let Some(headers) = self.inner.headers_mut() {
            if !headers.contains_key(http::header::CONTENT_TYPE) {
                if let Ok(hv) = HeaderValue::from_str(content_type) {
                    headers.insert(http::header::CONTENT_TYPE, hv);
                }
            }
        }
        self
    }

    /// Finish the builder into Response<Body>
    #[inline]
    pub fn build(self) -> Result<Response<Body>, SaphirError> {
        #[cfg(feature = "tracing-instrument")]
        let Builder {
            inner,
            cookies,
            mut body,
            span,
            ..
        } = self;
        #[cfg(not(feature = "tracing-instrument"))]
        let Builder { inner, cookies, mut body, .. } = self;
        let b = body.transmute();
        let raw = inner.body(b)?;

        Ok(Response {
            inner: raw,
            cookies: cookies.unwrap_or_default(),
            #[cfg(feature = "tracing-instrument")]
            span,
        })
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
mod json {
    use serde::Serialize;

    use super::*;

    impl Builder {
        pub fn json<T: Serialize>(self, t: &T) -> Result<Builder, Box<(Builder, SaphirError)>> {
            match serde_json::to_vec(t) {
                Ok(v) => Ok(self.content_type_if_not_set("application/json").body(v)),
                Err(e) => Err(Box::new((self, e.into()))),
            }
        }
    }
}

#[cfg(feature = "form")]
#[cfg_attr(docsrs, doc(cfg(feature = "form")))]
mod form {
    use serde::Serialize;

    use super::*;

    impl Builder {
        pub fn form<T: Serialize>(self, t: &T) -> Result<Builder, Box<(Builder, SaphirError)>> {
            match serde_urlencoded::to_string(t) {
                Ok(v) => Ok(self.content_type_if_not_set("application/x-www-form-urlencoded").body(v)),
                Err(e) => Err(Box::new((self, e.into()))),
            }
        }
    }
}

#[cfg(feature = "file")]
#[cfg_attr(docsrs, doc(cfg(feature = "file")))]
mod file {
    use super::*;
    use crate::{file::FileStream, prelude::Bytes};
    use futures::Stream;

    impl Builder {
        pub fn file<F: Into<FileStream>>(self, file: F) -> Builder {
            self.body(Box::new(file.into())
                as Box<
                    dyn Stream<Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync + 'static>>> + Send + 'static,
                >)
        }
    }
}
