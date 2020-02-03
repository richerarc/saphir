use std::future::Future;
use std::net::SocketAddr;
use std::mem::MaybeUninit;

use futures::prelude::*;
use futures::stream::StreamExt;
use futures::task::{Context, Poll};
use hyper::Body;
use hyper::server::conn::Http;
use hyper::service::Service;
use tokio::net::TcpListener;
use parking_lot::{Once, OnceState};
use unchecked_unwrap::UncheckedUnwrap;

use crate::error::SaphirError;
use crate::http_context::HttpContext;
use crate::request::Request;
use crate::response::Response;
use crate::router::{Builder as RouterBuilder, RouterChain, RouterChainEnd};
use crate::router::Router;
use crate::middleware::{Builder as MiddlewareStackBuilder, MiddlewareChain, MiddleChainEnd};

/// Default time for request handling is 30 seconds
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
/// Default listener ip addr is AnyAddr (0.0.0.0)
pub const DEFAULT_LISTENER_IFACE: &'static str = "0.0.0.0:0";

pub struct Stack {
    router: Router,
    middlewares: Box<dyn MiddlewareChain>,
}

unsafe impl Send for Stack {}
unsafe impl Sync for Stack {}

impl Stack {
    fn new_handler(&'static self, peer_addr: Option<SocketAddr>) -> StackHandler {
        StackHandler {
            stack: self,
            peer_addr,
        }
    }

    async fn invoke(&self, req: Request<Body>) -> Result<Response<Body>, SaphirError> {
        let ctx = HttpContext::new(req, self.router.clone());
        self.middlewares.next(ctx).await
    }
}

#[derive(Clone)]
pub struct StackHandler {
    stack: &'static Stack,
    peer_addr: Option<SocketAddr>,
}

impl Service<hyper::Request<hyper::Body>> for StackHandler {
    type Response = hyper::Response<hyper::Body>;
    type Error = SaphirError;
    type Future = Box<dyn Future<Output=Result<hyper::Response<hyper::Body>, Self::Error>> + Send + Unpin>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: hyper::Request<hyper::Body>) -> Self::Future {
        let req = Request::new(req, self.peer_addr.take());
        let fut = Box::pin(self.stack.invoke(req).map(|r| r.and_then(|r| r.into_raw())));

        Box::new(fut) as Box<dyn Future<Output=Result<hyper::Response<hyper::Body>, SaphirError>> + Send + Unpin>
    }
}

/// A struct representing certificate or private key configuration.
#[derive(Clone)]
pub enum SslConfig {
    /// File path
    FilePath(String),

    /// File content where all \n and space have been removed.
    FileData(String),
}

pub struct ListenerBuilder {
    iface: Option<String>,
    request_timeout_ms: Option<u64>,
    // TODO Not impl
    cert_config: Option<SslConfig>,
    // TODO Not impl
    key_config: Option<SslConfig>,
}

impl ListenerBuilder {
    #[inline]
    pub fn new() -> Self {
        ListenerBuilder {
            iface: None,
            request_timeout_ms: Some(DEFAULT_REQUEST_TIMEOUT_MS),
            cert_config: None,
            key_config: None,
        }
    }

    #[inline]
    pub fn interface(mut self, s: &str) -> Self {
        self.iface = Some(s.to_string());
        self
    }

    #[inline]
    pub fn request_timeout<T: Into<Option<u64>>>(mut self, timeout_ms: T) -> Self {
        self.request_timeout_ms = timeout_ms.into();
        self
    }

    #[inline]
    fn build(self) -> ListenerConfig {
        let ListenerBuilder { iface, request_timeout_ms, cert_config: _, key_config: _ } = self;

        let iface = iface.unwrap_or_else(|| {
            DEFAULT_LISTENER_IFACE.to_string()
        });

        ListenerConfig {
            iface,
            request_timeout_ms,
        }
    }
}

struct ListenerConfig {
    iface: String,
    request_timeout_ms: Option<u64>,
}

pub struct Builder<Controllers, Middlewares>
where
    Controllers: 'static + RouterChain + Unpin + Send + Sync,
    Middlewares: 'static + MiddlewareChain + Unpin + Send + Sync,
{
    listener: Option<ListenerBuilder>,
    router: RouterBuilder<Controllers>,
    middlewares: MiddlewareStackBuilder<Middlewares>,
}

impl<Controllers, Middlewares> Builder<Controllers, Middlewares>
where
    Controllers: 'static + RouterChain + Unpin + Send + Sync,
    Middlewares: 'static + MiddlewareChain + Unpin + Send + Sync,
{
    #[inline]
    pub fn configure_listener<F>(mut self, f: F) -> Self
        where F: FnOnce(ListenerBuilder) -> ListenerBuilder {
        let l = if let Some(builder) = self.listener.take() {
            builder
        } else {
            ListenerBuilder::new()
        };

        self.listener = Some(f(l));

        self
    }

    #[inline]
    pub fn configure_router<F, NewChain: RouterChain + Unpin + Send + Sync>(self, f: F) -> Builder<NewChain, Middlewares>
        where F: FnOnce(RouterBuilder<Controllers>) -> RouterBuilder<NewChain>
    {
        Builder {
            listener: self.listener,
            router: f(self.router),
            middlewares: self.middlewares,
        }
    }

    #[inline]
    pub fn configure_middlewares<F, NewChain: MiddlewareChain + Unpin + Send + Sync>(self, f: F) -> Builder<Controllers, NewChain>
        where F: FnOnce(MiddlewareStackBuilder<Middlewares>) -> MiddlewareStackBuilder<NewChain>
    {
        Builder {
            listener: self.listener,
            router: self.router,
            middlewares: f(self.middlewares),
        }
    }

    pub fn build(self) -> Server {
        Server {
            listener_config: self.listener.unwrap_or_else(|| ListenerBuilder::new()).build(),
            stack: Stack {
                router: self.router.build(),
                middlewares: self.middlewares.build(),
            },
        }
    }
}

pub struct Server {
    listener_config: ListenerConfig,
    stack: Stack,
}


static mut STACK: MaybeUninit<Stack> = MaybeUninit::uninit();
static INIT_STACK: Once = Once::new();

impl Server {
    #[inline]
    pub fn builder() -> Builder<RouterChainEnd, MiddleChainEnd> {
        Builder {
            listener: None,
            router: RouterBuilder::default(),
            middlewares: MiddlewareStackBuilder::default(),
        }
    }

    pub async fn run(self) -> Result<(), SaphirError> {
        let Server { listener_config, stack } = self;

        if INIT_STACK.state() != OnceState::New {
            return Err(SaphirError::Other("cannot run a second server".to_owned()));
        }

        INIT_STACK.call_once(|| {
            // # SAFETY #
            // We write only once in the static memory. No override.
            // Above check also make sure there is no second server.
            unsafe { STACK.as_mut_ptr().write(stack); }
        });

        // # SAFETY #
        // Memory has been initialized above.
        let stack = unsafe { STACK.as_ptr().as_ref().unchecked_unwrap() };

        let http = Http::new();
        let mut listener = TcpListener::bind(listener_config.iface).await?;
        let local_addr = listener.local_addr()?;

        info!("Saphir started and listening on : http://{}", local_addr);

        let incoming = listener.incoming();
        if let Some(request_timeout_ms) = listener_config.request_timeout_ms {
            use tokio::time::{Duration, timeout};
            incoming.for_each_concurrent(None, |client_socket| async {
                match client_socket {
                    Ok(client_socket) => {
                        let peer_addr = client_socket.peer_addr().ok();
                        let http_handler = http.serve_connection(client_socket, stack.new_handler(peer_addr));
                        let f = timeout(Duration::from_millis(request_timeout_ms), http_handler);

                        tokio::spawn(f);
                    }
                    Err(e) => {
                        warn!("incoming connection encountered an error: {}", e);
                    }
                }
            }).await;
        } else {
            incoming.for_each_concurrent(None, |client_socket| async {
                match client_socket {
                    Ok(client_socket) => {
                        let peer_addr = client_socket.peer_addr().ok();
                        let http_handler = http.serve_connection(client_socket, stack.new_handler(peer_addr));

                        tokio::spawn(http_handler);
                    }
                    Err(e) => {
                        warn!("incoming connection encountered an error: {}", e);
                    }
                }
            }).await;
        }

        Ok(())
    }
}