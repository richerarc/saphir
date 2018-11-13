use hyper::service::service_fn;
use http::*;
use utils;
use error::ServerError;
use middleware::MiddlewareStack;
use router::Router;
use futures::Future;
use futures::sync::oneshot::{Sender, channel};
use tokio::runtime::TaskExecutor;
use std::cell::RefCell;
use std::any::Any;

///
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 15000;

/// A struct representing listener configuration
pub struct ListenerConfig {
    request_timeout_ms: u64,
    uri: Option<String>,
    cert_path: Option<String>,
    key_path: Option<String>,
}

impl ListenerConfig {
    pub fn set_panic_handler<PanicHandler>(&mut self, panic_handler: PanicHandler)
        where PanicHandler: Fn(Box<dyn Any + 'static + Send>) + Send + Sync + 'static {
        rayon::ThreadPoolBuilder::new().panic_handler(panic_handler).build_global().expect("Setting the panic handler should never fail")
    }

    /// Set the default timeout for request in milliseconds. 0 means no timeout.
    pub fn set_request_timeout_ms(&mut self, timeout: u64) {
        self.request_timeout_ms = timeout;
    }

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
                request_timeout_ms: DEFAULT_REQUEST_TIMEOUT_MS,
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

/// Handle to signal the server on termination
pub struct ServerSpawn(Option<Sender<()>>);

impl ServerSpawn {
    /// Signal the server to terminate itself gracefully
    pub fn terminate(mut self) {
        if let Some(s) = self.0.take(){
            let _ = s.send(());
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

    /// Spawn the server inside the provided executor and return a ServerSpawn context to explicitly terminate it.
    pub fn spawn(&self, executor: TaskExecutor) -> Result<ServerSpawn, ::error::ServerError> {
        let uri: Uri = self.listener_config.borrow().uri()
            .expect("Fatal Error: No uri provided.\n You can fix this error by calling Server::set_uri or by configuring the listener with Server::configure_listener")
            .parse()?;

        let scheme = uri.scheme_part().expect("Fatal Error: The uri passed to launch the server doesn't contain a scheme.");
        let addr = uri.authority_part().expect("The uri passed to launch the server doesn't contain an authority.").as_str().parse()?;

        let listener = ::tokio::net::TcpListener::bind(&addr)?;

        let service = HttpService {
            router: self.router.clone(),
            middleware_stack: self.middleware_stack.clone(),
            request_timeout: self.listener_config.borrow().request_timeout_ms,
        };

        let (sender, receiver) = channel();

        let server_spawn = ServerSpawn(Some(sender));

        if scheme.eq(&::http_types::uri::Scheme::HTTP) {
            if let (Some(_), _) = self.listener_config.borrow().ssl_files_path() {
                warn!("SSL certificate paths are provided but the listener was configured to use unsecured HTTP, try changing the uri scheme for https");
            }

            let server = ::hyper::server::Builder::new(listener.incoming(), ::hyper::server::conn::Http::new()).serve(move || {
                let handler = service.clone();
                service_fn(move |req| {
                    handler.handle(req)
                })
            }).with_graceful_shutdown(receiver).map_err(|e| error!("server error: {}", e));

            executor.spawn(server);
            info!("Saphir successfully started and listening on {}", uri);
        } else if scheme.eq(&::http_types::uri::Scheme::HTTPS) {
            #[cfg(feature = "https")]
                {
                    if let (Some(cert_path), Some(key_path)) = self.listener_config.borrow().ssl_files_path() {
                        use std::sync::Arc;
                        use futures::Stream;
                        use server::ssl_loading_utils::*;
                        use tokio_rustls::TlsAcceptor;

                        let certs = load_certs(cert_path.as_ref());
                        let key = load_private_key(key_path.as_ref());
                        let mut cfg = ::rustls::ServerConfig::new(::rustls::NoClientAuth::new());
                        let _ = cfg.set_single_cert(certs, key);
                        let arc_config = Arc::new(cfg);

                        let acceptor = TlsAcceptor::from(arc_config);

                        let inc = listener.incoming().and_then(move |stream| {
                            acceptor.accept(stream)
                        });

                        let server = ::hyper::server::Builder::new(inc, ::hyper::server::conn::Http::new()).serve(move || {
                            let handler = service.clone();
                            service_fn(move |req| {
                                handler.handle(req)
                            })
                        }).with_graceful_shutdown(receiver).map_err(|e| error!("server error: {}", e));

                        executor.spawn(server);
                        info!("Saphir successfully started and listening on {}", uri);
                    } else {
                        return Err(::error::ServerError::BadListenerConfig);
                    }
                }

            #[cfg(not(feature = "https"))]
                return Err(::error::ServerError::UnsupportedUriScheme);
        } else {
            return Err(::error::ServerError::UnsupportedUriScheme);
        }

        Ok(server_spawn)
    }

    /// This method will run until the server terminates.
    pub fn run(&self) -> Result<(), ::error::ServerError> {
        let uri: Uri = self.listener_config.borrow().uri()
            .expect("Fatal Error: No uri provided.\n You can fix this error by calling Server::set_uri or by configuring the listener with Server::configure_listener")
            .parse()?;

        let scheme = uri.scheme_part().expect("Fatal Error: The uri passed to launch the server doesn't contain a scheme.");
        let addr = uri.authority_part().expect("The uri passed to launch the server doesn't contain an authority.").as_str().parse()?;

        let listener = ::tokio::net::TcpListener::bind(&addr)?;

        let service = HttpService {
            router: self.router.clone(),
            middleware_stack: self.middleware_stack.clone(),
            request_timeout: self.listener_config.borrow().request_timeout_ms,
        };

        if scheme.eq(&::http_types::uri::Scheme::HTTP) {
            if let (Some(_), _) = self.listener_config.borrow().ssl_files_path() {
                warn!("SSL certificate paths are provided but the listener was configured to use unsecured HTTP, try changing the uri scheme for https");
            }

            let server = ::hyper::server::Builder::new(listener.incoming(), ::hyper::server::conn::Http::new()).serve(move || {
                let handler = service.clone();
                service_fn(move |req| {
                    handler.handle(req)
                })
            }).map_err(|e| error!("server error: {}", e));

            info!("Saphir successfully started and listening on {}", uri);
            ::hyper::rt::run(server);
        } else if scheme.eq(&::http_types::uri::Scheme::HTTPS) {
            #[cfg(feature = "https")]
                {
                    if let (Some(cert_path), Some(key_path)) = self.listener_config.borrow().ssl_files_path() {
                        use std::sync::Arc;
                        use futures::Stream;
                        use server::ssl_loading_utils::*;
                        use tokio_rustls::TlsAcceptor;

                        let certs = load_certs(cert_path.as_ref());
                        let key = load_private_key(key_path.as_ref());
                        let mut cfg = ::rustls::ServerConfig::new(::rustls::NoClientAuth::new());
                        let _ = cfg.set_single_cert(certs, key);
                        let arc_config = Arc::new(cfg);

                        let acceptor = TlsAcceptor::from(arc_config);

                        let inc = listener.incoming().and_then(move |stream| {
                            acceptor.accept(stream)
                        });

                        let server = ::hyper::server::Builder::new(inc, ::hyper::server::conn::Http::new()).serve(move || {
                            let handler = service.clone();
                            service_fn(move |req| {
                                handler.handle(req)
                            })
                        }).map_err(|e| error!("server error: {}", e));

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

#[doc(hidden)]
#[derive(Clone)]
struct HttpService {
    router: Router,
    middleware_stack: MiddlewareStack,
    request_timeout: u64,
}

#[doc(hidden)]
impl HttpService {
    pub fn handle(&self, req: Request<Body>) -> Box<Future<Item=Response<Body>, Error=ServerError> + Send> {
        use std::time::{Instant, Duration};
        use server::utils::RequestContinuation::*;
        use futures::sync::oneshot::channel;
        use rayon;

        let (tx, rx) = channel();

        let HttpService {
            router,
            middleware_stack,
            request_timeout
        } = self.clone();

        Box::new(req.load_body().map_err(|e| ServerError::from(e)).and_then(move |mut request| {
            rayon::spawn(move || {
                let req_iat = Instant::now();
                let mut response = SyncResponse::new();

                if let Continue = middleware_stack.resolve(&mut request, &mut response) {
                    router.dispatch(&mut request, &mut response);
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

            let timeout = if request_timeout > 0 {
                Box::new(tokio::timer::Timeout::new(futures::empty::<Response<Body>, ServerError>(), Duration::from_millis(request_timeout)).then(|_| {
                    let mut resp = Response::new(Body::empty());
                    *resp.status_mut() = StatusCode::REQUEST_TIMEOUT;
                    futures::future::ok::<Response<Body>, ServerError>(resp)
                })) as Box<Future<Item=Response<Body>, Error=ServerError> + Send>
            } else {
                Box::new(futures::empty::<Response<Body>, ServerError>()) as Box<Future<Item=Response<Body>, Error=ServerError> + Send>
            };

            rx.map_err(|e| ServerError::from(e))
                .select(timeout)
                .map(|(r, _)| r)
                .map_err(|(e, _)| e)
        }))
    }
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