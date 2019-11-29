use parking_lot::RwLock;

use crate::http::*;
use crate::utils::UriPathMatcher;
use crate::utils::RequestContinuation;

/// Trait representing a controller
pub trait Controller: Send + Sync {
    /// Method invoked if the request gets routed to this controller. Nothing will be processed after a controller `handling` a request.
    /// When returning from this function, the `res` param is the response returned to the client.
    fn handle(&self, req: &mut SyncRequest, res: &mut SyncResponse);

    /// Method used by the router to know were to route a request addressed at a controller
    fn base_path(&self) -> &str;
}

///
pub struct RequestGuardCollection {
    guards: Vec<Box<dyn RequestGuard>>
}

impl RequestGuardCollection {
    ///
    pub fn new() -> Self {
        RequestGuardCollection {
            guards: Vec::new(),
        }
    }

    ///
    pub fn add<G: 'static + RequestGuard>(&mut self, guard: G) {
        self.guards.push(Box::new(guard));
    }

    ///
    pub fn add_boxed(&mut self, guard: Box<dyn RequestGuard>) {
        self.guards.push(guard);
    }
}

impl<G: 'static + RequestGuard> From<G> for RequestGuardCollection {
    fn from(guard: G) -> Self {
        let mut reqg = RequestGuardCollection::new();
        reqg.add(guard);
        reqg
    }
}

impl<'a, G: 'static + RequestGuard + Clone> From<&'a [G]> for RequestGuardCollection {
    fn from(guards: &'a [G]) -> Self {
        let mut reqg = RequestGuardCollection::new();
        for guard in guards.to_vec() {
            reqg.add(guard);
        }
        reqg
    }
}

impl<G: 'static + RequestGuard> From<Vec<G>> for RequestGuardCollection {
    fn from(guards: Vec<G>) -> Self {
        let mut reqg = RequestGuardCollection::new();
        for guard in guards {
            reqg.add(guard);
        }
        reqg
    }
}

impl From<Vec<Box<dyn RequestGuard>>> for RequestGuardCollection {
    fn from(guards: Vec<Box<dyn RequestGuard>>) -> Self {
        let mut reqg = RequestGuardCollection::new();
        for guard in guards {
            reqg.add_boxed(guard);
        }
        reqg
    }
}

use ::std::slice::Iter;

impl<'a> IntoIterator for &'a RequestGuardCollection {
    type Item = &'a Box<dyn RequestGuard>;
    type IntoIter = Iter<'a, Box<dyn RequestGuard>>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        self.guards.iter()
    }
}

/// A trait to provide an other layer of validation before allowing a request into a controller
pub trait RequestGuard {
    ///
    fn validate(&self, req: &mut SyncRequest, res: &mut SyncResponse) -> RequestContinuation;
}

type DelegateFunction<T> = dyn Fn(&T, &SyncRequest, &mut SyncResponse);
type ControllerDelegate<T> = (Method, UriPathMatcher, Option<RequestGuardCollection>, Box<DelegateFunction<T>>);

/// Struct to delegate a request to a registered function matching booth a `method` and a `path`
pub struct ControllerDispatch<T> {
    /// The context sent with the request to the function
    delegate_context: T,
    /// List of delegates
    delegates: RwLock<Vec<ControllerDelegate<T>>>,
}

impl<T: Send + Sync> ControllerDispatch<T> {
    ///
    pub fn new(delegate_context: T) -> Self {
        ControllerDispatch {
            delegate_context,
            delegates: RwLock::new(Vec::new()),
        }
    }

    /// Add a delegate function to handle a particular request
    /// # Example
    ///
    /// ```rust,no_run
    /// use saphir::*;
    /// 
    /// let u8_context = 1;
    /// let dispatch = ControllerDispatch::new(u8_context);
    /// dispatch.add(Method::GET, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    /// ```
    pub fn add<F>(&self, method: Method, path: &str, delegate_func: F)
        where for<'r, 's, 't0> F: 'static + Fn(&'r T, &'s SyncRequest, &'t0 mut SyncResponse) {
        self.delegates.write().push((method, UriPathMatcher::new(path).expect("Unable to add delegate, path is invalid"), None, Box::new(delegate_func)));
    }

    /// Add a delegate function to handle a particular request
    /// # Example
    ///
    /// ```rust,no_run
    /// use saphir::*;
    /// 
    /// let u8_context = 1;
    /// let guard = BodyGuard;
    /// let dispatch = ControllerDispatch::new(u8_context);
    /// dispatch.add_with_guards(Method::GET, "^/test$", guard.into(), |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    /// ```
    pub fn add_with_guards<F>(&self, method: Method, path: &str, guards: RequestGuardCollection, delegate_func: F)
        where for<'r, 's, 't0> F: 'static + Fn(&'r T, &'s SyncRequest, &'t0 mut SyncResponse) {
        self.delegates.write().push((method, UriPathMatcher::new(path).expect("Unable to add delegate, path is invalid"), Some(guards), Box::new(delegate_func)));
    }

    ///
    pub fn dispatch(&self, req: &mut SyncRequest, res: &mut SyncResponse) {
        use std::iter::FromIterator;
        let delegates_list = self.delegates.read();
        let method = req.method();

        let retained_delegate = Vec::from_iter(delegates_list.iter().filter(|x| {
            x.0 == method
        }));

        if retained_delegate.len() == 0 {
            res.status(StatusCode::METHOD_NOT_ALLOWED);
            return;
        }

        for del in retained_delegate {
            let (_, ref u_p_m, ref op_guards, ref boxed_func) = del;

            if req.current_path_match_all(u_p_m) {
                if let Some(ref guards) = op_guards {
                    for guard in guards {
                        use crate::RequestContinuation::*;
                        if let Stop = guard.validate(req, res) {
                            return;
                        }
                    }
                }
                boxed_func(&self.delegate_context, req, res);
                return;
            }
        }

        res.status(StatusCode::NOT_FOUND);
    }
}

unsafe impl<T> Sync for ControllerDispatch<T> {}

unsafe impl<T> Send for ControllerDispatch<T> {}

/// An helper struct embedding a `ControllerDispatch`.
pub struct BasicController<C> {
    base_path: String,
    dispatch: ControllerDispatch<C>,
}

impl<C: Send + Sync> Controller for BasicController<C> {
    fn handle(&self, req: &mut SyncRequest, res: &mut SyncResponse) {
        self.dispatch.dispatch(req, res);
    }

    fn base_path(&self) -> &str {
        &self.base_path
    }
}

impl<C: Send + Sync> BasicController<C> {
    ///
    pub fn new(name: &str, controller_context: C) -> Self {
        BasicController {
            base_path: name.to_string(),
            dispatch: ControllerDispatch::new(controller_context),
        }
    }

    /// Add a delegate function to handle a particular request
    /// # Example
    ///
    /// ```rust,no_run
    /// use saphir::*;
    /// use saphir::controller::BasicController;
    /// 
    /// let u8_context = 1;
    /// let u8_controller = BasicController::new("/test", u8_context);
    /// u8_controller.add(Method::GET, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    /// ```
    pub fn add<F>(&self, method: Method, path: &str, delegate_func: F)
        where for<'r, 's, 't0> F: 'static + Fn(&'r C, &'s SyncRequest, &'t0 mut SyncResponse) {
        self.dispatch.add(method, path, delegate_func);
    }

    /// Add a delegate function to handle a particular request
    /// # Example
    ///
    /// ```rust,no_run
    /// use saphir::*;
    /// use saphir::controller::BasicController;
    /// 
    /// let u8_context = 1;
    /// let u8_controller = BasicController::new("/test", u8_context);
    /// u8_controller.add(Method::GET, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    /// ```
    pub fn add_with_guards<F>(&self, method: Method, path: &str, guards: RequestGuardCollection, delegate_func: F)
        where for<'r, 's, 't0> F: 'static + Fn(&'r C, &'s SyncRequest, &'t0 mut SyncResponse) {
        self.dispatch.add_with_guards(method, path, guards, delegate_func);
    }
}

/// RequestGuard ensuring that a request has a body
pub struct BodyGuard;

impl RequestGuard for BodyGuard {
    fn validate(&self, req: &mut SyncRequest, _res: &mut SyncResponse) -> RequestContinuation {
        if req.body().len() <= 0 {
            return RequestContinuation::Stop
        }

        RequestContinuation::Continue
    }
}

