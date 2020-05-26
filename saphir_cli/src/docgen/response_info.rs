use crate::docgen::type_info::TypeInfo;
use crate::docgen::DocGen;
use syn::{Signature, ReturnType, ImplItemMethod, Type, PathArguments, GenericArgument, Error, NestedMeta, Meta, MetaList, Lit, LitInt, LitStr, Path};
use crate::docgen::crate_syn_browser::File;
use syn::token::{Macro, Token};
use syn::parse::{Parse, ParseStream};
use crate::openapi::OpenApiMimeType;

#[derive(Clone, Debug)]
pub(crate) struct ResponseInfo {
    pub(crate) code: u16,
    pub(crate) type_info: Option<TypeInfo>,
    pub(crate) mime: OpenApiMimeType,
}

impl DocGen {
    pub(crate) fn extract_response_info<'b>(
        &self,
        file: &'b File<'b>,
        im: &'b ImplItemMethod,
    ) -> Vec<ResponseInfo> {
        match &im.sig.output {
            ReturnType::Default => vec![ResponseInfo { code: 200, type_info: None, mime: OpenApiMimeType::Any }],
            ReturnType::Type(_tokens, t) => {
                let mut vec: Vec<ResponseInfo> = self.response_info_from_type(file, im, t)
                    .into_iter()
                    .map(|(c, r)| r)
                    .collect();

                if vec.is_empty() {
                    vec.push(ResponseInfo {
                        type_info: None,
                        code: 200,
                        mime: OpenApiMimeType::Any,
                    });
                }

                vec
            },
        }
    }

    fn response_info_from_openapi_meta<'b>(
        &self,
        file: &'b File<'b>,
        im: &'b ImplItemMethod,
        meta: &MetaList,
    ) -> Vec<(Option<u16>, ResponseInfo)> {
        let mut vec = Vec::new();

        for openapi_paths in &meta.nested {
            match openapi_paths {
                NestedMeta::Meta(m) => {
                    match m {
                        Meta::List(nl) => {
                            let i = nl.path.get_ident().map(|i| i.to_string());
                            match i.as_ref().map(|i| i.as_str()) {
                                Some("return") => {
                                    let mut codes: Vec<u16> = Vec::new();
                                    let mut types: Vec<String> = Vec::new();
                                    if nl.nested.is_empty() {
                                        continue;
                                    }
                                    for n in &nl.nested {
                                        match n {
                                            NestedMeta::Meta(Meta::NameValue(nv)) => {
                                                let r = nv.path.get_ident().map(|i| i.to_string());
                                                match r.as_ref().map(|i| i.as_str()) {
                                                    Some("code") => {
                                                        if let Lit::Int(i) = &nv.lit {
                                                            let c: u16 = match i.base10_parse() {
                                                                Ok(c) => c,
                                                                _ => continue,
                                                            };
                                                            if c < 100 || c >= 600 {
                                                                continue;
                                                            }
                                                            codes.push(c);
                                                        }
                                                    },
                                                    Some("type") => {
                                                        if let Lit::Str(s) = &nv.lit {
                                                            types.push(s.value());
                                                        }
                                                    },
                                                    _ => {},
                                                }
                                            },
                                            _ => {},
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

                                    for (code, type_name) in pairs {
                                        let path = match syn::parse_str::<Path>(type_name.as_str()) {
                                            Ok(path) => path,
                                            _ => {
                                                vec.push((Some(code), ResponseInfo {
                                                    code,
                                                    mime: OpenApiMimeType::Any,
                                                    type_info: None,
                                                }));
                                                continue;
                                            },
                                        };
                                        vec.extend(
                                            self.response_info_from_type_path(file, im, &path)
                                                .into_iter()
                                                .map(|(_, mut r)| {
                                                    r.code = code;
                                                    (Some(code), r)
                                                })
                                        );
                                    }
                                },
                                _ => continue,
                            }
                        },
                        _ => continue,
                    }
                },
                _ => continue,
            }
        }

        vec
    }

    fn response_info_from_type<'b>(
        &self,
        file: &'b File<'b>,
        im: &'b ImplItemMethod,
        t: &Type,
    ) -> Vec<(Option<u16>, ResponseInfo)> {
        let mut vec: Vec<(Option<u16>, ResponseInfo)> = Vec::new();

        for attr in &im.attrs {
            if attr.path.get_ident()
                .map(|i| i.to_string())
                .filter(|s| s.as_str() == "openapi")
                .is_none() {
                continue;
            }
            let meta = match attr.parse_meta() {
                Ok(Meta::List (m)) => m,
                _ => continue,
            };
            vec.extend(self.response_info_from_openapi_meta(file, im, &meta));
        }

        if vec.is_empty() {
            match t {
                Type::Path(tp) => {
                    vec.extend(self.response_info_from_type_path(file, im, &tp.path));
                },
                Type::Tuple(_tt) => {
                    // TODO: Tuple with with StatusCode or u16 mean a status code is specified for the associated return type.
                    //       We cannot possibly cover this case fully but we could at least handle simple cases where
                    //       the response is a litteral inside the method's body
                }
                _ => {}
            }
        }

        vec
    }

    fn response_info_from_type_path<'b>(
        &self,
        file: &'b File<'b>,
        im: &'b ImplItemMethod,
        path: &Path,
    ) -> Vec<(Option<u16>, ResponseInfo)> {
        let mut vec = Vec::new();
        if let Some(last) = path.segments.last() {
            let name = last.ident.to_string();
            match name.as_str() {
                "Result" => {
                    let mut results = self.extract_arguments(file, im, &last.arguments);
                    if results.len() == 2 {
                        let (error_code, mut error_response) = results.remove(1);
                        let (success_code, mut success_response) = results.remove(0);
                        success_response.code = success_code.unwrap_or(200);
                        error_response.code = error_code.unwrap_or(500);
                        vec.push((Some(success_response.code), success_response));
                        vec.push((Some(error_response.code), error_response));
                    }
                },
                "Option" => {
                    let mut result = self.extract_arguments(file, im, &last.arguments);
                    if result.len() == 1 {
                        let (success_code, mut success_response) = result.remove(0);
                        success_response.code = success_code.unwrap_or(200);
                        vec.push((success_code, success_response));
                        vec.push((Some(404), ResponseInfo { code: 404, type_info: None, mime: OpenApiMimeType::Any }));
                    }
                },
                "Json" => {
                    let mut result = self.extract_arguments(file, im, &last.arguments);
                    if result.len() == 1 {
                        let (_, mut success_response) = result.remove(0);
                        success_response.mime = OpenApiMimeType::Json;
                        vec.push((None, success_response));
                    }
                },
                "Form" => {
                    let mut result = self.extract_arguments(file, im, &last.arguments);
                    if result.len() == 1 {
                        let (_, mut success_response) = result.remove(0);
                        success_response.mime = OpenApiMimeType::Form;
                        vec.push((None, success_response));
                    }
                },
                // TODO: Find a way to handle this. This is a temp workaround for Lucid
                "JsonContent" | "NoCache" => {
                    let mut result = self.extract_arguments(file, im, &last.arguments);
                    if result.len() == 1 {
                        let (_, success_response) = result.remove(0);
                        vec.push((None, success_response));
                    }
                }
                _ => {
                    let type_info = TypeInfo::new_from_path(file, path);
                    vec.push((None, ResponseInfo {
                        type_info,
                        code: 200,
                        mime: OpenApiMimeType::Any,
                    }));
                }
            }
        }
        vec
    }

    fn extract_arguments<'b>(
        &self,
        file: &'b File<'b>,
        im: &'b ImplItemMethod,
        args: &PathArguments,
    ) -> Vec<(Option<u16>, ResponseInfo)> {
        match args {
            PathArguments::AngleBracketed(ab) => {
                ab.args
                    .iter()
                    .filter_map(|a| match a {
                        GenericArgument::Type(Type::Path(tp)) => Some(self.response_info_from_type_path(file, im, &tp.path)),
                        _ => None
                    })
                    .flatten()
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}