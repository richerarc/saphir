#![allow(clippy::let_and_return)]
use crate::{http_context::HttpContext, response::Builder};
use http::StatusCode;

macro_rules! impl_status_responder {
    ( $( $x:ty ),+ ) => {
        $(
            impl Responder for $x {
                fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
                    builder.status(self as u16)
                }
            }
        )+
    }
}

macro_rules! impl_body_responder {
    ( $( $x:ty ),+ ) => {
        $(
            impl Responder for $x {
                fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
                    builder.body(self)
                }
            }
        )+
    }
}

macro_rules! impl_plain_body_responder {
    ( $( $x:ty ),+ ) => {
        $(
            impl Responder for $x {
                fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
                    builder.header(http::header::CONTENT_TYPE, "text/plain").body(self)
                }
            }
        )+
    }
}

macro_rules! impl_tuple_responder {

    ( $($idx:tt -> $T:ident),+ ) => {

            impl<$($T:Responder),+> Responder for ($($T),+) {
                fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
                    $(let builder = self.$idx.respond_with_builder(builder, ctx);)+
                    builder
                }
            }
    }
}

/// Responder defines what type can generate a response
pub trait Responder {
    /// Consume self into a builder
    ///
    /// ```rust
    /// # use saphir::prelude::*;
    /// struct CustomResponder(String);
    ///
    /// impl Responder for CustomResponder {
    ///     fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
    ///         // Put the string as the response body
    ///         builder.body(self.0)
    ///     }
    /// }
    /// ```
    fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder;
}

impl<T> Responder for Vec<T>
where
    T: Responder,
{
    fn respond_with_builder(self, mut builder: Builder, ctx: &HttpContext) -> Builder {
        for responder in self {
            builder = responder.respond_with_builder(builder, ctx);
        }
        builder
    }
}

impl<T> Responder for &'static [T]
where
    T: Responder + Clone,
{
    fn respond_with_builder(self, mut builder: Builder, ctx: &HttpContext) -> Builder {
        for responder in self {
            builder = responder.clone().respond_with_builder(builder, ctx);
        }
        builder
    }
}

impl Responder for StatusCode {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        builder.status(self)
    }
}

impl Responder for () {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        builder.status(200)
    }
}

impl<T: Responder> Responder for Option<T> {
    fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
        if let Some(r) = self {
            r.respond_with_builder(builder, ctx).status_if_not_set(200)
        } else {
            builder.status_if_not_set(404)
        }
    }
}

impl<T: Responder, E: Responder> Responder for Result<T, E> {
    fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
        match self {
            Ok(r) => r.respond_with_builder(builder, ctx).status_if_not_set(200),
            Err(r) => r.respond_with_builder(builder, ctx).status_if_not_set(500),
        }
    }
}

impl Responder for hyper::Error {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        builder.status(500)
    }
}

impl Responder for Builder {
    fn respond_with_builder(self, _builder: Builder, _ctx: &HttpContext) -> Builder {
        self
    }
}

#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
mod json {
    use super::*;
    use crate::body::Json;
    use serde::Serialize;

    impl<T: Serialize> Responder for Json<T> {
        fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
            match builder.json(&self.0) {
                Ok(b) => b,
                Err((b, _e)) => b.status(500).body("Unable to serialize json data"),
            }
        }
    }
}

#[cfg(feature = "form")]
#[cfg_attr(docsrs, doc(cfg(feature = "form")))]
mod form {
    use super::*;
    use crate::body::Form;
    use serde::Serialize;

    impl<T: Serialize> Responder for Form<T> {
        fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
            match builder.form(&self.0) {
                Ok(b) => b,
                Err((b, _e)) => b.status(500).body("Unable to serialize form data"),
            }
        }
    }
}

impl_status_responder!(u16, i16, u32, i32, u64, i64, usize, isize);
impl_plain_body_responder!(String, &'static str);
impl_body_responder!(Vec<u8>, &'static [u8], hyper::body::Bytes);
impl_tuple_responder!(0->A, 1->B);
impl_tuple_responder!(0->A, 1->B, 2->C);
impl_tuple_responder!(0->A, 1->B, 2->C, 3->D);
impl_tuple_responder!(0->A, 1->B, 2->C, 3->D, 4->E);
impl_tuple_responder!(0->A, 1->B, 2->C, 3->D, 4->E, 5->F);

/// Trait used by the server, not meant for manual implementation
pub trait DynResponder {
    #[doc(hidden)]
    fn dyn_respond(&mut self, builder: Builder, ctx: &HttpContext) -> Builder;
}

impl<T> DynResponder for Option<T>
where
    T: Responder,
{
    fn dyn_respond(&mut self, builder: Builder, ctx: &HttpContext) -> Builder {
        self.take().ok_or(500).respond_with_builder(builder, ctx)
    }
}
