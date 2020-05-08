use crate::openapi::{OpenApi, OpenApiContent, OpenApiMimeTypes, OpenApiParameter, OpenApiParameterLocation, OpenApiPath, OpenApiPathMethod, OpenApiRequestBody, OpenApiResponse, OpenApiSchema, OpenApiType, OpenApiObjectType};
use crate::{Command, CommandResult};
use convert_case::{Case, Casing};
use serde::export::TryFrom;
use serde_derive::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;
use structopt::StructOpt;
use syn::{
    Attribute, Expr, Fields, File as AstFile, FnArg, GenericArgument, ImplItem, ImplItemMethod, Item, ItemEnum, ItemImpl, ItemStruct, Lit, Meta, NestedMeta,
    Pat, PathArguments, Signature, Type, UseTree,
};
use crate::docgen::type_info::TypeInfo;
use std::cell::RefCell;
use std::rc::Rc;
use std::pin::Pin;
use crate::docgen::route_info::RouteInfo;

mod type_info;
mod route_info;

macro_rules! print_project_path_error {
    ($file:expr, $project_path:expr) => {{
        let project_path = $project_path.to_str().map(|s| s.to_owned()).unwrap_or_else(|| format!("{:?}", $project_path));
        format!(
            "Unable to find `{}` in project root `{}`.
Make sure that you are either running this command from your project's root,
or that the argument --project-path (-p) point to the project's root.
You can see help with the --help flag.",
            $file, project_path
        )
    }};
}

/// Generate OpenAPI v3 from a Saphir application.
///
/// See: https://github.com/OAI/OpenAPI-Specification/blob/master/versions/3.0.2.md
#[derive(StructOpt, Debug, Default)]
pub(crate) struct DocGenArgs {
    /// (Optional) Limit doc generation to the URIs under this scope.
    ///
    /// For example, if you pass `/api/v1` and that your Saphir server had handlers
    /// for the following routes :
    /// - GET  /
    /// - GET  /about
    /// - GET  /api/v1/user
    /// - POST /api/v1/user
    /// , the generated doc would contain only the `/api/v2/user` endpoints.
    #[structopt(short = "s", long = "scope", default_value = "/", verbatim_doc_comment)]
    scope: String,

    /// (Optional) path to the Saphir server's root
    #[structopt(parse(from_os_str), short = "p", long = "project-path", default_value = ".")]
    project_path: PathBuf,

    /// (optionnal) Path to the `.cargo` directory. By default, read from the current executable's environment,
    ///             which work when running this command as a cargo sub-command.
    #[structopt(parse(from_os_str), long = "cargo-path", default_value = "~/.cargo")]
    cargo_path: PathBuf,

    /// (Optional) Resulting output path. Either the path to the resulting yaml file,
    ///            or a dir, which would then contain a openapi.yaml
    #[structopt(parse(from_os_str), default_value = ".")]
    output_file: PathBuf,
}

#[derive(Default)]
pub(crate) struct DocGen {
    pub args: <DocGen as Command>::Args,
    pub doc: OpenApi,
    pub operation_ids: RefCell<HashSet<String>>,
    pub handlers: RefCell<Vec<HandlerInfo>>,
    pub loaded_files_ast: RefCell<HashMap<String, Pin<Box<AstFile>>>>,
    pub dependancies: RefCell<HashMap<String, Pin<Box<HashMap<String, CargoDependancy>>>>>,
}

impl Command for DocGen {
    type Args = DocGenArgs;

    fn new(args: Self::Args) -> Self {
        let mut doc = OpenApi::default();
        doc.openapi_version = "3.0.3".to_string();
        Self {
            args,
            doc,
            ..Default::default()
        }
    }

    fn run(mut self) -> CommandResult {
        let now = Instant::now();
        self.read_cargo_toml()?;
        self.read_cargo_dependancies("crate", self.args.project_path.clone())?;
        self.read_rust_entrypoint_ast(self.args.project_path.clone(), "src/main.rs")?;
        self.parse_ast()?;
        let handlers = std::mem::take(&mut *self.handlers.borrow_mut());
        self.add_all_paths(handlers)?;
        let file = self.write_doc_file()?;
        println!("Succesfully created `{}` in {}ms", file, now.elapsed().as_millis());
        Ok(())
    }
}

#[derive(Default, Debug)]
pub(crate) struct CargoDependancy {
    pub name: String,
    pub version: String,
    pub targets: Vec<CargoTarget>,
}

#[derive(Default, Debug)]
pub(crate) struct CargoTarget {
    pub target_type: String,
    pub path: PathBuf,
}

impl DocGen {
    fn read_cargo_dependancies(&mut self, crate_name: &str, root_path: PathBuf) -> CommandResult {
        let metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(root_path.join("Cargo.toml"))
            .exec().map_err(|e| e.to_string())?;
        let mut dependancies = HashMap::new();
        for package in metadata.packages {
            let mut targets = Vec::new();
            for target in &package.targets {
                if target.kind.contains(&"bin".to_string()) || target.kind.contains(&"lib".to_string()) {
                    targets.push(CargoTarget {
                        target_type: target.kind.join(","),
                        path: target.src_path.clone(),
                    })
                }
            }
            let name = package.name.replace("-", "_");
            let dependancy = CargoDependancy {
                name: name.clone(),
                version: package.version.to_string(),
                targets,
            };
            dependancies.insert(name, dependancy);
        }
        self.dependancies.borrow_mut().insert(crate_name.to_string(), Box::pin(dependancies));
        Ok(())
    }

    fn write_doc_file(&self) -> Result<String, String> {
        let mut path = self.args.output_file.clone();
        if path.is_dir() {
            path = path.join("openapi.yaml");
        }
        match path.extension() {
            None => path = path.with_extension("yaml"),
            Some(ext) => {
                if ext.to_str() != Some("yaml") && ext.to_str() != Some("yml") {
                    return Err("output must be a yaml file.".to_string());
                }
            }
        }
        let f = File::create(&path).map_err(|_| format!("Unable to create file `{:?}`", &path))?;
        serde_yaml::to_writer(f, &self.doc).map_err(|_| format!("Unable to write to `{:?}`", path))?;
        Ok(path.to_str().unwrap_or_default().to_string())
    }

    fn read_cargo_toml(&mut self) -> CommandResult {
        #[derive(Deserialize)]
        struct Cargo {
            pub package: Package,
        }
        #[derive(Deserialize)]
        struct Package {
            pub name: String,
            pub version: String,
        }
        let cargo_path = self.args.project_path.clone().join("Cargo.toml");
        let mut f = File::open(&cargo_path).map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer)
            .map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;
        let cargo: Cargo = toml::from_str(buffer.as_str()).map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;

        self.doc.info.title = cargo.package.name;
        self.doc.info.version = cargo.package.version;

        Ok(())
    }

    fn read_rust_entrypoint_ast(&mut self, path: PathBuf, entry_file_path: &str) -> CommandResult {
        let file_path = path.join(entry_file_path);

        let mut f = File::open(&file_path).map_err(|_| print_project_path_error!(entry_file_path, path))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer)
            .map_err(|_| print_project_path_error!(entry_file_path, path))?;
        let path = file_path.to_str().ok_or(format!("Invalid path : `{:?}`", &file_path))?.to_string();
        self.read_mods_in_ast(path, "crate".to_string(), buffer)
    }

    fn read_mods_in_ast(&mut self, file: String, module_path: String, buffer: String) -> CommandResult {
        let file = std::path::Path::new(file.as_str());
        let dir = file.parent().ok_or(format!("`{:?}` is not a path to a rust file", file)).unwrap();
        let dir = dir.to_str().unwrap();

        let ast = syn::parse_file(buffer.as_str()).map_err(|_| format!("Unable to parse the module file `{:?}`", file))?;
        for md in ast.items.iter().filter_map(|i| match i {
            Item::Mod(md) => Some(md),
            _ => None
        }) {
            let mod_name = md.ident.to_string();
            if md.content.is_none() {
                self.read_mod_file(dir.to_string(), module_path.clone(), mod_name)?;
            }
        }
        self.loaded_files_ast.borrow_mut().insert(module_path.clone(), Box::pin(ast));
        Ok(())
    }

    fn read_mod_file(&mut self, dir: String, module_path: String, mod_name: String) -> CommandResult {
        let dir = std::path::Path::new(dir.as_str());
        let mut path = dir.join(mod_name.as_str());
        if path.is_dir() {
            path = path.join("mod.rs");
        } else {
            path = dir.join(format!("{}.rs", mod_name).as_str());
        }
        let path_str = path.to_str().ok_or(format!("Invalid path : `{:?}`", path))?.to_string();
        let mut f = File::open(path).map_err(|_| format!("Unable to read module `{}`", mod_name))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).map_err(|_| format!("Unable to read module `{}`", mod_name))?;

        self.read_mods_in_ast(path_str, format!("{}::{}", module_path, mod_name), buffer)
    }

    fn parse_ast(&self) -> CommandResult {
        for (module_path, ast) in self.loaded_files_ast.borrow().iter() {
            for md in ast.items.iter().filter_map(|i| match i {
                Item::Mod(md) => Some(md),
                _ => None
            }) {
                if let Some((_, items)) = &md.content {
                    self.parse_controllers_ast(module_path.clone(), items)?;
                }
            }
            self.parse_controllers_ast(module_path.clone(), &ast.items)?;
        }
        Ok(())
    }

    fn parse_controllers_ast(&self, module_path: String, items: &Vec<Item>) -> CommandResult {
        for im in items.iter().filter_map(|i| match i {
            Item::Impl(im) => Some(im),
            _ => None,
        }) {
            if let Some(controller) = self.get_controller_info(im) {
                self.parse_handlers_ast(module_path.clone(), controller, &im.items)?;
            }
        }
        Ok(())
    }

    fn get_controller_info(&self, im: &ItemImpl) -> Option<ControllerInfo> {
        for attr in &im.attrs {
            if let Some(first_seg) = attr.path.segments.first() {
                let t = im.self_ty.as_ref();
                match t {
                    Type::Path(p) => {
                        if let Some(struct_first_seg) = p.path.segments.first() {
                            if first_seg.ident.eq("controller") {
                                let controller_name = struct_first_seg.ident.to_string();
                                let name = controller_name.to_ascii_lowercase();
                                let name = &name[0..name.rfind("controller").unwrap_or_else(|| name.len())];
                                let mut name = name.to_string();
                                let mut prefix = None;
                                let mut version = None;
                                if let Ok(Meta::List(meta)) = attr.parse_meta() {
                                    for nested in meta.nested {
                                        if let NestedMeta::Meta(Meta::NameValue(nv)) = nested {
                                            if let Some(p) = nv.path.segments.first() {
                                                let value = match nv.lit {
                                                    Lit::Str(s) => s.value(),
                                                    Lit::Int(i) => i.to_string(),
                                                    _ => continue,
                                                };
                                                match p.ident.to_string().as_str() {
                                                    "name" => name = value,
                                                    "prefix" => prefix = Some(value),
                                                    "version" => version = Some(value),
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }

                                return Some(ControllerInfo {
                                    controller_name,
                                    name,
                                    prefix,
                                    version,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    // Return a vec of models to load from the current file
    fn parse_handlers_ast(&self, module_path: String, controller: ControllerInfo, items: &Vec<ImplItem>) -> CommandResult {
        for m in items.iter().filter_map(|i| match i {
            ImplItem::Method(m) => Some(m),
            _ => None,
        }) {
            let mut consume_cookies: bool = self.handler_has_cookies(&m);
            let routes = self.route_info_from_method_macro(&controller, &m);
            if routes.is_empty() {
                continue;
            }
            let parameters_info = self.parse_handler_parameters(module_path.clone(), &m, &routes[0].uri_params);
            if parameters_info.has_cookies_param {
                consume_cookies = true;
            }

            for route in routes {
                let operation_id = self.handler_operation_id_from_sig(&m.sig);
                self.handlers.borrow_mut().push(HandlerInfo {
                    module_path: module_path.clone(),
                    controller: controller.clone(),
                    route,
                    parameters: parameters_info.parameters.clone(),
                    operation_id,
                    use_cookies: consume_cookies,
                    body_info: parameters_info.body_info.clone(),
                });
            }
        }
        Ok(())
    }

    fn parse_handler_parameters(&self, module_path: String, m: &ImplItemMethod, uri_params: &Vec<String>) -> RouteParametersInfo {
        let mut parameters = Vec::new();
        let mut has_cookies_param = false;
        let mut body_type = None;
        for param in m.sig.inputs.iter().filter_map(|i| match i {
            FnArg::Typed(p) => Some(p),
            _ => None,
        }) {
            let param_name = match param.pat.as_ref() {
                Pat::Ident(i) => i.ident.to_string(),
                _ => continue,
            };

            let (param_type, optional) = match param.ty.as_ref() {
                Type::Path(p) => {
                    if let Some(s1) = p.path.segments.first() {
                        let mut param_type = s1.ident.to_string();
                        if param_type.as_str() == "CookieJar" {
                            has_cookies_param = true;
                            continue;
                        }
                        if param_type.as_str() == "Request" {
                            if let PathArguments::AngleBracketed(ab) = &s1.arguments {
                                if let Some(GenericArgument::Type(Type::Path(body_path))) = ab.args.first() {
                                    if let Some(seg) = body_path.path.segments.first() {
                                        body_type = Some(seg);
                                    }
                                }
                            }
                            continue;
                        }
                        if param_type.as_str() == "Json" || param_type.as_str() == "Form" {
                            body_type = Some(&s1);
                            continue;
                        }
                        let optional = param_type.as_str() == "Option";
                        if optional {
                            param_type = "String".to_string();
                            if let PathArguments::AngleBracketed(ab) = &s1.arguments {
                                if let Some(GenericArgument::Type(Type::Path(p))) = ab.args.first() {
                                    if let Some(i) = p.path.get_ident() {
                                        param_type = i.to_string();
                                    }
                                }
                            }
                        }

                        let api_type = OpenApiType::from_rust_type_str(param_type.as_str());
                        (api_type, optional)
                    } else {
                        (OpenApiType::string(), false)
                    }
                }
                _ => (OpenApiType::string(), false),
            };

            let location = if uri_params.contains(&param_name) {
                OpenApiParameterLocation::Path
            } else {
                OpenApiParameterLocation::Query
            };
            parameters.push(OpenApiParameter {
                name: param_name,
                required: !optional,
                location,
                schema: OpenApiSchema::Inline(param_type),
                ..Default::default()
            })
        }

        let mut body_info: Option<BodyParamInfo> = None;
        if let Some(body) = body_type {
            let body_type = body.ident.to_string();
            let openapi_type = match body_type.as_str() {
                "Json" => OpenApiMimeTypes::Json,
                "Form" => OpenApiMimeTypes::Form,
                _ => OpenApiMimeTypes::Any,
            };
            match body_type.as_str() {
                "Json" | "Form" => {
                    if let PathArguments::AngleBracketed(ag) = &body.arguments {
                        if let Some(GenericArgument::Type(t)) = ag.args.first() {
                            let loaded_files_ast = self.loaded_files_ast.borrow();
                            if let Some(ast_file) = loaded_files_ast.get(module_path.as_str()) {
                                if let Some(type_info) = self.find_type_info(
                                    ast_file,
                                    t
                                ) {
                                    body_info = Some(BodyParamInfo { openapi_type, type_info });
                                }
                            }
                        }
                    }
                }
                _ => {}
            };
        }

        RouteParametersInfo {
            parameters,
            has_cookies_param,
            body_info,
        }
    }

    // fn parse_ast_type(&self, module_path: String, t: &Type) -> Option<TypeInfo> {
    //     let mut type_info = None;
    //     match t {
    //         Type::Path(p) => {
    //             let name = p.path.get_ident().map(|i| i.to_string());
    //             if let Some(name) = name {
    //                 type_info = Some(TypeInfo { name, ..Default::default() });
    //             } else {
    //                 if let Some(s) = p.path.segments.first() {
    //                     match s.ident.to_string().as_str() {
    //                         "Vec" => {
    //                             if let PathArguments::AngleBracketed(ag) = &s.arguments {
    //                                 if let Some(GenericArgument::Type(t)) = ag.args.first() {
    //                                     type_info = self.parse_ast_type(module_path.clone(), t);
    //                                     if let Some(mut info) = type_info.as_mut() {
    //                                         info.is_array = true;
    //                                     }
    //                                 }
    //                             }
    //                         }
    //                         "Option" => {
    //                             if let PathArguments::AngleBracketed(ag) = &s.arguments {
    //                                 if let Some(GenericArgument::Type(t)) = ag.args.first() {
    //                                     type_info = self.parse_ast_type(module_path.clone(), t);
    //                                     if let Some(mut info) = type_info.as_mut() {
    //                                         info.is_optional = true;
    //                                     }
    //                                 }
    //                             }
    //                         }
    //                         _ => {}
    //                     }
    //                 }
    //             }
    //         }
    //         Type::Array(a) => {
    //             let len: Option<u32> = match &a.len {
    //                 Expr::Lit(l) => match &l.lit {
    //                     Lit::Int(i) => i.base10_parse().ok(),
    //                     _ => None,
    //                 },
    //                 _ => None,
    //             };
    //             type_info = self.parse_ast_type(module_path.clone(), t);
    //             if let Some(mut info) = type_info.as_mut() {
    //                 info.is_array = true;
    //                 info.min_array_len = len.clone();
    //                 info.max_array_len = len;
    //             }
    //         }
    //         _ => return None,
    //     };
    //
    //     if let Some(mut type_info) = type_info {
    //         if type_info.use_name.is_empty() {
    //             if let Some((use_path, use_name)) = self.resolve_use(module_path.clone(), type_info.name.clone()) {
    //                 type_info.use_name = use_name;
    //                 type_info.use_path = use_path;
    //                 Some(type_info)
    //             } else {
    //                 println!("Unable to resolve {:?} in {:?}", type_info.name, module_path);
    //                 None
    //             }
    //         } else {
    //             Some(type_info)
    //         }
    //     } else {
    //         None
    //     }
    // }

    fn add_all_paths(&mut self, handlers: Vec<HandlerInfo>) -> CommandResult {
        let (_, errors): (Vec<_>, Vec<_>) = handlers.into_iter().map(|handler| self.add_path(handler)).partition(Result::is_ok);
        let errors: Vec<_> = errors.into_iter().map(Result::unwrap_err).collect();
        if errors.len() > 0 {
            let mut error_message = format!("Some errors ({}) occured while processing the routes : ", errors.len());
            for error in errors {
                error_message = format!("{}\n- {}", error_message, error);
            }
            Err(error_message)
        } else {
            Ok(())
        }
    }

    // fn resolve_use(&self, module_path: String, type_name: String) -> Option<(String, String)> {
    //     let ast = self.loaded_files_ast.get(module_path.as_str())?;
    //     for u in ast.items.iter().filter_map(|i| match i {
    //         Item::Use(u) => Some(u),
    //         _ => None,
    //     }) {
    //         if let Some(resolved) = self.resolve_use_tree(&u.tree, module_path.clone(), None, &type_name) {
    //             return Some(resolved);
    //         }
    //     }
    //     //TODO: Implement glob pattern imports resolution
    //     //
    //     // println!("{:?} is possibly imported with a glob pattern ({})", type_name, module_path);
    //     // for u in ast.items.iter().filter_map(|i| match i {
    //     //     Item::Use(u) => Some(u),
    //     //     _ => None,
    //     // }) {
    //     //     match &u.tree {
    //     //         UseTree::Glob(g) => {
    //     //             if let Some(resolved) = self.resolve_glob_use_tree(g, module_path.clone(), None, &type_name) {
    //     //                 return Some(resolved);
    //     //             }
    //     //         }
    //     //         _ => {}
    //     //     }
    //     // }
    //     Some((module_path, type_name))
    // }
    //
    // // TODO: Implement this
    // // fn resolve_glob_use_tree(&self, use_glob: &UseGlob, self_module_path: String, current_type_path: Option<String>, type_name: &String) -> Option<String> {
    // //     None
    // // }
    //
    // fn resolve_use_tree(
    //     &self,
    //     use_tree: &UseTree,
    //     self_module_path: String,
    //     current_type_path: Option<String>,
    //     type_name: &String,
    // ) -> Option<(String, String)> {
    //     match use_tree {
    //         UseTree::Name(n) => {
    //             let name = n.ident.to_string();
    //             if name == *type_name {
    //                 if let Some(cur) = current_type_path {
    //                     return Some((cur, name));
    //                 }
    //             }
    //         }
    //         UseTree::Rename(r) => {
    //             let name = r.ident.to_string();
    //             let rename = r.rename.to_string();
    //             if rename == *type_name {
    //                 if let Some(cur) = current_type_path {
    //                     return Some((format!("{}::{}", cur, name), name));
    //                 }
    //             }
    //         }
    //         UseTree::Group(g) => {
    //             for t in &g.items {
    //                 if let Some(resolved) = self.resolve_use_tree(t, self_module_path.clone(), current_type_path.clone(), type_name) {
    //                     return Some(resolved);
    //                 }
    //             }
    //         }
    //         UseTree::Path(u) => {
    //             let mut first_segment = u.ident.to_string();
    //             if first_segment.as_str() == "self" {
    //                 first_segment = self_module_path.clone();
    //             }
    //             let path = if let Some(cur) = current_type_path {
    //                 format!("{}::{}", cur, first_segment)
    //             } else if first_segment.as_str() == "crate" {
    //                 first_segment
    //             } else {
    //                 return None;
    //             };
    //             return self.resolve_use_tree(&u.tree, self_module_path, Some(path), type_name);
    //         }
    //         UseTree::Glob(_) => {}
    //     }
    //     None
    // }

    fn add_path(&mut self, info: HandlerInfo) -> CommandResult {
        let path = info.route.uri;
        let method = info.route.method;
        let description = if info.use_cookies {
            Some("NOTE: This request consume cookies.".to_string())
        } else {
            None
        };
        let mut data = OpenApiPath {
            parameters: info.parameters,
            description,
            operation_id: info.operation_id,
            ..Default::default()
        };

        if let Some(mut body_info) = info.body_info {
            if method == OpenApiPathMethod::Get {
                let parameters = self.get_open_api_parameters_from_body_info(&body_info);
                data.parameters.extend(parameters);
            } else {
                data.request_body = self.get_open_api_body_param(&body_info);
            }
        }

        if !self.doc.paths.contains_key(path.as_str()) {
            self.doc.paths.insert(path.clone(), BTreeMap::new());
        }
        let path_map = self.doc.paths.get_mut(path.as_str()).expect("Should work because of previous statement");

        if data.responses.is_empty() {
            data.responses.insert(
                200,
                OpenApiResponse {
                    description: "successful operation".to_string(),
                    content: Default::default(),
                },
            );
        }
        path_map.insert(method, data);
        Ok(())
    }

    // TODO: Handle re-exports (pub use)
    // TODO: Handle type alias
    fn get_open_api_body_param(&self, body_info: &BodyParamInfo) -> Option<OpenApiRequestBody> {
        if let Some(t) = self.get_open_api_type_from_type_info(&body_info.type_info) {
            let mut content: HashMap<OpenApiMimeTypes, OpenApiContent> = HashMap::new();
            content.insert(
                body_info.openapi_type.clone(),
                OpenApiContent {
                    schema: OpenApiSchema::Inline(t),
                },
            );
            return Some(OpenApiRequestBody {
                description: body_info.type_info.name.clone(),
                required: !body_info.type_info.is_optional,
                content,
            });
        }
        None
    }

    fn get_open_api_parameters_from_body_info(&self, body_info: &BodyParamInfo) -> Vec<OpenApiParameter> {
        let mut parameters = Vec::new();
        if let Some(t) = self.get_open_api_type_from_type_info(&body_info.type_info) {
            if let OpenApiType::Object { object: OpenApiObjectType::Object { properties, required } } = t {
                for (name, openapi_type) in &properties {
                    parameters.push(OpenApiParameter {
                        name: name.clone(),
                        location: OpenApiParameterLocation::Query,
                        required: required.contains(name),
                        schema: OpenApiSchema::Inline(openapi_type.as_ref().clone()),
                        ..Default::default()
                    });
                }
            }
        }
        parameters
    }

    // TODO: Support HashMap
    fn get_open_api_type_from_type_info(&self, type_info: &TypeInfo) -> Option<OpenApiType> {
        // let ast = self.loaded_files_ast.get(type_info.use_path.as_str())?;
        // for item in &ast.items {
        //     match item {
        //         Item::Struct(s) => {
        //             let name = s.ident.to_string();
        //             if name == type_info.use_name {
        //                 if self.find_macro_attribute_flag(&s.attrs, "derive", "Deserialize") {
        //                     if let Some(s) = self.get_open_api_type_from_struct(type_info, &s) {
        //                         return Some(s);
        //                     }
        //                 }
        //             }
        //         }
        //         Item::Enum(e) => {
        //             let name = e.ident.to_string();
        //             if name == type_info.use_name {
        //                 if self.find_macro_attribute_flag(&e.attrs, "derive", "Deserialize") {
        //                     if let Some(s) = self.get_open_api_type_from_enum(type_info, &e) {
        //                         return Some(s);
        //                     }
        //                 }
        //             }
        //         }
        //         _ => {}
        //     }
        // }
        None
    }

    fn get_serde_field(&self, mut field_name: String, field_attributes: &Vec<Attribute>, container_attributes: &Vec<Attribute>) -> Option<String> {
        if self.find_macro_attribute_flag(field_attributes, "serde", "skip") || self.find_macro_attribute_flag(field_attributes, "serde", "skip_serializing") {
            return None;
        }
        if let Some(Lit::Str(rename)) = self.find_macro_attribute_named_value(field_attributes, "serde", "rename") {
            field_name = rename.value();
        } else if let Some(Lit::Str(rename)) = self.find_macro_attribute_named_value(container_attributes, "serde", "rename_all") {
            if let Ok(case) = Case::try_from(rename.value().as_str()) {
                field_name = field_name.to_case(case);
            }
        }
        Some(field_name)
    }

    fn find_macro_attribute_flag(&self, attrs: &Vec<Attribute>, macro_name: &str, value_name: &str) -> bool {
        for attr in attrs
            .iter()
            .filter(|a| a.path.get_ident().filter(|i| i.to_string().as_str() == macro_name).is_some())
        {
            if let Some(meta) = attr.parse_meta().ok() {
                if self.find_macro_attribute_flag_from_meta(&meta, value_name) {
                    return true;
                }
            }
        }
        false
    }
    fn find_macro_attribute_flag_from_meta(&self, meta: &Meta, value_name: &str) -> bool {
        match meta {
            Meta::List(l) => {
                for n in &l.nested {
                    match n {
                        NestedMeta::Meta(nm) => {
                            if self.find_macro_attribute_flag_from_meta(&nm, value_name) {
                                return true;
                            }
                        }
                        NestedMeta::Lit(l) => {
                            println!(" Litteral meta : {:?}", l);
                        }
                    }
                }
            }
            Meta::Path(p) => {
                if p.get_ident().map(|i| i.to_string()).filter(|s| s == value_name).is_some() {
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn find_macro_attribute_named_value(&self, attrs: &Vec<Attribute>, macro_name: &str, value_name: &str) -> Option<Lit> {
        for attr in attrs
            .iter()
            .filter(|a| a.path.get_ident().filter(|i| i.to_string().as_str() == macro_name).is_some())
        {
            if let Some(meta) = attr.parse_meta().ok() {
                if let Some(s) = self.find_macro_attribute_value_from_meta(&meta, value_name) {
                    return Some(s);
                }
            }
        }
        None
    }
    fn find_macro_attribute_value_from_meta(&self, meta: &Meta, value_name: &str) -> Option<Lit> {
        match meta {
            Meta::List(l) => {
                for n in &l.nested {
                    match n {
                        NestedMeta::Meta(nm) => {
                            if let Some(s) = self.find_macro_attribute_value_from_meta(&nm, value_name) {
                                return Some(s);
                            }
                        }
                        NestedMeta::Lit(l) => {
                            println!(" Litteral meta : {:?}", l);
                        }
                    }
                }
            }
            Meta::NameValue(nv) => {
                if nv.path.get_ident().map(|i| i.to_string()).filter(|s| s == value_name).is_some() {
                    return Some(nv.lit.clone());
                }
            }
            _ => {}
        }
        None
    }

    fn get_open_api_type_from_struct(&mut self, type_info: &TypeInfo, s: &ItemStruct) -> Option<OpenApiType> {
        let mut properties = HashMap::new();
        let mut required = Vec::new();
        for field in &s.fields {
            if let Some(field_name) = field
                .ident
                .as_ref()
                .map(|i| self.get_serde_field(i.to_string(), &field.attrs, &s.attrs))
                .flatten()
            {
                if let Some(mut field_type_info) = self.find_type_info(
                    type_info.file,
                    &field.ty,
                ) {
                    let field_type = self
                        .get_open_api_type_from_type_info(&field_type_info)
                        .unwrap_or_else(|| OpenApiType::from_rust_type_str(field_type_info.name.as_str()));
                    if !field_type_info.is_optional
                        && !self.find_macro_attribute_flag(&field.attrs, "serde", "default")
                        && self.find_macro_attribute_named_value(&field.attrs, "serde", "default").is_none()
                    {
                        required.push(field_name.clone());
                    }
                    properties.insert(field_name, Box::new(field_type));
                } else {
                    println!("Unsupported type : {:?}", &field.ty);
                }
            }
        }
        if !properties.is_empty() {
            Some(OpenApiType::object(properties, required))
        } else {
            Some(OpenApiType::anonymous_object())
        }
    }

    fn get_open_api_type_from_enum(&self, _type_info: &TypeInfo, e: &ItemEnum) -> Option<OpenApiType> {
        if e.variants.iter().all(|v| v.fields == Fields::Unit) {
            let mut values: Vec<String> = Vec::new();
            for variant in &e.variants {
                if let Some(name) = self.get_serde_field(variant.ident.to_string(), &variant.attrs, &e.attrs) {
                    values.push(name);
                }
            }
            return Some(OpenApiType::enums(values));
        }

        // TODO: properly support tuple and struct enum variants.
        //       this will require the &TypeInfo ref
        Some(OpenApiType::anonymous_object())
    }

    fn handler_operation_id_from_sig(&self, sig: &Signature) -> String {
        let method_name = sig.ident.to_string();
        let mut operation_id = method_name.clone();
        let mut i = 2;
        let mut operation_ids = self.operation_ids.borrow_mut();
        while operation_ids.contains(operation_id.as_str()) {
            operation_id = format!("{}_{}", &method_name, &i);
            i += 1;
        }
        operation_ids.insert(operation_id.clone());
        operation_id
    }

    fn handler_method_from_attr(&self, attr: &Attribute) -> Option<OpenApiPathMethod> {
        let ident = attr.path.get_ident()?;
        OpenApiPathMethod::from_str(ident.to_string().as_str())
    }

    fn handler_path_from_attr(&self, attr: &Attribute) -> Option<(String, Vec<String>)> {
        if let Ok(Meta::List(meta)) = attr.parse_meta() {
            if let Some(NestedMeta::Lit(Lit::Str(l))) = meta.nested.first() {
                let mut chars: Vec<char> = l.value().chars().collect();
                let mut params: Vec<String> = Vec::new();

                let mut i = 0;
                while i < chars.len() {
                    if chars[i] == '<' || chars[i] == '{' {
                        chars[i] = '{';
                        for j in i..chars.len() {
                            if chars[j] == '>' || chars[j] == '}' {
                                chars[j] = '}';
                                params.push((&chars[(i + 1)..j]).iter().collect());
                                i = j;
                                break;
                            }
                        }
                    }
                    i += 1;
                }

                return Some((chars.into_iter().collect(), params));
            }
        }
        None
    }

    fn handler_has_cookies(&self, m: &ImplItemMethod) -> bool {
        for attr in &m.attrs {
            if let Some(i) = attr.path.get_ident() {
                if i.to_string().as_str() == "cookies" {
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Clone, Debug, Default)]
struct ControllerInfo {
    controller_name: String,
    name: String,
    version: Option<String>,
    prefix: Option<String>,
}

impl ControllerInfo {
    pub fn base_path(&self) -> String {
        let mut path = self.name.clone();
        if let Some(ver) = &self.version {
            path = format!("v{}/{}", ver, path);
        }
        if let Some(prefix) = &self.prefix {
            path = format!("{}/{}", prefix, path);
        }
        path
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HandlerInfo {
    module_path: String,
    controller: ControllerInfo,
    route: RouteInfo,
    parameters: Vec<OpenApiParameter>,
    operation_id: String,
    use_cookies: bool,
    body_info: Option<BodyParamInfo>,
}

#[derive(Clone, Debug)]
pub(crate) struct BodyParamInfo {
    openapi_type: OpenApiMimeTypes,
    type_info: TypeInfo,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RouteParametersInfo {
    parameters: Vec<OpenApiParameter>,
    has_cookies_param: bool,
    body_info: Option<BodyParamInfo>,
}