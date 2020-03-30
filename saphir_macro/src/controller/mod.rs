use proc_macro2::{Ident, Span, TokenStream};
use syn::{export::ToTokens, spanned::Spanned, AttributeArgs, Error, GenericArgument, ItemImpl, PathArguments, Result, Type};

use quote::quote;

use crate::controller::{
    controller_attr::ControllerAttr,
    handler::{ArgsRepr, ArgsReprType, HandlerRepr, HandlerWrapperOpt, MapAfterLoad},
};

mod controller_attr;
mod handler;

pub fn expand_controller(args: AttributeArgs, input: ItemImpl) -> Result<TokenStream> {
    let controller_attr = ControllerAttr::new(args, &input)?;
    let handlers = handler::parse_handlers(input)?;

    let controller_implementation = controller_attr::gen_controller_trait_implementation(&controller_attr, handlers.as_slice());
    let struct_implementaion = gen_struct_implementation(controller_attr.ident.clone(), handlers)?;

    let mod_ident = Ident::new(&format!("SAPHIR_GEN_CONTROLLER_{}", &controller_attr.name), Span::call_site());
    Ok(quote! {
        mod #mod_ident {
            use super::*;
            use saphir::prelude::*;
            use std::str::FromStr;
            use std::collections::HashMap;
            #struct_implementaion

            #controller_implementation
        }
    })
}

fn gen_struct_implementation(controller_ident: Ident, handlers: Vec<HandlerRepr>) -> Result<TokenStream> {
    let mut handler_tokens = TokenStream::new();
    for handler in handlers {
        if handler.wrapper_options.needs_wrapper_fn() {
            gen_wrapper_handler(&mut handler_tokens, handler)?;
        } else {
            handler.original_method.to_tokens(&mut handler_tokens);
        }
    }

    let e = quote! {
        impl #controller_ident {
            #handler_tokens
        }
    };

    Ok(e)
}

fn gen_wrapper_handler(handler_tokens: &mut TokenStream, handler: HandlerRepr) -> Result<()> {
    let opts = &handler.wrapper_options;
    let m_ident = handler.original_method.sig.ident.clone();
    let return_type = handler.return_type;
    let mut o_method = handler.original_method;
    let mut m_inner_ident_str = m_ident.to_string();
    m_inner_ident_str.push_str("_wrapped");

    o_method.attrs.push(syn::parse_quote! {#[inline]});
    o_method.sig.ident = Ident::new(m_inner_ident_str.as_str(), Span::call_site());
    let inner_method_ident = o_method.sig.ident.clone();

    o_method.to_tokens(handler_tokens);

    let mut body_stream = TokenStream::new();
    init_multipart(&mut body_stream, opts);
    (quote! {let mut req = req}).to_tokens(&mut body_stream);
    gen_body_mapping(&mut body_stream, opts);
    gen_body_load(&mut body_stream, opts);
    gen_map_after_load(&mut body_stream, opts);
    (quote! {;}).to_tokens(&mut body_stream);
    gen_cookie_load(&mut body_stream, opts);
    gen_query_load(&mut body_stream, opts);
    let mut call_params_ident = Vec::new();
    for arg in &opts.fn_arguments {
        arg.gen_parameter(&mut body_stream, &mut call_params_ident)?;
    }
    let inner_call = gen_call_to_inner(inner_method_ident, call_params_ident, opts);

    let t = quote! {
        #[allow(unused_mut)]
        async fn #m_ident(&self, mut req: Request) -> Result<#return_type, SaphirError> {
            #body_stream
            Ok(#inner_call)
        }
    };

    t.to_tokens(handler_tokens);

    Ok(())
}

fn init_multipart(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if opts.init_multipart {
        (quote! {let multipart = Multipart::from_request(&mut req)?;

        })
        .to_tokens(stream);
    }
}

fn gen_cookie_load(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if opts.parse_cookies {
        (quote! {

            req.parse_cookies();
        })
        .to_tokens(stream);
    }
}

fn gen_query_load(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if opts.parse_query {
        (quote! {

        let mut query = req.uri().query().map(|query_str| serde_urlencoded::from_str::<HashMap<String, String>>(query_str)).transpose()?.unwrap_or_default();
        })
        .to_tokens(stream);
    }
}

fn gen_body_load(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if opts.need_body_load {
        (quote! {.load_body().await?}).to_tokens(stream);
    }
}

fn gen_map_after_load(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if let Some(m) = &opts.map_after_load {
        (match m {
            MapAfterLoad::Json => {
                quote! {.map(|b| Json(b))}
            }
            MapAfterLoad::Form => {
                quote! {.map(|b| Form(b))}
            }
        })
        .to_tokens(stream)
    }
}

fn gen_body_mapping(stream: &mut TokenStream, opts: &HandlerWrapperOpt) {
    if opts.map_multipart {
        (quote! {.map(|_| multipart)}).to_tokens(stream);
    } else if let Some(ty) = &opts.take_body_as {
        (quote! {.map(|mut b| b.take_as::<#ty>())}).to_tokens(stream);
    }
}

fn gen_call_to_inner(inner_method_ident: Ident, idents: Vec<Ident>, opts: &HandlerWrapperOpt) -> TokenStream {
    let mut call = TokenStream::new();

    (quote! {self.#inner_method_ident}).to_tokens(&mut call);

    gen_call_params(idents).to_tokens(&mut call);

    if !opts.sync_handler {
        (quote! {.await}).to_tokens(&mut call);
    }

    call
}

fn gen_call_params(idents: Vec<Ident>) -> TokenStream {
    let mut params = TokenStream::new();
    let paren = syn::token::Paren { span: Span::call_site() };

    paren.surround(&mut params, |params| {
        let mut ident_iter = idents.into_iter();

        if let Some(first_param) = ident_iter.next() {
            (quote! {#first_param}).to_tokens(params);

            for i in ident_iter {
                (quote! {, #i}).to_tokens(params)
            }
        }
    });

    params
}

impl ArgsRepr {
    pub fn gen_parameter(&self, stream: &mut TokenStream, call_ident: &mut Vec<Ident>) -> Result<()> {
        let mut self_flatten = self.clone();
        let (parameter_repr_type, optional) = if let ArgsReprType::Option(a) = &self.a_type {
            self_flatten.typ = self
                .typ
                .clone()
                .and_then(|t| t.path.segments.into_iter().next())
                .map(|first| first.arguments)
                .and_then(|a| {
                    if let PathArguments::AngleBracketed(p_a) = a {
                        return p_a.args.first().and_then(|g_a| {
                            if let GenericArgument::Type(Type::Path(type_path)) = g_a {
                                return Some(type_path.clone());
                            }

                            None
                        });
                    }

                    None
                });
            match a.as_ref() {
                ArgsReprType::SelfType | ArgsReprType::Request | ArgsReprType::Cookie => {
                    return Err(Error::new(
                        self.typ.as_ref().map(|t| t.span()).unwrap_or_else(Span::call_site),
                        "Optional parameters are only allowed for quey params, route params, or body param (Json or Form)",
                    ));
                }
                ArgsReprType::Option(_) => {
                    return Err(Error::new(
                        self.typ.as_ref().map(|t| t.span()).unwrap_or_else(Span::call_site),
                        "Option within option are not allowed as handler parameters",
                    ));
                }
                a => (a.clone(), true),
            }
        } else {
            (self.a_type.clone(), false)
        };

        self_flatten.a_type = parameter_repr_type;

        match &self_flatten.a_type {
            ArgsReprType::SelfType => {
                return Ok(());
            }
            ArgsReprType::Request => {
                call_ident.push(Ident::new("req", Span::call_site()));
                return Ok(());
            }
            ArgsReprType::Json => self_flatten.gen_json_param(stream, optional),
            ArgsReprType::Form => self_flatten.gen_form_param(stream, optional),
            ArgsReprType::Cookie => self_flatten.gen_cookie_param(stream),
            ArgsReprType::Params { is_query_param, .. } => {
                if *is_query_param {
                    self_flatten.gen_query_param(stream, optional);
                } else {
                    self_flatten.gen_path_param(stream, optional);
                }
            }
            ArgsReprType::Multipart => self_flatten.gen_multipart_param(stream),
            _ => { /* Nothing to do */ }
        }

        call_ident.push(Ident::new(self.name.as_str(), Span::call_site()));
        Ok(())
    }

    fn gen_multipart_param(&self, stream: &mut TokenStream) {
        let id = Ident::new(self.name.as_str(), Span::call_site());
        (quote! {

            let #id = multipart;
        })
        .to_tokens(stream);
    }

    fn gen_form_param(&self, stream: &mut TokenStream, optional: bool) {
        let id = Ident::new(self.name.as_str(), Span::call_site());
        let typ_raw = self.typ.as_ref().expect("This should not happens");

        let typ = self
            .typ
            .clone()
            .and_then(|t| t.path.segments.into_iter().next())
            .map(|first| first.arguments)
            .and_then(|a| {
                if let PathArguments::AngleBracketed(p_a) = a {
                    return p_a.args.first().and_then(|g_a| {
                        if let GenericArgument::Type(Type::Path(type_path)) = g_a {
                            return Some(type_path.clone());
                        }

                        None
                    });
                }

                None
            })
            .expect("This should not happens");

        (quote! {

            let #id = if let Some(form) = req.uri().query().map(|query_str| serde_urlencoded::from_str::<#typ>(query_str).map_err(SaphirError::SerdeUrlDe)) {
                Some(form.map(|x| Form(x)))
            } else {
                Some(req.body_mut().take_as::<#typ_raw>().await.map(|x| Form(x)))
            }.transpose()
        })
        .to_tokens(stream);

        if optional {
            (quote! {.ok().flatten()}).to_tokens(stream);
        } else {
            (quote! {?.ok_or_else(|| SaphirError::MissingParameter("form body".to_string(), false))?}).to_tokens(stream);
        }

        (quote! {;}).to_tokens(stream);
    }

    fn gen_json_param(&self, stream: &mut TokenStream, optional: bool) {
        let id = Ident::new(self.name.as_str(), Span::call_site());
        let typ = self.typ.as_ref().expect("This should not happens");

        (quote! {

            let #id = req.body_mut().take_as::<#typ>().await.map(|x| Json(x))
        })
        .to_tokens(stream);

        if optional {
            (quote! {.ok()}).to_tokens(stream);
        } else {
            (quote! {?}).to_tokens(stream);
        }

        (quote! {;}).to_tokens(stream);
    }

    fn gen_cookie_param(&self, stream: &mut TokenStream) {
        let id = Ident::new(self.name.as_str(), Span::call_site());
        (quote! {

            let #id = req.take_cookies();
        })
        .to_tokens(stream);
    }

    fn gen_path_param(&self, stream: &mut TokenStream, optional: bool) {
        let name = self.name.as_str();
        let id = Ident::new(self.name.as_str(), Span::call_site());
        (quote! {

            let #id = req.captures_mut().remove(#name)
        })
        .to_tokens(stream);

        self.gen_param_str_parsing(stream, name, optional);

        (quote! {;}).to_tokens(stream);
    }

    fn gen_query_param(&self, stream: &mut TokenStream, optional: bool) {
        let name = self.name.as_str();
        let id = Ident::new(self.name.as_str(), Span::call_site());
        (quote! {

            let #id = query.remove(#name)
        })
        .to_tokens(stream);

        self.gen_param_str_parsing(stream, name, optional);

        (quote! {;}).to_tokens(stream);
    }

    fn gen_param_str_parsing(&self, stream: &mut TokenStream, name: &str, optional: bool) {
        if !self.is_string() && self.typ.is_some() {
            let typ = self.typ.as_ref().unwrap();
            (quote! {.map(|p| p.parse::<#typ>()).transpose().map_err(|_| SaphirError::InvalidParameter(#name.to_string(), false))?}).to_tokens(stream);
        }

        if !optional {
            (quote! {.ok_or_else(|| SaphirError::MissingParameter(#name.to_string(), false))?}).to_tokens(stream);
        }
    }
}
