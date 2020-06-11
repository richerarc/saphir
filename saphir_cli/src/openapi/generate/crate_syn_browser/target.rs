use super::{Error, Module, Package, UseScope};
use cargo_metadata::Target as MetaTarget;
use lazycell::LazyCell;
use std::{cell::RefCell, collections::HashMap, fmt::Debug};

#[derive(Debug)]
pub struct Target<'b> {
    pub package: &'b Package<'b>,
    pub(crate) target: &'b MetaTarget,
    entrypoint: LazyCell<Module<'b>>,
    pub(crate) modules: RefCell<HashMap<String, &'b Module<'b>>>,
}

impl<'b> Target<'b> {
    pub fn new(package: &'b Package<'b>, target: &'b MetaTarget) -> Self {
        Self {
            package,
            target,
            entrypoint: LazyCell::new(),
            modules: RefCell::default(),
        }
    }

    pub fn entrypoint(&'b self) -> Result<&'b Module<'b>, Error> {
        if !self.entrypoint.filled() {
            let module = Module::new_crate(self)?;
            self.entrypoint.fill(module).expect("We should never be filling this twice");
            let module = self.entrypoint.borrow().unwrap();
            module.init_crate(self)?;
            self.modules.borrow_mut().insert(self.target.name.clone(), module);
        }
        Ok(self.entrypoint.borrow().expect("Should have been initialized by the previous statement"))
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
                let mut cur_module = self.entrypoint()?;
                for split in path_split {
                    if let Some(m) = cur_module.modules()?.iter().find(|m| m.name() == split) {
                        cur_module = m;
                    }
                }
                if cur_module.path() == path {
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
                lib.module_by_use_path(use_path.as_str())?
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
