use http::*;
use utils::ToRegex;
use utils::RequestParamCollection;
use utils::RequestContinuation;
use utils::RequestContinuation::*;
use regex::Regex;
use parking_lot::RwLock;
use std::sync::Arc;

/// Struct representing the layering of middlewares in the server
pub struct MiddlewareStack {
    middlewares: Arc<RwLock<Vec<(MiddlewareRule, Box<Middleware>)>>>
}

impl MiddlewareStack {
    ///
    pub fn new() -> Self {
        MiddlewareStack {
            middlewares: Arc::new(RwLock::new(Vec::new())),
        }
    }

    ///
    pub fn resolve(&self, req: &SyncRequest, res: &mut SyncResponse, params: &mut RequestParamCollection) -> RequestContinuation {
        let path = req.uri().path();

        for &(ref rule, ref middleware) in self.middlewares.read().iter() {
            if rule.validate_path(path) {
                match middleware.resolve(req, res) {
                    Continue(Some(p)) => params.add(p),
                    Stop => return Stop,
                    _ => continue,
                }
            }
        }

        Continue(None)
    }

    /// Method to apply a new middleware onto the stack where the `include_path` vec are all path affected by the middleware,
    /// and `exclude_path` are exclusion amongst the included paths.
    pub fn apply<M: 'static + Middleware>(&self, m: M, include_path: Vec<&str>, exclude_path: Option<Vec<&str>>) {
        let rule = MiddlewareRule::new(include_path, exclude_path);
        let boxed_m = Box::new(m);

        self.middlewares.write().push((rule, boxed_m))
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
    /// and doesn't match any exclusion. Returning `RequestContinuation::Next` will allow the request to continue through the stack, and
    /// returning `RequestContinuation::None` will cease the request processing, returning as response the modified `res` param.
    fn resolve(&self, req: &SyncRequest, res: &mut SyncResponse) -> RequestContinuation;
}

struct MiddlewareRule {
    included_path: Vec<Regex>,
    excluded_path: Option<Vec<Regex>>,
}

impl MiddlewareRule {
    pub fn new<R: ToRegex>(include_path: Vec<R>, exclude_path: Option<Vec<R>>) -> Self {
        let mut included_path = Vec::new();
        for include in include_path.iter() {
            included_path.push(reg!(include));
        }

        let mut excluded_path: Option<Vec<Regex>> = Option::None;

        if let Some(excludes) = exclude_path {
            let mut excludes_vec = Vec::new();
            for exclude in excludes.iter() {
                excludes_vec.push(reg!(exclude));
            }

            excluded_path = Some(excludes_vec);
        }

        MiddlewareRule {
            included_path,
            excluded_path,
        }
    }

    pub fn validate_path(&self, path: &str) -> bool {
        let path_clone = path.clone();
        if self.included_path.iter().enumerate().find(
            move |&(_index, r)| {
                r.is_match(path_clone)
            }
        ).is_some() {
            if let Some(ref excluded_path) = self.excluded_path {
                return excluded_path.iter().enumerate().find(
                    move |&(_index, re)| {
                        re.is_match(path_clone)
                    }
                ).is_none();
            } else {
                return true;
            }
        }

        false
    }
}