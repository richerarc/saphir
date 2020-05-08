use syn::ImplItemMethod;
use crate::docgen::{ControllerInfo, DocGen};
use crate::openapi::OpenApiPathMethod;

#[derive(Clone, Debug, Default)]
pub(crate) struct RouteInfo {
    pub(crate) method: OpenApiPathMethod,
    pub(crate) uri: String,
    pub(crate) uri_params: Vec<String>,
}

impl DocGen {
    pub(crate) fn route_info_from_method_macro(&self, controller: &ControllerInfo, m: &ImplItemMethod) -> Vec<RouteInfo> {
        let mut routes = Vec::new();
        for attr in &m.attrs {
            let method = match self.handler_method_from_attr(&attr) {
                Some(m) => m,
                None => continue,
            };

            let (path, uri_params) = match self.handler_path_from_attr(&attr) {
                Some(p) => p,
                None => continue,
            };
            let mut full_path = format!("/{}{}", controller.base_path(), path);
            if full_path.ends_with('/') {
                full_path = (&full_path[0..(full_path.len() - 1)]).to_string();
            }
            if !full_path.starts_with(self.args.scope.as_str()) {
                continue;
            }
            routes.push(RouteInfo {
                method,
                uri: full_path,
                uri_params,
            })
        }
        routes
    }
}