use hyper::body::Body as RawBody;
use hyper::body::Bytes;
use hyper::body::to_bytes;
use futures::{Future, TryFutureExt};
use futures::task::{Context, Poll};
use std::marker::PhantomData;
use std::pin::Pin;
use crate::error::SaphirError;

pub struct Body<T = Bytes> {
    inner: Option<RawBody>,
    fut: Option<Pin<Box<dyn Future<Output=Result<T, SaphirError>> + Send + Sync + 'static>>>,
}

impl<T: 'static> Body<T> {
    pub(crate) fn from_raw(raw: RawBody) -> Self {
        Body {
            inner: Some(raw),
            fut: None,
        }
    }

    pub(crate) fn into_raw(self) -> RawBody {
        self.inner.unwrap_or_else(|| RawBody::empty())
    }

    /// Performing take will give your a owned version of the body, leaving a empty one behind
    pub fn take(&mut self) -> Self {
        Body {
            inner: self.inner.take(),
            fut: None,
        }
    }
}

impl<T: 'static> Body<T> where T: FromBytes {
    pub(crate) async fn generate(raw: RawBody) -> Result<T, SaphirError> {
        T::from_bytes(to_bytes(raw).map_err(|e| SaphirError::from(e)).await?)
    }

    pub(crate) fn map_type<U: FromBytes>(self) -> Body<U> {
        let Body { inner, fut: _ } = self;

        Body {
            inner,
            fut: None,
        }
    }
}

pub trait FromBytes {
    fn from_bytes(bytes: Bytes) -> Result<Self, SaphirError> where Self: Sized;
}

impl FromBytes for Bytes {
    fn from_bytes(bytes: Bytes) -> Result<Self, SaphirError> where Self: Sized {
        Ok(bytes)
    }
}

impl<T: 'static + Unpin> Future for Body<T> where T: FromBytes {
    type Output = Result<T, SaphirError>;

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