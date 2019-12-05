use std::sync::Arc;

use crate::controller::Controller;
use crate::http::*;
use crate::utils::UriPathMatcher;

///
pub struct Builder {
    routes: Vec<(UriPathMatcher, Box<dyn Controller>)>
}

///
impl Builder {
    /// Create a new router builder
    pub fn new() -> Self {
        Builder {
            routes: Vec::new()
        }
    }

    /// Add a new controller with its route to the router
    /// # Example
    /// ```rust,no_run
    /// use saphir::*;
    /// use saphir::controller::BasicController;
    /// use saphir::router::Builder;
    /// 
    /// let u8_context = 1;
    /// let u8_controller = BasicController::new("/test", u8_context);
    /// u8_controller.add(Method::GET, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    ///
    /// let mut router = Builder::new()
    ///         .add(u8_controller)
    ///         .build();
    ///
    /// ```
    pub fn add<C: 'static + Controller>(mut self, controller: C) -> Self {
        let path_m = UriPathMatcher::new(controller.base_path()).expect("Unable to construct path");
        self.routes.push((path_m, Box::new(controller)));

        self
    }

    /// Add a new controller with its route to the router
    /// # Example
    /// ```rust,no_run
    /// use saphir::*;
    /// use saphir::controller::BasicController;
    /// use saphir::router::Builder;
    /// 
    /// let u8_context = 1;
    /// let u8_controller = BasicController::new("/test", u8_context);
    /// u8_controller.add(Method::GET, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    ///
    /// let mut router = Builder::new()
    ///         .add(u8_controller)
    ///         .build();
    ///
    /// ```
    pub fn route<C: 'static + Controller>(mut self, route: &str, controller: C) -> Self {
        let route_matcher = UriPathMatcher::new(route).and_then(|mut u| {u.append(controller.base_path())?; Ok(u)}).expect("Unable to construct path");
        self.routes.push((route_matcher, Box::new(controller)));

        self
    }

    /// Builds the router
    pub fn build(self) -> Router {
        let Builder {
            routes
        } = self;

        Router {
            routes: Arc::new(routes),
        }
    }
}

/// A Struct responsible of dispatching request towards controllers
pub struct Router {
    ///
    routes: Arc<Vec<(UriPathMatcher, Box<dyn Controller>)>>
}

impl Router {
    ///
    pub fn new() -> Self {
        Router {
            routes: Arc::new(Vec::new()),
        }
    }

    ///
    pub fn dispatch(&self, req: &mut SyncRequest, res: &mut SyncResponse) {
        let h: Option<(usize, &(UriPathMatcher, Box<dyn Controller>))> = self.routes.iter().enumerate().find(
            |&(_, &(ref re, _))| {
                req.current_path_match(re)
            }
        );

        if let Some((_, &(_, ref controller))) = h {
            controller.handle(req, res);
        } else {
            res.status(StatusCode::NOT_FOUND);
        }
    }
}

impl Clone for Router {
    fn clone(&self) -> Self {
        Router {
            routes: self.routes.clone(),
        }
    }
}