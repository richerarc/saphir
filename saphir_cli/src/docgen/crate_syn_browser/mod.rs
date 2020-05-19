use std::cell::{RefCell, Ref};
use crate::docgen::rust_module::AstFile;
use std::collections::HashMap;
use std::path::PathBuf;
use Error::*;
use syn::export::fmt::Display;
use syn::export::Formatter;
use cargo_metadata::{Package as MetaPackage, Target as MetaTarget, PackageId};
use syn::{File as SynFile, Item as SynItem, ItemMod as SynMod, ItemUse, UseTree};
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

// impl From<std::io::Error> for Error {
//     fn from(e: std::io::Error) -> Self {
//         FileError(Box::new(e))
//     }
// }

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

fn find_in_ref_vec<T, F>(r: Ref<Vec<T>>, criteria: F) -> Option<Ref<T>>
where F: Fn(&T) -> bool {
    if r.is_empty() { return None; }
    let p = Ref::map(r, |r| {
        for p in r {
            if criteria(p) {
                return p;
            }
        }
        return &r[0];
    });
    if criteria(&p) {
        Some(p)
    } else {
        None
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

    pub fn package_by_name(&'b self, name: &str) -> Option<&'b Package> {
        self.packages().iter().find(|p| p.package.name.as_str() == name)
    }

    pub fn packages(&'b self) -> &'b Vec<Package> {
        if !self.packages.filled() {
            let members: Vec<Package> = self.crate_metadata.workspace_members
                .iter()
                .map(|id| Package::new(self, id).expect("Should exist since we provided a proper PackageId"))
                .collect();
            self.packages.fill(members);
        }

        self.packages.borrow().expect("Should have been initialized by the previous statement")
    }
}

#[derive(Debug)]
pub struct Package<'b> {
    browser: &'b Browser<'b>,
    package: &'b MetaPackage,
    targets: LazyCell<Vec<Target<'b>>>,
}

impl<'b> Package<'b> {
    pub fn new(
        browser: &'b Browser<'b>,
        id: &'b PackageId,
    ) -> Option<Self> {
        let package = browser.crate_metadata.packages.iter().find(|p| p.id == *id)?;

        Some(Self {
            browser,
            package,
            targets: LazyCell::new(),
        })
    }

    fn targets(&'b self) -> &'b Vec<Target> {
        if !self.targets.filled() {
            let targets = self.package.targets
                .iter()
                .map(|t| Target::new(self, t))
                .collect();
            self.targets.fill(targets);
        }
        self.targets.borrow().expect("Should have been initialized by the previous statement")
    }

    pub fn bin_target(&'b self) -> Option<&'b Target> {
        self.targets().iter().find(|t| t.target.kind.contains(&"bin".to_string()))
    }
}

#[derive(Debug)]
pub struct Target<'b> {
    member: &'b Package<'b>,
    target: &'b MetaTarget,
    entrypoint: LazyCell<File<'b>>,
    files: RefCell<HashMap<String, &'b File<'b>>>,
}

impl<'b> Target<'b> {
    pub fn new(
        member: &'b Package<'b>,
        target: &'b MetaTarget,
    ) -> Self {
        Self {
            member,
            target,
            entrypoint: LazyCell::new(),
            files: RefCell::default(),
        }
    }

    pub fn entrypoint(&'b self) -> Result<&'b File, Error> {
        if !self.entrypoint.filled() {
            let file = File::new(self, &self.target.src_path, "crate".to_string() )?;
            self.entrypoint.fill(file);
            let file = self.entrypoint.borrow().unwrap();
            self.files.borrow_mut().insert(file.use_path.clone(), file);
        }
        Ok(self.entrypoint.borrow().expect("Should have been initialized by the previous statement"))
    }

    pub fn file_by_use_path(&'b self, path: &str) -> Result<Option<&'b File>, Error> {
        //TODO: This work, uncomment once file resolution below is successfully implemented
        if let Some(file) = self.files.borrow().get(path) {
            return Ok(Some(file));
        }
        if let Some(root) = path.split("::").next() {
            if root == "crate" {
            } else {
                // TODO: Implement resolution of dependancy files
                println!("Dependancies files resolution is not implemented yet");
            }
        }
        Ok(None)
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
            use_path: use_path.clone(),
            items: LazyCell::new(),
            modules: LazyCell::new(),
            dir: path.parent().expect("Valid file path should have valid parent folder").to_path_buf()
        };

        Ok(file)
    }

    pub fn items(&'b self) -> &'b Vec<Item<'b>> {
        if !self.items.filled() {
            let items = self.file.items
                .iter()
                .map(|i| Item::new(self, i))
                .collect();
            self.items.fill(items);
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
            self.modules.fill(modules);
            for m in self.modules.borrow().unwrap() {
                match &m {
                    Module::External { file, .. } => { self.target.files.borrow_mut().insert(file.use_path.clone(), &file); },
                    _ => { },
                }
            }
        }
        Ok(self.modules.borrow().expect("Should have been initialized by the previous statement"))
    }

    /// Return all syn::File items in this file, including inline modules.
    pub fn all_file_items(&'b self) -> Result<Vec<&'b Item<'b>>, Error> {
        let mut vec = Vec::new();
        vec.extend(self.items());
        for m in self.modules()? {
            match m {
                Module::Inline { items, .. } => vec.extend(items),
                _ => {}
            }
        }
        Ok(vec)
    }

    /// Return all syn::Item nested within this file, including included modules.
    pub fn all_items(&'b self) -> Result<Vec<&'b Item<'b>>, Error> {
        let mut vec = Vec::new();
        vec.extend(self.items());
        for m in self.modules()? {
            vec.extend(m.all_items()?);
        }
        Ok(vec)
    }

    pub fn find_impl(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error> {
        if let Some(item) = self.find_impl_inline(name)? {
            return Ok(Some(item));
        }

        for u in self.items().iter().filter_map(|i| match i.item {
            SynItem::Use(u) => Some(u),
            _ => None
        }) {
            for (path, name, alias) in self.expand_use_tree(&u.tree, None) {
                if alias.as_str() == name {
                    println!("({}) {}", alias, path);
                    if let Some(file) = self.target.file_by_use_path(path.as_str())? {
                        return file.find_impl_inline(name.as_str());
                    }
                }
            }
        }
        Ok(None)
    }

    fn find_impl_inline(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error> {
        for (item, item_name) in self.all_file_items()?.iter().filter_map(|i| match &i.item {
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

    fn expand_use_tree(&'b self, u: &'b UseTree, prefix: Option<String>) -> Vec<(String, String, String)> {
        match u {
            UseTree::Name(n) => {
                let name = n.ident.to_string();
                let path = prefix.unwrap_or_else(|| self.use_path.clone());
                vec![(path, name.clone(), name)]
            },
            UseTree::Rename(n) => {
                let name = n.ident.to_string();
                let rename = n.rename.to_string();
                let path = prefix.unwrap_or_else(|| self.use_path.clone());
                vec![(path, name, rename)]
            },
            UseTree::Group(g) => {
                let mut vec = Vec::new();
                for u in &g.items {
                    vec.extend(self.expand_use_tree(u, prefix.clone()))
                }
                vec
            },
            UseTree::Path(p) => {
                let path_segment = p.ident.to_string();
                let prefix = if prefix.is_none() {
                    if path_segment.as_str() == "self" {
                        self.use_path.clone()
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
                self.expand_use_tree(p.tree.as_ref(), Some(prefix))
            }
            UseTree::Glob(g) => {
                // TODO: Implement glob pattern resolution
                vec![]
            }
        }
    }
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

    pub fn items(&'b self) -> &'b Vec<Item<'b>> {
        match self {
            Module::Inline { items, .. } => items,
            Module::External { file, .. } => file.items(),
        }
    }

    pub fn all_items(&'b self) -> Result<Vec<&'b Item<'b>>, Error> {
        match self {
            Module::Inline { items, .. } => Ok(items.iter().collect()),
            Module::External { file, .. } => file.all_items(),
        }
    }
}