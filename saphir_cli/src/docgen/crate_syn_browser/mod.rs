use std::cell::{RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use Error::*;
use syn::export::fmt::Display;
use syn::export::Formatter;
use cargo_metadata::{Package as MetaPackage, Target as MetaTarget, PackageId};
use syn::{File as SynFile, Item as SynItem, ItemMod as SynMod, UseTree, Visibility};
use std::fs::File as FsFile;
use std::fmt::Debug;
use std::io::Read;
use lazycell::LazyCell;

#[derive(Debug)]
pub enum Error {
    CargoTomlError(Box<cargo_metadata::Error>),
    FileIoError(Box<PathBuf>, Box<std::io::Error>),
    FileParseError(Box<PathBuf>, Box<syn::Error>),
}

impl From<cargo_metadata::Error> for Error {
    fn from(e: cargo_metadata::Error) -> Self {
        CargoTomlError(Box::new(e))
    }
}

impl Into<String> for Error {
    fn into(self) -> String {
        format!("{}", self)
    }
}

// TODO: Pretty error messages
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CargoTomlError(_) => write!(f, "Unable to properly read the crate's metadata from the Cargo.toml manifest."),
            FileIoError(s, e) => write!(f, "unable to read `{}` : {}", s.to_str().unwrap_or_default(), e),
            FileParseError(s, e) => write!(f, "unable to parse `{}` : {}", s.to_str().unwrap_or_default(), e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CargoTomlError(e) => Some(e),
            FileIoError(_, e) => Some(e),
            FileParseError(_, e) => Some(e),
        }
    }
}

#[derive(Debug)]
pub struct Browser<'b> {
    crate_metadata: cargo_metadata::Metadata,
    packages: LazyCell<Vec<Package<'b>>>,
}

impl<'b> Browser<'b> {
    pub fn new(crate_path: PathBuf) -> Result<Self, Error> {
        let crate_metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(crate_path.join("Cargo.toml"))
            .exec()?;

        let browser = Self {
            crate_metadata,
            packages: LazyCell::new()
        };

        Ok(browser)
    }

    pub fn package_by_name(&self, name: &str) -> Option<&'b Package> {
        self.packages().iter().find(|p| p.meta.name.as_str() == name)
    }

    fn init_packages(&'b self) {
        if !self.packages.filled() {
            let members: Vec<Package> = self.crate_metadata.workspace_members
                .iter()
                .map(|id| Package::new(self, id).expect("Should exist since we provided a proper PackageId"))
                .collect();
            self.packages.fill(members).expect("We should never be filling this twice");
        }
    }

    pub fn packages(&'b self) -> &'b Vec<Package> {
        self.init_packages();
        self.packages.borrow().expect("Should have been initialized by the previous statement")
    }
}

#[derive(Debug)]
pub struct Package<'b> {
    name: String,
    browser: &'b Browser<'b>,
    meta: &'b MetaPackage,
    targets: LazyCell<Vec<Target<'b>>>,
    dependancies: RefCell<HashMap<String, Option<*const Package<'b>>>>,
    dependancies_to_free: RefCell<Vec<*mut Package<'b>>>,
}

impl Drop for Package<'_> {
    fn drop(&mut self) {
        for free_me in self.dependancies_to_free.borrow_mut().iter() {
            unsafe {
                let _ = Box::from_raw(*free_me);
            }
        }
    }
}

impl<'b> Package<'b> {
    pub fn new(
        browser: &'b Browser<'b>,
        id: &'b PackageId,
    ) -> Option<Self> {
        let package = browser.crate_metadata.packages.iter().find(|p| p.id == *id)?;
        let name = package.name.clone();

        Some(Self {
            browser,
            name,
            meta: package,
            targets: LazyCell::new(),
            dependancies: RefCell::new(HashMap::new()),
            dependancies_to_free: RefCell::new(Vec::new()),
        })
    }

    pub fn dependancy(&'b self, name: &str) -> Option<&'b Package> {
        if !self.dependancies.borrow().contains_key(name) {
            let package = self.meta.dependencies
                .iter()
                .find(|dep| dep.rename.as_ref().unwrap_or(&dep.name) == name)
                .map(|dep| {
                    self.browser.crate_metadata.packages
                        .iter()
                        .find(|package| package.name == dep.name && dep.req.matches(&package.version))
                        .map(|p| Package::new(self.browser, &p.id))
                        .flatten()
                })
                .flatten();
            let to_add = if let Some(package) = package {
                let raw_mut = Box::into_raw(Box::new(package));
                self.dependancies_to_free.borrow_mut().push(raw_mut);
                Some(raw_mut as *const Package)
            } else {
                None
            };
            self.dependancies.borrow_mut().insert(name.to_string(), to_add);
        }
        self.dependancies
            .borrow()
            .get(name)
            .map(|d| d.as_ref())
            .map(|d| d.map(|rc| rc.clone()))
            .flatten()
            .map(|b| {
                unsafe { &*b }
            })
    }

    fn targets(&'b self) -> &'b Vec<Target> {
        if !self.targets.filled() {
            let targets = self.meta.targets
                .iter()
                .map(|t| Target::new(self, t))
                .collect();
            self.targets.fill(targets).expect("We should never be filling this twice");
        }
        self.targets.borrow().expect("Should have been initialized by the previous statement")
    }

    pub fn bin_target(&'b self) -> Option<&'b Target> {
        self.targets().iter().find(|t| t.target.kind.contains(&"bin".to_string()))
    }

    pub fn lib_target(&'b self) -> Option<&'b Target> {
        self.targets().iter().find(|t| t.target.kind.contains(&"lib".to_string()))
    }
}

#[derive(Debug)]
pub struct Target<'b> {
    package: &'b Package<'b>,
    target: &'b MetaTarget,
    entrypoint_file: LazyCell<File<'b>>,
    entrypoint_module: LazyCell<Module<'b>>,
    modules: RefCell<HashMap<String, &'b Module<'b>>>,
}

impl<'b> Target<'b> {
    pub fn new(
        package: &'b Package<'b>,
        target: &'b MetaTarget,
    ) -> Self {
        Self {
            package,
            target,
            entrypoint_file: LazyCell::new(),
            entrypoint_module: LazyCell::new(),
            modules: RefCell::default(),
        }
    }

    #[inline]
    pub fn init_entrypoint_and_module(&'b self) -> Result<(), Error> {
        if !self.entrypoint_file.filled() {
            let file = File::new(self, &self.target.src_path, self.package.name.clone() )?;
            self.entrypoint_file.fill(file).expect("We should never be filling this twice");
        }
        if !self.entrypoint_module.filled() {
            let entrypoint = self.entrypoint_file.borrow().expect("Should have been initialized by the previous statement");
            let module = Module::Crate { file: &entrypoint };
            self.entrypoint_module.fill(module).expect("We should never be filling this twice");
            let module = self.entrypoint_module.borrow().unwrap();
            self.modules.borrow_mut().insert(entrypoint.use_path.clone(), module);
        }
        Ok(())
    }

    pub fn entrypoint(&'b self) -> Result<&'b File<'b>, Error> {
        self.init_entrypoint_and_module()?;
        Ok(self.entrypoint_file.borrow().expect("Should have been initialized by the previous statement"))
    }

    pub fn module(&'b self) -> Result<&'b Module<'b>, Error> {
        self.init_entrypoint_and_module()?;
        Ok(self.entrypoint_module.borrow().expect("Should have been initialized by the previous statement"))
    }

    pub fn module_by_use_path(&'b self, path: &str) -> Result<Option<&'b Module<'b>>, Error> {
        if let Some(module) = self.modules.borrow().get(path) {
            return Ok(Some(module));
        }
        let mut path_split = path.split("::");
        if let Some(mut root) = path_split.next() {
            if root == "crate" {
                root = self.package.name.as_str();
            }
            let module = if root == self.package.name.as_str() {
                let mut cur_module = self.module()?;
                for split in path_split {
                    if let Some(m) = cur_module.file().modules()?.iter().find(|m| m.name().as_str() == split) {
                        cur_module = m;
                    }
                }
                if cur_module.use_path() == path {
                    Some(cur_module)
                } else {
                    None
                }
            } else {
                let dep_package = match self.package.dependancy(root) {
                    Some(dep) => dep,
                    None => return Ok(None),
                };
                let lib = dep_package.lib_target();
                let lib = match lib {
                    Some(lib) => lib,
                    None => return Ok(None),
                };
                let mut remaining_path: Vec<&str> = vec![root];
                remaining_path.extend(path_split);
                let use_path = remaining_path.join("::");
                let module = lib.module_by_use_path(use_path.as_str())?;
                module
            };
            if let Some(module) = module {
                self.modules.borrow_mut().insert(path.to_string(), module);
            }
            Ok(module)
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
pub struct File<'b> {
    pub(crate) target: &'b Target<'b>,
    pub file: SynFile,
    pub use_path: String,
    items: LazyCell<Vec<Item<'b>>>,
    modules: LazyCell<Vec<Module<'b>>>,
    dir: PathBuf,
    uses: LazyCell<Vec<Use>>,
}

impl<'b> File<'b> {
    pub fn new(
        target: &'b Target<'b>,
        path: &PathBuf,
        use_path: String,
    ) -> Result<File<'b>, Error> {
        let mut f = FsFile::open(path).map_err(|e| FileIoError(Box::new(path.clone()), Box::new(e)))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).map_err(|e| FileIoError(Box::new(path.clone()), Box::new(e)))?;

        let file = syn::parse_file(buffer.as_str()).map_err(|e| FileParseError(Box::new(path.clone()), Box::new(e)))?;

        let file = Self {
            target,
            file,
            use_path,
            items: LazyCell::new(),
            modules: LazyCell::new(),
            dir: path.parent().expect("Valid file path should have valid parent folder").to_path_buf(),
            uses: LazyCell::new()
        };

        Ok(file)
    }

    pub fn items(&'b self) -> &'b Vec<Item<'b>> {
        if !self.items.filled() {
            let items = self.file.items
                .iter()
                .map(|i| Item::new(self, i))
                .collect();
            self.items.fill(items).expect("We should never be filling this twice");
        }
        self.items.borrow().expect("Should have been initialized by the previous statement")
    }

    pub fn modules(&'b self) -> Result<&'b Vec<Module>, Error> {
        if !self.modules.filled() {
            let modules = self.items().iter()
                .filter_map(|i| match i.item {
                    SynItem::Mod(m) => Some(m),
                    _ => None,
                })
                .map(|m| Module::new(self, m))
                .collect::<Result<Vec<_>, _>>()?;
            self.modules.fill(modules).expect("We should never be filling this twice");
            for m in self.modules.borrow().unwrap() {
                if let Module::External { file, .. } = &m {
                    self.target.modules.borrow_mut().insert(file.use_path.clone(), &m);
                }
            }
        }
        Ok(self.modules.borrow().expect("Should have been initialized by the previous statement"))
    }

    /// Return all syn::Item in this file, including inline modules.
    pub fn all_items(&'b self) -> Result<Vec<&'b Item<'b>>, Error> {
        let mut vec = Vec::new();
        vec.extend(self.items());
        for m in self.modules()? {
            vec.extend(m.all_items()?);
        }
        Ok(vec)
    }

    pub fn find_impl(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error> {
        if let Ok(Some(item)) = self.find_impl_inline(name) {
            return Ok(Some(item));
        }

        for u in self.uses() {
            if u.alias.as_str() == name {
                if let Some(module) = self.target.module_by_use_path(u.path.as_str())? {
                    return module.find_impl_inline(u.name.as_str());
                }
            }
        }

        //At this point, this is most likely a primitive.
        Ok(None)
    }

    fn find_impl_inline(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error> {
        for (item, item_name) in self.items().iter().filter_map(|i| match &i.item {
            SynItem::Enum(e) => Some((i, e.ident.to_string())),
            SynItem::Struct(s) => Some((i, s.ident.to_string())),
            _ => None
        }) {
            if item_name.as_str() == name {
                return Ok(Some(item));
            }
        }
        Ok(None)
    }

    fn uses(&'b self) -> &'b Vec<Use> {
        if !self.uses.filled() {
            let mut uses: Vec<Use> = Vec::new();
            for u in self.items().iter().filter_map(|i| match i.item {
                SynItem::Use(u) => Some(u),
                _ => None
            }) {
                let is_pub = match u.vis {
                    Visibility::Public(_) => true,
                    _ => false,
                };
                uses.extend(self.expand_use_tree(&u.tree, None, is_pub));
            }
            self.uses.fill(uses).expect("We should never be filling this twice");
        }
        self.uses.borrow().expect("Should have been initialized by the previous statement")
    }

    fn expand_use_tree(&'b self, u: &'b UseTree, prefix: Option<String>, is_pub: bool) -> Vec<Use> {
        match u {
            UseTree::Name(n) => {
                let name = n.ident.to_string();
                let path = prefix.unwrap_or_else(|| self.use_path.clone());
                vec![Use { path, alias: name.clone(), name, is_pub }]
            },
            UseTree::Rename(n) => {
                let name = n.ident.to_string();
                let alias = n.rename.to_string();
                let path = prefix.unwrap_or_else(|| self.use_path.clone());
                vec![Use { path, name, alias, is_pub }]
            },
            UseTree::Group(g) => {
                g.items
                    .iter()
                    .map(|u| self.expand_use_tree(u, prefix.clone(), is_pub))
                    .flatten()
                    .collect()
            },
            UseTree::Path(p) => {
                let path_segment = p.ident.to_string();
                let prefix = if prefix.is_none() {
                    if path_segment.as_str() == "self" {
                        self.use_path.clone()
                    } else if path_segment.as_str() == "crate" {
                        self.target.package.name.clone()
                    } else {
                        path_segment
                    }
                } else {
                    if let Some(p) = prefix {
                        format!("{}::{}", p, path_segment)
                    } else {
                        format!("{}::{}", self.use_path, path_segment)
                    }
                };
                self.expand_use_tree(p.tree.as_ref(), Some(prefix), is_pub)
            }
            UseTree::Glob(g) => {
                let path = prefix.expect("Glob pattern should have a path prefix");
                println!("glob (*) for {:?}", &path);
                if let Ok(Some(module)) = self.target.module_by_use_path(path.as_str()) {
                    println!("Found the module for our glob pattern : {:?}", module.name());
                    // let uses = module.uses().iter().cloned().collect();
                    vec![]
                } else {
                    // let split: Vec<&str> = path.split("::").collect();
                    // if !split.is_empty() {
                    //     let last = split.last().unwrap();
                    //     let path = &split[0..(split.len()-1)].join("::");
                    //     println!("Will look for {}::* in {:?}", last, path);
                    //     if let Ok(Some(module)) = self.target.module_by_use_path(path) {
                    //         if let Ok(Some(i)) = module.find_impl_inline(last) {
                    //             println!("Found impl {:?}", i.item);
                    //         }
                    //
                    //     }
                    // }
                    vec![]
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Use {
    pub path: String,
    pub name: String,
    pub alias: String,
    pub is_pub: bool,
}

#[derive(Debug)]
pub struct Item<'b> {
    pub file: &'b File<'b>,
    pub item: &'b SynItem,
}

impl<'b> Item<'b> {
    pub fn new(
        file: &'b File<'b>,
        item: &'b SynItem
    ) -> Self {
        Self {
            file,
            item
        }
    }
}

#[derive(Debug)]
pub enum Module<'b> {
    Crate {
        file: &'b File<'b>,
    },
    Inline {
        file: &'b File<'b>,
        module: &'b SynMod,
        items: Vec<Item<'b>>,
    },
    External {
        module: &'b SynMod,
        file: File<'b>,
    }
}

impl<'b> Module<'b> {
    pub fn new(
        file: &'b File<'b>,
        module: &'b SynMod,
    ) -> Result<Self, Error> {
        Ok(if module.content.is_some() {
            Module::Inline { file, module, items: module.content.as_ref().unwrap().1.iter().map(|i| Item::new(file, i)).collect() }
        } else {
            let name = module.ident.to_string();
            let use_path =  format!("{}::{}", file.use_path, &name);
            let mut path = file.dir.join(&name).join("mod.rs");
            if !path.exists() {
                path = file.dir.join(format!("{}.rs", name));
            }
            let module = Module::External {
                module,
                file: File::new(file.target, &path, use_path.clone())?
            };
            module
        })
    }

    pub fn name(&self) -> String {
        match self {
            Module::Crate { file, .. } => file.target.package.name.clone(),
            Module::Inline { module, .. } => module.ident.to_string(),
            Module::External { module, .. } => module.ident.to_string(),
        }
    }

    pub fn file(&self) -> &'b File {
        match self {
            Module::Crate { file, .. } => file,
            Module::Inline { file, .. } => file,
            Module::External { file, .. } => file,
        }
    }

    pub fn use_path(&'b self) -> String {
        format!("{}::{}", self.file().use_path.as_str(), self.name())
    }

    #[allow(dead_code)]
    pub fn items(&'b self) -> &'b Vec<Item<'b>> {
        match self {
            Module::Crate { file, .. } => file.items(),
            Module::Inline { items, .. } => items,
            Module::External { file, .. } => file.items(),
        }
    }

    pub fn all_items(&'b self) -> Result<Vec<&'b Item<'b>>, Error> {
        match self {
            Module::Crate { file, .. } => file.all_items(),
            Module::Inline { items, .. } => Ok(items.iter().collect()),
            Module::External { file, .. } => file.all_items(),
        }
    }

    pub fn find_impl_inline(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error> {
        for (item, item_name) in self.items().iter().filter_map(|i| match &i.item {
            SynItem::Enum(e) => Some((i, e.ident.to_string())),
            SynItem::Struct(s) => Some((i, s.ident.to_string())),
            _ => None
        }) {
            if item_name.as_str() == name {
                return Ok(Some(item));
            }
        }
        Ok(None)
    }

}