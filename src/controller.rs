use http::*;
use utils::ToRegex;
use regex::Regex;
use std::sync::RwLock;

pub trait Controller: Send + Sync {
    fn handle(&self, req: &SyncRequest, res: &mut Response<Body>);
}

type DelegateFunction<T> = Fn(&T, &SyncRequest, &mut Response<Body>);
type ControllerDelegate<T> = (Method, Regex, Box<DelegateFunction<T>>);

pub struct ControllerDispatch<T> {
    delegate_context: T,
    delegates: RwLock<Vec<ControllerDelegate<T>>>
}

impl<T: Send + Sync> ControllerDispatch<T> {
    pub fn new(delegate_context: T) -> Self {
        ControllerDispatch {
            delegate_context,
            delegates: RwLock::new(Vec::new()),
        }
    }

    pub fn add<F, R: ToRegex>(&self, method: Method, path: R, delegate_func: F)
        where for<'r, 's, 't0> F: 'static + Fn(&'r T, &'s SyncRequest, &'t0 mut Response<Body>) {
        self.delegates.write().unwrap().push((method, reg!(path), Box::new(delegate_func)));
    }

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

pub struct BasicController<C> {
    dispatch: ControllerDispatch<C>
}

impl<C: Send + Sync> Controller for BasicController<C> {
    fn handle(&self, req: &SyncRequest, res: &mut Response<Body>) {
        self.dispatch.dispatch(req, res);
    }
}

impl<C: Send + Sync> BasicController<C> {
    pub fn new(controller_context: C) -> Self {
        BasicController {
            dispatch: ControllerDispatch::new(controller_context),
        }
    }

    pub fn add<F, R: ToRegex>(&self, method: Method, path: R, delegate_func: F)
        where for<'r, 's, 't0> F: 'static + Fn(&'r C, &'s SyncRequest, &'t0 mut Response<Body>) {
        self.dispatch.add(method, path, delegate_func);
    }
}