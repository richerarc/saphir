use http::*;
use utils::ToRegex;
use regex::Regex;
use std::sync::RwLock;

/// Trait representing a controller
pub trait Controller: Send + Sync {
    /// Method invoked if the request gets routed to this controller. Nothing will be processed after a controller `handling` a request.
    /// When returning from this function, the `res` param is the response returned to the client.
    fn handle(&self, req: &SyncRequest, res: &mut Response<Body>);
}

type DelegateFunction<T> = Fn(&T, &SyncRequest, &mut Response<Body>);
type ControllerDelegate<T> = (Method, Regex, Box<DelegateFunction<T>>);

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
    /// let u8_context = 1;
    /// let dispatch = ControllerDispatch::new(u8_context);
    /// dispatch.add(Method::Get, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    /// ```
    pub fn add<F, R: ToRegex>(&self, method: Method, path: R, delegate_func: F)
        where for<'r, 's, 't0> F: 'static + Fn(&'r T, &'s SyncRequest, &'t0 mut Response<Body>) {
        self.delegates.write().unwrap().push((method, reg!(path), Box::new(delegate_func)));
    }

    ///
    pub fn dispatch(&self, req: &SyncRequest, res: &mut Response<Body>) {
        use std::iter::FromIterator;
        let delegates_list = self.delegates.read().unwrap();
        let method = req.method().clone();

        let retained_delegate = Vec::from_iter(delegates_list.iter().filter(move |x| {
            x.0 == method
        }));

        if retained_delegate.len() == 0 {
            res.set_status(StatusCode::MethodNotAllowed);
            return;
        }

        for del in retained_delegate {
            let (_, ref reg, ref boxed_func) = del;

            if reg.is_match(req.uri().path()) {
                boxed_func(&self.delegate_context, req, res);
                return;
            }
        }

        res.set_status(StatusCode::BadRequest);
    }
}

unsafe impl<T> Sync for ControllerDispatch<T> {}

unsafe impl<T> Send for ControllerDispatch<T> {}

/// An helper struct embedding a `ControllerDispatch`.
pub struct BasicController<C> {
    dispatch: ControllerDispatch<C>
}

impl<C: Send + Sync> Controller for BasicController<C> {
    fn handle(&self, req: &SyncRequest, res: &mut Response<Body>) {
        self.dispatch.dispatch(req, res);
    }
}

impl<C: Send + Sync> BasicController<C> {
    ///
    pub fn new(controller_context: C) -> Self {
        BasicController {
            dispatch: ControllerDispatch::new(controller_context),
        }
    }

    /// Add a delegate function to handle a particular request
    /// # Example
    ///
    /// ```rust,no_run
    /// let u8_context = 1;
    /// let u8_controller = BasicController::new(u8_context);
    /// u8_controller.add(Method::Get, "^/test$", |ctx, req, res| { println!("this will handle Get request done on <your_host>/test")});
    /// ```
    pub fn add<F, R: ToRegex>(&self, method: Method, path: R, delegate_func: F)
        where for<'r, 's, 't0> F: 'static + Fn(&'r C, &'s SyncRequest, &'t0 mut Response<Body>) {
        self.dispatch.add(method, path, delegate_func);
    }
}