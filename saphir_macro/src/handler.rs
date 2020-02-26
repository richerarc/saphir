use http::Method;
use proc_macro2::Ident;
use syn::{Attribute, AttrStyle, Error, ImplItem, ImplItemMethod, ItemFn, ItemImpl, AttributeArgs, NestedMeta, Lit, Meta, Path, MetaNameValue};
use syn::parse_macro_input;
use syn::parse::{Parse, ParseBuffer, Result};

use quote::quote;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use syn::export::{Span, ToTokens};
use proc_macro2::TokenStream;
use crate::controller::ControllerAttr;

pub struct HandlerAttrs {
    method: Method,
    path: String,
    guards: Vec<(Path, Option<Path>)>,
    is_async: bool,
}

impl HandlerAttrs {
    pub fn new(mut attrs: Vec<Attribute>) -> Self {
        let mut method = None;
        let mut path = String::new();
        let mut guards = Vec::new();

        let metas = attrs.iter_mut().map(|attr| attr.parse_meta().expect("Invalid function arguments")).collect::<Vec<Meta>>();
        for meta in metas {
            match meta {
                Meta::List(l) => {
                    if let Some(ident) = l.path.get_ident() {
                        if ident.to_string().eq("guard") {
                            let mut fn_path = None;
                            let mut data_path = None;

                            for n in l.nested {
                                if let NestedMeta::Meta(Meta::NameValue(MetaNameValue { path, eq_token: _, lit: Lit::Str(l) })) = n {
                                    let path = path.segments.first().expect("Missing path in guard attributes");
                                    match path.ident.to_string().as_str() {
                                        "fn" => {
                                            fn_path = syn::parse_str::<Path>(l.value().as_str()).ok();
                                        }

                                        "data" => {
                                            data_path = syn::parse_str::<Path>(l.value().as_str()).ok();
                                        }

                                        _ => { panic!("Unauthorized name in guard macro") }
                                    }
                                }
                            }

                            guards.push((fn_path.expect("Missing guard funtion"), data_path));

                        } else {
                            method = Some(Method::from_str(ident.to_string().to_uppercase().as_str()).expect("Invalid HTTP method"));

                            if let Some(NestedMeta::Lit(Lit::Str(str))) = l.nested.first() {
                                path = str.value();
                                if !path.starts_with("/") {
                                    panic!("Path must start with '/'")
                                }
                            }
                        }
                    }
                }
                Meta::NameValue(_) => { panic!("Invalid format") }
                Meta::Path(_) => { panic!("Invalid format") }
            }
        }

        HandlerAttrs {
            method: method.expect("HTTP method is missing"),
            path,
            guards,
            is_async: true,
        }
    }
}

pub fn parse_handlers(input: &ItemImpl) -> Vec<ImplItemMethod> {
    let mut vec = Vec::new();
    for item in &input.items {
        if let ImplItem::Method(m) = item {
            vec.push(m.clone());
        }
    }

    vec
}

pub fn gen_handlers_fn(attr: &ControllerAttr, handlers: Vec<ImplItemMethod>) -> TokenStream {
    let mut handler_stream = TokenStream::new();
    let ctrl_ident = attr.ident.clone();
    for handler in handlers {
        let HandlerAttrs { method, path, guards, is_async: _ } = HandlerAttrs::new(handler.attrs);
        let method = Ident::new(method.as_str(), Span::call_site());
        let handler_ident = handler.sig.ident;

        if guards.is_empty() {
            let handler_e = quote! {
                let b = b.add(Method::#method, #path, #ctrl_ident::#handler_ident);
            };
            handler_e.to_tokens(&mut handler_stream);
        } else {
            // let handler_e = quote! {
            //     let b = b.add_with_guards(Method::#method, #path, #ctrl_ident::#handler_ident, |g| {
            //         g.add(numeric_delay_guard, ())
            //     });
            // };
            // handler_e.to_tokens(&mut handler_stream);
        }
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