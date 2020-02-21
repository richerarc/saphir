// The `quote!` macro requires deep recursion.
#![recursion_limit = "512"]

#[macro_use]
extern crate quote;
#[macro_use]
extern crate syn;

extern crate proc_macro;
extern crate proc_macro2;

use quote::quote;
use proc_macro::{TokenStream, TokenTree, Literal as Literal1, Ident as Ident1};
use proc_macro2::{Ident, TokenStream as TokenStream2, Span};
use syn::{parse_macro_input, ItemImpl, Type};
use syn::token::Token;

#[proc_macro_attribute]
pub fn controller(attr: TokenStream, input: TokenStream) -> TokenStream {
    // let attr = parse_macro_input!(attr as Attribute);
    dbg!(attr);
    dbg!(input.to_string());
    let input = parse_macro_input!(input as ItemImpl);

    let ident = parse_controller_ident(&input).expect("Unable to locate controller Ident, don't use path in your impl");

    let base_path = gen_base_path_const(None, &ident);

    dbg!(base_path);

    let expanded = quote! {
        // ...
    };

    TokenStream::from(expanded)
}

fn gen_base_path_const(base_path: Option<String>, ident: &Ident) -> TokenStream2 {
    let mut base_path = base_path.unwrap_or_else(|| {
        ident.to_string().to_lowercase().trim_end_matches("controller").to_string()
    });

    base_path.insert(0, '/');

    let expanded = quote! {
        const BASE_PATH: &'static str = #base_path;
    };

    expanded
}

fn parse_controller_ident(input: &ItemImpl) -> Option<Ident> {
    match input.self_ty.as_ref() {
        Type::Path(p) => {
            if let Some(f) = p.path.segments.first() {
                return Some(f.ident.clone());
            }
        },
        _ => {}
    }

    None
}

// fn parse_controller_attr(attr: TokenStream) -> ControllerAttr {
//     let (idents, lits): (Vec<AttrPairType>, Vec<AttrPairType>) = attr.into_iter().filter_map(|t| {
//         match t {
//             TokenTree::Group(g) => {
//                 match g.stream() {
//                     TokenTree::Literal(l) => Some(AttrPairType::L(l)),
//                     _ => None,
//                 }
//             },
//             TokenTree::Ident(i) => Some(AttrPairType::I(i)),
//             TokenTree::Literal(l) => Some(AttrPairType::L(l)),
//             _ => None
//         }
//     }).partition(|a| {
//         match a {
//             AttrPairType::I(_) => true,
//             AttrPairType::L(_) => false,
//         }
//     });
//
//     let mut base_path = None;
//     let mut version = None;
//     let mut prefix = None;
//
//     ControllerAttr {
//         base_path: None,
//         version: None,
//         prefix: None
//     }
// }

struct ControllerAttr {
    base_path: Option<String>,
    version: Option<u16>,
    prefix: Option<String>
}

enum AttrPairType {
    I(Ident1),
    L(Literal1)
}