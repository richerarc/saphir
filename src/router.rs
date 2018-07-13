use http::*;
use utils::ToRegex;
use regex::Regex;

use controller::Controller;
use parking_lot::RwLock;
use std::sync::Arc;

/// A Struct responsible of dispatching request towards controllers
pub struct Router {
    ///
    routes: Arc<RwLock<Vec<(Regex, Box<Controller>)>>>
}

impl Router {
    ///
    pub fn new() -> Self {
        Router {
            routes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    ///
    pub fn dispatch(&self, req: &mut SyncRequest, res: &mut SyncResponse) {
        let routes = self.routes.read();
        let h: Option<(usize, &(Regex, Box<Controller>))> = routes.iter().enumerate().find(
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
    pub fn add<C: 'static + Controller>(&self, controller: C) {
        self.routes.write().push((reg!(controller.base_path()), Box::new(controller)))
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
    pub fn route<C: 'static + Controller, R: ToRegex>(&self, route: R, controller: C) {
        let mut cont_base_path = controller.base_path().to_string();
        if cont_base_path.starts_with('^') {
            cont_base_path.remove(0);
        }
        let mut route_str = route.as_str().to_string();
        route_str.push_str(&cont_base_path);
        self.routes.write().push((reg!(route_str), Box::new(controller)))
    }
}

impl Clone for Router {
    fn clone(&self) -> Self {
        Router {
            routes: self.routes.clone(),
        }
    }
}