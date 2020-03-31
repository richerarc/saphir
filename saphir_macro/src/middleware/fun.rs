use proc_macro2::{Ident, Span};
use syn::{Error, FnArg, ImplItem, Result, Signature, Type, TypeParamBound};

pub struct MidFnDef {
    pub def: ImplItem,
    pub fn_ident: Ident,
}

impl MidFnDef {
    pub fn new(mid_fn: ImplItem) -> Result<Self> {
        let mut m = if let ImplItem::Method(m) = mid_fn {
            m
        } else {
            return Err(Error::new_spanned(mid_fn, "The token named next is not method"));
        };

        check_signature(&m.sig)?;

        let fn_ident = Ident::new(&format!("{}_wrapped", m.sig.ident.to_string()), Span::call_site());
        m.sig.ident = fn_ident.clone();

        Ok(MidFnDef {
            def: ImplItem::Method(m),
            fn_ident,
        })
    }
}

fn check_signature(m: &Signature) -> Result<()> {
    if m.asyncness.is_none() {
        return Err(Error::new_spanned(m, "Invalid function signature, the middleware function should be async"));
    }

    if m.inputs.len() != 3 {
        return Err(Error::new_spanned(
            m,
            "Invalid middleware function input parameters.\nExpected the following parameters:\n (&self, _: HttpContext, _: &dyn MiddlewareChain)",
        ));
    }

    let mut input_args = m.inputs.iter();

    match input_args.next().expect("len was checked above") {
        FnArg::Receiver(_) => {}
        arg => {
            return Err(Error::new_spanned(arg, "Invalid 1st parameter, expected `&self`"));
        }
    }

    let arg2 = input_args.next().expect("len was checked above");
    let passed = match arg2 {
        FnArg::Typed(t) => {
            if let Type::Path(pt) = &*t.ty {
                pt.path
                    .segments
                    .first()
                    .ok_or_else(|| Error::new_spanned(&t.ty, "Unexpected type"))?
                    .ident
                    .to_string()
                    .eq("HttpContext")
            } else {
                false
            }
        }
        _ => false,
    };

    if !passed {
        return Err(Error::new_spanned(arg2, "Invalid 2nd parameter, expected `HttpContext`"));
    }

    let arg3 = input_args.next().expect("len was checked above");
    let passed = match arg3 {
        FnArg::Typed(t) => {
            if let Type::Reference(tr) = &*t.ty {
                if let Type::TraitObject(to) = &*tr.elem {
                    if let TypeParamBound::Trait(bo) = to.bounds.first().ok_or_else(|| Error::new_spanned(&t.ty, "Unexpected type"))? {
                        bo.path
                            .segments
                            .first()
                            .ok_or_else(|| Error::new_spanned(&t.ty, "Unexpected type"))?
                            .ident
                            .to_string()
                            .eq("MiddlewareChain")
                    } else {
                        false
                    }
                } else if let Type::Path(pt) = &*tr.elem {
                    pt.path
                        .segments
                        .first()
                        .ok_or_else(|| Error::new_spanned(&t.ty, "Unexpected type"))?
                        .ident
                        .to_string()
                        .eq("MiddlewareChain")
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
    };

    if !passed {
        return Err(Error::new_spanned(arg3, "Invalid 3nd parameter, expected `&dyn MiddlewareChain`"));
    }

    Ok(())
}
