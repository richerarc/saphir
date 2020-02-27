use http::Method;
use proc_macro2::Ident;
use syn::{Attribute, AttrStyle, Error, ImplItem, ImplItemMethod, ItemFn, ItemImpl, AttributeArgs, NestedMeta, Lit, Meta, Path, MetaNameValue, ReturnType, Type, FnArg, PathArguments, AngleBracketedGenericArguments, GenericArgument, TypePath};
use syn::parse_macro_input;
use syn::parse_quote;
use syn::parse::{Parse, ParseBuffer, Result};

use quote::quote;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use syn::export::{Span, ToTokens, TokenStreamExt};
use proc_macro2::TokenStream;
use crate::controller::ControllerAttr;

#[derive(Clone, Debug)]
pub enum MapAfterLoad {
    Json,
    Form
}

impl MapAfterLoad {
    pub fn new(i: &Ident) -> Option<Self> {
        match i.to_string().as_str() {
            "Json" => Some(MapAfterLoad::Json),
            "Form" => Some(MapAfterLoad::Form),
            _ => None
        }
    }
}

#[derive(Clone, Debug)]
pub struct HandlerWrapperOpt {
    pub sync_handler: bool,
    pub need_body_load: bool,
    pub request_unused: bool,
    pub take_body_as: Option<TypePath>,
    pub map_after_load: Option<MapAfterLoad>,
}

impl HandlerWrapperOpt {
    pub fn new(m: &ImplItemMethod) -> Self {
        let mut sync_handler = false;
        let mut need_body_load = false;
        let mut request_unused = true;
        let mut take_body_as = None;
        let mut map_after_load = None;

        if m.sig.asyncness.is_none() {
            sync_handler = true
        }

        m.sig.inputs.iter().for_each(|fn_arg| {
            if let FnArg::Typed(t) = fn_arg {
                if let Type::Path(p) = &*t.ty {
                    let f = p.path.segments.first();
                    match f.map(|s| (s.ident.to_string().eq("Request"), &s.arguments)) {
                        Some((true, PathArguments::AngleBracketed(a))) => {
                            request_unused = false;
                            if let Some(GenericArgument::Type(Type::Path(request_body_type))) = a.args.first() {
                                let request_body_type = request_body_type.path.segments.first();
                                let a = match request_body_type.as_ref().map(|b| {
                                    (b.ident.to_string().eq("Body"), &b.arguments)
                                }) {
                                    // Body<_>
                                    Some((true, PathArguments::AngleBracketed(a2))) => {
                                        Some(a2)
                                    }
                                    // Type<_>
                                    Some((false, PathArguments::AngleBracketed(_))) => {
                                        need_body_load = true;
                                        Some(a)
                                    }

                                    _ => {
                                        None
                                    }
                                };

                                take_body_as = take_body_as.take().or_else(|| {
                                    a.and_then(|a| a.args.first().and_then(|arg| {
                                        if let GenericArgument::Type(Type::Path(typ)) = arg {
                                            let ident = typ.path.segments.first().map(|t| &t.ident);
                                            if let Some(ident) = ident {
                                                if !ident.to_string().eq("Bytes") {
                                                    map_after_load = MapAfterLoad::new(ident);
                                                    return Some(typ.clone());
                                                }
                                            }
                                        }
                                        None
                                    }))
                                });
                            }
                        }
                        Some((true, _)) => {
                            request_unused = false;
                        }

                        _ => {}
                    }
                }
            }
        });

        HandlerWrapperOpt {
            sync_handler,
            need_body_load,
            request_unused,
            take_body_as,
            map_after_load,
        }
    }

    pub fn needs_wrapper_fn(&self) -> bool {
        self.sync_handler || self.need_body_load || self.request_unused || self.take_body_as.is_some()
    }
}

#[derive(Clone)]
pub struct HandlerAttrs {
    pub method: Method,
    pub path: String,
    pub guards: Vec<(Path, Option<Path>)>,
}

#[derive(Clone)]
pub struct HandlerRepr {
    pub attrs: HandlerAttrs,
    pub original_method: ImplItemMethod,
    pub return_type: Box<Type>,
    pub wrapper_options: HandlerWrapperOpt,
}

impl HandlerRepr {
    pub fn new(mut m: ImplItemMethod) -> Self {
        let wrapper_options = HandlerWrapperOpt::new(&m);
        let return_type = if let ReturnType::Type(_0, typ) = &m.sig.output {
            typ.clone()
        } else {
            panic!("Invalid handler return type")
        };
        HandlerRepr {
            attrs: HandlerAttrs::new(m.attrs.drain(..).collect()),
            original_method: m,
            return_type,
            wrapper_options,
        }
    }
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
        }
    }
}

pub fn parse_handlers(input: ItemImpl) -> Vec<HandlerRepr> {
    let mut vec = Vec::new();
    for item in input.items {
        if let ImplItem::Method(m) = item {
            vec.push(HandlerRepr::new(m));
        }
    }

    vec
}