use http::Method;
use proc_macro2::{Ident, Span};
use syn::{
    Attribute, Error, FnArg, GenericArgument, ImplItem, ImplItemMethod, ItemImpl, Lit, Meta, MetaNameValue, NestedMeta, Path, PathArguments, Result,
    ReturnType, Type, TypePath,
};

use std::str::FromStr;
use syn::spanned::Spanned;

#[derive(Clone, Debug)]
pub enum MapAfterLoad {
    Json,
    Form,
}

impl MapAfterLoad {
    pub fn new(i: &Ident) -> Option<Self> {
        match i.to_string().as_str() {
            "Json" => Some(MapAfterLoad::Json),
            "Form" => Some(MapAfterLoad::Form),
            _ => None,
        }
    }
}

pub enum ArgsReprType {
    Request,
    Json,
    Form,
    Params { is_query_param: bool },
    Cookie,
}

pub struct ArgsRepr {
    name: String,
    typ: TypePath,
    a_type: ArgsReprType,
}

impl ArgsRepr {
    pub fn new(_a: &FnArg) -> Result<ArgsRepr> {
        Err(Error::new(Span::call_site(), "whatever"))
    }
}

#[derive(Clone, Debug)]
pub struct HandlerWrapperOpt {
    pub sync_handler: bool,
    pub need_body_load: bool,
    pub request_unused: bool,
    pub parse_cookies: bool,
    pub take_body_as: Option<TypePath>,
    pub map_after_load: Option<MapAfterLoad>,
}

impl HandlerWrapperOpt {
    pub fn new(attrs: &HandlerAttrs, m: &ImplItemMethod) -> Self {
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
                                let a = match request_body_type.as_ref().map(|b| (b.ident.to_string().eq("Body"), &b.arguments)) {
                                    // Body<_>
                                    Some((true, PathArguments::AngleBracketed(a2))) => Some(a2),
                                    // Type<_>
                                    Some((false, PathArguments::AngleBracketed(_))) => {
                                        need_body_load = true;
                                        Some(a)
                                    }

                                    _ => None,
                                };

                                take_body_as = take_body_as.take().or_else(|| {
                                    a.and_then(|a| {
                                        a.args.first().and_then(|arg| {
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
                                        })
                                    })
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
            parse_cookies: attrs.cookie,
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
    pub cookie: bool,
}

#[derive(Clone)]
pub struct HandlerRepr {
    pub attrs: HandlerAttrs,
    pub original_method: ImplItemMethod,
    pub return_type: Box<Type>,
    pub wrapper_options: HandlerWrapperOpt,
}

impl HandlerRepr {
    pub fn new(mut m: ImplItemMethod) -> Result<Self> {
        let attrs = HandlerAttrs::new(m.attrs.drain(..).collect())?;
        let wrapper_options = HandlerWrapperOpt::new(&attrs, &m);
        let return_type = if let ReturnType::Type(_0, typ) = &m.sig.output {
            typ.clone()
        } else {
            return Err(Error::new_spanned(m.sig, "Invalid handler return type"));
        };
        Ok(HandlerRepr {
            attrs,
            original_method: m,
            return_type,
            wrapper_options,
        })
    }
}

impl HandlerAttrs {
    pub fn new(mut attrs: Vec<Attribute>) -> Result<Self> {
        let mut method = None;
        let mut path = String::new();
        let mut guards = Vec::new();
        let mut cookie = false;

        let metas = attrs.iter_mut().map(|attr| attr.parse_meta()).collect::<Result<Vec<Meta>>>()?;
        for meta in metas {
            match meta {
                Meta::List(l) => {
                    if let Some(ident) = l.path.get_ident() {
                        if ident.to_string().eq("guard") {
                            let mut fn_path = None;
                            let mut data_path = None;

                            for n in l.nested {
                                if let NestedMeta::Meta(Meta::NameValue(MetaNameValue { path, lit: Lit::Str(l), .. })) = n {
                                    let path = path
                                        .segments
                                        .first()
                                        .ok_or_else(|| Error::new_spanned(&path, "Missing parameters in guard attributes"))?;
                                    match path.ident.to_string().as_str() {
                                        "fn" => {
                                            fn_path = Some(
                                                syn::parse_str::<Path>(l.value().as_str())
                                                    .map_err(|_e| Error::new_spanned(path, "Expected path to a guard function"))?,
                                            );
                                        }

                                        "data" => {
                                            data_path = Some(
                                                syn::parse_str::<Path>(l.value().as_str())
                                                    .map_err(|_e| Error::new_spanned(path, "Expected path to a guard initializer function"))?,
                                            );
                                        }

                                        _ => {
                                            return Err(Error::new_spanned(path, "Unauthorized param in guard macro"));
                                        }
                                    }
                                }
                            }

                            guards.push((fn_path.ok_or_else(|| Error::new_spanned(&ident, "Missing guard function"))?, data_path));
                        } else {
                            method = Some(
                                Method::from_str(ident.to_string().to_uppercase().as_str()).map_err(|_e| Error::new_spanned(ident, "Invalid HTTP method"))?,
                            );

                            if let Some(NestedMeta::Lit(Lit::Str(str))) = l.nested.first() {
                                path = str.value();
                                if !path.starts_with('/') {
                                    return Err(Error::new_spanned(str, "Path must start with '/'"));
                                }
                            }
                        }
                    }
                }
                Meta::NameValue(n) => {
                    return Err(Error::new_spanned(n, "Invalid Handler attribute"));
                }
                Meta::Path(p) => {
                    if let Some(ident_str) = p.get_ident().map(|p| p.to_string()) {
                        if ident_str.starts_with("cookie") {
                            cookie = true;
                            continue;
                        }
                    }
                    return Err(Error::new_spanned(p, "Invalid Handler attribute"));
                }
            }
        }

        Ok(HandlerAttrs {
            method: method.ok_or_else(|| Error::new(attrs.first().map(|a| a.span()).unwrap_or_else(Span::call_site), "HTTP method is missing"))?,
            path,
            guards,
            cookie,
        })
    }
}

pub fn parse_handlers(input: ItemImpl) -> Result<Vec<HandlerRepr>> {
    let mut vec = Vec::new();
    for item in input.items {
        if let ImplItem::Method(m) = item {
            vec.push(HandlerRepr::new(m)?);
        }
    }

    Ok(vec)
}
