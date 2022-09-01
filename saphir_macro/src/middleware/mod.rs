use proc_macro2::{Ident, Span, TokenStream};
use syn::{Error, ImplItem, ItemImpl, Result};

use quote::quote;

mod fun;

pub fn expand_middleware(mut mid_impl: ItemImpl) -> Result<TokenStream> {
    let mid_fn = remove_middleware_fn(&mut mid_impl)?;

    let fn_def = fun::MidFnDef::new(mid_fn)?;
    mid_impl.items.push(fn_def.def);

    let middleware_ident = crate::utils::parse_item_impl_ident(&mid_impl)?;
    let middleware_name = middleware_ident.to_string();

    let mod_ident = Ident::new(&format!("SAPHIR_GEN_MIDDLEWARE_{}", &middleware_name), Span::call_site());
    let fn_ident = fn_def.fn_ident;

    Ok(quote! {
        #mid_impl

        mod #mod_ident {
            use super::*;
            use saphir::prelude::*;

            impl Middleware for #middleware_ident {
                fn next(&'static self, ctx: HttpContext, chain: &'static dyn MiddlewareChain) -> BoxFuture<'static, Result<HttpContext, SaphirError>> {
                    self.#fn_ident(ctx, chain).boxed()
                }
            }
        }
    })
}

fn remove_middleware_fn(input: &mut ItemImpl) -> Result<ImplItem> {
    let mid_fn_pos = input.items.iter().position(|item| {
        if let ImplItem::Method(m) = item {
            return m.sig.ident == "next";
        }

        false
    }).ok_or_else(|| Error::new_spanned(&input, "No method `next` found in the impl section of the middleware.\nMake sure the impl block contains a fn with the following signature:\n `async fn next(&self, _: HttpContext, _: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError>`"))?;

    let mid_fn = input.items.remove(mid_fn_pos);

    Ok(mid_fn)
}
