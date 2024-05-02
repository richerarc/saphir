use std::{
    fmt::{Debug, Display, Formatter},
    path::PathBuf,
};

mod browser;
mod file;
mod item;
mod module;
mod package;
mod target;

pub use self::{
    browser::Browser,
    file::File,
    item::{Impl, ImplItemKind, Item, ItemKind, Method},
    module::Module,
    package::Package,
    target::Target,
};

#[derive(Debug)]
pub enum Error {
    CargoToml(Box<cargo_metadata::Error>),
    FileIo(Box<PathBuf>, Box<std::io::Error>),
    FileParse(Box<PathBuf>, Box<syn::Error>),
}

impl From<cargo_metadata::Error> for Error {
    fn from(e: cargo_metadata::Error) -> Self {
        Error::CargoToml(Box::new(e))
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
            Error::CargoToml(_) => write!(f, "Unable to properly read the crate's metadata from the Cargo.toml manifest."),
            Error::FileIo(s, e) => write!(f, "unable to read `{}` : {}", s.to_str().unwrap_or_default(), e),
            Error::FileParse(s, e) => write!(f, "unable to parse `{}` : {}", s.to_str().unwrap_or_default(), e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::CargoToml(e) => Some(e),
            Error::FileIo(_, e) => Some(e),
            Error::FileParse(_, e) => Some(e),
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
