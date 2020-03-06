use futures::{Future, FutureExt};

use crate::{
    request::Request,
    responder::{DynResponder, Responder},
};
use std::pin::Pin;

/// Define a Handler of a potential http request
///
/// Implementing this trait on any type will allow the router to route request
/// towards it. Implemented by default on Controllers and on any `async
/// fn(Request<Body>) -> impl Responder`
pub trait Handler<T> {
    /// Responder returned by the handler
    type Responder: Responder;
    /// Specific future returning the responder
    type Future: Future<Output = Self::Responder>;

    /// Handle the http request, returning a future of a responder
    fn handle(&self, req: Request<T>) -> Self::Future;
}

impl<T, Fun, Fut, R> Handler<T> for Fun
where
    Fun: Fn(Request<T>) -> Fut,
    Fut: 'static + Future<Output = R> + Send,
    R: Responder,
{
    type Future = Box<dyn Future<Output = Self::Responder> + Unpin + Send>;
    type Responder = R;

    #[inline]
    fn handle(&self, req: Request<T>) -> Self::Future {
        Box::new(Box::pin((*self)(req)))
    }
}

#[doc(hidden)]
pub trait DynHandler<T> {
    fn dyn_handle(&self, req: Request<T>) -> Pin<Box<dyn Future<Output = Box<dyn DynResponder + Send>> + Unpin + Send>>;
}

impl<T, H, Fut, R> DynHandler<T> for H
where
    R: 'static + Responder + Send,
    Fut: 'static + Future<Output = R> + Unpin + Send,
    H: Handler<T, Future = Fut, Responder = R>,
{
    #[inline]
    fn dyn_handle(&self, req: Request<T>) -> Pin<Box<dyn Future<Output = Box<dyn DynResponder + Send>> + Unpin + Send>> {
        Box::pin(self.handle(req).map(|r| Box::new(Some(r)) as Box<dyn DynResponder + Send>))
    }
}
