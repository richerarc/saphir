use proc_macro2::TokenStream;
use syn::{export::ToTokens, AttributeArgs, Error, Item, Lit, Meta, NestedMeta, Result};

const MISSING_ATRIBUTE: &str = "openapi macro require at least one of the following attributes :
- mime
- name";

pub fn validate_openapi(args: AttributeArgs, input: Item) -> Result<TokenStream> {
    match &input {
        Item::Struct(_) | Item::Enum(_) => {
            if args.is_empty() {
                panic!(MISSING_ATRIBUTE);
            }
        }
        _ => panic!("openapi attribute can only be placed on Struct and Enum"),
    }
    let mut mime: Option<String> = None;
    let mut name: Option<String> = None;
    for arg in args.into_iter() {
        if let NestedMeta::Meta(m) = arg {
            if let Meta::NameValue(nv) = m {
                match nv.path.get_ident().map(|i| i.to_string()).as_deref() {
                    Some("mime") => {
                        if mime.is_some() {
                            return Err(Error::new_spanned(nv, "Cannot specify `mime` twice"))
                        }
                        mime = match nv.lit {
                            Lit::Str(s) => Some(s.value()),
                            _ => None,
                        }
                    },
                    Some("name") => {
                        if name.is_some() {
                            return Err(Error::new_spanned(nv, "Cannot specify `name` twice"))
                        }
                        name = match nv.lit {
                            Lit::Str(s) => Some(s.value()),
                            _ => None,
                        }
                    }
                    _ => return Err(Error::new_spanned(nv, "Unrecognized parameter")),
                }
            }
        }
    }

    if mime.is_none() || name.is_none() {
        panic!(MISSING_ATRIBUTE);
    }

    Ok(input.to_token_stream())
}
