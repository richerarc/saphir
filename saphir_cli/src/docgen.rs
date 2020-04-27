use structopt::StructOpt;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::fs::{self, DirEntry, File};
use tokio::prelude::*;
use std::error::Error;
use crate::{CommandResult, Command};
use syn::{Item, ItemMod, Type, TypePath};
// use futures::{stream, Stream, StreamExt}; // 0.3.1
use futures::future::{BoxFuture, FutureExt};
use std::pin::Pin;
use futures::lock::Mutex;
use std::sync::Arc;
use tokio::task;
use std::path::PathBuf;
use crate::openapi::OpenApi;
use std::fs::File as SyncFile;
use serde_derive::Deserialize;

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
            println!("Gen doc for scope: `{}` for project at path: `{:?}`", self.args.scope, self.args.project_path);
            let cargo_path = self.args.project_path.clone().join("Cargo.toml");
            let main_path = self.args.project_path.clone().join("src/main.rs");
            self.read_cargo_toml(cargo_path).await?;
            self.read_main_file(main_path).await?;
            self.write_doc_file().await?;
            Ok(())
        }.boxed()
    }
}

impl DocGen {
    async fn write_doc_file(&self) -> CommandResult {
        let mut path = self.args.output_file.clone();
        if path.is_dir() {
            path = path.join("openapi.yaml");
        }
        match path.extension() {
            None => path = path.join(".yaml"),
            Some(ext) => {
                if (ext.to_str() != Some("yaml")) {
                    return Err("output must be a yaml file.".to_string());
                }
            }
        }
        let f = SyncFile::create(&path).map_err(|e| format!("Unable to create file `{:?}`", &path))?;
        serde_yaml::to_writer(f, &self.doc).map_err(|e| format!("Unable to write to `{:?}`", path));
        println!("Succesfully created `{:?}`", path);
        Ok(())
    }

    async fn read_cargo_toml(&mut self, path: PathBuf) -> CommandResult {
        #[derive(Deserialize)]
        struct Cargo {
            pub package: Package
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

    async fn read_main_file(&self, path: PathBuf) -> CommandResult {
        let mut f = File::open(&path).await.map_err(|_| format!("Unable to read the main project file `{:?}`", &path))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).await.map_err(|_| format!("Unable to read the main project file `{:?}`", &path))?;
        let path = path.to_str().ok_or(format!("Invalid path : `{:?}`", &path))?.to_string();
        self.parse_ast(path, buffer).await
    }

    fn read_mod_file(&self, dir: String, mod_name: String) -> BoxFuture<CommandResult> {
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
            f.read_to_string(&mut buffer).await.map_err(|_| format!("Unable to read module `{}`", mod_name))?;

            self.parse_ast(path_str, buffer).await
        }.boxed()
    }

    fn parse_ast(&self, file: String, buffer: String) -> BoxFuture<CommandResult> {
        async move {
            let mut modules: Vec<String> = Vec::new();
            {
                let ast = syn::parse_file(buffer.as_str()).map_err(|_| format!("Unable to parse the module file `{:?}`", file))?;
                for item in ast.items {
                    match item {
                        Item::Mod(md) => {
                            let mod_name = md.ident.to_string();
                            if let Some((brace, items)) = md.content {
                                println!("`{}` is a module block", &mod_name);
                            } else {
                                modules.push(mod_name);
                            }
                        }
                        Item::Impl(im) => {
                            for attr in im.attrs {
                                if let Some(first_seg) = attr.path.segments.first() {
                                    let t = im.self_ty.as_ref();
                                    match t {
                                        Type::Path(p) => {
                                            if let Some(struct_first_seg) = p.path.segments.first() {
                                                if first_seg.ident.eq("controller") {
                                                    let controller_name = struct_first_seg.ident.to_string();
                                                    println!("`{}` is a controller", controller_name);
                                                }
                                            }
                                        },
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            let file = std::path::Path::new(file.as_str());
            let dir = file.parent().ok_or(format!("`{:?}` is not a path to a rust file", file)).unwrap();
            let dir = dir.to_str().unwrap();

            for module in modules {
                self.read_mod_file(dir.to_string(), module).await?;
            }

            Ok(())
        }.boxed()
    }

    // async fn process_ast_items<'a>(&'a self, items: Vec<Item>) -> CommandResult {
    //     // async move {
    //         println!("Items : {:#?}", items);
    //         Ok(())
    //     // }.boxed()
    // }
}