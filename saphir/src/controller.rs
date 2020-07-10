//! Controllers are responsible for handling requests and returning responses to
//! the client.
//!
//! More specifically a Controller defines a list of endpoint (Handlers) that
//! handle a request and return a Future of a
//! [`Responder`](../responder/trait.Responder.html). The [`Responder`](../
//! responder/trait.Responder.html) is responsible for the [`Response`](../
//! response/struct.Response.html) being generated
//!
//! To create a controller, simply implement the
//! [Controller](trait.Controller.html) trait on a struct:
//! ```rust
//! use saphir::prelude::*;
//!
//! struct BasicController;
//!
//! impl Controller for BasicController {
//!     const BASE_PATH: &'static str = "/basic";
//!
//!     fn handlers(&self) -> Vec<ControllerEndpoint<Self>>
//!     where
//!         Self: Sized {
//!         EndpointsBuilder::new()
//!             .add(Method::GET, "/healthz", BasicController::healthz)
//!             .build()
//!     }
//! }
//!
//! impl BasicController {
//!     async fn healthz(&self, req: Request<Body>) -> impl Responder {200}
//! }
//! ```

use crate::{
    body::Body,
    guard::{Builder as GuardBuilder, GuardChain, GuardChainEnd},
    request::Request,
    responder::{DynResponder, Responder},
};
use futures::future::BoxFuture;
use futures_util::future::{Future, FutureExt};
use http::Method;

/// Type definition to represent a endpoint within a controller
pub type ControllerEndpoint<C> = (
    Option<&'static str>,
    Method,
    &'static str,
    Box<dyn DynControllerHandler<C, Body> + Send + Sync>,
    Box<dyn GuardChain>,
);

/// Trait that defines how a controller handles its requests
pub trait Controller {
    /// Defines the base path from which requests are to be handled by this
    /// controller
    const BASE_PATH: &'static str;

    /// Returns a list of [`ControllerEndpoint`](type.ControllerEndpoint.html)
    ///
    /// Each [`ControllerEndpoint`](type.ControllerEndpoint.html) is then added
    /// to the router, which will dispatch requests accordingly
    fn handlers(&self) -> Vec<ControllerEndpoint<Self>>
    where
        Self: Sized;
}

/// Trait that defines a handler within a controller.
/// This trait is not meant to be implemented manually as there is a blanket
/// implementation for Async Fns
pub trait ControllerHandler<C, B> {
    /// An instance of a [`Responder`](../responder/trait.Responder.html) being
    /// returned by the handler
    type Responder: Responder;
    ///
    type Future: Future<Output = Self::Responder>;

    /// Handle the request dispatched from the
    /// [`Router`](../router/struct.Router.html)
    fn handle(&self, controller: &'static C, req: Request<B>) -> Self::Future;
}

///
pub trait DynControllerHandler<C, B> {
    ///
    fn dyn_handle(&self, controller: &'static C, req: Request<B>) -> BoxFuture<'static, Box<dyn DynResponder + Send>>;
}

/// Builder to simplify returning a list of endpoint in the `handlers` method of
/// the controller trait
#[derive(Default)]
pub struct EndpointsBuilder<C: Controller> {
    handlers: Vec<ControllerEndpoint<C>>,
}

impl<C: Controller> EndpointsBuilder<C> {
    /// Create a new endpoint builder
    #[inline]
    pub fn new() -> Self {
        Self { handlers: Default::default() }
    }

    /// Add a endpoint the the builder
    ///
    /// ```rust
    /// # use saphir::prelude::*;
    ///
    /// # struct BasicController;
    ///
    /// # impl Controller for BasicController {
    /// #     const BASE_PATH: &'static str = "/basic";
    /// #
    /// #     fn handlers(&self) -> Vec<ControllerEndpoint<Self>>
    /// #     where
    /// #         Self: Sized {
    /// #         EndpointsBuilder::new()
    /// #             .add(Method::GET, "/healthz", BasicController::healthz)
    /// #             .build()
    /// #     }
    /// # }
    /// #
    /// impl BasicController {
    ///     async fn healthz(&self, req: Request<Body>) -> impl Responder {200}
    /// }
    ///
    /// let b: EndpointsBuilder<BasicController> = EndpointsBuilder::new().add(Method::GET, "/healthz", BasicController::healthz);
    /// ```
    #[inline]
    pub fn add<H>(mut self, method: Method, route: &'static str, handler: H) -> Self
    where
        H: 'static + DynControllerHandler<C, Body> + Send + Sync,
    {
        self.handlers.push((None, method, route, Box::new(handler), GuardBuilder::default().build()));
        self
    }

    /// Add a guarded endpoint the the builder
    #[inline]
    pub fn add_with_guards<H, F, Chain>(mut self, method: Method, route: &'static str, handler: H, guards: F) -> Self
    where
        H: 'static + DynControllerHandler<C, Body> + Send + Sync,
        F: FnOnce(GuardBuilder<GuardChainEnd>) -> GuardBuilder<Chain>,
        Chain: GuardChain + 'static,
    {
        self.handlers
            .push((None, method, route, Box::new(handler), guards(GuardBuilder::default()).build()));
        self
    }

    /// Add but with a handler name
    #[inline]
    pub fn add_with_name<H>(mut self, handler_name: &'static str, method: Method, route: &'static str, handler: H) -> Self
    where
        H: 'static + DynControllerHandler<C, Body> + Send + Sync,
    {
        self.handlers
            .push((Some(handler_name), method, route, Box::new(handler), GuardBuilder::default().build()));
        self
    }

    /// Add with guard but with a handler name
    #[inline]
    pub fn add_with_guards_and_name<H, F, Chain>(mut self, handler_name: &'static str, method: Method, route: &'static str, handler: H, guards: F) -> Self
    where
        H: 'static + DynControllerHandler<C, Body> + Send + Sync,
        F: FnOnce(GuardBuilder<GuardChainEnd>) -> GuardBuilder<Chain>,
        Chain: GuardChain + 'static,
    {
        self.handlers
            .push((Some(handler_name), method, route, Box::new(handler), guards(GuardBuilder::default()).build()));
        self
    }

    /// Finish the builder into a `Vec<ControllerEndpoint<C>>`
    #[inline]
    pub fn build(self) -> Vec<ControllerEndpoint<C>> {
        self.handlers
    }
}

impl<C, B, Fun, Fut, R> ControllerHandler<C, B> for Fun
where
    C: 'static,
    Fun: Fn(&'static C, Request<B>) -> Fut,
    Fut: 'static + Future<Output = R> + Send,
    R: Responder,
{
    type Future = Box<dyn Future<Output = Self::Responder> + Unpin + Send>;
    type Responder = R;

    #[inline]
    fn handle(&self, controller: &'static C, req: Request<B>) -> Self::Future {
        Box::new(Box::pin((*self)(controller, req)))
    }
}

impl<C, T, H, Fut, R> DynControllerHandler<C, T> for H
where
    R: 'static + Responder + Send,
    Fut: 'static + Future<Output = R> + Unpin + Send,
    H: ControllerHandler<C, T, Future = Fut, Responder = R>,
{
    #[inline]
    fn dyn_handle(&self, controller: &'static C, req: Request<T>) -> BoxFuture<'static, Box<dyn DynResponder + Send>> {
        self.handle(controller, req).map(|r| Box::new(Some(r)) as Box<dyn DynResponder + Send>).boxed()
    }
}
