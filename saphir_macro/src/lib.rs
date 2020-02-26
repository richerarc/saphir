//! Saphir macro for auto trait implementation on controllers
//!
//! The base macro attribule look like this : `#[controller]` and is to be put on top of a Controller's method impl block
//!
//! ```rust
//! #use saphir::prelude::*;
//! #use saphir_macro::controller;
//!
//! #struct ExampleController;
//!
//! #[controller]
//! impl ExampleController {
//!     // ....
//! }
//! ```
//!
//! Different arguments can be passed to the controller macro:
//! - `name="<newName>"` will take place of the default controller name (by default the controller name is the struct name, lowercase, with the "controller keyword stripped"). the name will result as the basepath of the controller.
//! - `version=<u16>` use for api version, the version will be added before the name as the controller basepath
//! - `prefix="<prefix>"` add a prefix before the basepath and the version.
//!
//! ##Example
//!
//! ```rust
//! use saphir::prelude::*;
//! use saphir_macro::controller;
//!
//! struct ExampleController;
//!
//! #[controller(name="test", version=1, prefix="api")]
//! impl ExampleController {
//!     // ....
//! }
//! ```
//!
//! This will result in the Example controller being routed to `/api/v1/test`
//!

// The `quote!` macro requires deep recursion.
#![recursion_limit = "512"]

extern crate proc_macro;

use proc_macro::{TokenStream as TokenStream1};

use proc_macro2::{Ident, TokenStream, Span};
use quote::quote;
use syn::{AttributeArgs, ItemImpl, Meta, NestedMeta, parse_macro_input, Type, MetaList, MetaNameValue, Lit};
use crate::controller::ControllerAttr;

mod controller;
mod handler;

#[proc_macro_attribute]
pub fn controller(args: TokenStream1, input: TokenStream1) -> TokenStream1 {
    let args = parse_macro_input!(args as AttributeArgs);
    let input = parse_macro_input!(input as ItemImpl);
    let controller_attr = ControllerAttr::new(args, &input);

    let base_path = gen_base_path_const(&controller_attr);
    let handlers_fn = gen_handlers_fn(&controller_attr);
    let controller_implementation = gen_controller_implementation(&controller_attr, base_path, handlers_fn);

    let _ = handler::parse_handlers(&input);

    let mod_ident = Ident::new(&format!("SAPHIR_GEN_CONTROLLER_{}", &controller_attr.name), Span::call_site());
    let expanded = quote! {
        mod #mod_ident {
            use super::*;
            use saphir::prelude::*;
            #controller_implementation
        }
    };

    TokenStream1::from(expanded)
}

fn gen_base_path_const(attr: &ControllerAttr) -> TokenStream {
    let mut path = "/".to_string();

    if let Some(prefix) = attr.prefix.as_ref() {
        path.push_str(prefix);
        path.push('/');
    }

    if let Some(version) = attr.version {
        path.push('v');
        path.push_str(&format!("{}", version));
        path.push('/');
    }

    path.push_str(attr.name.as_str());

    let e = quote! {
        const BASE_PATH: &'static str = #path;
    };

    e
}

fn gen_handlers_fn(_attr: &ControllerAttr) -> TokenStream {
    let e = quote! {
        fn handlers(&self) -> Vec<ControllerEndpoint<Self>> where Self: Sized {
            let b = EndpointsBuilder::new();
            // handle dispatch logic here
            b.build()
        }
    };

    e
}

fn gen_controller_implementation(attr: &ControllerAttr, base_path: TokenStream, handler_fn: TokenStream) -> TokenStream {
    let ident = attr.ident.clone();
    let e = quote! {
        impl Controller for #ident {
            #base_path

            #handler_fn
        }
    };

    e
}