//! A guard is called before the request is processed by the router and
//! can modify the request data or stops request processing by returning a response immediately.

use crate::{
    body::Body,
    request::Request,
    responder::{DynResponder, Responder},
};
use futures::{future::BoxFuture, FutureExt};
use futures_util::future::Future;

/// Auto trait implementation over every function that match the definition of a guard.
pub trait GuardHandler<Data> {
    type Responder: Responder + Send;
    type Future: Future<Output = Result<Request<Body>, Self::Responder>> + Send;

    fn validate(&self, data: &'static Data, req: Request<Body>) -> Self::Future;
}

impl<Data, Fun, Fut, Resp> GuardHandler<Data> for Fun
where
    Data: 'static,
    Resp: Responder + Send,
    Fun: Fn(&'static Data, Request<Body>) -> Fut,
    Fut: 'static + Future<Output = Result<Request<Body>, Resp>> + Send,
{
    type Responder = Resp;
    type Future = BoxFuture<'static, Result<Request<Body>, Self::Responder>>;

    #[inline]
    fn validate(&self, data: &'static Data, req: Request<Body>) -> Self::Future {
        (*self)(data, req).boxed()
    }
}

/// Builder to apply guards onto the handler
pub struct Builder<Chain: GuardChain> {
    chain: Chain,
}

impl Default for Builder<GuardChainEnd> {
    fn default() -> Self {
        Self { chain: GuardChainEnd }
    }
}

impl<Chain: GuardChain + 'static> Builder<Chain> {
    pub fn add<Data, Handler>(self, handler: Handler, data: Data) -> Builder<GuardChainLink<Data, Handler, Chain>>
    where
        Data: 'static + Sync + Send,
        Handler: 'static + GuardHandler<Data> + Sync + Send,
    {
        Builder {
            chain: GuardChainLink {
                data,
                handler,
                rest: self.chain,
            },
        }
    }

    pub(crate) fn build(self) -> Box<dyn GuardChain> {
        Box::new(self.chain)
    }
}

#[doc(hidden)]
pub trait GuardChain: Sync + Send {
    fn validate(&'static self, req: Request<Body>) -> BoxFuture<'static, Result<Request<Body>, Box<dyn DynResponder + Send>>>;

    /// to avoid useless heap allocation if there is only a guard end chain
    fn is_end(&self) -> bool;
}

#[doc(hidden)]
pub struct GuardChainEnd;

impl GuardChain for GuardChainEnd {
    #[inline]
    fn validate(&'static self, req: Request<Body>) -> BoxFuture<'static, Result<Request<Body>, Box<dyn DynResponder + Send>>> {
        async { Ok(req) }.boxed()
    }

    #[inline]
    fn is_end(&self) -> bool {
        true
    }
}

#[doc(hidden)]
pub struct GuardChainLink<Data, Handler: GuardHandler<Data>, Rest: GuardChain> {
    data: Data,
    handler: Handler,
    rest: Rest,
}

impl<Data, Handler, Rest> GuardChain for GuardChainLink<Data, Handler, Rest>
where
    Data: Sync + Send + 'static,
    Handler: GuardHandler<Data> + Sync + Send + 'static,
    Rest: GuardChain + 'static,
{
    #[inline]
    fn validate(&'static self, req: Request<Body>) -> BoxFuture<'static, Result<Request<Body>, Box<dyn DynResponder + Send>>> {
        async move {
            match self.handler.validate(&self.data, req).await {
                Ok(req) => {
                    if self.rest.is_end() {
                        Ok(req)
                    } else {
                        self.rest.validate(req).await
                    }
                }
                Err(resp) => Err(Box::new(Some(resp)) as Box<dyn DynResponder + Send>),
            }
        }
        .boxed()
    }

    #[inline]
    fn is_end(&self) -> bool {
        false
    }
}
