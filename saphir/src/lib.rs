//! # Saphir is a fully async-await http server framework for rust
//! The goal is to give low-level control to your web stack (as hyper does)
//! without the time consuming task of doing everything from scratch.
//!
//! Just `use` the prelude module, and you're ready to go!
//!
//! # Quick Overview
//!
//!  Saphir provide multiple functionality through features. To try it out
//! without fuss, we suggest that use all the features:
//!
//! ```toml
//! saphir = { version = "2.0.0", features = ["full"] }
//! ```
//!
//! Then bootstrapping the server is as easy as:
//!
//! ```rust
//! use saphir::prelude::*;
//!
//! struct TestController {}
//!
//! #[controller]
//! impl TestController {
//!     #[get("/{var}/print")]
//!     async fn print_test(&self, var: String) -> (u16, String) {
//!         (200, var)
//!     }
//! }
//!
//! async fn test_handler(mut req: Request) -> (u16, Option<String>) {
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
//!                 .controller(TestController {})
//!         })
//!         .build();
//!
//!     // Start server with
//!     // server.run().await
//! #    Ok(())
//! }
//! ```
//! # Saphir's Features
//!
//! Even though we strongly recommend that you use at least the `macro` feature,
//! Saphir will work without any of the following feature, Saphir's features
//! don't rely on each other to work.
//!
//! - `macro` : Enable the `#[controller]` macro attribute for code generation,
//!   Recommended and active by default
//! - `https` : Provide everything to allow Saphir server to listen an accept
//!   HTTPS traffic
//! - `json`  : Add the `Json` wrapper type to simplify working with json data
//! - `form`  : Add the `Form` wrapper type to simplify working with urlencoded
//!   data
//!
//! *_More feature will be added in the future_*

#[macro_use]
extern crate log;

///
pub mod body;
///
pub mod controller;
/// Error definitions
pub mod error;
///
#[cfg(feature = "file")]
pub mod file;
///
pub mod guard;
/// Definition of types which can handle an http request
pub mod handler;
/// Context enveloping every request <-> response
pub mod http_context;
/// Saphir macro for code generation
#[cfg(feature = "macro")]
pub mod macros;
///
pub mod middleware;
/// The async Multipart Form-Data representation
#[cfg(feature = "multipart")]
pub mod multipart;
/// The Http Request type
pub mod request;
/// Definition of type which can map to a response
pub mod responder;
/// The Http Response type
pub mod response;
///
pub mod router;
/// Server implementation and default runtime
pub mod server;
///
pub mod utils;
///
pub use cookie;
///
pub use http;
#[doc(hidden)]
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
    pub use crate::body::Body;
    ///
    pub use crate::body::Bytes;
    ///
    #[cfg(feature = "form")]
    pub use crate::body::Form;
    ///
    #[cfg(feature = "json")]
    pub use crate::body::Json;
    ///
    pub use crate::controller::Controller;
    ///
    pub use crate::controller::ControllerEndpoint;
    ///
    pub use crate::controller::EndpointsBuilder;
    ///
    pub use crate::error::SaphirError;
    ///
    pub use crate::handler::Handler;
    #[cfg(feature = "operation")]
    pub use crate::http_context::operation::OperationId;
    ///
    pub use crate::http_context::HttpContext;
    ///
    #[cfg(feature = "macro")]
    pub use crate::macros::controller;
    ///
    pub use crate::middleware::MiddlewareChain;
    ///
    #[cfg(feature = "multipart")]
    pub use crate::multipart::Multipart;
    ///
    pub use crate::request::Request;
    ///
    pub use crate::responder::Responder;
    ///
    pub use crate::response::Builder;
    ///
    pub use crate::response::Response;
    ///
    pub use crate::server::Server;
    ///
    pub use crate::server::Stack;
    ///
    pub use cookie::Cookie;
    ///
    pub use cookie::CookieBuilder;
    ///
    pub use cookie::CookieJar;
    ///
    pub use http::header;
    ///
    pub use http::Extensions;
    ///
    pub use http::Method;
    ///
    pub use http::StatusCode;
    ///
    pub use http::Uri;
    ///
    pub use http::Version;
}
