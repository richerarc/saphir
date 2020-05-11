use syn::{Item, Type, PathArguments, GenericArgument, Expr, UseTree, Lit, ItemStruct, ItemEnum};
use crate::docgen::{DocGen};
use std::borrow::Borrow;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub(crate) struct TypeInfo {
    pub name: String,
    pub rust_type: RustType,
    pub is_array: bool,
    pub is_optional: bool,
    pub min_array_len: Option<u32>,
    pub max_array_len: Option<u32>,
}

#[derive(Clone, Debug)]
pub(crate) enum RustType {
    Struct {
        file_path: String,
        item: &'static ItemStruct,
    },
    Enum {
        file_path: String,
        item: &'static ItemEnum,
    },
    Primitive,
}

impl DocGen {
    pub fn find_type_info(
        &self,
        module_path: &str,
        t: &Type
    ) -> Option<TypeInfo> {
        match t.borrow() {
            Type::Path(p) => {
                if let Some(s) = p.path.segments.first() {
                    let name = s.ident.to_string();
                    if name == "Vec" || name == "Option" {
                        let ag = match &s.arguments {
                            PathArguments::AngleBracketed(ag) => ag,
                            _ =>  {
                                println!("{} need angle bracket type parameter. Maybe another type was aliased as {}, which is not supported.", name, name);
                                return None;
                            },
                        };
                        let t2 = match ag.args.iter().find_map(|a| match a {
                            GenericArgument::Type(t) => Some(t),
                            _ => None
                        }) {
                            Some(t) => t,
                            None => {
                                println!("{} should be provided a type in angle-bracketed format. Faulty type : {:?}", name, t);
                                return None;
                            }
                        };

                        if let Some(mut type_info) = self.find_type_info(module_path, t2) {
                            match name.as_str() {
                                "Vec" => type_info.is_array = true,
                                "Option" => type_info.is_optional = true,
                                _ => unreachable!()
                            }
                            return Some(type_info);
                        }
                    } else {
                        if let rust_type = self.find_type_in_module(module_path, name.as_str()) {
                            return Some(TypeInfo { name, rust_type, is_array: false, is_optional: false, min_array_len: None, max_array_len: None })
                        }
                    }
                }
            }
            Type::Array(a) => {
                let len: Option<u32> = match &a.len {
                    Expr::Lit(l) => match &l.lit {
                        Lit::Int(i) => i.base10_parse().ok(),
                        _ => None,
                    },
                    _ => None,
                };

                if let Some(mut type_info) = self.find_type_info(module_path, a.elem.as_ref()) {
                    type_info.is_array = true;
                    type_info.min_array_len = len.clone();
                    type_info.max_array_len = len;
                    return Some(type_info);
                }
            }
            _ => {},
        };
        None
    }

    fn find_type_in_module<'f>(
        &self,
        module_path: &str,
        item_name: &str,
    ) -> RustType {
        if let Some(rust_type) = self.find_type_in_file(module_path, item_name) {
            return rust_type;
        }

        if let Some(rust_type) = self.find_type_in_use(module_path, item_name) {
            return rust_type;
        }

        return RustType::Primitive;
    }

    fn find_type_in_file<'f>(
        &self,
        module_path: &str,
        item_name: &str,
    ) -> Option<RustType> {
        let file = *self.loaded_files_ast.borrow().get(module_path)?;
        for item in &file.items {
            match item {
                Item::Struct(s) if s.ident.to_string().as_str() == item_name => return Some(RustType::Struct {
                    file_path: module_path.to_string(),
                    item: s,
                }),
                Item::Enum(e) if e.ident.to_string().as_str() == item_name => return Some(RustType::Enum {
                    file_path: module_path.to_string(),
                    item: e,
                }),
                _ => {}
            }
        }

        None
    }

    fn find_type_in_use<'f>(
        &self,
        module_path: &str,
        item_name: &str,
    ) -> Option<RustType> {
        let file = *self.loaded_files_ast.borrow().get(module_path)?;

        let modules: HashSet<String> = file.items.iter().filter_map(|i| match i {
            Item::Mod(m) => Some(m.ident.to_string()),
            _ => None,
        }).collect();
        println!("Modules: {:?}", modules);

        for u in file.items.iter().filter_map(|i| match i {
            Item::Use(u) => Some(u),
            _ => None,
        }) {
            if let Some((module_path, module_item_name)) = self.find_type_in_use_tree(&u.tree, module_path, &modules,None, item_name) {
                let rust_type = self.find_type_in_file(module_path.as_str(), module_item_name.as_str());
                if let Some(rust_type) = rust_type {
                    return Some(rust_type);
                } else {
                    println!("{} not found in {:?}", item_name, module_path);

                    //TODO: Load dependancy here
                    let t = self.find_type_in_module(module_path.as_str(), item_name);
                    println!("Type : {:?}", t);
                }
            }
        }

       None
    }

    // TODO: Implement this
    // fn resolve_glob_use_tree(&self, use_glob: &UseGlob, self_module_path: String, current_type_path: Option<String>, type_name: &String) -> Option<String> {
    //     None
    // }

    // fn find_type_in_use_tree<'f>(
    //     &self,
    //     use_tree: &'f UseTree,
    //     file: &'f File,
    //     item_name: &str,
    // ) -> Option<(&'f File, &'f Item)> {
    //
    // }

    fn find_type_in_use_tree<'f>(
        &self,
        use_tree: &'f UseTree,
        module_path: &str,
        modules: &HashSet<String>,
        mut current_module_path: Option<String>,
        item_name: &str,
    ) -> Option<(String, String)> {
        match use_tree {
            UseTree::Name(n) => {
                let name = n.ident.to_string();
                if name == *item_name {
                    return Some((current_module_path.unwrap_or_else(|| module_path.to_string()), name));
                    // if let Some(item) = self.find_type_in_file(file, name.as_str()) {
                    //     return Some((file, item));
                    // }
                }
            }
            UseTree::Rename(r) => {
                let rename = r.rename.to_string();
                if rename == *item_name {
                    return Some((current_module_path.unwrap_or_else(|| module_path.to_string()), rename));
                    // if let Some(item) = self.find_type_in_file(file, rename.as_str()) {
                    //     return Some((file, item));
                    // }
                }
            }
            UseTree::Group(g) => {
                for t in &g.items {
                    if let Some(resolved) = self.find_type_in_use_tree(t, module_path, modules, current_module_path.clone(), item_name) {
                        return Some(resolved);
                    }
                }
            }
            UseTree::Path(u) => {
                let mut first_segment = u.ident.to_string();
                if first_segment.as_str() == "self" {
                    first_segment = module_path.to_string();
                }

                if current_module_path.is_none() && modules.contains(first_segment.as_str()) {
                    current_module_path = Some(module_path.to_string());
                }

                let path = if let Some(cur_path) = current_module_path {
                    format!("{}::{}", cur_path, first_segment)
                } else {
                    first_segment
                };

                return self.find_type_in_use_tree(
                    &u.tree,
                    module_path,
                    modules,
                    Some(path),
                    item_name,
                )

                // if first_segment.as_str() != "self" {
                //     if let Some(path) = module_path {
                //         module_path =
                //         if first_segment.as_str() == "crate" {
                //
                //         }
                //     }
                // }
                //
                //
                //
                // if first_segment.as_str() == "self" {
                //     first_segment = self_module_path.clone();
                // }
                // let path = if let Some(cur) = current_type_path {
                //     format!("{}::{}", cur, first_segment)
                // } else if first_segment.as_str() == "crate" {
                //     first_segment
                // } else {
                //     // TODO: Impl this
                //     println!("Dependancy types not currently supported; {:?}", ast_item_name);
                //     return None;
                // };
                // return self.find_type_in_use_tree(
                //     &u.tree,
                //     module_path,
                //     item_name,
                // );
                // println!("Use path : {:?}", u);
            }
            UseTree::Glob(_) => {}
        }
        None
    }

    // fn find_type_in_use_tree<'f>(
    //     &self,
    //     use_tree: &'f UseTree,
    //     file: &'f File,
    //     item_name: &str,
    // ) -> Option<(&'f File, &'f Item)> {
    //     match use_tree {
    //         UseTree::Name(n) => {
    //             let name = n.ident.to_string();
    //             if name == *item_name {
    //                 if let Some(item) = self.find_type_in_file(file, name.as_str()) {
    //                     return Some((file, item));
    //                 }
    //             }
    //         }
    //         UseTree::Rename(r) => {
    //             let rename = r.rename.to_string();
    //             if rename == *item_name {
    //                 if let Some(item) = self.find_type_in_file(file, rename.as_str()) {
    //                     return Some((file, item));
    //                 }
    //             }
    //         }
    //         UseTree::Group(g) => {
    //             for t in &g.items {
    //                 if let Some(resolved) = self.find_type_in_use_tree(t, file, item_name) {
    //                     return Some(resolved);
    //                 }
    //             }
    //         }
    //         UseTree::Path(u) => {
    //             let path = self.get_ast_path_from_use_tree_path(u);
    //             println!("Path : {}", path);
    //             println!("Full path : {:?}", u);
    //             // if first_segment.as_str() == "self" {
    //             //     first_segment = self_module_path.clone();
    //             // }
    //             // let path = if let Some(cur) = current_type_path {
    //             //     format!("{}::{}", cur, first_segment)
    //             // } else if first_segment.as_str() == "crate" {
    //             //     first_segment
    //             // } else {
    //             //     // TODO: Impl this
    //             //     println!("Dependancy types not currently supported; {:?}", ast_item_name);
    //             //     return None;
    //             // };
    //             // return Self::find_type_in_use_tree(
    //             //     loaded_files,
    //             //     loaded_dependancies,
    //             //     cur_file,
    //             //
    //             //
    //             //     &u.tree, self_module_path, Some(path), type_name
    //             // );
    //             // println!("Use path : {:?}", u);
    //         }
    //         UseTree::Glob(_) => {}
    //     }
    //     None
    // }
    //
    // fn get_ast_path_from_use_tree_path(&self, use_path: &UsePath) -> String {
    //     let mut first_segment = use_path.ident.to_string();
    //     match use_path.tree.as_ref() {
    //         UseTree::Path(p) => format!("{}::{}", first_segment, self.get_ast_path_from_use_tree_path(p)),
    //         _ => first_segment,
    //     }
    // }
}