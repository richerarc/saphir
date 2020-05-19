use std::cell::{RefCell, Ref};
use crate::docgen::rust_module::AstFile;
use std::collections::HashMap;
use std::path::PathBuf;
use Error::*;
use syn::export::fmt::Display;
use syn::export::Formatter;
use cargo_metadata::{Package as MetaPackage, Target as MetaTarget, PackageId};
use syn::{File as SynFile};
use std::fs::File as FsFile;
use std::fmt::Debug;
use std::io::Read;

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
pub struct Browser {
    crate_metadata: cargo_metadata::Metadata,
    packages: RefCell<Option<Vec<Package>>>,
}

impl Browser {
    pub fn new(crate_path: PathBuf) -> Result<Self, Error> {
        let crate_metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(crate_path.join("Cargo.toml"))
            .exec()?;

        let browser = Self {
            crate_metadata,
            packages: RefCell::new(None),
        };

        Ok(browser)
    }

    pub fn package_by_name(&self, name: &str) -> Option<Ref<Package>> {
        find_in_ref_vec(self.packages(), |p| unsafe { (*p.package).name.as_str() } == name)
    }

    pub fn first_package(&self) -> Option<Ref<Package>> {
        let packages = self.packages();
        if packages.is_empty() { return None; }
        Some(Ref::map(packages, |p| &p[0]))
    }

    pub fn packages(&self) -> Ref<Vec<Package>> {
        if self.packages.borrow().is_none() {
            let members: Vec<Package> = self.crate_metadata.workspace_members
                .iter()
                .map(|id| Package::new(self, id).expect("Should exist since we provided a proper PackageId"))
                .collect();
            *self.packages.borrow_mut() = Some(members);
        }

        return Ref::map(self.packages.borrow(), |m| m.as_ref().unwrap());
    }
}

#[derive(Debug)]
pub struct Package {
    browser: *const Browser,
    package: *const MetaPackage,
    targets: RefCell<Option<Vec<Target>>>,
}

impl Package {
    pub fn new(
        browser: &Browser,
        id: &PackageId,
    ) -> Option<Self> {
        let package = browser.crate_metadata.packages.iter().find(|p| p.id == *id)?;

        Some(Self {
            browser: browser as *const Browser,
            package: package as *const MetaPackage,
            targets: RefCell::new(None),
        })
    }

    fn targets(&self) -> Ref<Vec<Target>> {
        if self.targets.borrow().is_none() {
            let targets = unsafe { &(*self.package).targets }
                .iter()
                .map(|t| Target::new(self, t))
                .collect();
            *self.targets.borrow_mut() = Some(targets);
        }

        return Ref::map(self.targets.borrow(), |m| m.as_ref().unwrap());
    }

    pub fn bin_target(&self) -> Option<Ref<Target>> {
        find_in_ref_vec(self.targets(), |t| {
            let t = unsafe { &*t.target };
            t.kind.contains(&"bin".to_string())
        })
    }
}

#[derive(Debug)]
pub struct Target {
    member: *const Package,
    target: *const MetaTarget,
    entrypoint: RefCell<Option<File>>,
}

impl Target {
    pub fn new(
        member: &Package,
        target: &MetaTarget,
    ) -> Self {
        Self {
            member,
            target,
            entrypoint: RefCell::new(None),
        }
    }

    pub fn entrypoint(&self) -> Result<Ref<File>, Error> {
        if self.entrypoint.borrow().is_none() {
            let file = File::new(self, unsafe { &(*self.target).src_path })?;
            *self.entrypoint.borrow_mut() = Some(file);
        }
        Ok(Ref::map(self.entrypoint.borrow(), |e| e.as_ref().unwrap()))
    }
}

#[derive(Debug)]
pub struct File {
    target: *const Target,
    pub file: SynFile,
}

impl File {
    pub fn new(
        target: &Target,
        path: &PathBuf
    ) -> Result<Self, Error> {
        let mut f = FsFile::open(path).map_err(|e| FileIoError(Box::new(path.clone()), Box::new(e)))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).map_err(|e| FileIoError(Box::new(path.clone()), Box::new(e)))?;

        let file = syn::parse_file(buffer.as_str()).map_err(|e| FileParseError(Box::new(path.clone()), Box::new(e)))?;

        Ok(Self {
            target: target as *const Target,
            file,
        })
    }
}