use futures::Future;
use futures::sync::oneshot::{Sender, channel};
use hyper::service::service_fn;
use log::{info, error, warn};
use tokio::runtime::TaskExecutor;

use crate::http::*;
use crate::utils;
use crate::error::ServerError;
use crate::middleware::{MiddlewareStack, Builder as MidStackBuilder};
use crate::router::{Router, Builder as RouterBuilder};
use threadpool::ThreadPool;
use tokio::prelude::stream::Stream;

///
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 15000;

///
pub struct ListenerBuilder {
    request_timeout_ms: u64,
    uri: Option<String>,
    cert_path: Option<String>,
    key_path: Option<String>,
    thread_pool_size: Option<usize>,
}

impl ListenerBuilder {
    ///
    pub fn new() -> Self {
        ListenerBuilder {
            request_timeout_ms: DEFAULT_REQUEST_TIMEOUT_MS,
            uri: None,
            cert_path: None,
            key_path: None,
            thread_pool_size: None
        }
    }

    /// Set the thread_pool size for request handling, default is number of available CPU
    pub fn set_thread_pool_size(mut self, size: usize) -> Self {
        self.thread_pool_size = Some(size);
        self
    }

    /// Set the default timeout for request in milliseconds. 0 means no timeout.
    pub fn set_request_timeout_ms(mut self, timeout: u64) -> Self {
        self.request_timeout_ms = timeout;
        self
    }

    /// Set the listener uri (supported format is <scheme>://<interface>:<port>)
    pub fn set_uri(mut self, uri: &str) -> Self {
        self.uri = Some(uri.to_string());
        self
    }

    /// Set the listener ssl certificates files. The cert needs to be PEM encoded
    /// while the key can be either RSA or PKCS8
    pub fn set_ssl_certificates(mut self, cert_path: &str, key_path: &str) -> Self {
        self.cert_path = Some(cert_path.to_string());
        self.key_path = Some(key_path.to_string());
        self
    }

    /// Builds a new Listener Configuration
    pub fn build(self) -> ListenerConfig {
        let ListenerBuilder {
            request_timeout_ms,
            uri,
            cert_path,
            key_path,
            thread_pool_size,
        } = self;

        ListenerConfig {
            request_timeout_ms,
            uri,
            cert_path,
            key_path,
            thread_pool_size
        }
    }
}

/// A struct representing listener configuration
pub struct ListenerConfig {
    request_timeout_ms: u64,
    uri: Option<String>,
    cert_path: Option<String>,
    key_path: Option<String>,
    thread_pool_size: Option<usize>,
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
                thread_pool_size: None
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
pub struct ServerSpawn {
    tx: Option<Sender<()>>,
    #[cfg(feature = "request_handler")]
    handler: HttpService,
}

impl ServerSpawn {
    /// Signal the server to terminate itself gracefully
    pub fn terminate(mut self) {
        if let Some(s) = self.tx.take(){
            let _ = s.send(());
        }
    }

    /// Retrive the inner http request handler of the server
    #[cfg(feature = "request_handler")]
    pub fn get_request_handler(&self) -> &HttpService {
        &self.handler
    }
}

/// Builder for the Server type
pub struct Builder {
    middleware_stack: Option<MiddlewareStack>,
    router: Option<Router>,
    listener_config: Option<ListenerConfig>,
}

impl Builder {
    /// Creates a new builder
    pub fn new() -> Self {
        Builder {
            middleware_stack: None,
            router: None,
            listener_config: None,
        }
    }

    /// This method will call the provided closure with a mutable ref of the router
    /// Once into the closure it is possible to add controllers to the router.
    pub fn configure_router<F>(mut self, config_fn: F) -> Self where F: Fn(RouterBuilder) -> RouterBuilder {
        self.router = Some(config_fn(RouterBuilder::new()).build());
        self
    }

    /// This method will call the provided closure with a mutable ref of the middleware_stack
    /// Once into the closure it is possible to add middlewares to the middleware_stack.
    pub fn configure_middlewares<F>(mut self, config_fn: F) -> Self where F: Fn(MidStackBuilder) -> MidStackBuilder {
        self.middleware_stack = Some(config_fn(MidStackBuilder::new()).build());
        self
    }

    /// This method will call the provided closure with a mutable ref of the listener configurations
    /// Once into the closure it is possible to set the uri and ssl file paths.
    pub fn configure_listener<F>(mut self, config_fn: F) -> Self where F: Fn(ListenerBuilder) -> ListenerBuilder {
        self.listener_config = Some(config_fn(ListenerBuilder::new()).build());
        self
    }

    /// Converts the builder into the Server type
    pub fn build(self) -> Server {
        let Builder {
            middleware_stack,
            router,
            listener_config,
        } = self;

        let listener_config = listener_config.unwrap_or_else(|| ListenerConfig::new());

        Server {
            service: HttpService {
                router: router.unwrap_or_else(|| Router::new()),
                middleware_stack: middleware_stack.unwrap_or_else(|| MiddlewareStack::new()),
                request_timeout: listener_config.request_timeout_ms,
                thread_pool: ThreadPool::new(listener_config.thread_pool_size.unwrap_or_else(|| num_cpus::get())),
            },
            listener_config
        }
    }
}

/// The http server
pub struct Server {
    service: HttpService,
    listener_config: ListenerConfig,
}

impl Server {
    /// Create a new http server
    pub fn builder() -> Builder {
        Builder::new()
    }

    /// Retrive the inner http request handler of the server
    #[cfg(feature = "request_handler")]
    pub fn get_request_handler(&self) -> &HttpService {
        &self.service
    }

    /// Spawn the server inside the provided executor and return a ServerSpawn context to explicitly terminate it.
    pub fn spawn(&self, executor: TaskExecutor) -> Result<ServerSpawn, crate::error::ServerError> {
        let uri: Uri = self.listener_config.uri()
            .expect("Fatal Error: No uri provided.\n You can fix this error by calling Server::set_uri or by configuring the listener with Server::configure_listener")
            .parse()?;

        let scheme = uri.scheme_part().expect("Fatal Error: The uri passed to launch the server doesn't contain a scheme.");
        let addr = uri.authority_part().expect("The uri passed to launch the server doesn't contain an authority.").as_str().parse()?;

        let listener = ::tokio::net::TcpListener::bind(&addr)?;

        let service = self.service.clone();

        let (sender, receiver) = channel();

        let server_spawn = ServerSpawn {
            tx: Some(sender),
            #[cfg(feature = "request_handler")]
            handler: service.clone(),
        };

        if scheme.eq(&crate::http_types::uri::Scheme::HTTP) {
            if let (Some(_), _) = self.listener_config.ssl_files_path() {
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
        } else if scheme.eq(&crate::http_types::uri::Scheme::HTTPS) {
            #[cfg(feature = "https")]
                {
                    if let (Some(cert_path), Some(key_path)) = self.listener_config.ssl_files_path() {
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
                            service_fn(move |req| {
                                service.handle(req)
                            })
                        }).with_graceful_shutdown(receiver).map_err(|e| error!("server error: {}", e));

                        executor.spawn(server);
                        info!("Saphir successfully started and listening on {}", uri);
                    } else {
                        return Err(::error::ServerError::BadListenerConfig);
                    }
                }

            #[cfg(not(feature = "https"))]
                return Err(crate::error::ServerError::UnsupportedUriScheme);
        } else {
            return Err(crate::error::ServerError::UnsupportedUriScheme);
        }

        Ok(server_spawn)
    }

    /// This method will run until the server terminates.
    pub fn run(&self) -> Result<(), crate::error::ServerError> {
        use tokio::runtime::Builder;
        use tokio_signal::unix::{SIGINT, Signal, SIGQUIT, SIGTERM};

        let runtime = Builder::new().build()?;

        let signals = futures::future::select_all(vec![
            Signal::new(SIGTERM).flatten_stream().into_future(),
            Signal::new(SIGQUIT).flatten_stream().into_future(),
            Signal::new(SIGINT).flatten_stream().into_future()
        ]);

        let server_handle = self.spawn(runtime.executor())?;

        let termination = signals.map(move |((sig, _), _, _)| {
            // Stop servers
            info!("Received Unix Signal {}", sig.unwrap_or(0));
            info!("Terminating Saphir server ...");
            server_handle.terminate()
        });

        let _ = runtime.block_on_all(termination);

        Ok(())
    }
}

#[doc(hidden)]
#[derive(Clone)]
pub struct HttpService {
    router: Router,
    middleware_stack: MiddlewareStack,
    request_timeout: u64,
    thread_pool: ThreadPool,
}

#[doc(hidden)]
impl HttpService {
    pub fn handle(&self, req: Request<Body>) -> Box<Future<Item=Response<Body>, Error=ServerError> + Send> {
        use std::time::{Instant, Duration};
        use crate::server::utils::RequestContinuation::*;
        use futures::sync::oneshot::channel;

        let (tx, rx) = channel();

        let HttpService {
            router,
            middleware_stack,
            request_timeout,
            thread_pool,
        } = self.clone();

        Box::new(req.load_body().map_err(|e| ServerError::from(e)).and_then(move |mut request| {
            thread_pool.execute(move || {
                let req_iat = Instant::now();
                let mut response = SyncResponse::new();

                if let Continue = middleware_stack.resolve(&mut request, &mut response) {
                    router.dispatch(&mut request, &mut response);
                }

                let final_res = response.build_response().unwrap_or_else(|_| {
                    let empty: &[u8] = b"";
                    let mut res = Response::new(empty.into());
                    *res.status_mut() = StatusCode::from_u16(500).expect("Unable to set status code to 500, this should not happens");
                    res
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