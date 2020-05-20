use syn::{Item as SynItem, Type, PathArguments, GenericArgument, Expr, Lit};
use crate::docgen::{DocGen};
use std::borrow::Borrow;
use crate::docgen::crate_syn_browser::File;

#[derive(Clone, Debug)]
pub(crate) struct TypeInfo {
    pub name: String,
    pub type_path: Option<String>,
    pub is_type_serializable: bool,
    pub is_type_deserializable: bool,
    pub is_array: bool,
    pub is_optional: bool,
    pub min_array_len: Option<u32>,
    pub max_array_len: Option<u32>,
}

impl DocGen {
    pub fn find_type_info<'b>(
        &self,
        file: &'b File<'b>,
        t: &Type
    ) -> Option<TypeInfo> {
        match t.borrow() {
            Type::Path(p) => {
                if let Some(s) = p.path.segments.first() {
                    let name = s.ident.to_string();
                    if name == "Vec" || name == "Option" {
                        let ag = match &s.arguments {
                            PathArguments::AngleBracketed(ag) => ag,
                            _ =>  {
                                println!("{} need angle bracket type parameter. Maybe another type was aliased as {}, which is not supported.", name, name);
                                return None;
                            },
                        };
                        let t2 = match ag.args.iter().find_map(|a| match a {
                            GenericArgument::Type(t) => Some(t),
                            _ => None
                        }) {
                            Some(t) => t,
                            None => {
                                println!("{} should be provided a type in angle-bracketed format. Faulty type : {:?}", name, t);
                                return None;
                            }
                        };

                        if let Some(mut type_info) = self.find_type_info(file, t2) {
                            match name.as_str() {
                                "Vec" => type_info.is_array = true,
                                "Option" => type_info.is_optional = true,
                                _ => unreachable!()
                            }
                            return Some(type_info);
                        }
                    } else {
                        let type_impl = file.find_impl(name.as_str()).ok().flatten();
                        let type_path = type_impl.map(|i| i.file.use_path.clone());
                        let item_attrs = type_impl.map(|i| {
                            match i.item {
                                SynItem::Struct(s) => &s.attrs,
                                SynItem::Enum(e) => &e.attrs,
                                _ => unreachable!(),
                            }
                        });
                        let is_type_serializable = item_attrs.map(|attrs|
                            self.find_macro_attribute_flag(attrs, "derive", "Serialize")
                        ).unwrap_or_default();
                        let is_type_deserializable = item_attrs.map(|attrs|
                            self.find_macro_attribute_flag(attrs, "derive", "Deserialize")
                        ).unwrap_or_default();
                        return Some(TypeInfo { name, type_path, is_type_serializable, is_type_deserializable, is_array: false, is_optional: false, min_array_len: None, max_array_len: None });
                    }
                }
            }
            Type::Array(a) => {
                let len: Option<u32> = match &a.len {
                    Expr::Lit(l) => match &l.lit {
                        Lit::Int(i) => i.base10_parse().ok(),
                        _ => None,
                    },
                    _ => None,
                };

                if let Some(mut type_info) = self.find_type_info(file, a.elem.as_ref()) {
                    type_info.is_array = true;
                    type_info.min_array_len = len.clone();
                    type_info.max_array_len = len;
                    return Some(type_info);
                }
            }
            _ => {},
        };
        None
    }
}