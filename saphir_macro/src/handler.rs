use http::Method;
use proc_macro2::Ident;
use syn::{Attribute, AttrStyle, Error, ImplItem, ImplItemMethod, ItemFn, ItemImpl, AttributeArgs, NestedMeta, Lit, Meta};
use syn::parse_macro_input;
use syn::parse::{Parse, ParseBuffer, Result};

use quote::quote;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use syn::export::{Span, ToTokens};
use proc_macro2::TokenStream;
use crate::controller::ControllerAttr;

pub fn parse_handlers(input: &ItemImpl) -> Vec<ImplItemMethod> {
    let mut vec = Vec::new();
    for item in &input.items {
        if let ImplItem::Method(m) = item {
            vec.push(m.clone());
        }
    }

    vec
}

pub fn parse_fn_metas(mut attrs: Vec<Attribute>) -> (Method, String) {
    let mut method = None;
    let mut path = String::new();

    let metas = attrs.iter_mut().map(|attr| attr.parse_meta().expect("Invalid function arguments")).collect::<Vec<Meta>>();
    for meta in metas {
        match meta {
            Meta::List(l) => {
                if let Some(ident) = l.path.get_ident() {
                    method = Some(Method::from_str(ident.to_string().to_uppercase().as_str()).expect("Invalid HTTP method"));
                }

                if let Some(NestedMeta::Lit(Lit::Str(str))) = l.nested.first() {
                    path = str.value();
                    if !path.starts_with("/") {
                        panic!("Path must start with '/'")
                    }
                }
            }
            Meta::NameValue(_) => { panic!("Invalid format") }
            Meta::Path(_) => { panic!("Invalid format") }
        }
    }

    (method.expect("HTTP method is missing"), path)
}

pub fn gen_handlers_fn(attr: &ControllerAttr, handlers: Vec<ImplItemMethod>) -> TokenStream {
    let mut handler_stream = TokenStream::new();
    let ctrl_ident = attr.ident.clone();
    for handler in handlers {
        let (method, path) = parse_fn_metas(handler.attrs);
        let method = Ident::new(method.as_str(), Span::call_site());
        let handler_ident = handler.sig.ident;

        let handler_e = quote! {
            let b = b.add(Method::#method, #path, #ctrl_ident::#handler_ident);
        };
        handler_e.to_tokens(&mut handler_stream);
    }

    let e = quote! {
        fn handlers(&self) -> Vec<ControllerEndpoint<Self>> where Self: Sized {
            let b = EndpointsBuilder::new();

            #handler_stream

            b.build()
        }
    };

    e
}

pub struct HandlerOptions {
    pub paths_and_methods: HashMap<String, HashSet<Method>>,
    pub cookies: bool,
    pub guards: Vec<String>,
}