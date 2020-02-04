//! ### Saphir is a fully async-await http server framework for rust
//! The goal is to give low-level control to your web stack (as hyper does) without the time consuming task of doing everything from scratch.
//!
//! Just `use` the prelude module, and you're ready to go!
//!
//! ## Quick server setup
//! ```ignore
//! use saphir::prelude::*;
//!
//!
//! async fn test_handler(mut req: Request<Body>) -> (u16, Option<String>) {
//!     (200, req.captures_mut().remove("variable"))
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), SaphirError> {
//!     env_logger::init();
//!
//!     let server = Server::builder()
//!         .configure_listener(|l| {
//!             l.interface("127.0.0.1:3000")
//!         })
//!         .configure_router(|r| {
//!             r.route("/{variable}/print", Method::GET, test_handler)
//!         })
//!         .build().await?;
//!
//!     server.run().await
//! }
//! ```

#[macro_use]
extern crate log;

/// Error definitions
pub mod error;
/// Server implementation and default runtime
pub mod server;
///
pub mod utils;
/// Context enveloping every request <-> response
pub mod http_context;
/// The Http Request type
pub mod request;
/// The Http Response type
pub mod response;
/// Definition of type which can map to a response
pub mod responder;
/// Definition of types which can handle an http request
pub mod handler;
///
pub mod router;
///
pub mod middleware;
///
pub mod controller;
///
pub use cookie;
///
pub use http;
///
pub use hyper;

/// Contains everything you need to bootstrap your http server
///
/// ```rust
/// use saphir::prelude::*;
///
/// // implement magic
/// ```
pub mod prelude {
    ///
    pub use http::Method;
    ///
    pub use http::StatusCode;
    ///
    pub use http::Version;
    ///
    pub use http::Uri;
    ///
    pub use http::Extensions;
    ///
    pub use http::header;
    ///
    pub use hyper::Body as Body;
    ///
    pub use hyper::body as body;
    ///
    pub use crate::error::SaphirError;
    ///
    pub use crate::handler::Handler;
    ///
    pub use crate::responder::Responder;
    ///
    pub use crate::http_context::HttpContext;
    ///
    pub use crate::middleware::MiddlewareChain;
    ///
    pub use crate::request::Request;
    ///
    pub use crate::response::Builder;
    ///
    pub use crate::response::Response;
    ///
    pub use crate::server::Server;
    ///
    pub use crate::server::Stack;
    ///
    pub use crate::controller::Controller;
    ///
    pub use crate::controller::ControllerEndpoint;
    ///
    pub use crate::controller::EndpointsBuilder;
    ///
    pub use cookie::Cookie;
    ///
    pub use cookie::CookieJar;
    ///
    pub use cookie::CookieBuilder;
}