use crate::docgen::DocGen;
use crate::openapi::OpenApiPathMethod;
use syn::{Attribute, ImplItemMethod};

#[derive(Clone, Debug)]
pub(crate) struct RouteInfo {
    pub(crate) method: OpenApiPathMethod,
    pub(crate) uri: String,
    pub(crate) uri_params: Vec<String>,
    pub(crate) operation_id: String,
}

impl DocGen {
    /// Retrieve RouteInfo from a method with a saphir route macro.
    pub(crate) fn extract_route_info_from_method_macro(&self, base_path: &str, attr: &Attribute, impl_method: &ImplItemMethod) -> Option<RouteInfo> {
        let method = self.handler_method_from_attr(&attr)?;
        let (path, uri_params) = self.handler_path_from_attr(&attr)?;

        let mut full_path = format!("/{}{}", base_path, path);
        if full_path.ends_with('/') {
            full_path = (&full_path[0..(full_path.len() - 1)]).to_string();
        }
        if !full_path.starts_with(self.args.scope.as_str()) {
            return None;
        }
        let operation_id = self.handler_operation_id_from_sig(&impl_method.sig);
        Some(RouteInfo {
            method,
            uri: full_path,
            uri_params,
            operation_id,
        })
    }
}
