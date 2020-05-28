use crate::docgen::crate_syn_browser::Module;
use lazycell::LazyCell;
use std::fmt::Debug;
use syn::{
    ImplItem as SynImplItem, ImplItemMethod as SynImplItemMethod, Item as SynItem, ItemEnum as SynItemEnum, ItemImpl as SynItemImpl,
    ItemStruct as SynItemStruct, ItemUse as SynItemUse,
};

#[derive(Debug)]
pub struct Item<'b> {
    // TODO: scope can possibly be a `&'b dyn UseScope<'b>`
    pub scope: &'b Module<'b>,
    pub item: &'b SynItem,
    kind: LazyCell<ItemKind<'b>>,
}

impl<'b> Item<'b> {
    pub fn kind(&'b self) -> &'b ItemKind<'b> {
        self.kind.borrow().expect("Should be initialized by Item::init_new() after created with new()")
    }
}

#[derive(Debug)]
pub enum ItemKind<'b> {
    Use(Use<'b>),
    Struct(Struct<'b>),
    Enum(Enum<'b>),
    Impl(Impl<'b>),
    Unsupported,
}

#[derive(Debug)]
pub struct Use<'b> {
    pub item: &'b Item<'b>,
    pub syn: &'b SynItemUse,
}

#[derive(Debug)]
pub struct Enum<'b> {
    pub item: &'b Item<'b>,
    pub syn: &'b SynItemEnum,
}

#[derive(Debug)]
pub struct Struct<'b> {
    pub item: &'b Item<'b>,
    pub syn: &'b SynItemStruct,
}

#[derive(Debug)]
pub struct Impl<'b> {
    pub item: &'b Item<'b>,
    pub syn: &'b SynItemImpl,
    items: LazyCell<Vec<ImplItem<'b>>>,
}

impl<'b> Impl<'b> {
    pub fn items(&'b self) -> &'b Vec<ImplItem<'b>> {
        self.items.borrow().expect("Impl should always be initialized")
    }
}

#[derive(Debug)]
pub struct ImplItem<'b> {
    pub im: &'b Impl<'b>,
    pub syn: &'b SynImplItem,
    kind: LazyCell<ImplItemKind<'b>>,
}

impl<'b> ImplItem<'b> {
    pub fn kind(&'b self) -> &'b ImplItemKind<'b> {
        self.kind.borrow().expect("Impl item should be initialized by crate_syn_browser")
    }
}

impl<'b> Impl<'b> {
    pub(self) fn new(syn: &'b SynItemImpl, item: &'b Item<'b>) -> Self {
        Impl {
            item,
            syn,
            items: LazyCell::new(),
        }
    }

    pub(self) fn init_new(&'b self) {
        let items: Vec<ImplItem<'b>> = self
            .syn
            .items
            .iter()
            .map(|i| ImplItem {
                im: &self,
                syn: i,
                kind: LazyCell::new(),
            })
            .collect();
        self.items.fill(items).expect("init_new must be called only once");

        for (i, syn_item) in self.syn.items.iter().enumerate() {
            let impl_item = self
                .items
                .borrow()
                .expect("initialized above")
                .get(i)
                .expect("should be 1:1 with the items added above");
            impl_item
                .kind
                .fill(match syn_item {
                    SynImplItem::Method(syn) => ImplItemKind::Method(Method { syn, impl_item: &impl_item }),
                    _ => ImplItemKind::Unsupported,
                })
                .expect("should be filled only once")
        }
    }
}

#[derive(Debug)]
pub enum ImplItemKind<'b> {
    Method(Method<'b>),
    Unsupported,
}

#[derive(Debug)]
pub struct Method<'b> {
    pub impl_item: &'b ImplItem<'b>,
    pub syn: &'b SynImplItemMethod,
}

impl<'b> Item<'b> {
    pub fn new(scope: &'b Module<'b>, item: &'b SynItem) -> Self {
        Self {
            scope,
            item,
            kind: LazyCell::new(),
        }
    }

    pub fn init_new(&'b self) {
        self.kind
            .fill(match self.item {
                SynItem::Use(syn) => ItemKind::Use(Use { syn, item: &self }),
                SynItem::Enum(syn) => ItemKind::Enum(Enum { syn, item: &self }),
                SynItem::Struct(syn) => ItemKind::Struct(Struct { syn, item: &self }),
                SynItem::Impl(syn) => ItemKind::Impl(Impl::new(syn, &self)),
                _ => ItemKind::Unsupported,
            })
            .expect("init_new should be called only once");

        #[allow(clippy::single_match)]
        match self.kind() {
            ItemKind::Impl(i) => i.init_new(),
            _ => {}
        }
    }
}
