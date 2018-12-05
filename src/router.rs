use http::*;
use utils::ToRegex;
use regex::Regex;

use controller::Controller;
use std::sync::Arc;

///
pub struct Builder {
    routes: Vec<(Regex, Box<Controller>)>
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
    /// let u8_context = 1;
    /// let u8_controller = BasicController::new(u8_context);
    /// u8_controller.add(Method::Get, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    ///
    /// let mut router = Router::new();
    /// router.add("/test", u8_controller);
    ///
    /// ```
    pub fn add<C: 'static + Controller>(mut self, controller: C) -> Self {
        self.routes.push((reg!(controller.base_path()), Box::new(controller)));

        self
    }

    /// Add a new controller with its route to the router
    /// # Example
    /// ```rust,no_run
    /// let u8_context = 1;
    /// let u8_controller = BasicController::new(u8_context);
    /// u8_controller.add(Method::Get, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    ///
    /// let mut router = Router::new();
    /// router.add("/test", u8_controller);
    ///
    /// ```
    pub fn route<C: 'static + Controller, R: ToRegex>(mut self, route: R, controller: C) -> Self {
        let mut cont_base_path = controller.base_path().to_string();
        if cont_base_path.starts_with('^') {
            cont_base_path.remove(0);
        }
        let mut route_str = route.as_str().to_string();
        route_str.push_str(&cont_base_path);
        self.routes.push((reg!(route_str), Box::new(controller)));

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
    routes: Arc<Vec<(Regex, Box<Controller>)>>
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
        let h: Option<(usize, &(Regex, Box<Controller>))> = self.routes.iter().enumerate().find(
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