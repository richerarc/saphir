use proc_macro2::Ident;
use syn::{AttributeArgs, ItemImpl, NestedMeta, Meta, MetaList, MetaNameValue, Type, Lit};

#[derive(Debug)]
pub struct ControllerAttr {
    pub ident: Ident,
    pub name: String,
    pub version: Option<u16>,
    pub prefix: Option<String>,
}

impl ControllerAttr {
    pub fn new(args: AttributeArgs, input: &ItemImpl) -> ControllerAttr {
        let mut name = None;
        let mut version = None;
        let mut prefix = None;

        let ident = parse_controller_ident(input).expect("A controller can't have no ident");

        for m in args.into_iter().filter_map(|a| {
            if let NestedMeta::Meta(m) = a {
                Some(m)
            } else {
                None
            }
        }) {
            match m {
                Meta::Path(_p) => {
                    // no path at this time
                    // a path is => #[test]
                    //                ----
                }
                Meta::List(MetaList { path: _, paren_token: _, nested: _ }) => {
                    // no List at this time
                    // a List is => #[test(A, B)]
                    //                ----------
                }
                Meta::NameValue(MetaNameValue { path, eq_token: _, lit }) => {
                    match (path.segments.first().map(|p| p.ident.to_string()).as_ref().map(|s| s.as_str()), lit) {
                        (Some("name"), Lit::Str(bp)) => {
                            name = Some(bp.value().trim_matches('/').to_string());
                        },
                        (Some("version"), Lit::Str(v)) => {
                            version = Some(v.value().parse::<u16>().expect("Invalid version, expected number between 1 & u16::MAX"))
                        },
                        (Some("version"), Lit::Int(v)) => {
                            version = Some(v.base10_parse::<u16>().expect("Invalid version, expected number between 1 & u16::MAX"))
                        },
                        (Some("prefix"), Lit::Str(p)) => {
                            prefix = Some(p.value().trim_matches('/').to_string());
                        },
                        _ => {}
                    }
                }
            }
        }

        let name = name.unwrap_or_else(|| {
            ident.to_string().to_lowercase().trim_end_matches("controller").to_string()
        });

        ControllerAttr {
            ident,
            name,
            version,
            prefix,
        }
    }
}

fn parse_controller_ident(input: &ItemImpl) -> Option<Ident> {
    match input.self_ty.as_ref() {
        Type::Path(p) => {
            if let Some(f) = p.path.segments.first() {
                return Some(f.ident.clone());
            }
        }
        _ => {}
    }

    None
}