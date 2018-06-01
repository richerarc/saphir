use http::*;
use utils::ToRegex;
use utils::RequestContinuation;
use utils::RequestContinuation::*;
use regex::Regex;
use std::sync::RwLock;

pub struct MiddlewareStack {
    middlewares: RwLock<Vec<(MiddlewareRule, Box<Middleware>)>>
}

impl MiddlewareStack {
    pub fn new() -> Self {
        MiddlewareStack {
            middlewares: RwLock::new(Vec::new()),
        }
    }

    pub fn resolve(&self, req: &SyncRequest, res: &mut Response<Body>) -> RequestContinuation {
        let path = req.path();

        for &(ref rule, ref middleware) in self.middlewares.read().unwrap().iter() {
            if rule.validate_path(path) {
                if let None = middleware.resolve(req, res) {
                    return None;
                }
            }
        }

        Next
    }

    pub fn apply<M: 'static + Middleware>(&mut self, m: M, include_path: Vec<&str>, exclude_path: Option<Vec<&str>>) {
        let rule = MiddlewareRule::new(include_path, exclude_path);
        let boxed_m = Box::new(m);

        self.middlewares.write().unwrap().push((rule, boxed_m))
    }
}

pub trait Middleware: Send + Sync {
    fn resolve(&self, req: &SyncRequest, res: &mut Response<Body>) -> RequestContinuation;
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

        let mut excluded_path : Option<Vec<Regex>> = Option::None;

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
            move | &(_index, r) | {
                r.is_match(path_clone)
            }
        ).is_some() {

            if let Some(ref excluded_path) = self.excluded_path {
                return excluded_path.iter().enumerate().find(
                    move | &(_index, re) | {
                        re.is_match(path_clone)
                    }
                ).is_none()
            } else {
                return true;
            }
        }

        false
    }
}