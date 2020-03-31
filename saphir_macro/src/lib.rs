// The `quote!` macro requires deep recursion.
#![recursion_limit = "512"]

extern crate proc_macro;

use proc_macro::TokenStream as TokenStream1;
use syn::{parse_macro_input, AttributeArgs, ItemImpl};

mod controller;
mod guard;
mod middleware;
mod utils;

/// Saphir macro for auto trait implementation on controllers
///
/// The base macro attribule look like this : `#[controller]` and is to be put
/// on top of a Controller's method impl block
///
/// ```ignore
/// #use saphir::prelude::*;
/// #use saphir_macro::controller;
///
/// #struct ExampleController;
///
/// #[controller]
/// impl ExampleController {
///     // ....
/// }
/// ```
///
/// Different arguments can be passed to the controller macro:
/// - `name="<newName>"` will take place of the default controller name (by
///   default the controller name is the struct name, lowercase, with the
///   "controller keyword stripped"). the name will result as the basepath of
///   the controller.
/// - `version=<u16>` use for api version, the version will be added before the
///   name as the controller basepath
/// - `prefix="<prefix>"` add a prefix before the basepath and the version.
///
/// ##Example
///
/// ```ignore
/// use saphir::prelude::*;
/// use saphir_macro::controller;
///
/// struct ExampleController;
///
/// #[controller(name="test", version=1, prefix="api")]
/// impl ExampleController {
///     // ....
/// }
/// ```
///
/// This will result in the Example controller being routed to `/api/v1/test`
#[proc_macro_attribute]
pub fn controller(args: TokenStream1, input: TokenStream1) -> TokenStream1 {
    let args = parse_macro_input!(args as AttributeArgs);
    let input = parse_macro_input!(input as ItemImpl);

    let expanded = controller::expand_controller(args, input).unwrap_or_else(|e| e.to_compile_error());

    TokenStream1::from(expanded)
}

#[proc_macro_attribute]
pub fn middleware(_args: TokenStream1, input: TokenStream1) -> TokenStream1 {
    let input = parse_macro_input!(input as ItemImpl);

    let expanded = middleware::expand_middleware(input).unwrap_or_else(|e| e.to_compile_error());

    TokenStream1::from(expanded)
}

#[proc_macro_attribute]
pub fn guard(_args: TokenStream1, input: TokenStream1) -> TokenStream1 {
    let input = parse_macro_input!(input as ItemImpl);

    let expanded = guard::expand_guard(input).unwrap_or_else(|e| e.to_compile_error());

    TokenStream1::from(expanded)
}
