use crate::openapi::{OpenApi, OpenApiParameter, OpenApiPath, OpenApiPathMethod, OpenApiResponse, OpenApiSchema, OpenApiType};
use crate::{Command, CommandResult};
use futures::future::{BoxFuture, FutureExt};
use serde_derive::Deserialize;
use std::collections::BTreeMap;
use std::fs::File as SyncFile;
use std::path::PathBuf;
use structopt::StructOpt;
use syn::{Attribute, ImplItem, Item, ItemImpl, Lit, Meta, NestedMeta, Type};
use tokio::fs::File;
use tokio::prelude::*;

/// Generate OpenAPI v3 from a Saphir application.
///
/// See: https://github.com/OAI/OpenAPI-Specification/blob/master/versions/3.0.2.md
#[derive(StructOpt, Debug)]
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

    /// (Optional) Resulting output path. Either the path to the resulting yaml file,
    ///            or a dir, which would then contain a openapi.yaml
    #[structopt(parse(from_os_str), default_value = ".")]
    output_file: PathBuf,
}

pub(crate) struct DocGen {
    pub args: <DocGen as Command>::Args,
    pub doc: OpenApi,
}

impl Command for DocGen {
    type Args = DocGenArgs;

    fn new(args: Self::Args) -> Self {
        let mut doc = OpenApi::default();
        doc.openapi_version = "3.0.1".to_string();
        Self { args, doc }
    }

    fn run<'a>(mut self) -> BoxFuture<'a, CommandResult> {
        async move {
            let cargo_path = self.args.project_path.clone().join("Cargo.toml");
            let main_path = self.args.project_path.clone().join("src/main.rs");
            self.read_cargo_toml(cargo_path).await?;
            self.read_main_file(main_path).await?;
            self.write_doc_file().await?;
            Ok(())
        }
        .boxed()
    }
}

impl DocGen {
    async fn write_doc_file(&self) -> CommandResult {
        let mut path = self.args.output_file.clone();
        if path.is_dir() {
            path = path.join("openapi.yaml");
        }
        match path.extension() {
            None => path = path.with_extension("yaml"),
            Some(ext) => {
                if ext.to_str() != Some("yaml") {
                    return Err("output must be a yaml file.".to_string());
                }
            }
        }
        let f = SyncFile::create(&path).map_err(|_| format!("Unable to create file `{:?}`", &path))?;
        serde_yaml::to_writer(f, &self.doc).map_err(|_| format!("Unable to write to `{:?}`", path))?;
        println!("Succesfully created {:?}", path);
        Ok(())
    }

    async fn read_cargo_toml(&mut self, path: PathBuf) -> CommandResult {
        #[derive(Deserialize)]
        struct Cargo {
            pub package: Package,
        }
        #[derive(Deserialize)]
        struct Package {
            pub name: String,
            pub version: String,
        }
        let mut f = File::open(&path).await.map_err(|_| format!("Unable to read Cargo.toml"))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).await.map_err(|_| format!("Unable to read Cargo.toml"))?;
        let cargo: Cargo = toml::from_str(buffer.as_str()).map_err(|_| format!("Unable to read Cargo.toml"))?;

        self.doc.info.title = cargo.package.name;
        self.doc.info.version = cargo.package.version;

        Ok(())
    }

    async fn read_main_file(&mut self, path: PathBuf) -> CommandResult {
        let mut f = File::open(&path)
            .await
            .map_err(|_| format!("Unable to read the main project file `{:?}`", &path))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer)
            .await
            .map_err(|_| format!("Unable to read the main project file `{:?}`", &path))?;
        let path = path.to_str().ok_or(format!("Invalid path : `{:?}`", &path))?.to_string();
        self.parse_ast(path, buffer).await
    }

    fn read_mod_file(&mut self, dir: String, mod_name: String) -> BoxFuture<CommandResult> {
        async move {
            let dir = std::path::Path::new(dir.as_str());
            let mut path = dir.join(mod_name.as_str());
            if path.is_dir() {
                path = path.join("mod.rs");
            } else {
                path = dir.join(format!("{}.rs", mod_name).as_str());
            }
            let path_str = path.to_str().ok_or(format!("Invalid path : `{:?}`", path))?.to_string();
            let mut f = File::open(path).await.map_err(|_| format!("Unable to read module `{}`", mod_name))?;
            let mut buffer = String::new();
            f.read_to_string(&mut buffer)
                .await
                .map_err(|_| format!("Unable to read module `{}`", mod_name))?;

            self.parse_ast(path_str, buffer).await
        }
        .boxed()
    }

    fn parse_ast(&mut self, file: String, buffer: String) -> BoxFuture<CommandResult> {
        async move {
            let mut modules: Vec<String> = Vec::new();
            {
                let ast = syn::parse_file(buffer.as_str()).map_err(|_| format!("Unable to parse the module file `{:?}`", file))?;
                for item in &ast.items {
                    match item {
                        Item::Mod(md) => {
                            let mod_name = md.ident.to_string();
                            if let Some((_, items)) = &md.content {
                                self.parse_controllers_ast(items)?;
                            } else {
                                modules.push(mod_name);
                            }
                        }
                        _ => {}
                    }
                }
                self.parse_controllers_ast(&ast.items)?;
            }

            let file = std::path::Path::new(file.as_str());
            let dir = file.parent().ok_or(format!("`{:?}` is not a path to a rust file", file)).unwrap();
            let dir = dir.to_str().unwrap();

            for module in modules {
                self.read_mod_file(dir.to_string(), module).await?;
            }

            Ok(())
        }
        .boxed()
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

    fn parse_controllers_ast(&mut self, items: &Vec<Item>) -> CommandResult {
        for im in items.iter().filter_map(|i| match i {
            Item::Impl(im) => Some(im),
            _ => None,
        }) {
            if let Some(controller) = self.get_controller_info(im) {
                self.parse_handlers_ast(controller, &im.items)?;
            }
        }
        Ok(())
    }

    fn parse_handlers_ast(&mut self, controller: ControllerInfo, items: &Vec<ImplItem>) -> CommandResult {
        for m in items.iter().filter_map(|i| match i {
            ImplItem::Method(m) => Some(m),
            _ => None,
        }) {
            for attr in &m.attrs {
                let method = match self.handler_method_from_attr(&attr) {
                    Some(m) => m,
                    None => continue,
                };

                let (path, uri_params) = match self.handler_path_from_attr(&attr) {
                    Some(p) => p,
                    None => continue,
                };
                let mut full_path = format!("/{}{}", controller.base_path(), path);
                if full_path.ends_with('/') {
                    full_path = (&full_path[0..(full_path.len() - 1)]).to_string();
                }

                if !full_path.starts_with(self.args.scope.as_str()) {
                    continue;
                }

                let mut parameters = Vec::new();
                for param in uri_params {
                    parameters.push(OpenApiParameter {
                        name: param,
                        required: true,
                        schema: OpenApiSchema {
                            openapi_type: OpenApiType::String,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                }

                self.add_path(
                    full_path,
                    method,
                    OpenApiPath {
                        parameters,
                        ..Default::default()
                    },
                )?;
            }
        }
        Ok(())
    }

    fn add_path(&mut self, path: String, method: OpenApiPathMethod, mut data: OpenApiPath) -> CommandResult {
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
}

#[derive(Debug, Default)]
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
