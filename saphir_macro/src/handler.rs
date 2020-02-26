use http::Method;
use proc_macro2::Ident;
use syn::{Attribute, AttrStyle, Error, ImplItem, ImplItemMethod, ItemFn, ItemImpl, AttributeArgs, NestedMeta, Lit};
use syn::parse_macro_input;
use syn::parse::{Parse, ParseBuffer, Result};

use quote::quote;
use std::collections::{HashMap, HashSet};

pub fn parse_handlers(input: &ItemImpl) -> Vec<ImplItemMethod> {
    let mut vec = Vec::new();
    for item in &input.items {
        if let ImplItem::Method(m) = item {
            vec.push(m.clone());
        }
    }

    vec
}

pub struct HandlerOptions {
    pub paths_and_methods: HashMap<String, HashSet<Method>>,
    pub cookies: bool,
    pub guards: Vec<String>,
}