use http::*;
use utils::ToRegex;
use regex::Regex;

use controller::Controller;

/// A Struct responsible of dispatching request towards controllers
pub struct Router {
    ///
    routes: Vec<(Regex, Box<Controller>)>
}

impl Router {
    ///
    pub fn new() -> Self {
        Router {
            routes: Vec::new(),
        }
    }

    ///
    pub fn dispatch(&self, req: &SyncRequest, res: &mut Response<Body>) {
        let request_path = req.path();
        let h: Option<(usize, &(Regex, Box<Controller>))> = self.routes.iter().enumerate().find(
            move |&(_, &(ref re, _))| {
                re.is_match(request_path)
            }
        );

        if let Some((_, &(_, ref controller))) = h {
            controller.handle(req, res);
        } else {
            res.set_status(StatusCode::NotFound);
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
    pub fn add<C: 'static + Controller, R: ToRegex>(&mut self, route: R, controller: C) {
        self.routes.push((reg!(route), Box::new(controller)))
    }
}