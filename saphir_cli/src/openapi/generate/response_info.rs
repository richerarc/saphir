use crate::openapi::{
    generate::{crate_syn_browser::Method, type_info::TypeInfo, Gen},
    schema::{OpenApiMimeType, OpenApiSchema},
};
use syn::{GenericArgument, Lit, Meta, MetaList, NestedMeta, Path, PathArguments, ReturnType, Type};

#[derive(Clone, Debug, Default)]
pub(crate) struct ResponseInfo {
    pub(crate) code: u16,
    pub(crate) type_info: Option<TypeInfo>,
    pub(crate) mime: OpenApiMimeType,
    pub(crate) anonymous_type: Option<AnonymousType>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AnonymousType {
    pub(crate) schema: OpenApiSchema,
    pub(crate) name: Option<String>,
}

impl Gen {
    fn get_openapi_metas<'b>(&self, method: &'b Method<'b>) -> Vec<MetaList> {
        method
            .syn
            .attrs
            .iter()
            .filter(|attr| attr.path.get_ident().map(|i| i.to_string()).filter(|s| s.as_str() == "openapi").is_some())
            .filter_map(|attr| match attr.parse_meta() {
                Ok(Meta::List(m)) => Some(m),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn extract_response_info<'b>(&mut self, method: &'b Method<'b>) -> Vec<ResponseInfo> {
        let mut vec: Vec<(Option<u16>, ResponseInfo)> = Vec::new();

        let openapi_metas = self.get_openapi_metas(method);
        for meta in &openapi_metas {
            vec.extend(self.response_info_from_openapi_meta(method, meta));
        }

        if vec.is_empty() {
            if let ReturnType::Type(_tokens, t) = &method.syn.sig.output {
                vec = self.response_info_from_type(method, t);
            }
        }

        if !vec.is_empty() {
            for meta in &openapi_metas {
                self.override_response_info_from_openapi_meta(method, meta, &mut vec);
            }
        } else {
            vec.push((
                None,
                ResponseInfo {
                    code: 200,
                    mime: OpenApiMimeType::Any,
                    ..Default::default()
                },
            ));
        }

        vec.into_iter().map(|(_, r)| r).collect()
    }

    fn response_info_from_openapi_meta<'b>(&mut self, method: &'b Method<'b>, meta: &MetaList) -> Vec<(Option<u16>, ResponseInfo)> {
        let mut vec = Vec::new();

        for openapi_paths in &meta.nested {
            match openapi_paths {
                NestedMeta::Meta(Meta::List(nl)) => {
                    let i = nl.path.get_ident().map(|i| i.to_string());
                    match i.as_deref() {
                        Some("return") => {
                            let mut codes: Vec<u16> = Vec::new();
                            let mut types: Vec<String> = Vec::new();
                            let mut mime: Option<String> = None;
                            let mut name: Option<String> = None;
                            if nl.nested.is_empty() {
                                continue;
                            }
                            for n in &nl.nested {
                                if let NestedMeta::Meta(Meta::NameValue(nv)) = n {
                                    let r = nv.path.get_ident().map(|i| i.to_string());
                                    match r.as_deref() {
                                        Some("code") => {
                                            if let Lit::Int(i) = &nv.lit {
                                                let c: u16 = match i.base10_parse() {
                                                    Ok(c) => c,
                                                    _ => continue,
                                                };
                                                if !(100..600).contains(&c) {
                                                    continue;
                                                }
                                                codes.push(c);
                                            }
                                        }
                                        Some("type") => {
                                            if let Lit::Str(s) = &nv.lit {
                                                types.push(s.value());
                                            }
                                        }
                                        Some("mime") => {
                                            if let Lit::Str(s) = &nv.lit {
                                                mime = Some(s.value());
                                            }
                                        }
                                        Some("name") => {
                                            if let Lit::Str(s) = &nv.lit {
                                                name = Some(s.value());
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            let mut pairs = Vec::new();
                            let nb_codes = codes.len();
                            let nb_types = types.len();
                            if nb_codes == 1 && nb_types == 1 {
                                pairs.push((codes.remove(0), types.remove(0)));
                            } else if nb_codes == 1 && nb_types > 0 {
                                let code = codes.remove(0);
                                for t in types {
                                    pairs.push((code, t));
                                }
                            } else if nb_codes > 1 && nb_types == 1 {
                                let t = types.remove(0);
                                for code in codes {
                                    pairs.push((code, t.clone()));
                                }
                            }

                            let mime = mime.map(OpenApiMimeType::from);

                            for (code, type_name) in pairs {
                                if !type_name.is_empty() {
                                    if let Some(mut anonymous_type) = self.openapitype_from_raw(method.impl_item.im.item.scope, type_name.as_str()) {
                                        anonymous_type.name = name.clone();
                                        vec.push((
                                            Some(code),
                                            ResponseInfo {
                                                code,
                                                mime: mime.clone().unwrap_or(OpenApiMimeType::Any),
                                                type_info: None,
                                                anonymous_type: Some(anonymous_type),
                                            },
                                        ));
                                        continue;
                                    }
                                }

                                let path = match syn::parse_str::<Path>(type_name.as_str()) {
                                    Ok(path) => path,
                                    _ => {
                                        vec.push((
                                            Some(code),
                                            ResponseInfo {
                                                code,
                                                mime: mime.clone().unwrap_or(OpenApiMimeType::Any),
                                                ..Default::default()
                                            },
                                        ));
                                        continue;
                                    }
                                };

                                vec.extend(self.response_info_from_type_path(method, &path).into_iter().map(|(_, mut r)| {
                                    r.code = code;
                                    if let Some(m) = &mime {
                                        r.mime = m.clone();
                                    }
                                    (Some(code), r)
                                }));
                            }
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            }
        }

        vec
    }

    fn override_response_info_from_openapi_meta<'b>(&mut self, method: &'b Method<'b>, meta: &MetaList, responses: &mut Vec<(Option<u16>, ResponseInfo)>) {
        let mut extra_responses: Vec<(Option<u16>, ResponseInfo)> = Vec::new();
        for openapi_paths in &meta.nested {
            match openapi_paths {
                NestedMeta::Meta(Meta::List(nl)) => {
                    let i = nl.path.get_ident().map(|i| i.to_string());
                    match i.as_deref() {
                        Some("return_override") => {
                            let mut codes: Vec<u16> = Vec::new();
                            let mut type_path: Option<String> = None;
                            let mut mime: Option<String> = None;
                            let mut name: Option<String> = None;
                            if nl.nested.is_empty() {
                                continue;
                            }
                            for n in &nl.nested {
                                if let NestedMeta::Meta(Meta::NameValue(nv)) = n {
                                    let r = nv.path.get_ident().map(|i| i.to_string());
                                    match r.as_deref() {
                                        Some("code") => {
                                            if let Lit::Int(i) = &nv.lit {
                                                let c: u16 = match i.base10_parse() {
                                                    Ok(c) => c,
                                                    _ => continue,
                                                };
                                                if !(100..600).contains(&c) {
                                                    continue;
                                                }
                                                codes.push(c);
                                            }
                                        }
                                        Some("type") => {
                                            if let Lit::Str(s) = &nv.lit {
                                                type_path = Some(s.value());
                                            }
                                        }
                                        Some("mime") => {
                                            if let Lit::Str(s) = &nv.lit {
                                                mime = Some(s.value());
                                            }
                                        }
                                        Some("name") => {
                                            if let Lit::Str(s) = &nv.lit {
                                                name = Some(s.value());
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            let mime = mime.map(OpenApiMimeType::from);

                            // Match by code first, then by type
                            let mut matched_on_code = false;
                            let mut res = None;
                            if codes.len() == 1 {
                                res = responses.iter_mut().find(|(_, ri)| codes[0] == ri.code);
                                matched_on_code = res.is_some();
                            }
                            if let (Some(type_path), None) = (&type_path, &res) {
                                res = responses.iter_mut().find(|(_, ri)| {
                                    if let Some(ti) = &ri.type_info {
                                        // TODO: clever-er match
                                        return ti.name == *type_path;
                                    }
                                    false
                                });
                            }

                            if let Some(res) = res {
                                if let Some(mime) = mime {
                                    res.1.mime = mime;
                                }

                                if !matched_on_code {
                                    if let Some(first_code) = codes.first() {
                                        res.1.code = *first_code;
                                        res.0 = Some(*first_code);

                                        if codes.len() > 1 {
                                            for code in codes.iter().skip(1) {
                                                let mut new_res = (Some(*code), res.1.clone());
                                                new_res.1.code = *code;
                                                extra_responses.push(new_res);
                                            }
                                        }
                                    }
                                } else {
                                    if let Some(type_path) = type_path {
                                        let anonymous_type = self.openapitype_from_raw(method.impl_item.im.item.scope, type_path.as_str());
                                        if let Some(mut anonymous_type) = anonymous_type {
                                            if let Some(name) = name {
                                                anonymous_type.name = Some(name);
                                            }

                                            res.1.type_info = None;
                                            res.1.anonymous_type = Some(anonymous_type);
                                        }
                                    }
                                }
                            }
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            }
        }

        responses.extend(extra_responses);
    }

    fn response_info_from_type<'b>(&self, method: &'b Method<'b>, t: &Type) -> Vec<(Option<u16>, ResponseInfo)> {
        match t {
            Type::Path(tp) => {
                return self.response_info_from_type_path(method, &tp.path);
            }
            Type::Tuple(_tt) => {
                // TODO: Tuple with with StatusCode or u16 mean a status
                //       code is specified for the associated return type.
                //       We cannot possibly cover this case fully but we
                //       could at least handle simple cases where
                //       the response is a litteral inside the method's body
            }
            _ => {}
        }

        Vec::new()
    }

    fn response_info_from_type_path<'b>(&self, method: &'b Method<'b>, path: &Path) -> Vec<(Option<u16>, ResponseInfo)> {
        let mut vec = Vec::new();
        if let Some(last) = path.segments.last() {
            let name = last.ident.to_string();
            match name.as_str() {
                "Result" => {
                    let mut results = self.extract_arguments(method, &last.arguments);
                    if results.len() == 2 {
                        for (error_code, mut error_response) in results.remove(1) {
                            error_response.code = error_code.unwrap_or(500);
                            vec.push((Some(error_response.code), error_response));
                        }

                        for (success_code, mut success_response) in results.remove(0) {
                            success_response.code = success_code.unwrap_or(200);
                            vec.push((Some(success_response.code), success_response));
                        }
                    }
                }
                "Option" => {
                    let mut result = self.extract_arguments(method, &last.arguments);
                    if result.len() == 1 {
                        for (success_code, mut success_response) in result.remove(0) {
                            success_response.code = success_code.unwrap_or(200);
                            vec.push((success_code, success_response));
                            vec.push((
                                Some(404),
                                ResponseInfo {
                                    code: 404,
                                    ..Default::default()
                                },
                            ));
                        }
                    }
                }
                "Vec" => {
                    let mut result = self.extract_arguments(method, &last.arguments);
                    if result.len() == 1 {
                        for (_, mut success_response) in result.remove(0) {
                            if let Some(type_info) = success_response.type_info.as_mut() {
                                type_info.is_array = true;
                            }
                            vec.push((None, success_response));
                        }
                    }
                }
                "Json" => {
                    let mut result = self.extract_arguments(method, &last.arguments);
                    if result.len() == 1 {
                        for (_, mut success_response) in result.remove(0) {
                            success_response.mime = OpenApiMimeType::Json;
                            vec.push((None, success_response));
                        }
                    }
                }
                "Form" => {
                    let mut result = self.extract_arguments(method, &last.arguments);
                    if result.len() == 1 {
                        for (_, mut success_response) in result.remove(0) {
                            success_response.mime = OpenApiMimeType::Form;
                            vec.push((None, success_response));
                        }
                    }
                }
                // TODO: Find a way to handle this. This is a temp workaround for Lucid
                "JsonContent" | "NoCache" => {
                    let mut result = self.extract_arguments(method, &last.arguments);
                    if result.len() == 1 {
                        for (_, mut success_response) in result.remove(0) {
                            if name.as_str() == "JsonContent" {
                                success_response.mime = OpenApiMimeType::Json;
                            }
                            vec.push((None, success_response));
                        }
                    }
                }
                _ => {
                    let type_info = TypeInfo::new_from_path(method.impl_item.im.item.scope, path);
                    vec.push((
                        None,
                        ResponseInfo {
                            mime: type_info
                                .as_ref()
                                .and_then(|t| t.mime.as_ref().map(|m| OpenApiMimeType::from(m.clone())))
                                .unwrap_or(OpenApiMimeType::Any),
                            type_info,
                            code: 200,
                            ..Default::default()
                        },
                    ));
                }
            }
        }
        vec
    }

    fn extract_arguments<'b>(&self, method: &'b Method<'b>, args: &PathArguments) -> Vec<Vec<(Option<u16>, ResponseInfo)>> {
        match args {
            PathArguments::AngleBracketed(ab) => ab
                .args
                .iter()
                .filter_map(|a| match a {
                    GenericArgument::Type(Type::Path(tp)) => Some(self.response_info_from_type_path(method, &tp.path)),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}
