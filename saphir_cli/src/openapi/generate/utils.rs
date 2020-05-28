use convert_case::{Case, Casing};
use serde::export::TryFrom;
use syn::{Attribute, Lit, Meta, NestedMeta};

pub(crate) fn get_serde_field(mut field_name: String, field_attributes: &[Attribute], container_attributes: &[Attribute]) -> Option<String> {
    if find_macro_attribute_flag(field_attributes, "serde", "skip") || find_macro_attribute_flag(field_attributes, "serde", "skip_serializing") {
        return None;
    }
    if let Some(Lit::Str(rename)) = find_macro_attribute_named_value(field_attributes, "serde", "rename") {
        field_name = rename.value();
    } else if let Some(Lit::Str(rename)) = find_macro_attribute_named_value(container_attributes, "serde", "rename_all") {
        if let Ok(case) = Case::try_from(rename.value().as_str()) {
            field_name = field_name.to_case(case);
        }
    }
    Some(field_name)
}

pub(crate) fn find_macro_attribute_flag(attrs: &[Attribute], macro_name: &str, value_name: &str) -> bool {
    for attr in attrs
        .iter()
        .filter(|a| a.path.get_ident().filter(|i| i.to_string().as_str() == macro_name).is_some())
    {
        if let Ok(meta) = attr.parse_meta() {
            if find_macro_attribute_flag_from_meta(&meta, value_name) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn find_macro_attribute_flag_from_meta(meta: &Meta, value_name: &str) -> bool {
    match meta {
        Meta::List(l) => {
            for n in &l.nested {
                match n {
                    NestedMeta::Meta(nm) => {
                        if find_macro_attribute_flag_from_meta(&nm, value_name) {
                            return true;
                        }
                    }
                    NestedMeta::Lit(_) => {}
                }
            }
        }
        Meta::Path(p) => {
            if p.get_ident().map(|i| i.to_string()).filter(|s| s == value_name).is_some() {
                return true;
            }
        }
        _ => {}
    }
    false
}

pub(crate) fn find_macro_attribute_named_value(attrs: &[Attribute], macro_name: &str, value_name: &str) -> Option<Lit> {
    for attr in attrs
        .iter()
        .filter(|a| a.path.get_ident().filter(|i| i.to_string().as_str() == macro_name).is_some())
    {
        if let Ok(meta) = attr.parse_meta() {
            if let Some(s) = find_macro_attribute_value_from_meta(&meta, value_name) {
                return Some(s);
            }
        }
    }
    None
}
pub(crate) fn find_macro_attribute_value_from_meta(meta: &Meta, value_name: &str) -> Option<Lit> {
    match meta {
        Meta::List(l) => {
            for n in &l.nested {
                match n {
                    NestedMeta::Meta(nm) => {
                        if let Some(s) = find_macro_attribute_value_from_meta(&nm, value_name) {
                            return Some(s);
                        }
                    }
                    NestedMeta::Lit(l) => {
                        println!(" Litteral meta : {:?}", l);
                    }
                }
            }
        }
        Meta::NameValue(nv) => {
            if nv.path.get_ident().map(|i| i.to_string()).filter(|s| s == value_name).is_some() {
                return Some(nv.lit.clone());
            }
        }
        _ => {}
    }
    None
}
