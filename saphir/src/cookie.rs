use crate::{http_context::HttpContext, responder::Responder, response::Builder};
pub use saphir_cookie::*;

impl Responder for Cookie<'static> {
    fn respond_with_builder(self, builder: Builder, _ctx: &HttpContext) -> Builder {
        builder.cookie(self)
    }
}

impl Responder for CookieBuilder<'static> {
    fn respond_with_builder(self, builder: Builder, ctx: &HttpContext) -> Builder {
        self.finish().respond_with_builder(builder, ctx)
    }
}

impl Responder for CookieJar {
    fn respond_with_builder(self, mut builder: Builder, _ctx: &HttpContext) -> Builder {
        *builder.cookies_mut() = self;
        builder
    }
}
