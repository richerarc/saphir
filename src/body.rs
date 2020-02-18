use hyper::body::Body as RawBody;
use hyper::body::Bytes;
use hyper::body::to_bytes;
use futures::{Future, TryFutureExt};
use futures::task::{Context, Poll};
use std::pin::Pin;
use crate::error::SaphirError;

pub struct Body<T = Bytes> where T: FromBytes {
    inner: Option<RawBody>,
    fut: Option<Pin<Box<dyn Future<Output=Result<T::Out, SaphirError>> + Send + Sync + 'static>>>,
}

impl<T: 'static> Body<T> where T: FromBytes {
    pub(crate) async fn generate(raw: RawBody) -> Result<T::Out, SaphirError> {
        T::from_bytes(to_bytes(raw).map_err(|e| SaphirError::from(e)).await?)
    }

    pub(crate) fn from_raw(raw: RawBody) -> Self {
        Body {
            inner: Some(raw),
            fut: None,
        }
    }

    pub(crate) fn into_raw(self) -> RawBody {
        self.inner.unwrap_or_else(|| RawBody::empty())
    }

    /// Performing `take` will give your a owned version of the body, leaving a empty one behind
    pub fn take(&mut self) -> Self {
        Body {
            inner: self.inner.take(),
            fut: None,
        }
    }

    /// Performing `take_as` will give your a owned version of the body as U, leaving a empty one behind
    pub fn take_as<U: FromBytes>(&mut self) -> Body<U> {
        Body {
            inner: self.inner.take(),
            fut: None,
        }
    }
}

pub trait FromBytes {
    type Out;
    fn from_bytes(bytes: Bytes) -> Result<Self::Out, SaphirError> where Self: Sized;
}

impl<T: 'static + Unpin> Future for Body<T> where T: FromBytes {
    type Output = Result<T::Out, SaphirError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(fut) = self.fut.as_mut() {
            fut.as_mut().poll(cx)
        } else if let Some(body) = self.inner.take() {
            self.fut = Some(Box::pin(Self::generate(body)));

            self.fut.as_mut().expect("This won't happens since freshly allocated to Some(_)").as_mut().poll(cx)
        } else {
            Poll::Ready(Err(SaphirError::BodyAlreadyTaken))
        }
    }
}

impl FromBytes for Bytes {
    type Out = Bytes;

    fn from_bytes(bytes: Bytes) -> Result<Self, SaphirError> where Self: Sized {
        Ok(bytes)
    }
}

impl FromBytes for String {
    type Out = String;

    fn from_bytes(bytes: Bytes) -> Result<Self, SaphirError> where Self: Sized {
        String::from_utf8(bytes.to_vec()).map_err(|e| SaphirError::Custom(Box::new(e)))
    }
}

impl FromBytes for Vec<u8> {
    type Out = Vec<u8>;

    fn from_bytes(bytes: Bytes) -> Result<Self, SaphirError> where Self: Sized {
        Ok(bytes.to_vec())
    }
}

#[cfg(feature = "json")]
pub mod json {
    use serde::Deserialize;
    use std::marker::PhantomData;
    use crate::body::FromBytes;
    use hyper::body::Bytes;
    use crate::error::SaphirError;

    pub struct Json<T>(PhantomData<T>);

    impl<T> FromBytes for Json<T> where T: for<'a> Deserialize<'a> {
        type Out = T;

        fn from_bytes(bytes: Bytes) -> Result<Self::Out, SaphirError> where Self: Sized {
            Ok(serde_json::from_slice(bytes.as_ref())?)
        }
    }
}

#[cfg(feature = "form")]
pub mod form {
    use serde::Deserialize;
    use std::marker::PhantomData;
    use crate::body::FromBytes;
    use hyper::body::Bytes;
    use crate::error::SaphirError;

    pub struct Form<T>(PhantomData<T>);

    impl<T> FromBytes for Form<T> where T: for<'a> Deserialize<'a> {
        type Out = T;

        fn from_bytes(bytes: Bytes) -> Result<Self::Out, SaphirError> where Self: Sized {
            Ok(serde_urlencoded::from_bytes(bytes.as_ref())?)
        }
    }
}

#[doc(hidden)]
pub trait TransmuteBody {
    fn transmute(&mut self) -> Body<Bytes>;
}

#[doc(hidden)]
impl<T> TransmuteBody for Option<T> where T: Into<RawBody> {
    fn transmute(&mut self) -> Body<Bytes> {
        Body::from_raw(if let Some(b) = self.take() {
            b.into()
        } else {
            RawBody::empty()
        })
    }
}