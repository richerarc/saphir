use proc_macro2::{Ident, Span};
use syn::{
    AngleBracketedGenericArguments, Error, FnArg, GenericArgument, ImplItem, PatType, Path, PathArguments, Result, ReturnType, Signature, Type, TypePath,
};

pub struct GuardFnDef {
    pub def: ImplItem,
    pub responder: Path,
    pub fn_ident: Ident,
}

impl GuardFnDef {
    pub fn new(g_fn: ImplItem) -> Result<Self> {
        let mut m = if let ImplItem::Method(m) = g_fn {
            m
        } else {
            return Err(Error::new_spanned(g_fn, "The token named validate is not method"));
        };

        let responder = check_signature(&m.sig)?;

        let fn_ident = Ident::new(&format!("{}_wrapped", m.sig.ident.to_string()), Span::call_site());
        m.sig.ident = fn_ident.clone();

        Ok(GuardFnDef {
            def: ImplItem::Method(m),
            responder,
            fn_ident,
        })
    }
}

fn check_signature(m: &Signature) -> Result<Path> {
    if m.asyncness.is_none() {
        return Err(Error::new_spanned(m, "Invalid function signature, the guard function should be async"));
    }

    if m.inputs.len() != 2 {
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
        FnArg::Typed(PatType { ty, .. }) => {
            if let Type::Path(TypePath {
                path: Path { segments, .. }, ..
            }) = ty.as_ref()
            {
                segments
                    .first()
                    .ok_or_else(|| Error::new_spanned(&ty, "Unexpected type"))?
                    .ident
                    .to_string()
                    .eq("Request")
            } else {
                false
            }
        }
        _ => false,
    };

    if !passed {
        return Err(Error::new_spanned(arg2, "Invalid 2nd parameter, expected `Request`"));
    }

    let resp = if let ReturnType::Type(_, ret) = &m.output {
        if let Type::Path(TypePath {
            path: Path { segments, .. }, ..
        }) = ret.as_ref()
        {
            let r = segments.first().ok_or_else(|| Error::new_spanned(segments, "Unexpected type"))?;
            if r.ident.to_string().ne("Result") {
                return Err(Error::new_spanned(
                    &r,
                    &format!(
                        "Invalid return type for the validate fn, expected Result<Request, impl Responder>, got {}",
                        r.ident.to_string()
                    ),
                ));
            }
            if let PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) = &r.arguments {
                if args.len() != 2 {
                    return Err(Error::new_spanned(
                        args,
                        "Unexpected return type for the validate fn, expected 2 type argument inside Result",
                    ));
                }
                let mut args = args.iter();

                let out1 = args.next().expect("len checked above");
                if let GenericArgument::Type(Type::Path(TypePath {
                    path: Path { segments, .. }, ..
                })) = out1
                {
                    let segment_name = segments
                        .first()
                        .ok_or_else(|| Error::new_spanned(segments, "Unexpected type"))?
                        .ident
                        .to_string();
                    if segment_name.ne("Request") {
                        return Err(Error::new_spanned(
                            segments,
                            &format!(
                                "Invalid return type for the validate fn, expected Result<Request, impl Responder>, got Result<{}, ..>",
                                segment_name
                            ),
                        ));
                    }
                }
                let out2 = args.next().expect("len checked above");
                if let GenericArgument::Type(Type::Path(TypePath { path, .. })) = out2 {
                    path.clone()
                } else {
                    return Err(Error::new_spanned(
                        &m.output,
                        "Unexpected return type for the validate fn, expected Result<Request, impl Responder>",
                    ));
                }
            } else {
                return Err(Error::new_spanned(
                    &m.output,
                    "Unexpected return type for the validate fn, expected Result<Request, impl Responder>",
                ));
            }
        } else {
            return Err(Error::new_spanned(
                &m.output,
                "Unexpected return type for the validate fn, expected Result<Request, impl Responder>",
            ));
        }
    } else {
        return Err(Error::new_spanned(
            &m.output,
            "Invalid return type for the validate fn, expected Result<Request, impl Responder>, got ()",
        ));
    };

    Ok(resp)
}
