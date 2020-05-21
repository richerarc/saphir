use crate::docgen::{BodyParamInfo, DocGen, RouteParametersInfo};
use syn::{ImplItemMethod, PathArguments, GenericArgument, Type, Pat, FnArg};
use crate::openapi::{OpenApiParameter, OpenApiMimeTypes, OpenApiSchema, OpenApiParameterLocation, OpenApiType};
use crate::docgen::route_info::RouteInfo;
use crate::docgen::crate_syn_browser::File;
use crate::docgen::type_info::TypeInfo;
use crate::docgen::response_info::ResponseInfo;

#[derive(Clone, Debug, Default)]
pub(crate) struct HandlerInfo {
    pub(crate) use_cookies: bool,
    pub(crate) parameters: Vec<OpenApiParameter>,
    pub(crate) body_info: Option<BodyParamInfo>,
    pub(crate) routes: Vec<RouteInfo>,
    pub(crate) responses: Vec<ResponseInfo>,
}

impl DocGen {
    pub(crate) fn extract_handler_info<'b>(&self, base_path: &str, file: &'b File<'b>, impl_method: &ImplItemMethod) -> Result<Option<HandlerInfo>, String> {
        let mut consume_cookies: bool = self.handler_has_cookies(&impl_method);

        let routes: Vec<RouteInfo> = impl_method.attrs
            .iter()
            .filter_map(|attr| self.extract_route_info_from_method_macro(base_path, attr, impl_method))
            .collect();

        if routes.is_empty() {
            return Ok(None);
        }

        let parameters_info = self.parse_handler_parameters(file, &impl_method, &routes[0].uri_params);
        if parameters_info.has_cookies_param {
            consume_cookies = true;
        }

        let responses = self.extract_response_info(file, &impl_method)?;


        Ok(Some(HandlerInfo {
            use_cookies: consume_cookies,
            parameters: parameters_info.parameters.clone(),
            body_info: parameters_info.body_info.clone(),
            routes,
            responses
        }))
    }

    fn handler_has_cookies(&self, m: &ImplItemMethod) -> bool {
        for attr in &m.attrs {
            if let Some(i) = attr.path.get_ident() {
                if i.to_string().as_str() == "cookies" {
                    return true;
                }
            }
        }
        false
    }

    /// TODO: better typing for parameters.
    ///       implement a ParameterInfo struct with typing for param, fill HandlerInfo with this,
    ///       separate the discovery of BodyInfo and cookies usage from parameters.
    fn parse_handler_parameters<'b>(&self, file: &'b File<'b>, m: &ImplItemMethod, uri_params: &[String]) -> RouteParametersInfo {
        let mut parameters = Vec::new();
        let mut has_cookies_param = false;
        let mut body_type = None;
        for param in m.sig.inputs.iter().filter_map(|i| match i {
            FnArg::Typed(p) => Some(p),
            _ => None,
        }) {
            let param_name = match param.pat.as_ref() {
                Pat::Ident(i) => i.ident.to_string(),
                _ => continue,
            };

            let (param_type, optional) = match param.ty.as_ref() {
                Type::Path(p) => {
                    if let Some(s1) = p.path.segments.last() {
                        let mut param_type = s1.ident.to_string();
                        if param_type.as_str() == "CookieJar" {
                            has_cookies_param = true;
                            continue;
                        }
                        if param_type.as_str() == "Request" {
                            if let PathArguments::AngleBracketed(ab) = &s1.arguments {
                                if let Some(GenericArgument::Type(Type::Path(body_path))) = ab.args.first() {
                                    if let Some(seg) = body_path.path.segments.first() {
                                        body_type = Some(seg);
                                    }
                                }
                            }
                            continue;
                        }
                        if param_type.as_str() == "Json" || param_type.as_str() == "Form" {
                            body_type = Some(&s1);
                            continue;
                        }
                        let optional = param_type.as_str() == "Option";
                        if optional {
                            param_type = "String".to_string();
                            if let PathArguments::AngleBracketed(ab) = &s1.arguments {
                                if let Some(GenericArgument::Type(Type::Path(p))) = ab.args.first() {
                                    if let Some(i) = p.path.get_ident() {
                                        param_type = i.to_string();
                                    }
                                }
                            }
                        }

                        let api_type = OpenApiType::from_rust_type_str(param_type.as_str());
                        (api_type, optional)
                    } else {
                        (OpenApiType::string(), false)
                    }
                }
                _ => (OpenApiType::string(), false),
            };

            let location = if uri_params.contains(&param_name) {
                OpenApiParameterLocation::Path
            } else {
                OpenApiParameterLocation::Query
            };
            parameters.push(OpenApiParameter {
                name: param_name,
                required: !optional,
                location,
                schema: OpenApiSchema::Inline(param_type),
                ..Default::default()
            })
        }

        let mut body_info: Option<BodyParamInfo> = None;
        if let Some(body) = body_type {
            let body_type = body.ident.to_string();
            let openapi_type = match body_type.as_str() {
                "Json" => OpenApiMimeTypes::Json,
                "Form" => OpenApiMimeTypes::Form,
                _ => OpenApiMimeTypes::Any,
            };
            match body_type.as_str() {
                "Json" | "Form" => {
                    if let PathArguments::AngleBracketed(ag) = &body.arguments {
                        if let Some(GenericArgument::Type(t)) = ag.args.first() {
                            if let Some(type_info) = TypeInfo::new(
                                file,
                                t
                            ) {
                                body_info = Some(BodyParamInfo { openapi_type, type_info });
                            }
                        }
                    }
                }
                _ => {}
            };
        }

        RouteParametersInfo {
            parameters,
            has_cookies_param,
            body_info,
        }
    }
}