use crate::{
    controller::{Controller, DynControllerHandler},
    error::SaphirError,
    handler::DynHandler,
    request::Request,
    responder::{DynResponder, Responder},
    response::Response,
    utils::{EndpointResolver, EndpointResolverResult},
};
use futures::future::BoxFuture;
use http::Method;
use hyper::Body;
use std::{collections::HashMap, sync::Arc};

pub struct Builder<Chain: RouterChain + Send + Unpin + 'static + Sync> {
    resolver: HashMap<String, EndpointResolver>,
    chain: Chain,
}

impl Default for Builder<RouterChainEnd> {
    fn default() -> Self {
        Self {
            resolver: Default::default(),
            chain: RouterChainEnd {
                handlers: Default::default(),
            },
        }
    }
}

impl<Controllers: 'static + RouterChain + Unpin + Send + Sync> Builder<Controllers> {
    pub fn route<H: 'static + DynHandler<Body> + Send + Sync>(
        mut self,
        route: &str,
        method: Method,
        handler: H,
    ) -> Self {
        let endpoint_id = if let Some(er) = self.resolver.get_mut(route) {
            er.add_method(method.clone());
            er.id()
        } else {
            let er = EndpointResolver::new(route, method.clone()).expect("Unable to construct endpoint resolver");
            let er_id = er.id();
            self.resolver.insert(route.to_string(), er);
            er_id
        };

        self.chain.add_handler(endpoint_id, method, Box::new(handler));

        self
    }

    pub fn controller<C: Controller + Send + Unpin + Sync>(
        mut self,
        controller: C,
    ) -> Builder<RouterChainLink<C, Controllers>> {
        let mut handlers = HashMap::new();
        for (method, subroute, handler) in controller.handlers() {
            let route = format!("{}{}", C::BASE_PATH, subroute);
            let endpoint_id = if let Some(er) = self.resolver.get_mut(&route) {
                er.add_method(method.clone());
                er.id()
            } else {
                let er = EndpointResolver::new(&route, method.clone()).expect("Unable to construct endpoint resolver");
                let er_id = er.id();
                self.resolver.insert(route, er);
                er_id
            };

            handlers.insert((endpoint_id, method), handler);
        }

        Builder {
            resolver: self.resolver,
            chain: RouterChainLink {
                controller,
                handlers,
                rest: self.chain,
            },
        }
    }

    /// Builds the router
    pub fn build(self) -> Router {
        let Builder {
            resolver,
            chain: controllers,
        } = self;

        Router {
            inner: Arc::new(RouterInner {
                resolvers: resolver.into_iter().map(|(_, e)| e).collect(),
                chain: Box::new(controllers),
            }),
        }
    }
}

struct RouterInner {
    resolvers: Vec<EndpointResolver>,
    chain: Box<dyn RouterChain + Send + Unpin + Sync>,
}

#[derive(Clone)]
pub struct Router {
    inner: Arc<RouterInner>,
}

impl Router {
    pub fn builder() -> Builder<RouterChainEnd> {
        Builder::default()
    }

    pub fn resolve(&self, req: &mut Request<Body>) -> Result<u64, u16> {
        let mut method_not_allowed = false;
        for endpoint_resolver in &self.inner.resolvers {
            match endpoint_resolver.resolve(req) {
                EndpointResolverResult::InvalidPath => continue,
                EndpointResolverResult::MethodNotAllowed => method_not_allowed = true,
                EndpointResolverResult::Match => return Ok(endpoint_resolver.id()),
            }
        }

        if method_not_allowed {
            Err(405)
        } else {
            Err(404)
        }
    }

    pub async fn handle(self, mut req: Request<Body>) -> Result<Response<Body>, SaphirError> {
        match self.resolve(&mut req) {
            Ok(id) => self.dispatch(id, req).await,
            Err(status) => status.respond(),
        }
    }

    pub async fn dispatch(&self, resolver_id: u64, req: Request<Body>) -> Result<Response<Body>, SaphirError> {
        if let Some(responder) = self.inner.chain.dispatch(resolver_id, req) {
            responder.await.dyn_respond()
        } else {
            404.respond()
        }
    }
}

pub trait RouterChain {
    fn dispatch(&self, resolver_id: u64, req: Request<Body>) -> Option<BoxFuture<'static, Box<dyn DynResponder>>>;
    fn add_handler(&mut self, endpoint_id: u64, method: Method, handler: Box<dyn DynHandler<Body> + Send + Sync>);
}

pub struct RouterChainEnd {
    handlers: HashMap<(u64, Method), Box<dyn DynHandler<Body> + Send + Sync>>,
}

impl RouterChain for RouterChainEnd {
    #[inline]
    fn dispatch(&self, resolver_id: u64, req: Request<Body>) -> Option<BoxFuture<'static, Box<dyn DynResponder>>> {
        if let Some(handler) = self.handlers.get(&(resolver_id, req.method().clone())) {
            Some(handler.dyn_handle(req))
        } else {
            None
        }
    }

    #[inline]
    fn add_handler(&mut self, endpoint_id: u64, method: Method, handler: Box<dyn DynHandler<Body> + Send + Sync>) {
        self.handlers.insert((endpoint_id, method), handler);
    }
}

pub struct RouterChainLink<C, Rest: RouterChain> {
    controller: C,
    handlers: HashMap<(u64, Method), Box<dyn DynControllerHandler<C, Body> + Send + Sync>>,
    rest: Rest,
}

impl<C, Rest: RouterChain> RouterChain for RouterChainLink<C, Rest> {
    #[inline]
    fn dispatch(&self, resolver_id: u64, req: Request<Body>) -> Option<BoxFuture<'static, Box<dyn DynResponder>>> {
        if let Some(handler) = self.handlers.get(&(resolver_id, req.method().clone())) {
            Some(handler.dyn_handle(&self.controller, req))
        } else {
            self.rest.dispatch(resolver_id, req)
        }
    }

    #[inline]
    fn add_handler(&mut self, endpoint_id: u64, method: Method, handler: Box<dyn DynHandler<Body> + Send + Sync>) {
        self.rest.add_handler(endpoint_id, method, handler);
    }
}
