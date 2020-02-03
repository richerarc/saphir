use crate::response::{Builder, Response};
use crate::error::SaphirError;
use http::StatusCode;
use hyper::Body;

macro_rules! impl_status_responder {
    ( $( $x:ty ),+ ) => {
        $(
            impl Responder for $x {
                fn respond_with_builder(self, builder: Builder) -> Builder {
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
                fn respond_with_builder(self, builder: Builder) -> Builder {
                    builder.body(self)
                }
            }
        )+
    }
}

macro_rules! impl_tuple_responder {

    ( $($idx:tt -> $T:ident),+ ) => {

            impl<$($T:Responder),+> Responder for ($($T),+) {
                fn respond_with_builder(self, builder: Builder) -> Builder {
                    $(let builder = self.$idx.respond_with_builder(builder);)+
                    builder
                }
            }
    }
}

pub trait Responder {
    fn respond_with_builder(self, builder: Builder) -> Builder;

    fn respond(self) -> Result<Response<Body>, SaphirError> where Self: Sized {
        self.respond_with_builder(Builder::new()).build()
    }
}

impl Responder for StatusCode {
    fn respond_with_builder(self, builder: Builder) -> Builder {
        builder.status(self)
    }
}

impl<T: Responder> Responder for Option<T> {
    fn respond_with_builder(self, builder: Builder) -> Builder {
        if let Some(r) = self {
            r.respond_with_builder(builder.status(200))
        } else {
            builder.status(404)
        }
    }
}

impl<T: Responder, E: Responder> Responder for Result<T, E> {
    fn respond_with_builder(self, builder: Builder) -> Builder {
        match self {
            Ok(r) => r.respond_with_builder(builder),
            Err(r) => r.respond_with_builder(builder),
        }
    }
}

impl Responder for hyper::Error {
    fn respond_with_builder(self, builder: Builder) -> Builder {
        builder.status(500)
    }
}

impl Responder for Builder {
    fn respond_with_builder(self, _builder: Builder) -> Builder {
        self
    }
}

impl_status_responder!(u16, i16, u32, i32, u64, i64, usize, isize);
impl_body_responder!(String, &'static str, Vec<u8>, &'static [u8], hyper::body::Bytes);
impl_tuple_responder!(0->A, 1->B);
impl_tuple_responder!(0->A, 1->B, 2->C);
impl_tuple_responder!(0->A, 1->B, 2->C, 3->D);
impl_tuple_responder!(0->A, 1->B, 2->C, 3->D, 4->E);
impl_tuple_responder!(0->A, 1->B, 2->C, 3->D, 4->E, 5->F);

pub trait DynResponder {
    fn dyn_respond(&mut self) -> Result<Response<Body>, SaphirError>;
}

impl<T> DynResponder for Option<T> where T: Responder {
    fn dyn_respond(&mut self) -> Result<Response<Body>, SaphirError> {
        self.take().ok_or(500).respond()
    }
}