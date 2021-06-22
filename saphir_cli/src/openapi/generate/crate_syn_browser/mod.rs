use std::{
    fmt::{Debug, Display, Formatter},
    path::PathBuf,
};
use Error::*;

mod browser;
mod file;
mod item;
mod module;
mod package;
mod target;

pub use self::{
    browser::Browser,
    file::File,
    item::{Enum, Impl, ImplItem, ImplItemKind, Item, ItemKind, Method, Struct, Use},
    module::{CrateModule, FileModule, InlineModule, Module, ModuleKind},
    package::Package,
    target::Target,
};

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

impl From<Error> for String {
    fn from(e: Error) -> String {
        format!("{}", e)
    }
}

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

pub trait UseScope<'b> {
    fn path(&'b self) -> &'b str;
    fn target(&'b self) -> &'b Target<'b>;
    fn uses(&'b self) -> &'b Vec<ExpandedUse>;
    fn pub_uses(&'b self) -> Vec<&'b ExpandedUse>;
    fn find_type_definition(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error>;
    fn find_type_definition_inline(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error>;
    fn expand_path(&'b self, path: &str) -> String {
        let mut splitted: Vec<&str> = path.split("::").collect();
        let first = match splitted.first() {
            Some(f) => *f,
            _ => return "".to_string(),
        };
        if splitted.len() == 1 {
            return first.to_string();
        }
        let first = match first {
            "self" => self.path().to_string(),
            "crate" => self.target().package.name.clone(),
            "super" => {
                let split: Vec<&str> = self.path().split("::").collect();
                split[..(split.len() - 1)].join("::")
            }
            _ => first.to_string(),
        };

        splitted[0] = first.as_str();
        splitted.join("::")
    }
}

#[derive(Debug, Clone)]
pub struct ExpandedUse {
    pub path: String,
    pub name: String,
    pub alias: String,
    pub is_pub: bool,
}
