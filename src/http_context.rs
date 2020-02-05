use crate::{request::Request, router::Router};

/// Context representing the relationship between a request and a response
/// This structure only appears inside Middleware since the act before and after the request
///
/// There is no guaranty the the request nor the response will be set at any given time, since they could be moved out by a badly implemented middleware
pub struct HttpContext<B> {
    /// The incoming request before it is handled by the router
    pub request: Request<B>,
    pub(crate) router: Router,
}

impl<B> HttpContext<B> {
    pub(crate) fn new(request: Request<B>, router: Router) -> Self {
        HttpContext { request, router }
    }
}
