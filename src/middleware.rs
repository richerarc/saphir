use std::sync::Arc;

use log::error;
use crate::http::*;
use crate::utils::{RequestContinuation, UriPathMatcher};
use crate::utils::RequestContinuation::*;

///
pub struct Builder {
    stack: Vec<(MiddlewareRule, Box<dyn Middleware>)>,
}

impl Builder {
    /// Creates a new MiddlewareStack Builder
    pub fn new() -> Self {
        Builder {
            stack: Vec::new()
        }
    }

    /// Method to apply a new middleware onto the stack where the `include_path` vec are all path affected by the middleware,
    /// and `exclude_path` are exclusion amongst the included paths.
    pub fn apply<M: 'static + Middleware>(mut self, m: M, include_path: Vec<&str>, exclude_path: Option<Vec<&str>>) -> Self {
        let rule = MiddlewareRule::new(include_path, exclude_path);
        let boxed_m = Box::new(m);

        self.stack.push((rule, boxed_m));

        self
    }

    /// Build the middleware stack
    pub fn build(self) -> MiddlewareStack {
        let Builder {
            stack,
        } = self;

        MiddlewareStack {
            middlewares: Arc::new(stack),
        }
    }
}

/// Struct representing the layering of middlewares in the server
pub struct MiddlewareStack {
    middlewares: Arc<Vec<(MiddlewareRule, Box<dyn Middleware>)>>
}

impl MiddlewareStack {
    ///
    pub fn new() -> Self {
        MiddlewareStack {
            middlewares: Arc::new(Vec::new())
        }
    }

    ///
    pub fn resolve(&self, req: &mut SyncRequest, res: &mut SyncResponse) -> RequestContinuation {
        for &(ref rule, ref middleware) in self.middlewares.iter() {
            if rule.validate_path(req.uri().path()) {
                if let Stop = middleware.resolve(req, res) {
                    return Stop;
                }
            }
        }

        Continue
    }
}

impl Clone for MiddlewareStack {
    fn clone(&self) -> Self {
        MiddlewareStack {
            middlewares: self.middlewares.clone(),
        }
    }
}

/// The trait a struct need to `impl` to be considered as a middleware
pub trait Middleware: Send + Sync {
    /// This method will be invoked if the request is targeting an included path, (as defined when "applying" the middleware to the stack)
    /// and doesn't match any exclusion. Returning `RequestContinuation::Continue` will allow the request to continue through the stack, and
    /// returning `RequestContinuation::Stop` will cease the request processing, returning as response the modified `res` param.
    fn resolve(&self, req: &mut SyncRequest, res: &mut SyncResponse) -> RequestContinuation;
}

struct MiddlewareRule {
    included_path: Vec<UriPathMatcher>,
    excluded_path: Option<Vec<UriPathMatcher>>,
}

impl MiddlewareRule {
    pub fn new(include_path: Vec<&str>, exclude_path: Option<Vec<&str>>) -> Self {
        MiddlewareRule {
            included_path: include_path.iter().filter_map(|p| UriPathMatcher::new(p).map_err(|e| error!("Unable to construct included middleware route: {}", e)).ok()).collect(),
            excluded_path: exclude_path.map(|ex| ex.iter().filter_map(|p| UriPathMatcher::new(p).map_err(|e| error!("Unable to construct excluded middleware route: {}", e)).ok()).collect()),
        }
    }

    pub fn validate_path(&self, path: &str) -> bool {
        if self.included_path.iter().any(|m_p| m_p.match_start(path)) {
            if let Some(ref excluded_path) = self.excluded_path {
                return !excluded_path.iter().any(|m_e_p| m_e_p.match_start(path));
            } else {
                return true;
            }
        }

        false
    }
}