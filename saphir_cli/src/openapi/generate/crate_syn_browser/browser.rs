use super::{Error, Package};
use lazycell::LazyCell;
use std::{fmt::Debug, path::PathBuf};

#[derive(Debug)]
pub struct Browser<'b> {
    pub(crate) crate_metadata: cargo_metadata::Metadata,
    packages: LazyCell<Vec<Package<'b>>>,
}

impl<'b> Browser<'b> {
    pub fn new(crate_path: PathBuf) -> Result<Self, Error> {
        let crate_metadata = cargo_metadata::MetadataCommand::new().manifest_path(crate_path.join("Cargo.toml")).exec()?;

        let browser = Self {
            crate_metadata,
            packages: LazyCell::new(),
        };

        Ok(browser)
    }

    pub fn package_by_name(&self, name: &str) -> Option<&'b Package> {
        self.packages().iter().find(|p| p.meta.name.as_str() == name)
    }

    fn init_packages(&'b self) {
        if !self.packages.filled() {
            let members: Vec<Package> = self
                .crate_metadata
                .workspace_members
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
