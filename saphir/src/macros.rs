//! Saphir provides a proc_macro attribute and multiple function attributes.
//!
//! # The `#[controller]` Macro
//!
//! This macro is an attribute macro that need to be place on the `impl block`
//! of a Saphir controller. It has 3 optionnal parameters:
//! - `prefix="<pre>"` : This will prefix any controller route by the specified
//!   route prefix
//! - `version=<u16>`  : This will insert the `/v#` path segment between the
//!   prefix and the base controller route
//! - `name="<name>"`  : This will route the controller at /<name>.
//!
//! If none of these are used, the controller will be routed at its own name, in
//! lowercase, with the controller keyword trimmed.
//!
//! # Function Attributes
//! We also parse several function attributes that can be placed above a
//! controller function (endpoint).
//!
//! ## The `#[<method>("/<path>")]` Attribute
//! This one is the attribute to add a endpoint to your controller, simply add a
//! method and a path above your endpoint function, and there ya go.
//! E.g. `#[get("/users/<user_id>")]` would route its function to
//! /users/<user_id> with the HTTP method GET accepted.
//!
//! Path segments wrapped between '<' and '>', e.g. <user_id>, are considered parameters
//! and mapped to the function parameter of the same name.
//!
//! The following parameters types are supported:
//!  - `CookieJar`: Collection of all the cookies in the request
//!  - `Json`: The request body interpreted in Json.
//!          If the request body is not valid Json, a 400 Bad Request response is returned.
//!  - `Form`: The request body interpreted as a standard form. (application/x-www-form-urlencoded)
//!          If the request body is not a valid Form, a 400 Bad Request response is returned.
//!  - `Multipart`: The request body interpreted as multipart form data (multipart/form-data)
//!               If the request body is not a valid multipart form, a 400 Bad Request response is returned.
//!  - `Ext<MyExtensionType>`: Retrieve the MyExtensionType from the request extensions.
//!                          Request extensions are data that you can attach to the request
//!                          within Middlewares and Guards.
//!  - `Extensions`: Collection of all the extensions attached to the request.
//!                This is the whole owned collection, so it cannot be used in conjunction
//!                with single Ext<T> parameters.
//!  - `Request`: The whole owned Saphir request.
//!             This is the whole owned request, so it cannot be used in conjunction
//!             of any of the above. (All of the above can be retrieved from this request)
//!  - `Option`: Any body parameter, path parameter or query string parameter (see below)
//!            can be marked as optionnal.
//!  - `<T>`: Any other unhandled parameter type is considered a query string parameter.
//!         T must implement FromStr.
//!
//! We support even custom methods, and for convinience, `#[any(/your/path)]`
//! will be treated as : _any method_ being accepted.
//!
//! ## The `#[openapi(...)]` Attribute
//! This attribute can be added to a controller function (endpoint) to add
//! informations about the endpoint for OpenAPI generation through saphir's
//! CLI.
//! This attribute can be present multiple times and can include any number of
//! `return`, `return_override` and `params` parameters:
//!
//! ### The `return(...)` openapi parameter
//! **Syntax: `return(code = <code>, type = "<type_path>"[, mime = <mime>])`**
//!
//! Specify a possible return code & type, and optionally a mime type.
//! The type must be a valid type path included (`use`) in the file.
//! E.g. `#[openapi(return(code = 200, type = "Json<MyType>")]`
//!
//! `type` support infering the mimetype of built-in responders such as
//! `Json<T>` and `Form<T>`, so the following are ecquivalent :
//! - `#[openapi(return(code = 200, type = "Json<MyType>")]`
//! - `#[openapi(return(code = 200, type = "self::MyType", mime = "json")]`
//! - `#[openapi(return(code = 200, type = "MyType", mime =
//!   "application/json")]`
//!
//! `type` can also be a string describing a raw object, for example :
//! `#[openapi(return(code = 200, type = "[{code: String, name: String}]))",
//! mime = "json"))]`
//!
//! You can also specify multiples codes that would return a similar type.
//! For example, if you have a type `MyJsonError` rendering an error as a json
//! payload, and your endpoint can return a 404 and a 500 in such a format,
//! you could write it as such :
//! `#[openapi(return(type = "MyJsonError", mime = "json", code = 404, code =
//! 500))]`
//!
//!
//! ### The `return_override(...)` openapi parameter
//! **Syntax: `return_override(type = "<type_path>", code = <code>[, mime = <mime>])`**
//!
//! Saphir provide some default API information for built-in types.
//! For example, a `Result::Ok` result has a status code of 200 by default, a
//! `Result::Err` a status code of 500, and a `Option::None` a status code of
//! 404. So, the following handler :
//! ```rust
//! # #[macro_use] extern crate saphir_macro;
//! # use crate::saphir::prelude::*;
//! #
//! # fn main() {}
//! #
//! # enum MyError {
//! #     Unknown
//! # }
//! # impl Responder for MyError {
//! #    fn respond_with_builder(self,builder: Builder,ctx: &HttpContext) -> Builder {
//! #        unimplemented!()
//! #    }
//! # }
//! #
//! # struct MyController {}
//! # #[controller(name = "my-controller")]
//! # impl MyController {
//! #[get("/")]
//! async fn my_handler(&self) -> Result<Option<String>, MyError> { /*...*/ Ok(None) }
//! # }
//! ```
//! will generate by default the same documentation as if it was written as
//! such:
//! ```rust
//! # #[macro_use] extern crate saphir_macro;
//! # use crate::saphir::prelude::*;
//! #
//! # fn main() {}
//! #
//! # enum MyError {
//! #     Unknown
//! # }
//! # impl Responder for MyError {
//! #    fn respond_with_builder(self,builder: Builder,ctx: &HttpContext) -> Builder {
//! #        unimplemented!()
//! #    }
//! # }
//! #
//! # struct MyController {}
//! # #[controller(name = "my-controller")]
//! # impl MyController {
//! #[get("/")]
//! #[openapi(return(code = 200, type = "String", mime = "text/plain"))]
//! #[openapi(return(code = 404, type = ""), return(code = 500, type = "MyError"))]
//! async fn my_handler(&self) -> Result<Option<String>, MyError> { /*...*/ Ok(None) }
//! # }
//! ```
//!
//! If you want to start with these defaults and override the return of a single
//! type in the composed result, for example specifying that `MyError` is
//! rendered as a json document, then you can use `return_override` like this :
//! ```rust
//! # #[macro_use] extern crate saphir_macro;
//! # use crate::saphir::prelude::*;
//! #
//! # fn main() {}
//! #
//! # enum MyError {
//! #     Unknown
//! # }
//! # impl Responder for MyError {
//! #    fn respond_with_builder(self,builder: Builder,ctx: &HttpContext) -> Builder {
//! #        unimplemented!()
//! #    }
//! # }
//! #
//! # struct MyController {}
//! # #[controller(name = "my-controller")]
//! # impl MyController {
//! #[get("/")]
//! #[openapi(return_override(type = "MyError", mime = "application/json"))]
//! async fn my_handler(&self) -> Result<Option<String>, MyError> { /*...*/ Ok(None) }
//! # }
//! ```
//!
//! ## The `#[cookies]` Attribute
//! This will ensure cookies are parsed in the request before the endpoint
//! function is called, cookies can than be accessed with
//! `req.cookies().get("<cookie_name>")`.
//!
//! ## The `#[guard]` Attribute
//! This will add a request guard before your endpoint. It has two parameters:
//! - `fn="path::to::your::guard_fn"` : *REQUIRED* This is used to specify what
//!   guard function is to be called before your endpoint
//! - `data="path::to::initializer"`  : _Optional_ This is used to instantiate
//!   the data that will be passed to the guard function. this function takes a
//!   reference of the controller type it is used in.
//!
//! ## The `#[validate(...)` Attribute
//! **Syntax: `#[validate(exclude(excluded_param_1, excluded_param_2))]`**
//!
//! When using the `validate-requests` feature flag, saphir will generate validation
//! code for all `Json<T>` and `Form<T>` request payloads using the [`validator`](https://github.com/Keats/validator) crate.
//! Any `T` which does not implement the `validator::Validate` trait will cause
//! compilation error.
//! This macro attribute can be used to exclude validation on certain request
//! parameters.
//! Example:
//! ```rust
//! # #[macro_use] extern crate saphir_macro;
//! # use crate::saphir::prelude::*;
//! #
//! # fn main() {}
//! #
//! # enum MyError {
//! #     Unknown
//! # }
//! # impl Responder for MyError {
//! #    fn respond_with_builder(self,builder: Builder,ctx: &HttpContext) -> Builder {
//! #        unimplemented!()
//! #    }
//! # }
//! #
//! # #[controller(name = "my-controller")]
//! #[derive(Deserialize)]
//! struct MyPayload {
//!    a: String,
//! }
//!
//! struct MyController {}
//! impl MyController {
//!     #[post("/")]
//!     #[validator(exclude(req))]
//!     async fn my_handler(&self, req: Json<MyPayload>) -> Result<(), MyError> { /*...*/ Ok(()) }
//! }
//! ```
//!
//! # Type Attributes (Struct & Enum)
//! These attributes can be added on top of a `struct` or `enum` definition.
//!
//! ## The `#[openapi(mime = <mime>)]` Attribute
//! This attribute specify the OpenAPI mimetype for this type.

pub use futures::future::{BoxFuture, FutureExt};
pub use saphir_macro::{controller, guard, middleware, openapi};
