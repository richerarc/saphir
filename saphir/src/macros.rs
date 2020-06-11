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
//! controller function (endpoint)
//!
//! ## The `#[<method>("/<path>")] Attribute`
//! This one is the attribute to add a endpoint to your controller, simply add a
//! method and a path above your endpoint function, and there ya go.
//! E.g. `#[get("/users/<user_id>")]` would route its function to
//! /users/<user_id> with the HTTP method GET accepted.
//!
//! We support even custom methods, and for convinience, `#[any(/your/path)]`
//! will be treated as : _any method_ being accepted.
//!
//! ## The `#[openapi(...)] Attribute`
//! This attribute can be added to a controller function (endpoint) to add
//! informations about the endpoint for OpenAPI generation through saphir's
//! CLI.
//! This attribute can be present multiple times and can include any number of
//! `return` and `params` parameters:
//!
//! ### return(code = <code>, type = "<type_path>"[, mime = <mime>])
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
//! `#[openapi(return(code = 200, type = "[{code: String, name: String}]", mime
//! = "json"))]`
//!
//! ## The `#[cookies] Attribute`
//! This will ensure cookies are parsed in the request before the endpoint
//! function is called, cookies can than be accessed with
//! `req.cookies().get("<cookie_name>")`.
//!
//! ## The `#[guard] Attribute`
//! This will add a request guard before your endpoint. it has two parameters:
//! - `fn="path::to::your::guard_fn"` : *REQUIRED* This is used to specify what
//!   guard function is to be called before your endpoint
//! - `data="path::to::initializer"`  : _Optional_ This is used to instantiate
//!   the data that will be passed to the guard function. this function takes a
//!   reference of the controller type it is used in.
//!
//! # Type Attributes (Struct & Enum)
//! These attributes can be added on top of a `struct` or `enum` definition.
//!
//! ## The `#[openapi(mime = <mime>)] Attribute`
//! This attribute specify the OpenAPI mimetype for this type.

pub use futures::future::{BoxFuture, FutureExt};
pub use saphir_macro::{controller, guard, middleware, openapi};
