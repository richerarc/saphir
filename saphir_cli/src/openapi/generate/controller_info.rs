use crate::openapi::generate::{
    crate_syn_browser::{Impl, ImplItemKind},
    handler_info::HandlerInfo,
    Gen,
};
use syn::{Lit, Meta, NestedMeta, Type};

#[derive(Clone, Debug, Default)]
pub(crate) struct ControllerInfo {
    pub(crate) controller_name: String,
    pub(crate) name: String,
    pub(crate) version: Option<String>,
    pub(crate) prefix: Option<String>,
    pub(crate) handlers: Vec<HandlerInfo>,
}

impl ControllerInfo {
    pub fn base_path(&self) -> String {
        let mut path = self.name.clone();
        if let Some(ver) = &self.version {
            path = format!("v{}/{}", ver, path);
        }
        if let Some(prefix) = &self.prefix {
            path = format!("{}/{}", prefix, path);
        }
        path
    }
}

impl Gen {
    /// Retrieve ControllerInfo from an implementation block.
    /// Saphir does not currently support multiple implementation blocks for the
    /// same controller.
    pub(crate) fn extract_controller_info<'b>(&mut self, im: &'b Impl<'b>) -> Option<ControllerInfo> {
        for attr in &im.syn.attrs {
            if let Some(first_seg) = attr.path.segments.first() {
                let t = im.syn.self_ty.as_ref();
                if let Type::Path(p) = t {
                    if let Some(struct_first_seg) = p.path.segments.first() {
                        if first_seg.ident.eq("controller") {
                            let controller_name = struct_first_seg.ident.to_string();
                            let name = controller_name.to_ascii_lowercase();
                            let name = &name[0..name.rfind("controller").unwrap_or(name.len())];
                            let mut name = name.to_string();
                            let mut prefix = None;
                            let mut version = None;
                            if let Ok(Meta::List(meta)) = attr.parse_meta() {
                                for nested in meta.nested {
                                    if let NestedMeta::Meta(Meta::NameValue(nv)) = nested {
                                        if let Some(p) = nv.path.segments.first() {
                                            let value = match nv.lit {
                                                Lit::Str(s) => s.value(),
                                                Lit::Int(i) => i.to_string(),
                                                _ => continue,
                                            };
                                            match p.ident.to_string().as_str() {
                                                "name" => name = value,
                                                "prefix" => prefix = Some(value),
                                                "version" => version = Some(value),
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                            }

                            let mut controller = ControllerInfo {
                                controller_name,
                                name,
                                prefix,
                                version,
                                handlers: Vec::new(),
                            };
                            let mut handlers = im
                                .items()
                                .iter()
                                .filter_map(|i| match i.kind() {
                                    ImplItemKind::Method(m) => self.extract_handler_info(controller.base_path().as_str(), m),
                                    _ => None,
                                })
                                .collect::<Vec<_>>();
                            std::mem::swap(&mut controller.handlers, &mut handlers);
                            return Some(controller);
                        }
                    }
                }
            }
        }
        None
    }
}
