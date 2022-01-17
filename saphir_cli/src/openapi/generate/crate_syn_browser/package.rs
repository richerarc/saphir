use super::{Browser, Target};
use cargo_metadata::{Package as MetaPackage, PackageId};
use lazycell::LazyCell;
use std::{cell::RefCell, collections::HashMap, fmt::Debug};

#[derive(Debug)]
pub struct Package<'b> {
    pub name: String,
    pub browser: &'b Browser<'b>,
    pub(crate) meta: &'b MetaPackage,
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
    pub fn new(browser: &'b Browser<'b>, id: &'b PackageId) -> Option<Self> {
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
            let package = self
                .meta
                .dependencies
                .iter()
                .find(|dep| dep.rename.as_ref().unwrap_or(&dep.name) == name).and_then(|dep| {
                    self.browser
                        .crate_metadata
                        .packages
                        .iter()
                        .find(|package| package.name == dep.name && dep.req.matches(&package.version)).and_then(|p| Package::new(self.browser, &p.id))
                });
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
            .map(|d| d.as_ref()).and_then(|d| d.copied())
            .map(|b| unsafe { &*b })
    }

    fn targets(&'b self) -> &'b Vec<Target> {
        if !self.targets.filled() {
            let targets = self.meta.targets.iter().map(|t| Target::new(self, t)).collect();
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
