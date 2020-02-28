//! Saphir macro for auto trait implementation on controllers
//!
//! The base macro attribule look like this : `#[controller]` and is to be put on top of a Controller's method impl block
//!
//! ```ignore
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
//! ```ignore
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

use proc_macro::TokenStream as TokenStream1;

use proc_macro2::{Ident, Span, TokenStream};
use syn::{export::ToTokens, parse_macro_input, AttributeArgs, ItemImpl, Result};

use quote::quote;

use crate::{
    controller::ControllerAttr,
    handler::{HandlerRepr, HandlerWrapperOpt, MapAfterLoad},
};

mod controller;
mod handler;

#[proc_macro_attribute]
pub fn controller(args: TokenStream1, input: TokenStream1) -> TokenStream1 {
    let args = parse_macro_input!(args as AttributeArgs);
    let input = parse_macro_input!(input as ItemImpl);

    let expanded = expand_controller(args, input).unwrap_or_else(|e| e.to_compile_error());

    TokenStream1::from(expanded)
}

fn expand_controller(args: AttributeArgs, input: ItemImpl) -> Result<TokenStream> {
    let controller_attr = ControllerAttr::new(args, &input)?;
    let handlers = handler::parse_handlers(input)?;

    let controller_implementation = controller::gen_controller_trait_implementation(&controller_attr, handlers.as_slice());
    let struct_implementaion = gen_struct_implementation(controller_attr.ident.clone(), handlers);

    let mod_ident = Ident::new(&format!("SAPHIR_GEN_CONTROLLER_{}", &controller_attr.name), Span::call_site());
    Ok(quote! {
        mod #mod_ident {
            use super::*;
            use saphir::prelude::*;
            use std::str::FromStr;
            #struct_implementaion

            #controller_implementation
        }
    })
}

fn gen_struct_implementation(controller_ident: Ident, handlers: Vec<HandlerRepr>) -> TokenStream {
    let mut handler_tokens = TokenStream::new();
    for handler in handlers {
        if handler.wrapper_options.needs_wrapper_fn() {
            gen_wrapper_handler(&mut handler_tokens, handler);
        } else {
            handler.original_method.to_tokens(&mut handler_tokens);
        }
    }

    let e = quote! {
        impl #controller_ident {
            #handler_tokens
        }
    };

    e
}

fn gen_wrapper_handler(handler_tokens: &mut TokenStream, handler: HandlerRepr) {
    let opts = &handler.wrapper_options;
    let m_ident = handler.original_method.sig.ident.clone();
    let return_type = handler.return_type;
    let mut o_method = handler.original_method;
    let mut m_inner_ident_str = m_ident.to_string();
    m_inner_ident_str.push_str("_wrapped");

    o_method.attrs.push(syn::parse_quote! {#[inline]});
    o_method.sig.ident = Ident::new(m_inner_ident_str.as_str(), Span::call_site());
    let inner_method_ident = o_method.sig.ident.clone();

    o_method.to_tokens(handler_tokens);

    let mut body_stream = TokenStream::new();
    (quote! {let mut req = req}).to_tokens(&mut body_stream);
    gen_body_mapping(&mut body_stream, opts);
    gen_body_load(&mut body_stream, opts);
    gen_map_after_load(&mut body_stream, opts);
    (quote! {;}).to_tokens(&mut body_stream);
    let parse_cookies = gen_cookie_load(opts);
    let inner_call = gen_call_to_inner(inner_method_ident, opts);

    let t = quote! {
        #[allow(unused_mut)]
        async fn #m_ident(&self, mut req: Request) -> Result<#return_type, SaphirError> {
            #body_stream
            #parse_cookies
            Ok(#inner_call)
        }
    };

    t.to_tokens(handler_tokens);
}

fn gen_cookie_load(opts: &HandlerWrapperOpt) -> TokenStream {
    if opts.parse_cookies {
        return quote! {
            req.parse_cookies();
        };
    }

    quote! {}
}

fn gen_body_load(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if opts.need_body_load {
        (quote! {.load_body().await?}).to_tokens(stream);
    }
}

fn gen_map_after_load(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if let Some(m) = &opts.map_after_load {
        (match m {
            MapAfterLoad::Json => {
                quote! {.map(|b| Json(b))}
            }
            MapAfterLoad::Form => {
                quote! {.map(|b| Form(b))}
            }
        })
        .to_tokens(stream)
    }
}

fn gen_body_mapping(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if let Some(ty) = &opts.take_body_as {
        (quote! {.map(|mut b| b.take_as::<#ty>())}).to_tokens(stream);
    }
}

fn gen_call_to_inner(inner_method_ident: Ident, opts: &HandlerWrapperOpt) -> TokenStream {
    let mut call = TokenStream::new();

    (quote! {self.#inner_method_ident}).to_tokens(&mut call);

    gen_call_params(opts).to_tokens(&mut call);

    if !opts.sync_handler {
        (quote! {.await}).to_tokens(&mut call);
    }

    call
}

fn gen_call_params(opts: &HandlerWrapperOpt) -> TokenStream {
    let mut params = TokenStream::new();
    let paren = syn::token::Paren { span: Span::call_site() };

    paren.surround(&mut params, |params| {
        if !opts.request_unused {
            (quote! {req}).to_tokens(params)
        }
    });

    params
}
