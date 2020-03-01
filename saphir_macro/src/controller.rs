use crate::handler::{HandlerAttrs, HandlerRepr};
use proc_macro2::{Ident, TokenStream};
use syn::{AttributeArgs, Error, ItemImpl, Lit, Meta, MetaNameValue, NestedMeta, Result, Type};

use quote::quote;
use syn::export::ToTokens;

#[derive(Debug)]
pub struct ControllerAttr {
    pub ident: Ident,
    pub name: String,
    pub version: Option<u16>,
    pub prefix: Option<String>,
}

impl ControllerAttr {
    pub fn new(args: AttributeArgs, input: &ItemImpl) -> Result<ControllerAttr> {
        let mut name = None;
        let mut version = None;
        let mut prefix = None;

        let ident = parse_controller_ident(input)?;

        for m in args.into_iter().filter_map(|a| if let NestedMeta::Meta(m) = a { Some(m) } else { None }) {
            match m {
                Meta::Path(p) => {
                    return Err(Error::new_spanned(p, "Unexpected Attribute on controller impl"));
                }
                Meta::List(l) => {
                    return Err(Error::new_spanned(l, "Unexpected Attribute on controller impl"));
                }
                Meta::NameValue(MetaNameValue { path, lit, .. }) => {
                    match (path.segments.first().map(|p| p.ident.to_string()).as_ref().map(|s| s.as_str()), lit) {
                        (Some("name"), Lit::Str(bp)) => {
                            name = Some(bp.value().trim_matches('/').to_string());
                        }
                        (Some("version"), Lit::Str(v)) => {
                            version = Some(
                                v.value()
                                    .parse::<u16>()
                                    .or_else(|_| Err(Error::new_spanned(v, "Invalid version, expected number between 1 & u16::MAX")))?,
                            );
                        }
                        (Some("version"), Lit::Int(v)) => {
                            version = Some(
                                v.base10_parse::<u16>()
                                    .or_else(|_| Err(Error::new_spanned(v, "Invalid version, expected number between 1 & u16::MAX")))?,
                            );
                        }
                        (Some("prefix"), Lit::Str(p)) => {
                            prefix = Some(p.value().trim_matches('/').to_string());
                        }
                        _ => {
                            return Err(Error::new_spanned(path, "Unexpected Param in controller macro"));
                        }
                    }
                }
            }
        }

        let name = name.unwrap_or_else(|| ident.to_string().to_lowercase().trim_end_matches("controller").to_string());

        Ok(ControllerAttr { ident, name, version, prefix })
    }
}

fn parse_controller_ident(input: &ItemImpl) -> Result<Ident> {
    if let Type::Path(p) = input.self_ty.as_ref() {
        if let Some(f) = p.path.segments.first() {
            return Ok(f.ident.clone());
        }
    }

    Err(Error::new_spanned(input, "Unable to parse impl ident. this is fatal"))
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
        let HandlerAttrs { methods_paths, guards, .. } = &handler.attrs;
        let handler_ident = handler.original_method.sig.ident.clone();

        for (method, path) in methods_paths {
            let method = method.as_str();
            if guards.is_empty() {
                let handler_e = quote! {
                    let b = b.add(Method::from_str(#method).expect("Method was validated by the macro expansion"), #path, #ctrl_ident::#handler_ident);
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
                    let b = b.add_with_guards(Method::from_str(#method).expect("Method was validated the macro expansion"), #path, #ctrl_ident::#handler_ident, |g| {
                        #guard_stream
                        g
                    });
                };
                handler_e.to_tokens(&mut handler_stream);
            }
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
