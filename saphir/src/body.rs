use crate::error::SaphirError;
use futures::{
    task::{Context, Poll},
    Future, TryFutureExt,
};
use http::HeaderMap;
use http_body::SizeHint;
use hyper::body::{to_bytes, Body as RawBody, HttpBody};
use std::pin::Pin;

pub use hyper::body::Bytes;

#[cfg(feature = "form")]
pub use form::Form;
#[cfg(feature = "json")]
pub use json::Json;

pub struct Body<T = Bytes>
where
    T: FromBytes,
{
    inner: Option<RawBody>,
    fut: Option<Pin<Box<dyn Future<Output = Result<T::Out, SaphirError>> + Send + Sync + 'static>>>,
}

impl Body<Bytes> {
    pub fn empty() -> Self {
        Body {
            inner: Some(RawBody::empty()),
            fut: None,
        }
    }
}

impl<T: 'static> Body<T>
where
    T: FromBytes,
{
    #[inline]
    pub(crate) async fn generate(raw: RawBody) -> Result<T::Out, SaphirError> {
        T::from_bytes(to_bytes(raw).map_err(|e| SaphirError::from(e)).await?)
    }

    #[inline]
    pub(crate) fn from_raw(raw: RawBody) -> Self {
        Body { inner: Some(raw), fut: None }
    }

    #[inline]
    pub(crate) fn into_raw(self) -> RawBody {
        self.inner.unwrap_or_else(|| RawBody::empty())
    }

    /// Performing `take` will give your a owned version of the body, leaving a empty one behind
    #[inline]
    pub fn take(&mut self) -> Self {
        Body {
            inner: self.inner.take(),
            fut: None,
        }
    }

    /// Performing `take_as` will give your a owned version of the body as U, leaving a empty one behind
    #[inline]
    pub fn take_as<U: FromBytes>(&mut self) -> Body<U> {
        Body {
            inner: self.inner.take(),
            fut: None,
        }
    }
}

pub trait FromBytes {
    type Out;
    fn from_bytes(bytes: Bytes) -> Result<Self::Out, SaphirError>
    where
        Self: Sized;
}

impl<T: 'static + Unpin> Future for Body<T>
where
    T: FromBytes,
{
    type Output = Result<T::Out, SaphirError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(fut) = self.fut.as_mut() {
            fut.as_mut().poll(cx)
        } else if let Some(body) = self.inner.take() {
            self.fut = Some(Box::pin(Self::generate(body)));

            self.fut
                .as_mut()
                .expect("This won't happens since freshly allocated to Some(_)")
                .as_mut()
                .poll(cx)
        } else {
            Poll::Ready(Err(SaphirError::BodyAlreadyTaken))
        }
    }
}

impl FromBytes for Bytes {
    type Out = Bytes;

    #[inline]
    fn from_bytes(bytes: Bytes) -> Result<Self, SaphirError>
    where
        Self: Sized,
    {
        Ok(bytes)
    }
}

impl FromBytes for String {
    type Out = String;

    #[inline]
    fn from_bytes(bytes: Bytes) -> Result<Self, SaphirError>
    where
        Self: Sized,
    {
        String::from_utf8(bytes.to_vec()).map_err(|e| SaphirError::Custom(Box::new(e)))
    }
}

impl FromBytes for Vec<u8> {
    type Out = Vec<u8>;

    #[inline]
    fn from_bytes(bytes: Bytes) -> Result<Self, SaphirError>
    where
        Self: Sized,
    {
        Ok(bytes.to_vec())
    }
}

#[cfg(feature = "json")]
pub mod json {
    use crate::{body::FromBytes, error::SaphirError};
    use hyper::body::Bytes;
    use serde::Deserialize;
    use std::ops::{Deref, DerefMut};

    pub struct Json<T>(pub T);

    impl<T> Deref for Json<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> DerefMut for Json<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> FromBytes for Json<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        type Out = T;

        #[inline]
        fn from_bytes(bytes: Bytes) -> Result<Self::Out, SaphirError>
        where
            Self: Sized,
        {
            Ok(serde_json::from_slice(bytes.as_ref())?)
        }
    }
}

#[cfg(feature = "form")]
pub mod form {
    use crate::{body::FromBytes, error::SaphirError};
    use hyper::body::Bytes;
    use serde::Deserialize;
    use std::ops::{Deref, DerefMut};

    pub struct Form<T>(pub T);

    impl<T> Deref for Form<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> DerefMut for Form<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> FromBytes for Form<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        type Out = T;

        #[inline]
        fn from_bytes(bytes: Bytes) -> Result<Self::Out, SaphirError>
        where
            Self: Sized,
        {
            Ok(serde_urlencoded::from_bytes(bytes.as_ref())?)
        }
    }
}

impl<T: FromBytes + Unpin> HttpBody for Body<T> {
    type Data = Bytes;
    type Error = SaphirError;

    fn poll_data(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, SaphirError>>> {
        if self.inner.is_none() {
            return Poll::Ready(Some(Err(SaphirError::BodyAlreadyTaken)));
        }

        let p = unsafe {
            self.map_unchecked_mut(|s| s.inner.as_mut().expect("This won't happen since checked in the lines above"))
                .poll_data(cx)
        };

        match p {
            Poll::Ready(Some(res)) => Poll::Ready(Some(res.map_err(|e| SaphirError::from(e)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_trailers(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        if self.inner.is_none() {
            return Poll::Ready(Err(SaphirError::BodyAlreadyTaken));
        }

        let p = unsafe {
            self.map_unchecked_mut(|s| s.inner.as_mut().expect("This won't happen since checked in the lines above"))
                .poll_trailers(cx)
        };

        match p {
            Poll::Ready(res) => Poll::Ready(res.map_err(|e| SaphirError::from(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        if self.inner.is_none() {
            return true;
        }

        self.inner.as_ref().expect("This won't happen since checked in the lines above").is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        if self.inner.is_none() {
            return SizeHint::with_exact(0);
        }

        self.inner.as_ref().expect("This won't happen since checked in the lines above").size_hint()
    }
}

#[doc(hidden)]
pub trait TransmuteBody {
    fn transmute(&mut self) -> Body<Bytes>;
}

#[doc(hidden)]
impl<T> TransmuteBody for Option<T>
where
    T: Into<RawBody>,
{
    #[inline]
    fn transmute(&mut self) -> Body<Bytes> {
        Body::from_raw(if let Some(b) = self.take() { b.into() } else { RawBody::empty() })
    }
}

impl<T: FromBytes> Into<RawBody> for Body<T> {
    #[inline]
    fn into(self) -> RawBody {
        let Body { inner, fut: _ } = self;
        inner.unwrap_or_else(|| RawBody::empty())
    }
}
