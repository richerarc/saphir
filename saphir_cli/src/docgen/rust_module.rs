use std::cell::RefCell;
use std::collections::HashMap;
use std::pin::Pin;
use std::path::{PathBuf, Path};
use crate::CommandResult;
use std::fs::File;
use std::io::Read;
use syn::File as SynFile;
use crate::openapi::OpenApiSchema::Ref;

#[derive(Default, Debug, Clone)]
pub(crate) struct CargoDependancy {
    pub name: String,
    pub version: String,
    pub manifest_path: PathBuf,
    pub lib_path: PathBuf
}

#[derive(Default, Debug, Clone)]
pub(crate) struct CargoTarget {
    pub target_type: String,
    pub path: PathBuf,
}

pub(crate) struct AstFile {
    pub ast: &'static SynFile,
    pub path: PathBuf,
    pub module_path: String,
}

#[derive(Default)]
pub(crate) struct RustModule {
    pub name: String,
    // pub packages: &'static [cargo_metadata::Package],
    // pub package: &'static cargo_metadata::Package,
    // pub root_path: PathBuf,
    pub loaded_files_ast: RefCell<HashMap<String, Option<AstFile>>>,
    loaded_files_to_free: RefCell<Vec<*mut SynFile>>,
}

impl RustModule {
    pub fn new(
        name: String,
        // packages: &'static [cargo_metadata::Package],
        // package: &'static cargo_metadata::Package,
    ) -> Result<Self, String> {
        // println!("{:?}", package.targets);
        Ok(Self {
            name,
            // packages,
            // package,
            loaded_files_ast: RefCell::new(HashMap::new()),
            loaded_files_to_free: RefCell::new(Vec::new())
        })
    }
    // pub fn new_from_root_path(
    //     name: String,
    //     root_path: PathBuf,
    //     entrypoint: PathBuf,
    // ) -> Result<Self, String> {
    //     Self::new(
    //         name,
    //         root_path.clone(),
    //         root_path.join("Cargo.toml"),
    //         entrypoint
    //     )
    // }
    //
    // fn new(
    //     name: String,
    //     root_path: PathBuf,
    //     manifest_path: PathBuf,
    //     entrypoint: PathBuf,
    // ) -> Result<Self, String> {
    //     let mut module = RustModule {
    //         name,
    //         root_path,
    //         manifest_path,
    //         ..Default::default()
    //     };
    //
    //     // module.load_cargo_dependancies();
    //
    //     Ok(module)
    // }
    //
    // fn load_ast(&mut self, module_path: String, path: PathBuf) -> CommandResult {
    //     let mut f = File::open(&path).map_err(|_| format!("Unable to read file `{:?}`", path))?;
    //     let mut buffer = String::new();
    //     f.read_to_string(&mut buffer)
    //         .map_err(|_| format!("Unable to read file `{:?}`", path))?;
    //
    //     let ast = syn::parse_file(buffer.as_str()).map_err(|_| format!("Unable to parse the module file `{:?}`", path))?;
    //
    //     let leaked: &'static mut SynFile = Box::leak(Box::new(ast));
    //     self.loaded_files_to_free.borrow_mut().push(leaked as *mut SynFile);
    //     self.loaded_files_ast.borrow_mut().insert(module_path.clone(), Some(AstFile {
    //         ast: leaked,
    //         path,
    //         module_path
    //     }));
    //     Ok(())
    // }



    //
    // fn get_dependancy(&mut self, name: &str) -> Result<Option<&CargoDependancy>, String> {
    //     if self.dependancies.is_none() {
    //         self.load_cargo_dependancies()?;
    //     }
    //     let dependancies = self.dependancies.as_ref().expect("loaded by `load_cargo_dependancies`");
    //     Ok(dependancies.get(name))
    // }

    // fn load_cargo_dependancies(&mut self) -> CommandResult {
    //     let mut dependancies = HashMap::new();
    //     let metadata = cargo_metadata::MetadataCommand::new()
    //         .manifest_path(self.manifest_path.clone())
    //         .exec().map_err(|e| e.to_string())?;
    //     let mut dep = Vec::new();
    //     let resolve = metadata.resolve.as_ref().ok_or_else(|| format!("Unable to properly read Cargo.toml of `{}`", self.name))?;
    //     // for node in &resolve.nodes {
    //     //     println!("node : {:?}", node);
    //     // }
    //     println!("members : {:?}", metadata.workspace_members);
    //     let main_package = metadata.packages.iter().find(|p| p.name == "lucid");
    //     println!("main package : {:?}", main_package);
    //     // println!("packages : {:?}", metadata.packages);
    //     for package in metadata.packages {
    //         // println!("dependancy : {:?}", &package);
    //         if let Some(lib_target) = &package.targets.iter().find(|t| t.kind.contains(&"lib".to_string())) {
    //             let lib_path = lib_target.src_path.clone();
    //             let name = package.name.replace("-", "_");
    //             let dependancy = CargoDependancy {
    //                 name: name.clone(),
    //                 version: package.version.to_string(),
    //                 lib_path,
    //                 manifest_path: package.manifest_path,
    //             };
    //             // if dep.contains(&name) {
    //             // }
    //             dep.push(name.clone());
    //             dependancies.insert(name, dependancy);
    //         }
    //     }
    //     self.dependancies = Some(dependancies);
    //     println!("Loaded dependancies of `{}`", self.name);
    //     Ok(())
    // }
}