use crate::docgen::type_info::TypeInfo;
use crate::docgen::DocGen;
use syn::{Signature, ReturnType, ImplItemMethod, Type, PathArguments, GenericArgument};
use crate::docgen::crate_syn_browser::File;
use syn::token::{Macro, Token};
use syn::parse::{Parse, ParseStream};
use crate::openapi::OpenApiMimeTypes;

#[derive(Clone, Debug)]
pub(crate) struct ResponseInfo {
    pub(crate) code: u16,
    pub(crate) type_info: Option<TypeInfo>,
    pub(crate) mime: OpenApiMimeTypes,
}

impl DocGen {
    pub(crate) fn extract_response_info<'b>(
        &self,
        file: &'b File<'b>,
        im: &'b ImplItemMethod,

    ) -> Vec<ResponseInfo> {
        match &im.sig.output {
            ReturnType::Default => vec![ResponseInfo { code: 200, type_info: None, mime: OpenApiMimeTypes::Any }],
            ReturnType::Type(_tokens, t) => {
                let mut vec: Vec<ResponseInfo> = self.response_info_from_type(file, im, t)
                    .into_iter()
                    .map(|(c, r)| r)
                    .collect();

                if vec.is_empty() {
                    vec.push(ResponseInfo {
                        type_info: None,
                        code: 200,
                        mime: OpenApiMimeTypes::Any,
                    });
                }

                vec
            },
        }
    }

    fn response_info_from_type<'b>(
        &self,
        file: &'b File<'b>,
        im: &'b ImplItemMethod,
        t: &'b Type,
    ) -> Vec<(Option<u16>, ResponseInfo)> {
        let mut vec = Vec::new();

        match t {
            Type::Path(tp) => {
                if let Some(last) = tp.path.segments.last() {
                    let name = last.ident.to_string();
                    // println!("type : {:?} with args : {:?}", &name, last.arguments);
                    match name.as_str() {
                        "Result" => {
                            let mut results = self.extract_arguments(file, im, &last.arguments);
                            if results.len() == 2 {
                                println!("Result : {:?}", results);
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
                                let (mut success_code, mut success_response) = result.remove(0);
                                success_response.code = success_code.unwrap_or(200);
                                vec.push((success_code, success_response));
                                vec.push((Some(404), ResponseInfo { code: 404, type_info: None, mime: OpenApiMimeTypes::Any }));
                            }
                        },
                        "Json" => {
                            let mut result = self.extract_arguments(file, im, &last.arguments);
                            if result.len() == 1 {
                                let (_, mut success_response) = result.remove(0);
                                success_response.mime = OpenApiMimeTypes::Json;
                                vec.push((None, success_response));
                            }
                        },
                        "Form" => {
                            let mut result = self.extract_arguments(file, im, &last.arguments);
                            if result.len() == 1 {
                                let (_, mut success_response) = result.remove(0);
                                success_response.mime = OpenApiMimeTypes::Form;
                                vec.push((None, success_response));
                            }
                        },
                        // TODO: Find a way to handle this. This is a temp workaround for Lucid
                        "JsonContent" | "NoCache" => {
                            let mut result = self.extract_arguments(file, im, &last.arguments);
                            if result.len() == 1 {
                                let (_, mut success_response) = result.remove(0);
                                vec.push((None, success_response));
                            }
                        }
                        _ => {
                            let type_info = TypeInfo::new(file, t);
                            vec.push((None, ResponseInfo {
                                type_info,
                                code: 200,
                                mime: OpenApiMimeTypes::Any,
                            }));
                        }
                    }
                }
            },
            Type::Tuple(tt) => {
                // TODO: Tuple with with StatusCode or u16 mean a status code is specified for the associated return type.
                //       We cannot possibly cover this case fully but we could at least handle simple cases where
                //       the response is a litteral inside the method's body
            }
            _ => { }
        }

        vec
    }

    fn extract_arguments<'b>(
        &self,
        file: &'b File<'b>,
        im: &'b ImplItemMethod,
        args: &'b PathArguments,
    ) -> Vec<(Option<u16>, ResponseInfo)> {
        match args {
            PathArguments::AngleBracketed(ab) => {
                ab.args
                    .iter()
                    .filter_map(|a| match a {
                        GenericArgument::Type(t) => Some(self.response_info_from_type(file, im, t)),
                        _ => None
                    })
                    .flatten()
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}