use crate::openapi::generate::{
    crate_syn_browser::UseScope,
    utils::{find_macro_attribute_flag, find_macro_attribute_named_value},
};
use syn::{Expr, GenericArgument, Item as SynItem, Lit, Path, PathArguments, Type};

/// Informations about a Rust Type required to create a corresponding
/// OpenApiType
#[derive(Clone, Debug)]
pub(crate) struct TypeInfo {
    pub(crate) name: String,
    pub(crate) type_path: Option<String>,
    pub(crate) is_type_serializable: bool,
    pub(crate) is_type_deserializable: bool,
    pub(crate) is_array: bool,
    pub(crate) is_optional: bool,
    pub(crate) min_array_len: Option<u32>,
    pub(crate) max_array_len: Option<u32>,
    pub(crate) mime: Option<String>,
}

impl TypeInfo {
    /// Retrieve TypeInfo for a syn::Type found in a crate_syn_browser::File.
    pub fn new<'b>(scope: &'b dyn UseScope<'b>, t: &Type) -> Option<TypeInfo> {
        match t {
            Type::Path(p) => {
                return TypeInfo::new_from_path(scope, &p.path);
            }
            Type::Array(a) => {
                let len: Option<u32> = match &a.len {
                    Expr::Lit(l) => match &l.lit {
                        Lit::Int(i) => i.base10_parse().ok(),
                        _ => None,
                    },
                    _ => None,
                };

                if let Some(mut type_info) = TypeInfo::new(scope, a.elem.as_ref()) {
                    type_info.is_array = true;
                    type_info.min_array_len = len;
                    type_info.max_array_len = len;
                    return Some(type_info);
                }
            }
            Type::Reference(tr) => return TypeInfo::new(scope, tr.elem.as_ref()),
            _ => {}
        };
        None
    }

    pub fn new_from_path<'b>(scope: &'b dyn UseScope<'b>, path: &Path) -> Option<TypeInfo> {
        if let Some(s) = path.segments.last() {
            let name = s.ident.to_string();
            if name == "Vec" || name == "Option" {
                let ag = match &s.arguments {
                    PathArguments::AngleBracketed(ag) => ag,
                    _ => {
                        println!(
                            "{} need angle bracket type parameter. Maybe another type was aliased as {}, which is not supported.",
                            name, name
                        );
                        return None;
                    }
                };
                let t2 = match ag.args.iter().find_map(|a| match a {
                    GenericArgument::Type(t) => Some(t),
                    _ => None,
                }) {
                    Some(t) => t,
                    None => {
                        println!("{} should be provided a type in angle-bracketed format. Faulty type : {:?}", name, path);
                        return None;
                    }
                };

                if let Some(mut type_info) = TypeInfo::new(scope, t2) {
                    match name.as_str() {
                        "Vec" => type_info.is_array = true,
                        "Option" => type_info.is_optional = true,
                        _ => unreachable!(),
                    }
                    return Some(type_info);
                }
            } else {
                let path = path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<String>>().join("::");
                let type_impl = scope.find_type_definition(path.as_str()).ok().flatten();
                let type_path = type_impl.map(|i| i.scope.path().to_string());
                let item_attrs = type_impl.map(|i| match i.item {
                    SynItem::Struct(s) => &s.attrs,
                    SynItem::Enum(e) => &e.attrs,
                    _ => unreachable!(),
                });
                let is_type_serializable = item_attrs
                    .map(|attrs| find_macro_attribute_flag(attrs, "derive", "Serialize"))
                    .unwrap_or_default();
                let is_type_deserializable = item_attrs
                    .map(|attrs| find_macro_attribute_flag(attrs, "derive", "Deserialize"))
                    .unwrap_or_default();
                let mime = item_attrs
                    .map(|attrs| find_macro_attribute_named_value(attrs, "openapi", "mime"))
                    .flatten()
                    .map(|m| match m {
                        Lit::Str(s) => Some(s.value()),
                        _ => None,
                    })
                    .flatten();
                return Some(TypeInfo {
                    name,
                    type_path,
                    is_type_serializable,
                    is_type_deserializable,
                    is_array: false,
                    is_optional: false,
                    min_array_len: None,
                    max_array_len: None,
                    mime,
                });
            }
        }
        None
    }
}
