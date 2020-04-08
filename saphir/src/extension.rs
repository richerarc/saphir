use std::ops::{Deref, DerefMut};
use std::borrow::{Borrow, BorrowMut};
use crate::responder::Responder;
use crate::response::Builder;
use crate::http_context::HttpContext;
use crate::request::{FromRequest, Request};

pub enum ExtError {
    /// The extension type was not found, the type name of the missing extension is returned
    MissingExtension(&'static str),
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

impl<T> Responder for Ext<T> {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        builder.extension(self.into_inner())
    }
}

impl<T, U> FromRequest<T> for Ext<U> {
    type Err = ExtError;

    fn from_request(req: &mut Request<T>) -> Result<Self, Self::Err> {
        req.extensions_mut().remove::<U>().ok_or_else(|| ExtError::MissingExtension(std::any::type_name::<U>()))
    }
}