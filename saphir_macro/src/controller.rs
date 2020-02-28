use crate::handler::{HandlerAttrs, HandlerRepr};
use proc_macro2::{Ident, TokenStream};
use syn::{AttributeArgs, ItemImpl, Lit, Meta, MetaList, MetaNameValue, NestedMeta, Type};

use quote::quote;
use syn::export::{Span, ToTokens};

#[derive(Debug)]
pub struct ControllerAttr {
    pub ident: Ident,
    pub name: String,
    pub version: Option<u16>,
    pub prefix: Option<String>,
}

impl ControllerAttr {
    pub fn new(args: AttributeArgs, input: &ItemImpl) -> ControllerAttr {
        let mut name = None;
        let mut version = None;
        let mut prefix = None;

        let ident = parse_controller_ident(input).expect("A controller can't have no ident");

        for m in args.into_iter().filter_map(|a| if let NestedMeta::Meta(m) = a { Some(m) } else { None }) {
            match m {
                Meta::Path(_p) => {
                    // no path at this time
                    // a path is => #[test]
                    //                ----
                }
                Meta::List(MetaList {
                    path: _,
                    paren_token: _,
                    nested: _,
                }) => {
                    // no List at this time
                    // a List is => #[test(A, B)]
                    //                ----------
                }
                Meta::NameValue(MetaNameValue { path, eq_token: _, lit }) => {
                    match (path.segments.first().map(|p| p.ident.to_string()).as_ref().map(|s| s.as_str()), lit) {
                        (Some("name"), Lit::Str(bp)) => {
                            name = Some(bp.value().trim_matches('/').to_string());
                        }
                        (Some("version"), Lit::Str(v)) => {
                            version = Some(v.value().parse::<u16>().expect("Invalid version, expected number between 1 & u16::MAX"))
                        }
                        (Some("version"), Lit::Int(v)) => {
                            version = Some(v.base10_parse::<u16>().expect("Invalid version, expected number between 1 & u16::MAX"))
                        }
                        (Some("prefix"), Lit::Str(p)) => {
                            prefix = Some(p.value().trim_matches('/').to_string());
                        }
                        _ => {}
                    }
                }
            }
        }

        let name = name.unwrap_or_else(|| ident.to_string().to_lowercase().trim_end_matches("controller").to_string());

        ControllerAttr { ident, name, version, prefix }
    }
}

fn parse_controller_ident(input: &ItemImpl) -> Option<Ident> {
    match input.self_ty.as_ref() {
        Type::Path(p) => {
            if let Some(f) = p.path.segments.first() {
                return Some(f.ident.clone());
            }
        }
        _ => {}
    }

    None
}

pub fn gen_controller_trait_implementation(attrs: &ControllerAttr, handlers: &[HandlerRepr]) -> TokenStream {
    let controller_base_path = gen_controller_base_path_const(attrs);
    let controller_handlers_fn = gen_controller_handlers_fn(attrs, handlers);

    let ident = &attrs.ident;
    let e = quote! {
        impl Controller for #ident {
            #controller_base_path

            #controller_handlers_fn
        }
    };

    e
}

fn gen_controller_base_path_const(attr: &ControllerAttr) -> TokenStream {
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

fn gen_controller_handlers_fn(attr: &ControllerAttr, handlers: &[HandlerRepr]) -> TokenStream {
    let mut handler_stream = TokenStream::new();
    let ctrl_ident = attr.ident.clone();

    for handler in handlers {
        let HandlerAttrs { method, path, guards } = &handler.attrs;
        let method = Ident::new(method.as_str(), Span::call_site());
        let handler_ident = handler.original_method.sig.ident.clone();

        if guards.is_empty() {
            let handler_e = quote! {
                let b = b.add(Method::#method, #path, #ctrl_ident::#handler_ident);
            };
            handler_e.to_tokens(&mut handler_stream);
        } else {
            let mut guard_stream = TokenStream::new();

            for (fn_path, data) in guards {
                let guard_e = if let Some(data) = data {
                    quote! {
                        let g = g.add(#fn_path, #data(self));
                    }
                } else {
                    quote! {
                        let g = g.add(#fn_path, ());
                    }
                };

                guard_e.to_tokens(&mut guard_stream);
            }

            let handler_e = quote! {
                let b = b.add_with_guards(Method::#method, #path, #ctrl_ident::#handler_ident, |g| {
                    #guard_stream
                    g
                });
            };
            handler_e.to_tokens(&mut handler_stream);
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
