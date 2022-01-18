use crate::openapi::{
    generate::{crate_syn_browser::Method, Gen},
    schema::OpenApiPathMethod,
};
use convert_case::Casing;
use syn::{Attribute, Lit, Meta, NestedMeta, Signature};

#[derive(Clone, Debug)]
pub(crate) struct RouteInfo {
    pub(crate) method: OpenApiPathMethod,
    pub(crate) uri: String,
    pub(crate) uri_params: Vec<String>,
    pub(crate) operation_id: String,
    pub(crate) operation_name: String,
}

impl Gen {
    /// Retrieve Routes (RouteInfo) from a method with a saphir route macro.
    pub(crate) fn extract_routes_info_from_method_macro(&self, method: &Method, controller_path: &str) -> Vec<RouteInfo> {
        let routes: Vec<_> = method
            .syn
            .attrs
            .iter()
            .filter_map(|attr| self.handler_method_from_attr(attr).zip(Some(attr)))
            .filter_map(|(method, attr)| self.handler_path_from_attr(attr).map(|(path, uri_params)| (method, path, uri_params)))
            .collect();

        let multi = routes.len() > 1;

        routes
            .into_iter()
            .filter_map(|(m, path, uri_params)| self.extract_route_info_from_method_macro(controller_path, m, path, uri_params, multi, &method.syn.sig))
            .collect()
    }

    fn extract_route_info_from_method_macro(
        &self,
        controller_path: &str,
        method: OpenApiPathMethod,
        path: String,
        uri_params: Vec<String>,
        multi: bool,
        sig: &Signature,
    ) -> Option<RouteInfo> {
        let mut full_path = format!("/{}{}", controller_path, path);
        if full_path.ends_with('/') {
            full_path = (&full_path[0..(full_path.len() - 1)]).to_string();
        }
        if !full_path.starts_with(self.args.scope.as_str()) {
            return None;
        }
        let operation_id = self.handler_operation_id_from_sig(sig);
        let operation_name = self.handler_operation_name_from_sig(sig, if multi { Some(method.to_str()) } else { None });

        Some(RouteInfo {
            method,
            uri: full_path,
            uri_params,
            operation_id,
            operation_name,
        })
    }

    fn handler_operation_name_from_sig(&self, sig: &Signature, prefix: Option<&str>) -> String {
        let name = sig.ident.to_string();
        match prefix {
            Some(p) => format!("{}_{}", p, name),
            None => name,
        }
        .to_case((&self.args.operation_name_case).into())
    }

    fn handler_method_from_attr(&self, attr: &Attribute) -> Option<OpenApiPathMethod> {
        let ident = attr.path.get_ident()?;
        OpenApiPathMethod::from_str(ident.to_string().as_str())
    }

    fn handler_path_from_attr(&self, attr: &Attribute) -> Option<(String, Vec<String>)> {
        if let Ok(Meta::List(meta)) = attr.parse_meta() {
            if let Some(NestedMeta::Lit(Lit::Str(l))) = meta.nested.first() {
                let mut chars: Vec<char> = l.value().chars().collect();
                let mut params: Vec<String> = Vec::new();

                let mut i = 0;
                while i < chars.len() {
                    if chars[i] == '<' || chars[i] == '{' {
                        chars[i] = '{';
                        let start = i;
                        for j in start..chars.len() {
                            if chars[j] == '>' || chars[j] == '}' {
                                chars[j] = '}';
                                params.push((&chars[(i + 1)..j]).iter().collect());
                                i = j;
                                break;
                            }
                        }
                    }
                    i += 1;
                }

                return Some((chars.into_iter().collect(), params));
            }
        }
        None
    }
}
