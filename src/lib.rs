#![deny(missing_docs)]
#![deny(warnings)]

//! # Saphir
//!
//! Saphir is a progressive http server framework based on Hyper-rs that aims to reduce the time spent on playing with futures and
//! limiting the amount of copied code amongst request matching.
//!
//! Saphir provide what's needed to easily start with your own server with middleware, controllers and request routing.
//!
//! Futures version will comes with more macro and a nightly experiment is currently being tested to reproduces decorator in rust.

#[macro_use]
extern crate log;
extern crate hyper;
extern crate futures;
extern crate tokio;
extern crate regex;
extern crate chrono;
extern crate ansi_term;
extern crate url;

#[macro_use]
mod utils;
mod http;
mod error;
mod middleware;
mod controller;
mod router;
mod server;

pub use utils::*;
pub use http::*;
pub use utils::RequestContinuation;
pub use middleware::Middleware;
pub use middleware::MiddlewareStack;
pub use controller::Controller;
pub use controller::BasicController;
pub use controller::ControllerDispatch;
pub use controller::RequestGuard;
pub use controller::RequestGuardCollection;
pub use controller::BodyGuard;
pub use router::Router;
pub use server::Server;
pub use error::ServerError;