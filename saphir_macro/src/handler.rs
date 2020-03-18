use std::str::FromStr;

use http::Method;
use proc_macro2::Ident;
use syn::{
    Attribute, Error, FnArg, GenericArgument, ImplItem, ImplItemMethod, ItemImpl, Lit, Meta, MetaNameValue, NestedMeta, Pat, PatIdent, PatType, Path,
    PathArguments, PathSegment, Result, ReturnType, Type, TypePath,
};

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

#[derive(Clone, Debug)]
pub enum ArgsReprType {
    SelfType,
    Request,
    Json,
    Form,
    Multipart,
    Params { is_query_param: bool, is_string: bool },
    Cookie,
    Option(Box<ArgsReprType>),
}

impl ArgsReprType {
    pub fn new(attrs: &HandlerAttrs, name: &str, p: &PathSegment) -> Result<Self> {
        let typ_ident_str = p.ident.to_string();
        match typ_ident_str.as_str() {
            "Request" => Ok(ArgsReprType::Request),
            "CookieJar" => Ok(ArgsReprType::Cookie),
            "Json" => Ok(ArgsReprType::Json),
            "Form" => Ok(ArgsReprType::Form),
            "Multipart" => Ok(ArgsReprType::Multipart),
            "Option" => {
                if let PathArguments::AngleBracketed(a) = &p.arguments {
                    let a = a.args.first().ok_or_else(|| Error::new_spanned(a, "Option types need an type argument"))?;
                    if let GenericArgument::Type(Type::Path(t)) = a {
                        let p2 = t
                            .path
                            .segments
                            .first()
                            .ok_or_else(|| Error::new_spanned(a, "Option types need an type path argument"))?;
                        return Ok(ArgsReprType::Option(Box::new(ArgsReprType::new(attrs, name, p2)?)));
                    }
                }
                Err(Error::new_spanned(p, "Invalid option type"))
            }
            _params => Ok(ArgsReprType::Params {
                is_query_param: !attrs.methods_paths.iter().any(|(_, path)| path.contains(name)),
                is_string: typ_ident_str.eq("String"),
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArgsRepr {
    pub name: String,
    pub typ: Option<TypePath>,
    pub a_type: ArgsReprType,
}

impl ArgsRepr {
    pub fn new(attrs: &HandlerAttrs, a: &FnArg) -> Result<ArgsRepr> {
        match a {
            FnArg::Receiver(r) => {
                if r.mutability.is_some() {
                    return Err(Error::new_spanned(r, "Controller references are immutable static references, remove 'mut'"));
                }

                if r.reference.is_none() {
                    return Err(Error::new_spanned(r, "Controller cannot be passed as owned value, please use &self"));
                }

                Ok(ArgsRepr {
                    name: "self".to_string(),
                    typ: None,
                    a_type: ArgsReprType::SelfType,
                })
            }
            FnArg::Typed(t) => match t.pat.as_ref() {
                Pat::Ident(i) => Self::parse_pat_ident(attrs, t, i),
                Pat::Reference(r) => Err(Error::new_spanned(r.and_token, "Unexpected referece, help: remove '&'")),
                _ => Err(Error::new_spanned(t, "Unexpected handler argument format")),
            },
        }
    }

    fn parse_pat_ident(attrs: &HandlerAttrs, t: &PatType, i: &PatIdent) -> Result<Self> {
        let name = i.ident.to_string();

        if let Some(rf) = i.by_ref {
            return Err(Error::new_spanned(rf, "Invalid handler argument, consider removing 'ref'"));
        }
        if let Type::Path(p) = t.ty.as_ref() {
            let typ = Some(p.clone());
            let p = p
                .path
                .segments
                .first()
                .ok_or_else(|| Error::new_spanned(p, "Invalid handler argument, argument should have an ident"))?;
            let a_type = ArgsReprType::new(attrs, &name, p)?;

            return Ok(ArgsRepr { name, typ, a_type });
        } else if let Type::Reference(r) = t.ty.as_ref() {
            return Err(Error::new_spanned(r.and_token, "Unexpected reference, help: remove '&'"));
        }

        Err(Error::new_spanned(i, "Invalid handler argument, argument should be TypePath"))
    }

    pub fn is_string(&self) -> bool {
        match self.a_type {
            ArgsReprType::Params { is_string, .. } => is_string,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HandlerWrapperOpt {
    pub sync_handler: bool,
    pub need_body_load: bool,
    pub request_unused: bool,
    pub parse_cookies: bool,
    pub parse_query: bool,
    pub init_multipart: bool,
    pub map_multipart: bool,
    pub take_body_as: Option<TypePath>,
    pub map_after_load: Option<MapAfterLoad>,
    pub fn_arguments: Vec<ArgsRepr>,
}

impl HandlerWrapperOpt {
    pub fn new(attrs: &HandlerAttrs, m: &ImplItemMethod) -> Result<Self> {
        let mut sync_handler = false;
        let mut need_body_load = false;
        let mut request_unused = true;
        let mut parse_query = false;
        let mut parse_cookies = attrs.cookie;
        let mut take_body_as = None;
        let mut map_after_load = None;
        let mut init_multipart = false;
        let mut map_multipart = false;

        if m.sig.asyncness.is_none() {
            sync_handler = true
        }

        let fn_arguments = m.sig.inputs.iter().map(|fn_a| ArgsRepr::new(attrs, fn_a)).collect::<Result<Vec<ArgsRepr>>>()?;

        fn_arguments.iter().for_each(|a_repr| match &a_repr.a_type {
            ArgsReprType::Cookie => parse_cookies = true,
            ArgsReprType::Params { is_query_param: true, .. } => parse_query = true,
            ArgsReprType::Multipart => init_multipart = true,
            ArgsReprType::Option(inner) => {
                if let ArgsReprType::Params { is_query_param: true, .. } = inner.as_ref() {
                    parse_query = true;
                }
            }
            _ => {}
        });

        let req_param: Option<&ArgsRepr> = fn_arguments
            .iter()
            .find(|a_repr| if let ArgsReprType::Request = a_repr.a_type { true } else { false });

        if let Some(req_param) = req_param {
            request_unused = false;
            if let Some(PathArguments::AngleBracketed(a)) = req_param.typ.as_ref().and_then(|t| t.path.segments.first()).map(|s| &s.arguments) {
                if let Some(GenericArgument::Type(Type::Path(request_body_type))) = a.args.first() {
                    let request_body_type = request_body_type.path.segments.first();
                    let a = match request_body_type
                        .as_ref()
                        .map(|b| (b.ident.to_string(), &b.arguments))
                        .as_ref()
                        .map(|(id, ar)| (id.as_str(), ar))
                    {
                        // Body<_>
                        Some(("Body", PathArguments::AngleBracketed(a2))) => Some(a2),
                        // Type<_> of Type
                        Some((_, PathArguments::AngleBracketed(_))) => {
                            need_body_load = true;
                            Some(a)
                        }
                        Some((id, PathArguments::None)) => {
                            if id == "Multipart" {
                                init_multipart = true;
                                map_multipart = true;
                            } else {
                                need_body_load = true;
                            }
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
        }

        Ok(HandlerWrapperOpt {
            sync_handler,
            need_body_load,
            request_unused,
            parse_cookies,
            parse_query,
            init_multipart,
            map_multipart,
            take_body_as,
            map_after_load,
            fn_arguments,
        })
    }

    pub fn needs_wrapper_fn(&self) -> bool {
        self.sync_handler
            || self.need_body_load
            || self.request_unused
            || self.take_body_as.is_some()
            || self.parse_query
            || self.parse_cookies
            || self.fn_arguments.len() > 2
    }
}

#[derive(Clone)]
pub struct HandlerAttrs {
    pub methods_paths: Vec<(Method, String)>,
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
        let attrs = HandlerAttrs::new(m.attrs.drain(..).collect(), &m)?;
        let wrapper_options = HandlerWrapperOpt::new(&attrs, &m)?;
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
    pub fn new(mut attrs: Vec<Attribute>, m: &ImplItemMethod) -> Result<Self> {
        let mut methods_paths = Vec::new();
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
                            let method =
                                Method::from_str(ident.to_string().to_uppercase().as_str()).map_err(|_e| Error::new_spanned(ident, "Invalid HTTP method"))?;

                            if let Some(NestedMeta::Lit(Lit::Str(str))) = l.nested.first() {
                                let path = str.value();
                                if !path.starts_with('/') {
                                    return Err(Error::new_spanned(str, "Path must start with '/'"));
                                }

                                methods_paths.push((method, path));
                            } else {
                                return Err(Error::new_spanned(l, "Missing path for method"));
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

        if methods_paths.is_empty() {
            return Err(Error::new_spanned(
                &m.sig,
                "Missing Router attribute for handler, help: adde something like `#[get(\"/\")]`",
            ));
        }

        Ok(HandlerAttrs { methods_paths, guards, cookie })
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
