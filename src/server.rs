use hyper::Server as HyperServer;
use hyper::service::service_fn;
use http::*;
use utils;
use error::ServerError;
use std::sync::Arc;
use middleware::MiddlewareStack;
use router::Router;
use futures::Future;

/// The http server
pub struct Server {
    middleware_stack: Arc<MiddlewareStack>,
    router: Arc<Router>,
}

impl Server {
    /// Create a new server from a `Router` and a `MiddlewareStack`
    pub fn new(router: Router, middleware_stack: Option<MiddlewareStack>) -> Self {
        let middleware_stack = middleware_stack.unwrap_or_else(|| { MiddlewareStack::new() });
        //let http_service = HyperHttpService::new(router, middleware_stack);

        Server {
            middleware_stack: Arc::new(middleware_stack),
            router: Arc::new(router),
        }
    }

    /// This method will run untill the server terminates, `uri` defines the listener uri.
    pub fn run(&self, uri: &str) -> Result<(), ::error::ServerError> {
        let url:Uri = uri.parse()?;

        let scheme = url.scheme_part().expect("The uri passed to launch the server doesn't contain a scheme.");
        if !scheme.eq(&::http_types::uri::Scheme::HTTP) {
            return Err(::error::ServerError::UnsupportedUriScheme);
        }

        let addr = url.authority_part().expect("The uri passed to launch the server doesn't contain an authority.").as_str().parse()?;
        let middleware_stack_clone = self.middleware_stack.clone();
        let router_clone = self.router.clone();
        let server = HyperServer::bind(&addr)
            .serve(move || {
                let middleware_stack_clone_svc = middleware_stack_clone.clone();
                let router_clone_svc = router_clone.clone();
                service_fn(move |req| {
                    http_service(req, &middleware_stack_clone_svc, &router_clone_svc)
                })
            })
            .map_err(|e| error!("server error: {}", e));
        ;
        info!("Saphir successfully started and listening on {}", addr);
        ::hyper::rt::run(server);
        Ok(())
    }
}

fn http_service(req: Request<Body>, middleware_stack: &Arc<MiddlewareStack>, router: &Arc<Router>)
                -> Box<Future<Item=Response<Body>, Error=ServerError> + Send> {
    use std::time::Instant;
    use server::utils::RequestContinuation::*;
    use futures::sync::oneshot::channel;
    use std::thread;

    let (tx, rx) = channel();
    let middleware_stack_c = middleware_stack.clone();
    let router_c = router.clone();

    Box::new(req.load_body().map_err(|e| ServerError::from(e)).and_then(move |request| {
        thread::spawn(move || {
            let req_iat = Instant::now();
            let mut response = SyncResponse::new();

            if let Next = middleware_stack_c.resolve(&request, &mut response) {
                router_c.dispatch(&request, &mut response);
            }

            let final_res = response.build_response().unwrap_or_else(|_| {
                let empty: &[u8] = b"";
                Response::new(empty.into())
            });

            let resp_status = final_res.status();

            let _ = tx.send(final_res);

            let elapsed = req_iat.elapsed();

            use ansi_term::Colour::*;

            let status_str = resp_status.to_string();

            let status = match resp_status.as_u16() {
                0...199 => Cyan.paint(status_str),
                200...299 => Green.paint(status_str),
                400...599 => Red.paint(status_str),
                _ => Yellow.paint(status_str),
            };

            info!("{} {} {} - {:.3}ms", request.method(), request.uri().path(), status, (elapsed.as_secs() as f64
                + elapsed.subsec_nanos() as f64 * 1e-9) * 1000 as f64);
        });

        rx.map_err(|e| ServerError::from(e))
    }))
}
