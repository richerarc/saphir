use crate::{
    http_context::HttpContext,
    request::{FromRequest, Request},
    responder::Responder,
    response::Builder,
};
use std::{
    borrow::{Borrow, BorrowMut},
    ops::{Deref, DerefMut},
};

use crate::{body::Body, prelude::Bytes};
pub use http::Extensions;

pub enum ExtError {
    /// The extension type was not found, the type name of the missing extension
    /// is returned
    MissingExtension(&'static str),
}

impl Responder for ExtError {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        debug!("Missing extension of type: {}", std::any::type_name::<Self>());
        builder.status(500)
    }
}

pub struct Ext<T>(pub T);

impl<T> Ext<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Ext<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Ext<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> AsRef<T> for Ext<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> AsMut<T> for Ext<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> Borrow<T> for Ext<T> {
    fn borrow(&self) -> &T {
        &self.0
    }
}

impl<T> BorrowMut<T> for Ext<T> {
    fn borrow_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Send + Sync + 'static> Responder for Ext<T> {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        builder.extension(self.into_inner())
    }
}

impl<T> FromRequest for Ext<T>
where
    T: Send + Sync + 'static,
{
    type Err = ExtError;
    type Fut = futures::future::Ready<Result<Self, Self::Err>>;

    fn from_request(req: &mut Request) -> Self::Fut {
        futures::future::ready(
            req.extensions_mut()
                .remove::<T>()
                .ok_or_else(|| ExtError::MissingExtension(std::any::type_name::<T>()))
                .map(Ext),
        )
    }
}

impl FromRequest for Extensions {
    type Err = ();
    type Fut = futures::future::Ready<Result<Self, Self::Err>>;

    fn from_request(req: &mut Request<Body<Bytes>>) -> Self::Fut {
        futures::future::ready(Ok(std::mem::take(req.extensions_mut())))
    }
}
