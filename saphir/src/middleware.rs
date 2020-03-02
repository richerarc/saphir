//! A middleware is an object being called before the request is processed by the router, allowing
//! to continue or stop the processing of a given request by calling / omitting next.
//!
//! ```ignore
//!        chain.next(_)       chain.next(_)
//!              |               |
//!              |               |
//! +---------+  |  +---------+  |  +---------+
//! |         +--+->+         +--+->+         |
//! | Middle  |     | Middle  |     |  Router |
//! | ware1   |     | ware2   |     |         |
//! |         +<----+         +<----+         |
//! +---------+     +---------+     +---------+
//! ```
//!
//! Once the request is fully processed by the stack or whenever a middleware returns an error,
//! the request is terminated and the response is generated, the response then becomes available
//! to the middleware
//!
//! A middleware is defined as the following:
//!
//! ```rust
//!# use saphir::prelude::*;
//!# struct CustomData;
//!#
//! async fn example_middleware(data: &CustomData, ctx: HttpContext<Body>, chain: &dyn MiddlewareChain) -> Result<Response<Body>, SaphirError> {
//!     // Do work before the request is handled by the router
//!
//!     let res = chain.next(ctx).await?;
//!
//!     // Do work with the response
//!
//!     Ok(res)
//! }
//! ```
//!
//! *SAFETY NOTICE*
//!
//! Inside the middleware chain we need a little bit of unsafe code. This code allow us to consider
//! the futures generated by the middlewares as 'static. This is considered safe since all
//! middleware data lives within the server stack which has a static lifetime over your application.
//! We plan to remove this unsafe code as soon as we find another solution to it.

use crate::{body::Body, error::SaphirError, http_context::HttpContext, response::Response, utils::UriPathMatcher};
use futures::{future::BoxFuture, FutureExt};
use futures_util::future::Future;

/// Auto trait implementation over every function that match the definition of a middleware.
pub trait MiddlewareHandler<Data> {
    fn next(&self, data: &Data, ctx: HttpContext<Body>, chain: &dyn MiddlewareChain) -> BoxFuture<'static, Result<Response<Body>, SaphirError>>;
}

impl<Data, Fun, Fut> MiddlewareHandler<Data> for Fun
where
    Data: 'static,
    Fun: Fn(&'static Data, HttpContext<Body>, &'static dyn MiddlewareChain) -> Fut,
    Fut: 'static + Future<Output = Result<Response<Body>, SaphirError>> + Send,
{
    #[inline]
    fn next(&self, data: &Data, ctx: HttpContext<Body>, chain: &dyn MiddlewareChain) -> BoxFuture<'static, Result<Response<Body>, SaphirError>> {
        // # SAFETY #
        // The middleware chain and data are initialized in static memory when calling run on Server.
        let (data, chain) = unsafe {
            (
                std::mem::transmute::<&'_ Data, &'static Data>(data),
                std::mem::transmute::<&'_ dyn MiddlewareChain, &'static dyn MiddlewareChain>(chain),
            )
        };
        (*self)(data, ctx, chain).boxed()
    }
}

/// Builder to apply middleware onto the http stack
pub struct Builder<Chain: MiddlewareChain> {
    chain: Chain,
}

impl Default for Builder<MiddleChainEnd> {
    fn default() -> Self {
        Self { chain: MiddleChainEnd }
    }
}

impl<Chain: MiddlewareChain + 'static> Builder<Chain> {
    /// Method to apply a new middleware onto the stack where the `include_path` vec are all path affected by the middleware,
    /// and `exclude_path` are exclusion amongst the included paths.
    ///
    /// ```rust
    /// use saphir::middleware::Builder as MBuilder;
    ///# use saphir::prelude::*;
    ///
    ///# async fn log_middleware(
    ///#     prefix: &String,
    ///#     ctx: HttpContext<Body>,
    ///#     chain: &dyn MiddlewareChain,
    ///# ) -> Result<Response<Body>, SaphirError> {
    ///#     println!("{} | new request on path: {}", prefix, ctx.request.uri().path());
    ///#     let res = chain.next(ctx).await?;
    ///#     println!("{} | new response with status: {}", prefix, res.status());
    ///#     Ok(res)
    ///# }
    ///#
    /// let builder = MBuilder::default().apply(log_middleware, "LOG".to_string(), vec!["/"], None);
    /// ```
    pub fn apply<'a, Data, Handler, E>(
        self,
        handler: Handler,
        data: Data,
        include_path: Vec<&str>,
        exclude_path: E,
    ) -> Builder<MiddlewareChainLink<Data, Handler, Chain>>
    where
        Data: Sync + Send,
        Handler: 'static + MiddlewareHandler<Data> + Sync + Send,
        E: Into<Option<Vec<&'a str>>>,
    {
        let rule = Rule::new(include_path, exclude_path.into());
        Builder {
            chain: MiddlewareChainLink {
                rule,
                data,
                handler,
                rest: self.chain,
            },
        }
    }

    pub(crate) fn build(self) -> Box<dyn MiddlewareChain> {
        Box::new(self.chain)
    }
}

pub(crate) struct Rule {
    included_path: Vec<UriPathMatcher>,
    excluded_path: Option<Vec<UriPathMatcher>>,
}

impl Rule {
    #[doc(hidden)]
    pub fn new(include_path: Vec<&str>, exclude_path: Option<Vec<&str>>) -> Self {
        Rule {
            included_path: include_path
                .iter()
                .filter_map(|p| {
                    UriPathMatcher::new(p)
                        .map_err(|e| error!("Unable to construct included middleware route: {}", e))
                        .ok()
                })
                .collect(),
            excluded_path: exclude_path.map(|ex| {
                ex.iter()
                    .filter_map(|p| {
                        UriPathMatcher::new(p)
                            .map_err(|e| error!("Unable to construct excluded middleware route: {}", e))
                            .ok()
                    })
                    .collect()
            }),
        }
    }

    #[doc(hidden)]
    pub fn validate_path(&self, path: &str) -> bool {
        if self.included_path.iter().any(|m_p| m_p.match_non_exhaustive(path)) {
            if let Some(ref excluded_path) = self.excluded_path {
                return !excluded_path.iter().any(|m_e_p| m_e_p.match_non_exhaustive(path));
            } else {
                return true;
            }
        }

        false
    }
}

#[doc(hidden)]
pub trait MiddlewareChain: Sync + Send {
    fn next(&self, ctx: HttpContext<Body>) -> BoxFuture<'static, Result<Response<Body>, SaphirError>>;
}

#[doc(hidden)]
pub struct MiddleChainEnd;

impl MiddlewareChain for MiddleChainEnd {
    #[doc(hidden)]
    #[inline]
    fn next(&self, ctx: HttpContext<Body>) -> BoxFuture<'static, Result<Response<Body>, SaphirError>> {
        async {
            let (router, request) = (ctx.router, ctx.request);
            router.handle(request).await
        }
        .boxed()
    }
}

#[doc(hidden)]
pub struct MiddlewareChainLink<Data, Handler: MiddlewareHandler<Data>, Rest: MiddlewareChain> {
    rule: Rule,
    data: Data,
    handler: Handler,
    rest: Rest,
}

#[doc(hidden)]
impl<Data, Handler, Rest> MiddlewareChain for MiddlewareChainLink<Data, Handler, Rest>
where
    Data: Sync + Send,
    Handler: MiddlewareHandler<Data> + Sync + Send,
    Rest: MiddlewareChain,
{
    #[doc(hidden)]
    #[inline]
    fn next(&self, ctx: HttpContext<Body>) -> BoxFuture<'static, Result<Response<Body>, SaphirError>> {
        if self.rule.validate_path(ctx.request.uri().path()) {
            self.handler.next(&self.data, ctx, &self.rest)
        } else {
            self.rest.next(ctx)
        }
    }
}
