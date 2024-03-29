use http::Method;
use proc_macro2::{Ident, TokenStream};
use quote::{quote_spanned, ToTokens};
use std::str::FromStr;
use syn::{
    spanned::Spanned, Attribute, Error, Expr, FnArg, GenericArgument, ImplItem, ImplItemMethod, ItemImpl, Lit, Meta, MetaNameValue, NestedMeta, Pat, PatIdent,
    PatType, Path, PathArguments, PathSegment, Result, ReturnType, Type, TypePath,
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
    Ext,
    Extensions,
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
            "Ext" => Ok(ArgsReprType::Ext),
            "Extensions" => Ok(ArgsReprType::Extensions),
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
                is_query_param: !attrs.methods_paths.iter().any(|(_, path)| path.contains(&format!("<{}>", name))),
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
    #[cfg(feature = "validate-requests")]
    pub validated: bool,
    #[cfg(feature = "validate-requests")]
    pub is_vec: bool,
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
                    #[cfg(feature = "validate-requests")]
                    validated: false,
                    #[cfg(feature = "validate-requests")]
                    is_vec: false,
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

            #[cfg(feature = "validate-requests")]
            {
                let validated = !attrs.validator_exclusions.contains(&name);
                let mut is_vec = false;

                let typ_ident_str = p.ident.to_string();
                let mut validated_type = None;

                match typ_ident_str.as_str() {
                    "Json" => {
                        if let PathArguments::AngleBracketed(a) = &p.arguments {
                            let a = a.args.first().ok_or_else(|| Error::new_spanned(a, "Json types need an type argument"))?;
                            if let GenericArgument::Type(Type::Path(t)) = a {
                                validated_type = t.path.segments.first().map(|p2| p2.ident.to_string());
                            }
                        }
                    }
                    "Form" => {
                        if let PathArguments::AngleBracketed(a) = &p.arguments {
                            let a = a.args.first().ok_or_else(|| Error::new_spanned(a, "Form types need an type argument"))?;
                            if let GenericArgument::Type(Type::Path(t)) = a {
                                validated_type = t.path.segments.first().map(|p2| p2.ident.to_string());
                            }
                        }
                    }
                    _ => (),
                };

                if let Some("Vec") = validated_type.as_deref() {
                    is_vec = true;
                }

                return Ok(ArgsRepr {
                    name,
                    typ,
                    a_type,
                    validated,
                    is_vec,
                });
            }

            #[cfg(not(feature = "validate-requests"))]
            {
                return Ok(ArgsRepr { name, typ, a_type });
            }
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
                        Some(("Body", pat_arg)) => {
                            if let PathArguments::AngleBracketed(a2) = pat_arg {
                                Some(a2)
                            } else {
                                None
                            }
                        }
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
                                        if ident != "Bytes" {
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
pub struct GuardDef {
    pub guard_type: Path,
    pub init_data: Option<Lit>,
    pub init_expr: Option<Expr>,
    pub init_fn: Option<Path>,
}

impl ToTokens for GuardDef {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let span = tokens.span();
        let guard_type = &self.guard_type;
        if let Some(init_fn) = self.init_fn.as_ref() {
            init_fn.to_tokens(tokens);
        } else {
            quote_spanned!(span=> #guard_type::new).to_tokens(tokens);
        }

        let paren = syn::token::Paren { span };

        paren.surround(tokens, |t| {
            if self.init_fn.is_some() {
                quote_spanned!(span=> self).to_tokens(t);
                if self.init_expr.is_some() || self.init_data.is_some() {
                    syn::token::Comma { spans: [span] }.to_tokens(t);
                }
            }

            if let Some(init_expr) = self.init_expr.as_ref() {
                syn::token::Brace { span }.surround(t, |t2| {
                    init_expr.to_tokens(t2);
                });
            } else if let Some(init_data) = self.init_data.as_ref() {
                init_data.to_tokens(t);
            }
        });
    }
}

#[derive(Clone)]
pub struct HandlerAttrs {
    pub methods_paths: Vec<(Method, String)>,
    pub guards: Vec<GuardDef>,
    pub cookie: bool,
    #[cfg(feature = "validate-requests")]
    pub validator_exclusions: Vec<String>,
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
        let attrs = HandlerAttrs::new(std::mem::take(&mut m.attrs), &m)?;
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
    fn empty_with_capacity(capacity: usize) -> Self {
        Self {
            methods_paths: Vec::with_capacity(capacity),
            guards: Vec::with_capacity(capacity),
            cookie: false,
            #[cfg(feature = "validate-requests")]
            validator_exclusions: Vec::new(),
        }
    }

    pub fn new(attrs: Vec<Attribute>, method: &ImplItemMethod) -> Result<Self> {
        let mut handler = HandlerAttrs::empty_with_capacity(attrs.len());

        // FIXME: This contains a lot of duplication and could be simplified
        for attr in attrs {
            let ident = match attr.path.get_ident() {
                Some(i) if i == "cookie" => {
                    handler.cookie = true;
                    continue;
                }
                Some(i) => i,
                None => continue,
            };
            let meta = attr.parse_meta()?;
            match meta {
                Meta::List(attribute) => {
                    if ident == "guard" {
                        let mut guard_type_path = None;
                        let mut init_fn = None;
                        let mut init_data = None;
                        let mut init_expr = None;

                        for guard_meta in attribute.nested {
                            if let NestedMeta::Meta(Meta::NameValue(MetaNameValue { path, lit, .. })) = guard_meta {
                                let path = path
                                    .segments
                                    .first()
                                    .ok_or_else(|| Error::new_spanned(&path, "Missing parameters in guard attributes"))?;
                                match path.ident.to_string().as_str() {
                                    "init_fn" => {
                                        if let Lit::Str(lit_str) = lit {
                                            init_fn = Some(
                                                syn::parse_str::<Path>(lit_str.value().as_str())
                                                    .map_err(|_e| Error::new_spanned(path, "Expected path to a guard function"))?,
                                            );
                                        } else {
                                            return Err(Error::new_spanned(lit, "Expected path to a guard function"));
                                        }
                                    }

                                    "init_data" => {
                                        init_data = Some(lit);
                                    }

                                    "init_expr" => {
                                        if let Lit::Str(lit_str) = lit {
                                            init_expr = Some(
                                                syn::parse_str::<Expr>(lit_str.value().as_str())
                                                    .map_err(|_e| Error::new_spanned(path, "Expected a valid rust expression"))?,
                                            );
                                        } else {
                                            return Err(Error::new_spanned(lit, "Expected a string of a valid rust expression"));
                                        }
                                    }

                                    _ => {
                                        return Err(Error::new_spanned(path, "Unauthorized param in guard macro"));
                                    }
                                }
                            } else if let NestedMeta::Meta(Meta::Path(p)) = guard_meta {
                                guard_type_path = Some(p);
                            }
                        }

                        let guard = GuardDef {
                            guard_type: guard_type_path.ok_or_else(|| Error::new_spanned(ident, "Missing guard"))?,
                            init_data,
                            init_fn,
                            init_expr,
                        };

                        handler.guards.push(guard);
                    } else if ident == "openapi" {
                        if attribute.nested.is_empty() {
                            return Err(Error::new_spanned(ident, "openapi attribute cannot be empty"));
                        }
                        for openapi_attributes in &attribute.nested {
                            match openapi_attributes {
                                NestedMeta::Meta(Meta::List(openapi_attribute)) => {
                                    let i = openapi_attribute.path.get_ident().map(|i| i.to_string());
                                    match i.as_deref() {
                                        Some("return") => {
                                            let mut nb_code = 0;
                                            let mut nb_type = 0;
                                            let mut nb_mime = 0;
                                            let mut nb_name = 0;
                                            if openapi_attribute.nested.is_empty() {
                                                return Err(Error::new_spanned(openapi_attribute, "openapi return attribute cannot be empty"));
                                            }
                                            for openapi_return_attribute in &openapi_attribute.nested {
                                                match openapi_return_attribute {
                                                    NestedMeta::Meta(m) => {
                                                        match m {
                                                            Meta::NameValue(nv) => {
                                                                let r = nv.path.get_ident().map(|i| i.to_string());
                                                                match r.as_deref() {
                                                                    Some("code") => {
                                                                        if let Lit::Int(i) = &nv.lit {
                                                                            let c: u16 =
                                                                                i.base10_parse().map_err(|_| Error::new_spanned(i, "Invalid status code"))?;
                                                                            if !(100..600).contains(&c) {
                                                                                return Err(Error::new_spanned(i, "Invalid status code"));
                                                                            }
                                                                            nb_code += 1;
                                                                        }
                                                                    }
                                                                    Some("type") => {
                                                                        if let Lit::Str(_) = &nv.lit {
                                                                            nb_type += 1;
                                                                        // TODO:
                                                                        // Validate
                                                                        // raw object
                                                                        // syntax
                                                                        } else {
                                                                            return Err(Error::new_spanned(
                                                                                m,
                                                                                "Invalid type : expected a type name/path/raw object wrapped in double-quotes",
                                                                            ));
                                                                        }
                                                                    }
                                                                    Some("mime") => {
                                                                        if let Lit::Str(_) = &nv.lit {
                                                                            nb_mime += 1;
                                                                        } else {
                                                                            return Err(Error::new_spanned(m, "Expected a mimetype string"));
                                                                        }
                                                                    }
                                                                    Some("name") => {
                                                                        if let Lit::Str(_) = &nv.lit {
                                                                            nb_name += 1;
                                                                        } else {
                                                                            return Err(Error::new_spanned(m, "Expected a string"));
                                                                        }
                                                                    }
                                                                    _ => return Err(Error::new_spanned(&nv.path, "Invalid openapi return attribute")),
                                                                }
                                                            }
                                                            _ => return Err(Error::new_spanned(m, "Invalid openapi return attribute")),
                                                        }
                                                    }
                                                    _ => return Err(Error::new_spanned(openapi_attribute, "Invalid openapi return attribute")),
                                                }
                                            }

                                            if nb_code == 0 {
                                                return Err(Error::new_spanned(openapi_attribute, "openapi return missing `code` value"));
                                            }

                                            if nb_type == 0 {
                                                return Err(Error::new_spanned(openapi_attribute, "openapi return missing `type` value"));
                                            }

                                            if nb_mime > 1 {
                                                return Err(Error::new_spanned(
                                                    openapi_attribute,
                                                    "openapi cannot have more than 1 `mime` for the same return",
                                                ));
                                            }

                                            if nb_name > 1 {
                                                return Err(Error::new_spanned(openapi_attribute, "Cannot specify the name twice"));
                                            }

                                            if nb_code > 1 && nb_type > 1 {
                                                return Err(Error::new_spanned(
                                                    openapi_attribute,
                                                    "openapi return cannot have both multiple codes and multiple types.\
                                                        \nPlease add a return() group for each code-type pair.
                                                    ",
                                                ));
                                            }
                                        }
                                        Some("return_override") => {
                                            let mut nb_code = 0;
                                            let mut nb_type = 0;
                                            let mut nb_mime = 0;
                                            let mut nb_name = 0;
                                            if openapi_attribute.nested.is_empty() {
                                                return Err(Error::new_spanned(openapi_attribute, "openapi return attribute cannot be empty"));
                                            }
                                            for openapi_return_attribute in &openapi_attribute.nested {
                                                match openapi_return_attribute {
                                                    NestedMeta::Meta(m) => {
                                                        match m {
                                                            Meta::NameValue(nv) => {
                                                                let r = nv.path.get_ident().map(|i| i.to_string());
                                                                match r.as_deref() {
                                                                    Some("code") => {
                                                                        if let Lit::Int(i) = &nv.lit {
                                                                            let c: u16 =
                                                                                i.base10_parse().map_err(|_| Error::new_spanned(i, "Invalid status code"))?;
                                                                            if !(100..600).contains(&c) {
                                                                                return Err(Error::new_spanned(i, "Invalid status code"));
                                                                            }
                                                                            nb_code += 1;
                                                                        }
                                                                    }
                                                                    Some("type") => {
                                                                        if let Lit::Str(_) = &nv.lit {
                                                                            nb_type += 1;
                                                                        // TODO:
                                                                        // Validate
                                                                        // raw object
                                                                        // syntax
                                                                        } else {
                                                                            return Err(Error::new_spanned(
                                                                                m,
                                                                                "Invalid type : expected a type name/path/raw object wrapped in double-quotes",
                                                                            ));
                                                                        }
                                                                    }
                                                                    Some("mime") => {
                                                                        if let Lit::Str(_) = &nv.lit {
                                                                            nb_mime += 1;
                                                                        } else {
                                                                            return Err(Error::new_spanned(m, "Expected a mimetype string"));
                                                                        }
                                                                    }
                                                                    Some("name") => {
                                                                        if let Lit::Str(_) = &nv.lit {
                                                                            nb_name += 1;
                                                                        } else {
                                                                            return Err(Error::new_spanned(m, "Expected a string"));
                                                                        }
                                                                    }
                                                                    _ => return Err(Error::new_spanned(&nv.path, "Invalid openapi return attribute")),
                                                                }
                                                            }
                                                            _ => return Err(Error::new_spanned(m, "Invalid openapi return attribute")),
                                                        }
                                                    }
                                                    _ => return Err(Error::new_spanned(openapi_attribute, "Invalid openapi return attribute")),
                                                }
                                            }

                                            // TODO: confirm specified type to override is in the return signature
                                            if nb_type == 0 {
                                                return Err(Error::new_spanned(openapi_attribute, "You must specify which `type` to override"));
                                            }

                                            if nb_type > 1 {
                                                return Err(Error::new_spanned(openapi_attribute, "can only specify one `type` per return_override"));
                                            }

                                            if nb_mime > 1 {
                                                return Err(Error::new_spanned(openapi_attribute, "cannot specify multiple mimes for an override"));
                                            }

                                            if nb_name > 1 {
                                                return Err(Error::new_spanned(openapi_attribute, "Cannot specify the name twice"));
                                            }

                                            if nb_code == 0 && nb_mime == 0 {
                                                return Err(Error::new_spanned(openapi_attribute, "must override either `code`(s) or `mime`"));
                                            }
                                        }
                                        Some("param") => {
                                            if openapi_attribute.nested.is_empty() {
                                                return Err(Error::new_spanned(openapi_attribute, "openapi param attribute cannot be empty"));
                                            }
                                        }
                                        _ => return Err(Error::new_spanned(openapi_attribute, "Invalid openapi attribute")),
                                    }
                                }
                                _ => return Err(Error::new_spanned(openapi_attributes, "Invalid openapi attribute")),
                            }
                        }
                    } else if ident == "validator" {
                        #[cfg(not(feature = "validate-requests"))]
                        {
                            return Err(Error::new_spanned(ident, "This requires the \"validate-requests\" feature flag in Saphir"));
                        }

                        #[cfg(feature = "validate-requests")]
                        {
                            if attribute.nested.is_empty() {
                                return Err(Error::new_spanned(ident, "validator attribute cannot be empty"));
                            }

                            for validator_attributes in &attribute.nested {
                                match validator_attributes {
                                    NestedMeta::Meta(Meta::List(validator_attribute)) => {
                                        let i = validator_attribute.path.get_ident().map(|i| i.to_string());
                                        match i.as_deref() {
                                            Some("exclude") => {
                                                if validator_attribute.nested.is_empty() {
                                                    return Err(Error::new_spanned(validator_attribute, "validator exclude attribute cannot be empty"));
                                                }
                                                for excluded_meta in &validator_attribute.nested {
                                                    match excluded_meta {
                                                        NestedMeta::Lit(Lit::Str(excluded)) => {
                                                            handler.validator_exclusions.push(excluded.value());
                                                        }
                                                        _ => return Err(Error::new_spanned(validator_attribute, "Expected a list of quoted parameter names")),
                                                    }
                                                }
                                            }
                                            _ => return Err(Error::new_spanned(validator_attribute, "Invalid validator attribute")),
                                        }
                                    }
                                    NestedMeta::Meta(Meta::Path(p)) if p.is_ident("exclude") => {
                                        return Err(Error::new_spanned(p, "expected a list of excluded parameters"))
                                    }
                                    _ => return Err(Error::new_spanned(validator_attributes, "Invalid validator attribute")),
                                }
                            }
                        }
                    } else {
                        let method =
                            Method::from_str(ident.to_string().to_uppercase().as_str()).map_err(|_e| Error::new_spanned(ident, "Invalid HTTP method"))?;

                        if let Some(NestedMeta::Lit(Lit::Str(str))) = attribute.nested.first() {
                            let path = str.value();
                            if !path.starts_with('/') {
                                return Err(Error::new_spanned(str, "Path must start with '/'"));
                            }

                            handler.methods_paths.push((method, path));
                        } else {
                            return Err(Error::new_spanned(attribute, "Missing path for method"));
                        }
                    }
                }
                Meta::NameValue(_) => {}
                Meta::Path(p) => {
                    if let Some(ident_str) = p.get_ident().map(|p| p.to_string()) {
                        if ident_str.starts_with("cookie") {
                            handler.cookie = true;
                        }
                    }
                }
            }
        }

        if handler.methods_paths.is_empty() {
            return Err(Error::new_spanned(
                &method.sig,
                "Missing Router attribute for handler, help: adde something like `#[get(\"/\")]`",
            ));
        }

        Ok(handler)
    }
}

pub fn parse_handlers(input: ItemImpl) -> Result<Vec<HandlerRepr>> {
    input
        .items
        .into_iter()
        .filter_map(|item| match item {
            ImplItem::Method(m) => Some(HandlerRepr::new(m)),
            _ => None,
        })
        .collect()
}
