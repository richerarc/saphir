//! A middleware is an object being called before the request is processed by
//! the router, allowing to continue or stop the processing of a given request
//! by calling / omitting next.
//!
//! ```text
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
//! Once the request is fully processed by the stack or whenever a middleware
//! returns an error, the request is terminated and the response is generated,
//! the response then becomes available to the middleware
//!
//! A middleware is defined as the following:
//!
//! ```rust
//! # use saphir::prelude::*;
//! # struct CustomData;
//! #
//! async fn example_middleware(data: &CustomData, ctx: HttpContext, chain: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError> {
//!     // Do work before the request is handled by the router
//!
//!     let ctx = chain.next(ctx).await?;
//!
//!     // Do work with the response
//!
//!     Ok(ctx)
//! }
//! ```
//!
//! *SAFETY NOTICE*
//!
//! Inside the middleware chain we need a little bit of unsafe code. This code
//! allow us to consider the futures generated by the middlewares as 'static.
//! This is considered safe since all middleware data lives within the server
//! stack which has a static lifetime over your application. We plan to remove
//! this unsafe code as soon as we find another solution to it.

use crate::{
    error::{InternalError, SaphirError},
    http_context::HttpContext,
    utils::UriPathMatcher,
};
use futures::{future::BoxFuture, FutureExt};
use futures_util::future::Future;

pub trait Middleware {
    fn next(&'static self, ctx: HttpContext, chain: &'static dyn MiddlewareChain) -> BoxFuture<'static, Result<HttpContext, SaphirError>>;
}

impl<Fun, Fut> Middleware for Fun
where
    Fun: Fn(HttpContext, &'static dyn MiddlewareChain) -> Fut,
    Fut: 'static + Future<Output = Result<HttpContext, SaphirError>> + Send,
{
    #[inline]
    fn next(&'static self, ctx: HttpContext, chain: &'static dyn MiddlewareChain) -> BoxFuture<'static, Result<HttpContext, SaphirError>> {
        (*self)(ctx, chain).boxed()
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
    /// Method to apply a new middleware onto the stack where the `include_path`
    /// vec are all path affected by the middleware, and `exclude_path` are
    /// exclusion amongst the included paths.
    ///
    /// ```rust
    /// use saphir::middleware::Builder as MBuilder;
    /// # use saphir::prelude::*;
    ///
    /// # async fn log_middleware(
    /// #     ctx: HttpContext,
    /// #     chain: &dyn MiddlewareChain,
    /// # ) -> Result<HttpContext, SaphirError> {
    /// #     println!("new request on path: {}", ctx.state.request_unchecked().uri().path());
    /// #     let ctx = chain.next(ctx).await?;
    /// #     println!("new response with status: {}", ctx.state.response_unchecked().status());
    /// #     Ok(ctx)
    /// # }
    /// #
    /// let builder = MBuilder::default().apply(log_middleware, vec!["/"], None);
    /// ```
    pub fn apply<'a, Mid, E>(self, mid: Mid, include_path: Vec<&str>, exclude_path: E) -> Builder<MiddlewareChainLink<Mid, Chain>>
    where
        Mid: 'static + Middleware + Sync + Send,
        E: Into<Option<Vec<&'a str>>>,
    {
        let rule = Rule::new(include_path, exclude_path.into());
        Builder {
            chain: MiddlewareChainLink { rule, mid, rest: self.chain },
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
    fn next(&self, ctx: HttpContext) -> BoxFuture<'static, Result<HttpContext, SaphirError>>;
}

#[doc(hidden)]
pub struct MiddleChainEnd;

impl MiddlewareChain for MiddleChainEnd {
    #[doc(hidden)]
    #[allow(unused_mut)]
    #[inline]
    fn next(&self, mut ctx: HttpContext) -> BoxFuture<'static, Result<HttpContext, SaphirError>> {
        async {
            let router = ctx.router.take().ok_or(SaphirError::Internal(InternalError::Stack))?;
            router.dispatch(ctx).await
        }
        .boxed()
    }
}

#[doc(hidden)]
pub struct MiddlewareChainLink<Mid: Middleware, Rest: MiddlewareChain> {
    rule: Rule,
    mid: Mid,
    rest: Rest,
}

#[doc(hidden)]
impl<Mid, Rest> MiddlewareChain for MiddlewareChainLink<Mid, Rest>
where
    Mid: Middleware + Sync + Send + 'static,
    Rest: MiddlewareChain,
{
    #[doc(hidden)]
    #[allow(clippy::transmute_ptr_to_ptr)]
    #[inline]
    fn next(&self, ctx: HttpContext) -> BoxFuture<'static, Result<HttpContext, SaphirError>> {
        // # SAFETY #
        // The middleware chain and data are initialized in static memory when calling
        // run on Server.
        let (mid, rest) = unsafe {
            (
                std::mem::transmute::<&'_ Mid, &'static Mid>(&self.mid),
                std::mem::transmute::<&'_ dyn MiddlewareChain, &'static dyn MiddlewareChain>(&self.rest),
            )
        };

        if ctx.state.request().filter(|req| self.rule.validate_path(req.uri().path())).is_some() {
            mid.next(ctx, rest)
        } else {
            rest.next(ctx)
        }
    }
}
