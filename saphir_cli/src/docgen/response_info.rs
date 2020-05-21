use crate::docgen::type_info::TypeInfo;
use crate::docgen::DocGen;
use syn::{Signature, ReturnType, ImplItemMethod, Type};
use crate::docgen::crate_syn_browser::File;
use syn::token::{Macro, Token};
use syn::parse::{Parse, ParseStream};

#[derive(Clone, Debug)]
pub(crate) struct ResponseInfo {
    pub(crate) code: u16,
    pub(crate) type_info: Option<TypeInfo>,
}

impl DocGen {
    pub(crate) fn extract_response_info(
        &self,
        file: &File,
        im: &ImplItemMethod,

    ) -> Result<Vec<ResponseInfo>, String> {
        match &im.sig.output {
            ReturnType::Default => Ok(vec![ResponseInfo { code: 200, type_info: None }]),
            ReturnType::Type(_tokens, t) => {
                // let parsed: Punctuated<PathSegment, Token![::]> = syn::parse(tokens)?;
                self.response_info_from_type(file, im, t)
            }
        }
    }

    fn response_info_from_type(
        &self,
        file: &File,
        im: &ImplItemMethod,
        t: &Type,
    ) -> Result<Vec<ResponseInfo>, String> {
        let vec = Vec::new();

        match t {
            Type::Path(tp) => {
                if let Some(first) = tp.path.segments.first() {
                    let name = first.ident.to_string();
                    println!("type : {:?} with args : {:?}", &name, first.arguments);
                }
            },
            Type::Tuple(tt) => {
                // TODO: Tuple with with StatusCode or u16 mean a status code is specified for the associated return type.
                //       We cannot possibly cover this case fully but we could at least handle simple cases where
                //       the response is a litteral inside the method's body
            }
            _ => { }
        }

        Ok(vec)
    }
}