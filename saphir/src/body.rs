#![allow(clippy::type_complexity)]

use crate::error::SaphirError;
use futures::{
    task::{Context, Poll},
    Future, StreamExt,
};
use http::HeaderMap;
use hyper::body::{Body as RawBody, Buf, HttpBody, SizeHint};
use std::pin::Pin;

pub use hyper::body::Bytes;

#[cfg(feature = "form")]
pub use form::Form;
#[cfg(feature = "json")]
pub use json::Json;
use std::ops::DerefMut;

#[doc(hidden)]
pub(crate) static mut REQUEST_BODY_BYTES_LIMIT: Option<usize> = None;

pub(crate) enum BodyInner {
    Raw(RawBody),
    Memory(Bytes),
}

impl BodyInner {
    pub fn empty() -> Self {
        BodyInner::Raw(RawBody::empty())
    }

    #[inline]
    pub(crate) fn from_raw(raw: RawBody) -> Self {
        BodyInner::Raw(raw)
    }

    #[inline]
    pub(crate) fn into_raw(self) -> RawBody {
        match self {
            BodyInner::Raw(r) => r,
            BodyInner::Memory(b) => RawBody::from(b),
        }
    }

    pub async fn load(self) -> Result<Bytes, SaphirError> {
        unsafe {
            if let Some(0) = REQUEST_BODY_BYTES_LIMIT {
                return Ok(Bytes::new());
            }
        }
        match self {
            BodyInner::Raw(mut r) => {
                let first = if let Some(buf) = r.next().await.transpose().map_err(SaphirError::from)? {
                    buf
                } else {
                    return Ok(Bytes::new());
                };

                unsafe {
                    if REQUEST_BODY_BYTES_LIMIT.as_ref().filter(|p| first.len() >= **p).is_some() {
                        return Ok(first);
                    }
                }

                let second = if let Some(buf) = r.next().await.transpose().map_err(SaphirError::from)? {
                    buf
                } else {
                    return Ok(first);
                };

                let cap = first.remaining() + second.remaining() + r.size_hint().lower() as usize;
                let mut vec = Vec::with_capacity(cap);
                vec.extend_from_slice(first.as_ref());
                vec.extend_from_slice(second.as_ref());

                unsafe {
                    if REQUEST_BODY_BYTES_LIMIT.as_ref().filter(|p| vec.len() >= **p).is_some() {
                        return Ok(vec.into());
                    }
                }

                while let Some(buf) = r.next().await.transpose().map_err(SaphirError::from)? {
                    vec.extend_from_slice(buf.as_ref());
                    unsafe {
                        if REQUEST_BODY_BYTES_LIMIT.as_ref().filter(|p| vec.len() >= **p).is_some() {
                            break;
                        }
                    }
                }

                Ok(vec.into())
            }
            BodyInner::Memory(b) => Ok(b),
        }
    }
}

impl<T> From<T> for BodyInner
where
    T: Into<RawBody>,
{
    fn from(b: T) -> Self {
        BodyInner::Raw(b.into())
    }
}

pub struct Body<T = Bytes>
where
    T: FromBytes,
{
    inner: Option<BodyInner>,
    fut: Option<Pin<Box<dyn Future<Output = Result<(T::Out, Bytes), SaphirError>> + Send + Sync + 'static>>>,
}

impl Body<Bytes> {
    pub fn empty() -> Self {
        Body {
            inner: Some(BodyInner::empty()),
            fut: None,
        }
    }
}

impl<T: 'static> Body<T>
where
    T: FromBytes,
{
    #[inline]
    pub(crate) async fn generate(inner: BodyInner) -> Result<(T::Out, Bytes), SaphirError> {
        T::from_bytes(inner.load().await?)
    }

    #[inline]
    pub(crate) fn from_raw(raw: RawBody) -> Self {
        Body {
            inner: Some(BodyInner::from_raw(raw)),
            fut: None,
        }
    }

    #[inline]
    pub(crate) fn into_raw(self) -> RawBody {
        self.inner.unwrap_or_else(BodyInner::empty).into_raw()
    }

    /// Performing `take` will give your a owned version of the body, leaving a
    /// empty one behind
    #[inline]
    pub fn take(&mut self) -> Self {
        Body {
            inner: self.inner.take(),
            fut: None,
        }
    }

    /// Performing `take_as` will give your a owned version of the body as U,
    /// leaving a empty one behind
    #[inline]
    pub fn take_as<U: FromBytes>(&mut self) -> Body<U> {
        Body {
            inner: self.inner.take(),
            fut: None,
        }
    }
}

impl<T: FromBytes> Default for Body<T> {
    fn default() -> Self {
        Body { inner: None, fut: None }
    }
}

pub trait FromBytes {
    type Out;
    fn from_bytes(bytes: Bytes) -> Result<(Self::Out, Bytes), SaphirError>
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
            match fut.as_mut().poll(cx) {
                Poll::Ready(res) => Poll::Ready(res.map(|(out, b)| {
                    self.inner = Some(BodyInner::Memory(b));
                    out
                })),
                Poll::Pending => Poll::Pending,
            }
        } else if let Some(body) = self.inner.take() {
            self.fut = Some(Box::pin(Self::generate(body)));

            match self
                .fut
                .as_mut()
                .expect("This won't happens since freshly allocated to Some(_)")
                .as_mut()
                .poll(cx)
            {
                Poll::Ready(res) => Poll::Ready(res.map(|(out, b)| {
                    self.inner = Some(BodyInner::Memory(b));
                    out
                })),
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Err(SaphirError::BodyAlreadyTaken))
        }
    }
}

impl FromBytes for Bytes {
    type Out = Bytes;

    #[inline]
    fn from_bytes(bytes: Bytes) -> Result<(Self, Bytes), SaphirError>
    where
        Self: Sized,
    {
        Ok((bytes.clone(), bytes))
    }
}

impl FromBytes for String {
    type Out = String;

    #[inline]
    fn from_bytes(bytes: Bytes) -> Result<(Self, Bytes), SaphirError>
    where
        Self: Sized,
    {
        String::from_utf8(bytes.to_vec())
            .map_err(|e| SaphirError::Custom(Box::new(e)))
            .map(|s| (s, bytes))
    }
}

impl FromBytes for Vec<u8> {
    type Out = Vec<u8>;

    #[inline]
    fn from_bytes(bytes: Bytes) -> Result<(Self, Bytes), SaphirError>
    where
        Self: Sized,
    {
        Ok((bytes.to_vec(), bytes))
    }
}

#[cfg(feature = "json")]
pub mod json {
    use crate::{body::FromBytes, error::SaphirError};
    use hyper::body::Bytes;
    use serde::Deserialize;
    use std::{
        borrow::{Borrow, BorrowMut},
        ops::{Deref, DerefMut},
    };

    pub struct Json<T>(pub T);

    impl<T> Json<T> {
        pub fn into_inner(self) -> T {
            self.0
        }

        #[deprecated(since = "2.2.7", note = "Please use the into_inner function instead")]
        pub fn unwrap(self) -> T {
            self.0
        }
    }

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

    impl<T> AsRef<T> for Json<T> {
        fn as_ref(&self) -> &T {
            &self.0
        }
    }

    impl<T> AsMut<T> for Json<T> {
        fn as_mut(&mut self) -> &mut T {
            &mut self.0
        }
    }

    impl<T> Borrow<T> for Json<T> {
        fn borrow(&self) -> &T {
            &self.0
        }
    }

    impl<T> BorrowMut<T> for Json<T> {
        fn borrow_mut(&mut self) -> &mut T {
            &mut self.0
        }
    }

    impl<T> FromBytes for Json<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        type Out = T;

        #[inline]
        fn from_bytes(bytes: Bytes) -> Result<(Self::Out, Bytes), SaphirError>
        where
            Self: Sized,
        {
            Ok((serde_json::from_slice(bytes.as_ref())?, bytes))
        }
    }
}

#[cfg(feature = "form")]
pub mod form {
    use crate::{body::FromBytes, error::SaphirError};
    use hyper::body::Bytes;
    use serde::Deserialize;
    use std::{
        borrow::{Borrow, BorrowMut},
        ops::{Deref, DerefMut},
    };

    pub struct Form<T>(pub T);

    impl<T> Form<T> {
        pub fn into_inner(self) -> T {
            self.0
        }

        #[deprecated(since = "2.2.7", note = "Please use the into_inner function instead")]
        pub fn unwrap(self) -> T {
            self.0
        }
    }

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

    impl<T> AsRef<T> for Form<T> {
        fn as_ref(&self) -> &T {
            &self.0
        }
    }

    impl<T> AsMut<T> for Form<T> {
        fn as_mut(&mut self) -> &mut T {
            &mut self.0
        }
    }

    impl<T> Borrow<T> for Form<T> {
        fn borrow(&self) -> &T {
            &self.0
        }
    }

    impl<T> BorrowMut<T> for Form<T> {
        fn borrow_mut(&mut self) -> &mut T {
            &mut self.0
        }
    }

    impl<T> FromBytes for Form<T>
    where
        T: for<'a> Deserialize<'a>,
    {
        type Out = T;

        #[inline]
        fn from_bytes(bytes: Bytes) -> Result<(Self::Out, Bytes), SaphirError>
        where
            Self: Sized,
        {
            Ok((serde_urlencoded::from_bytes(bytes.as_ref())?, bytes))
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

        unsafe {
            self.map_unchecked_mut(|s| s.inner.as_mut().expect("This won't happen since checked in the lines above"))
                .poll_data(cx)
        }
    }

    fn poll_trailers(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        if self.inner.is_none() {
            return Poll::Ready(Err(SaphirError::BodyAlreadyTaken));
        }

        unsafe {
            self.map_unchecked_mut(|s| s.inner.as_mut().expect("This won't happen since checked in the lines above"))
                .poll_trailers(cx)
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

impl HttpBody for BodyInner {
    type Data = Bytes;
    type Error = SaphirError;

    fn poll_data(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, SaphirError>>> {
        if let BodyInner::Memory(b) = self.deref_mut() {
            if !b.is_empty() {
                Poll::Ready(Some(Ok(b.slice(..))))
            } else {
                Poll::Ready(None)
            }
        } else {
            let p = unsafe {
                self.map_unchecked_mut(|s| match s {
                    BodyInner::Raw(r) => r,
                    BodyInner::Memory(_) => unreachable!("This is unreachable since checked above"),
                })
                .poll_data(cx)
            };

            match p {
                Poll::Ready(Some(res)) => Poll::Ready(Some(res.map_err(SaphirError::from))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    fn poll_trailers(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        if let BodyInner::Memory(_b) = self.deref_mut() {
            Poll::Ready(Ok(None))
        } else {
            let p = unsafe {
                self.map_unchecked_mut(|s| match s {
                    BodyInner::Raw(r) => r,
                    BodyInner::Memory(_) => unreachable!("This is unreachable since checked above"),
                })
                .poll_trailers(cx)
            };

            match p {
                Poll::Ready(res) => Poll::Ready(res.map_err(SaphirError::from)),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            BodyInner::Raw(r) => r.is_end_stream(),
            BodyInner::Memory(b) => b.remaining() > 0,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            BodyInner::Raw(r) => r.size_hint(),
            BodyInner::Memory(b) => SizeHint::with_exact(b.remaining() as u64),
        }
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
        let Body { inner, .. } = self;
        inner.unwrap_or_else(BodyInner::empty).into_raw()
    }
}
