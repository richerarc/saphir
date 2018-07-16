use hyper::service::service_fn;
use http::*;
use utils;
use error::ServerError;
use middleware::MiddlewareStack;
use router::Router;
use futures::Future;
use std::cell::RefCell;

/// A struct representing listener configuration
pub struct ListenerConfig {
    uri: Option<String>,
    cert_path: Option<String>,
    key_path: Option<String>,
}

impl ListenerConfig {
    /// Set the listener uri (supported format is <scheme>://<interface>:<port>)
    pub fn set_uri(&mut self, uri: &str) {
        self.uri = Some(uri.to_string())
    }

    /// Set the listener ssl certificates files. The cert needs to be PEM encoded
    /// while the key can be either RSA or PKCS8
    pub fn set_ssl_certificates(&mut self, cert_path: &str, key_path: &str) {
        self.cert_path = Some(cert_path.to_string());
        self.key_path = Some(key_path.to_string());
    }
}

#[doc(hidden)]
mod listener_config_ext {
    use super::*;

    impl ListenerConfig {
        #[doc(hidden)]
        pub fn new() -> Self {
            ListenerConfig {
                uri: None,
                cert_path: None,
                key_path: None,
            }
        }

        #[doc(hidden)]
        pub fn uri(&self) -> Option<String> {
            self.uri.clone()
        }

        #[doc(hidden)]
        pub fn ssl_files_path(&self) -> (Option<String>, Option<String>) {
            (self.cert_path.clone(), self.key_path.clone())
        }
    }
}

/// The http server
pub struct Server {
    middleware_stack: MiddlewareStack,
    router: Router,
    listener_config: RefCell<ListenerConfig>,
}

impl Server {
    /// Create a new http server
    pub fn new() -> Self {
        Server {
            middleware_stack: MiddlewareStack::new(),
            router: Router::new(),
            listener_config: RefCell::new(ListenerConfig::new()),
        }
    }

    /// Allows to set the listener uri (supported format is <scheme>://<interface>:<port>)
    pub fn set_uri(&self, uri: &str) -> &Self {
        self.listener_config.borrow_mut().set_uri(uri);
        &self
    }

    /// This method will call the provided closure with a mutable ref of the router
    /// Once into the closure it is possible to add controllers to the router.
    pub fn configure_router<F>(&self, config_fn: F) -> &Self where F: Fn(&Router) {
        config_fn(&self.router);
        &self
    }

    /// This method will call the provided closure with a mutable ref of the middleware_stack
    /// Once into the closure it is possible to add middlewares to the middleware_stack.
    pub fn configure_middlewares<F>(&self, config_fn: F) -> &Self where F: Fn(&MiddlewareStack) {
        config_fn(&self.middleware_stack);
        &self
    }

    /// This method will call the provided closure with a mutable ref of the listener configurations
    /// Once into the closure it is possible to set the uri and ssl file paths.
    pub fn configure_listener<F>(&self, config_fn: F) -> &Self where F: Fn(&mut ListenerConfig) {
        config_fn(&mut *self.listener_config.borrow_mut());
        &self
    }

    /// This method will run untill the server terminates.
    pub fn run(&self) -> Result<(), ::error::ServerError> {
        let uri: Uri = self.listener_config.borrow().uri()
            .expect("Fatal Error: No uri provided.\n You can fix this error by calling Server::set_uri or by configuring the listener with Server::configure_listener")
            .parse()?;

        let scheme = uri.scheme_part().expect("Fatal Error: The uri passed to launch the server doesn't contain a scheme.");
        let addr = uri.authority_part().expect("The uri passed to launch the server doesn't contain an authority.").as_str().parse()?;

        let listener = ::tokio::net::TcpListener::bind(&addr)?;

        let middleware_stack_clone = self.middleware_stack.clone();
        let router_clone = self.router.clone();

        if scheme.eq(&::http_types::uri::Scheme::HTTP) {
            if let (Some(_), _) = self.listener_config.borrow().ssl_files_path() {
                warn!("SSL certificate paths are provided but the listener was configured to use unsecured HTTP, try changing the uri scheme for https");
            }
            let server = ::hyper::server::Builder::new(listener.incoming(), ::hyper::server::conn::Http::new())
                .serve(move || {
                    let middleware_stack_clone_svc = middleware_stack_clone.clone();
                    let router_clone_svc = router_clone.clone();
                    service_fn(move |req| {
                        http_service(req, &middleware_stack_clone_svc, &router_clone_svc)
                    })
                })
                .map_err(|e| error!("server error: {}", e));

            info!("Saphir successfully started and listening on {}", uri);
            ::hyper::rt::run(server);
        } else if scheme.eq(&::http_types::uri::Scheme::HTTPS) {
            #[cfg(feature = "https")]
            {
                if let (Some(cert_path), Some(key_path)) = self.listener_config.borrow().ssl_files_path() {
                    use std::sync::Arc;
                    use futures::Stream;
                    use server::ssl_loading_utils::*;
                    use tokio_rustls::ServerConfigExt;

                    let certs = load_certs(cert_path.as_ref());
                    let key = load_private_key(key_path.as_ref());
                    let mut cfg = ::rustls::ServerConfig::new(::rustls::NoClientAuth::new());
                    cfg.set_single_cert(certs, key);
                    let arc_config = Arc::new(cfg);

                    let inc = listener.incoming().and_then(move |stream| {
                        arc_config.clone().accept_async(stream)
                    });

                    let server = ::hyper::server::Builder::new(inc, ::hyper::server::conn::Http::new())
                        .serve(move || {
                            let middleware_stack_clone_svc = middleware_stack_clone.clone();
                            let router_clone_svc = router_clone.clone();
                            service_fn(move |req| {
                                http_service(req, &middleware_stack_clone_svc, &router_clone_svc)
                            })
                        })
                        .map_err(|e| error!("server error: {}", e));

                    info!("Saphir successfully started and listening on {}", uri);
                    ::hyper::rt::run(server);
                } else {
                    return Err(::error::ServerError::BadListenerConfig);
                }
            }

            #[cfg(not(feature = "https"))]
            return Err(::error::ServerError::UnsupportedUriScheme);
        } else {
            return Err(::error::ServerError::UnsupportedUriScheme);
        }

        Ok(())
    }
}

fn http_service(req: Request<Body>, middleware_stack: &MiddlewareStack, router: &Router)
                -> Box<Future<Item=Response<Body>, Error=ServerError> + Send> {
    use std::time::Instant;
    use server::utils::RequestContinuation::*;
    use futures::sync::oneshot::channel;
    use std::thread;

    let (tx, rx) = channel();
    let middleware_stack_c = middleware_stack.clone();
    let router_c = router.clone();

    Box::new(req.load_body().map_err(|e| ServerError::from(e)).and_then(move |mut request| {
        thread::spawn(move || {
            let req_iat = Instant::now();
            let mut response = SyncResponse::new();

            if let Continue = middleware_stack_c.resolve(&mut request, &mut response) {
                router_c.dispatch(&mut request, &mut response);
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

#[doc(hidden)]
#[cfg(feature = "https")]
mod ssl_loading_utils {
    use rustls;
    use std::fs;
    use std::io::BufReader;

    pub fn load_certs(filename: &str) -> Vec<rustls::Certificate> {
        let certfile = fs::File::open(filename).expect("cannot open certificate file");
        let mut reader = BufReader::new(certfile);
        rustls::internal::pemfile::certs(&mut reader).expect("Unable to load certificate")
    }

    pub fn load_private_key(filename: &str) -> rustls::PrivateKey {
        let rsa_keys = {
            let keyfile = fs::File::open(filename)
                .expect("cannot open private key file");
            let mut reader = BufReader::new(keyfile);
            rustls::internal::pemfile::rsa_private_keys(&mut reader)
                .expect("file contains invalid rsa private key")
        };

        let pkcs8_keys = {
            let keyfile = fs::File::open(filename)
                .expect("cannot open private key file");
            let mut reader = BufReader::new(keyfile);
            rustls::internal::pemfile::pkcs8_private_keys(&mut reader)
                .expect("file contains invalid pkcs8 private key (encrypted keys not supported)")
        };

        // prefer to load pkcs8 keys
        if !pkcs8_keys.is_empty() {
            pkcs8_keys[0].clone()
        } else {
            assert!(!rsa_keys.is_empty(), "Unable to load key");
            rsa_keys[0].clone()
        }
    }
}