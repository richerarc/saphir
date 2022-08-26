use proc_macro2::{Ident, Span, TokenStream};
use syn::{Error, ImplItem, ItemImpl, Result};

use quote::quote;

mod fun;

pub fn expand_guard(mut guard_impl: ItemImpl) -> Result<TokenStream> {
    let guard_fn = remove_validate_fn(&mut guard_impl)?;

    let fn_def = fun::GuardFnDef::new(guard_fn)?;
    guard_impl.items.push(fn_def.def);

    let guard_ident = crate::utils::parse_item_impl_ident(&guard_impl)?;
    let guard_name = guard_ident.to_string();

    let mod_ident = Ident::new(&format!("SAPHIR_GEN_GUARD_{}", &guard_name), Span::call_site());
    let fn_ident = fn_def.fn_ident;
    let resp_type = fn_def.responder;

    Ok(quote! {
        #guard_impl

        mod #mod_ident {
            use super::*;
            use saphir::prelude::*;

            impl Guard for #guard_ident {
                type Future = BoxFuture<'static, Result<Request, Self::Responder>>;
                type Responder = #resp_type;

                fn validate(&'static self, req: Request<Body<Bytes>>) -> Self::Future {
                    self.#fn_ident(req).boxed()
                }
            }
        }
    })
}

fn remove_validate_fn(input: &mut ItemImpl) -> Result<ImplItem> {
    let mid_fn_pos = input.items.iter().position(|item| {
        if let ImplItem::Method(m) = item {
            return m.sig.ident.to_string().eq("validate");
        }

        false
    }).ok_or_else(|| Error::new_spanned(&input, "No method `validate` found in the impl section of the middleware.\nMake sure the impl block contains a fn with the following signature:\n `async fn validate(&self, req: Request) -> Result<Request, impl Responder>`"))?;

    let mid_fn = input.items.remove(mid_fn_pos);

    Ok(mid_fn)
}
