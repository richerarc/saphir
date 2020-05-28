use super::Error;
use crate::docgen::crate_syn_browser::{ExpandedUse, File, Item, Target, UseScope};
use lazycell::LazyCell;
use std::fmt::Debug;
use syn::{Item as SynItem, ItemMod as SynItemMod, UseTree, Visibility};

#[derive(Debug)]
pub struct Module<'b> {
    kind: LazyCell<ModuleKind<'b>>,
    name: String,
    path: String,
    items: LazyCell<Vec<Item<'b>>>,
    modules: LazyCell<Vec<Module<'b>>>,
    uses: LazyCell<Vec<ExpandedUse>>,
}

impl<'b> UseScope<'b> for Module<'b> {
    fn path(&'b self) -> &'b str {
        self.path.as_str()
    }

    fn target(&'b self) -> &'b Target<'b> {
        match self.kind.borrow().expect("Kind should always be initialized") {
            ModuleKind::Crate(m) => m.target,
            ModuleKind::File(m) => m.parent_module.target(),
            ModuleKind::Inline(m) => m.parent_module.target(),
        }
    }

    fn uses(&'b self) -> &'b Vec<ExpandedUse> {
        self.init_uses();
        self.use_cell().borrow().expect("Initialized above")
    }

    fn pub_uses(&'b self) -> Vec<&'b ExpandedUse> {
        self.init_uses();
        self.use_cell().borrow().expect("Initialized above").iter().filter(|u| u.is_pub).collect()
    }

    fn find_type_definition(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error> {
        if let Ok(Some(item)) = self.find_type_definition_inline(name) {
            return Ok(Some(item));
        }

        for u in self.uses() {
            if u.alias.as_str() == name {
                if let Some(module) = self.target().module_by_use_path(u.path.as_str())? {
                    return module.find_type_definition(u.name.as_str());
                }
            }
        }

        //At this point, this is most likely a primitive.
        Ok(None)
    }

    fn find_type_definition_inline(&'b self, name: &str) -> Result<Option<&'b Item<'b>>, Error> {
        for (item, item_name) in self.items().iter().filter_map(|i| match &i.item {
            SynItem::Enum(e) => Some((i, e.ident.to_string())),
            SynItem::Struct(s) => Some((i, s.ident.to_string())),
            _ => None,
        }) {
            if item_name.as_str() == name {
                return Ok(Some(item));
            }
        }
        Ok(None)
    }
}

#[derive(Debug)]
pub enum ModuleKind<'b> {
    Crate(CrateModule<'b>),
    File(FileModule<'b>),
    Inline(InlineModule<'b>),
}

#[derive(Debug)]
pub struct CrateModule<'b> {
    module: &'b Module<'b>,
    target: &'b Target<'b>,
    file: File<'b>,
}

#[derive(Debug)]
pub struct FileModule<'b> {
    module: &'b Module<'b>,
    syn_module: &'b SynItemMod,
    parent_module: &'b Module<'b>,
    file: File<'b>,
}

#[derive(Debug)]
pub struct InlineModule<'b> {
    module: &'b Module<'b>,
    syn_module: &'b SynItemMod,
    parent_module: &'b Module<'b>,
    items: &'b Vec<SynItem>,
}

impl<'b> Module<'b> {
    pub(crate) fn new_crate(target: &'b Target<'b>) -> Result<Module<'b>, Error> {
        let name = target.package.name.clone();
        let module = Module {
            name: name.clone(),
            path: name,
            items: LazyCell::new(),
            modules: LazyCell::new(),
            uses: LazyCell::new(),
            kind: LazyCell::new(),
        };
        Ok(module)
    }

    pub(crate) fn init_crate(&'b self, target: &'b Target<'b>) -> Result<(), Error> {
        let kind = ModuleKind::Crate(CrateModule {
            module: &self,
            target,
            file: File::new(target, &target.target.src_path, self.name.clone())?,
        });
        self.kind.fill(kind).expect("init_crate should be called exactly once");
        Ok(())
    }

    pub(crate) fn new(parent: &'b Module<'b>, syn_mod: &'b SynItemMod) -> Result<Self, Error> {
        let name = syn_mod.ident.to_string();
        let path = format!("{}::{}", parent.path(), &name);
        let module = Module {
            kind: LazyCell::new(),
            name,
            path,
            items: LazyCell::new(),
            modules: LazyCell::new(),
            uses: LazyCell::new(),
        };
        Ok(module)
    }

    pub(crate) fn init_new(&'b self, parent: &'b Module<'b>, syn_mod: &'b SynItemMod) -> Result<(), Error> {
        let kind = if let Some((_, items)) = &syn_mod.content {
            ModuleKind::Inline(InlineModule {
                module: &self,
                syn_module: syn_mod,
                parent_module: parent,
                items,
            })
        } else {
            let mut dir = parent.file().dir.join(&self.name).join("mod.rs");
            if !dir.exists() {
                dir = parent.file().dir.join(format!("{}.rs", &self.name));
            }
            ModuleKind::File(FileModule {
                module: &self,
                parent_module: parent,
                syn_module: syn_mod,
                file: File::new(parent.target(), &dir, self.path.clone())?,
            })
        };
        self.kind.fill(kind).expect("init_new should be called exactly once");
        Ok(())
    }

    fn file(&'b self) -> &'b File<'b> {
        match self.kind.borrow().expect("Kind should always be initialized") {
            ModuleKind::Crate(m) => &m.file,
            ModuleKind::File(m) => &m.file,
            ModuleKind::Inline(m) => &m.parent_module.file(),
        }
    }

    pub fn name(&'b self) -> &'b str {
        self.name.as_str()
    }

    fn items(&'b self) -> &Vec<Item<'b>> {
        if !self.items.filled() {
            let items = match self.kind.borrow().expect("Kind should always be initialized") {
                ModuleKind::Crate(m) => &m.file.file.items,
                ModuleKind::File(m) => &m.file.file.items,
                ModuleKind::Inline(m) => &m.items,
            };
            let items = items.iter().map(|i| Item::new(&self, i)).collect();
            self.items.fill(items).expect("Should be called only once");
            for i in self.items.borrow().expect("Filled above").iter() {
                i.init_new();
            }
        }
        self.items.borrow().expect("Filled above")
    }

    #[allow(dead_code)]
    fn parent_scope(&'b self) -> Option<&'b dyn UseScope<'b>> {
        match self.kind.borrow().expect("Kind should always be initialized") {
            ModuleKind::Crate(_) => None,
            ModuleKind::File(m) => Some(m.parent_module),
            ModuleKind::Inline(m) => Some(m.parent_module),
        }
    }

    fn use_cell(&'b self) -> &'b LazyCell<Vec<ExpandedUse>> {
        &self.uses
    }

    pub fn modules(&'b self) -> Result<&'b Vec<Module>, Error> {
        let cell = &self.modules;
        if !cell.filled() {
            let mut syn_modules = Vec::new();
            let modules = self
                .items()
                .iter()
                .filter_map(|i| match i.item {
                    SynItem::Mod(m) => Some(m),
                    _ => None,
                })
                .map(|m| {
                    syn_modules.push(m);
                    Module::new(self, m)
                })
                .collect::<Result<Vec<_>, _>>()?;
            cell.fill(modules).expect("We should never be filling this twice");
            for (i, m) in cell.borrow().expect("We just filled this").iter().enumerate() {
                m.init_new(self, syn_modules.get(i).expect("mapped above"))?;
                self.target().modules.borrow_mut().insert(m.path().to_string(), &m);
            }
        }
        Ok(cell.borrow().expect("Should have been initialized by the previous statement"))
    }

    fn init_uses(&'b self) {
        if !self.use_cell().filled() {
            self.use_cell()
                .fill(
                    self.items()
                        .iter()
                        .filter_map(|i| match &i.item {
                            SynItem::Use(u) => {
                                let is_pub = match u.vis {
                                    Visibility::Public(_) => true,
                                    _ => false,
                                };
                                // TODO: This can technically be prefixed by uses in the file containing this
                                // inline module
                                Some(self.expand_use_tree(&u.tree, None, is_pub))
                            }
                            _ => None,
                        })
                        .flatten()
                        .collect(),
                )
                .expect("We shouldn't be filling twice");
        }
    }

    fn expand_use_tree(&'b self, u: &'b UseTree, prefix: Option<String>, is_pub: bool) -> Vec<ExpandedUse> {
        match u {
            UseTree::Name(n) => {
                let name = n.ident.to_string();
                let path = prefix.unwrap_or_else(|| self.path().to_string());
                vec![ExpandedUse {
                    path,
                    alias: name.clone(),
                    name,
                    is_pub,
                }]
            }
            UseTree::Rename(n) => {
                let name = n.ident.to_string();
                let alias = n.rename.to_string();
                let path = prefix.unwrap_or_else(|| self.path().to_string());
                vec![ExpandedUse { path, name, alias, is_pub }]
            }
            UseTree::Group(g) => g.items.iter().map(|u| self.expand_use_tree(u, prefix.clone(), is_pub)).flatten().collect(),
            UseTree::Path(p) => {
                let path_segment = p.ident.to_string();
                let prefix = if prefix.is_none() {
                    if path_segment.as_str() == "self" {
                        self.path().to_string()
                    } else if path_segment.as_str() == "crate" {
                        self.target().package.name.clone()
                    } else if path_segment.as_str() == "super" {
                        let split: Vec<&str> = self.path().split("::").collect();
                        split[..(split.len() - 1)].join("::")
                    } else {
                        path_segment
                    }
                } else if let Some(p) = prefix {
                    format!("{}::{}", p, path_segment)
                } else {
                    format!("{}::{}", self.path(), path_segment)
                };
                self.expand_use_tree(p.tree.as_ref(), Some(prefix), is_pub)
            }
            UseTree::Glob(_) => {
                let path = prefix.expect("Glob pattern should have a path prefix");
                if let Ok(Some(module)) = self.target().module_by_use_path(path.as_str()) {
                    module.pub_uses().iter().cloned().cloned().collect()
                } else {
                    vec![]
                }
            }
        }
    }

    pub fn all_items(&'b self) -> Result<Vec<&'b Item<'b>>, Error> {
        let mut vec = Vec::new();
        vec.extend(self.items());
        for m in self.modules()? {
            vec.extend(m.all_items()?);
        }
        Ok(vec)
    }
}
