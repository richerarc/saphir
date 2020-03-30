use proc_macro2::Ident;
use syn::{Error, ItemImpl, Result, Type};

pub fn parse_item_impl_ident(input: &ItemImpl) -> Result<Ident> {
    if let Type::Path(p) = input.self_ty.as_ref() {
        if let Some(f) = p.path.segments.first() {
            return Ok(f.ident.clone());
        }
    }

    Err(Error::new_spanned(input, "Unable to parse impl ident. this is fatal"))
}
