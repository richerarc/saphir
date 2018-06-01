use http::*;
use utils;
use std::sync::Arc;
use hyper::Error;
use middleware::MiddlewareStack;
use router::Router;
use futures::Future;

/// The http server
pub struct Server {
    http_service: HyperHttpService,
}

impl Server {
    /// Create a new server from a `Router` and a `MiddlewareStack`
    pub fn new(router: Router, middleware_stack: Option<MiddlewareStack>) -> Self {
        let http_service = HyperHttpService::new(router, middleware_stack);

        Server {
            http_service,
        }
    }

    /// This method will run untill the server terminates, `port` defines the listener port.
    pub fn run(&self, port: u16) -> Result<(), ::error::ServerError> {
        let addr = format!("0.0.0.0:{}", port).as_str().parse()?;
        let service_clone = self.http_service.clone();
        let server = Http::new().bind(&addr, move || Ok(service_clone.clone()))?;
        info!("Saphir successfully started and listening on port {}", port);
        server.run()?;
        Ok(())
    }
}

struct HyperHttpService {
    middleware_stack: Arc<MiddlewareStack>,
    router: Arc<Router>,
}

impl HyperHttpService {
    pub fn new(router: Router, middleware_stack: Option<MiddlewareStack>) -> Self {
        let middleware_stack = middleware_stack.unwrap_or_else(|| { MiddlewareStack::new() });

        HyperHttpService {
            middleware_stack: Arc::new(middleware_stack),
            router: Arc::new(router),
        }
    }
}

impl Service for HyperHttpService {
    type Request = Request;
    type Response = Response;
    type Error = Error;
    type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, req: <Self as Service>::Request) -> <Self as Service>::Future {
        use std::time::Instant;
        use server::utils::RequestContinuation::*;
        use futures::sync::oneshot::channel;
        use std::thread;

        let (tx, rx) = channel();
        let service_clone = self.clone();

        Box::new(req.load_body().and_then(move |request| {
            thread::spawn(move || {
                let req_iat = Instant::now();
                let mut response = Response::new();

                if let Next = service_clone.middleware_stack.resolve(&request, &mut response) {
                    service_clone.router.dispatch(&request, &mut response);
                }

                let elapsed = req_iat.elapsed();

                use ansi_term::Colour::*;

                let resp_status = response.status();
                let status_str = resp_status.to_string();

                let status = match resp_status.as_u16() {
                    0...199 => Cyan.paint(status_str),
                    200...299 => Green.paint(status_str),
                    400...599 => Red.paint(status_str),
                    _ => Yellow.paint(status_str),
                };


                info!("{} {} {} - {:.3}ms", request.method(), request.path(), status, (elapsed.as_secs() as f64
                    + elapsed.subsec_nanos() as f64 * 1e-9) * 1000 as f64);

                let _ = tx.send(response);
            });

            rx.map_err(|e| Error::from(::std::io::Error::new(::std::io::ErrorKind::Other, e)))
        }))
    }
}

impl Clone for HyperHttpService {
    fn clone(&self) -> Self {
        HyperHttpService {
            middleware_stack: self.middleware_stack.clone(),
            router: self.router.clone(),
        }
    }
}