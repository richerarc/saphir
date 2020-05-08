use syn::{File, Item, Type, PathArguments, GenericArgument, Expr, UseTree, Lit};
use std::collections::HashMap;
use crate::docgen::{CargoDependancy, DocGen};

#[derive(Clone, Debug)]
pub(crate) struct TypeInfo {
    pub name: String,
    pub ast_file_path: String,
    pub ast_item_name: String,
    pub is_array: bool,
    pub is_optional: bool,
    pub min_array_len: Option<u32>,
    pub max_array_len: Option<u32>,
}

impl DocGen {
    pub fn find_type_info(
        &self,
        ast_file_path: &str,
        t: &Type
    ) -> Option<TypeInfo> {
        match t {
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

                        if let Some(mut type_info) = self.find_type_info(ast_file_path, t2) {
                            match name.as_str() {
                                "Vec" => type_info.is_array = true,
                                "Option" => type_info.is_optional = true,
                                _ => unreachable!()
                            }
                            return Some(type_info);
                        }
                    } else {
                        if let Some((ast_file_path, ast_item_name)) = self.find_type(ast_file_path, name.as_str()) {
                            return Some(TypeInfo { name, ast_file_path, ast_item_name, is_array: false, is_optional: false, min_array_len: None, max_array_len: None })
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

                if let Some(mut type_info) = self.find_type_info(ast_file_path, a.elem.as_ref()) {
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

    fn find_type(
        &self,
        ast_file_path: &str,
        ast_item_name: &str,
    ) -> Option<(String, String)> {
        let loaded_files_ast = self.loaded_files_ast.borrow();
        let cur_file = loaded_files_ast.get(ast_file_path)?;

        if self.find_type_in_file(cur_file, ast_item_name).is_some() {
            return Some((ast_file_path.to_string(), ast_item_name.to_string()));
        }

        if let Some(result) = self.find_type_in_use(ast_file_path, ast_item_name) {
            return Some(result);
        }

        None
    }

    fn find_type_in_file<'f>(
        &self,
        file: &'f File,
        ast_item_name: &str,
    ) -> Option<&'f Item> {
        // Find type in current file
        for item in &file.items {
            match item {
                Item::Struct(s) if s.ident.to_string().as_str() == ast_item_name => return Some(item),
                Item::Enum(e) if e.ident.to_string().as_str() == ast_item_name => return Some(item),
                _ => {}
            }
        }

        None
    }

    fn find_type_in_use(
        &self,
        ast_file_path: &str,
        ast_item_name: &str,
    ) -> Option<(String, String)> {
        let loaded_files_ast = self.loaded_files_ast.borrow();
        let cur_file = loaded_files_ast.get(ast_file_path)?;

        for u in cur_file.items.iter().filter_map(|i| match i {
            Item::Use(u) => Some(u),
            _ => None,
        }) {
            if let Some(resolved) = self.find_type_in_use_tree(&u.tree, ast_file_path, ast_item_name) {
                return Some(resolved);
            }
        }

       None
    }

    // TODO: Implement this
    // fn resolve_glob_use_tree(&self, use_glob: &UseGlob, self_module_path: String, current_type_path: Option<String>, type_name: &String) -> Option<String> {
    //     None
    // }

    fn find_type_in_use_tree(
        &self,
        use_tree: &UseTree,
        ast_file_path: &str,
        ast_item_name: &str,
    ) -> Option<(String, String)> {
        let loaded_files_ast = self.loaded_files_ast.borrow();
        let cur_file = loaded_files_ast.get(ast_file_path)?;

        match use_tree {
            UseTree::Name(n) => {
                let name = n.ident.to_string();
                if name == *ast_item_name {
                    if self.find_type_in_file(cur_file, name.as_str()).is_some() {
                        return Some((ast_file_path.to_string(), ast_item_name.to_string()));
                    }
                }
            }
            UseTree::Rename(r) => {
                let rename = r.rename.to_string();
                if rename == *ast_item_name {
                    if self.find_type_in_file(cur_file, rename.as_str()).is_some() {
                        return Some((ast_file_path.to_string(), ast_item_name.to_string()));
                    }
                }
            }
            UseTree::Group(g) => {
                for t in &g.items {
                    if let Some(resolved) = self.find_type_in_use_tree(t, ast_file_path, ast_item_name) {
                        return Some(resolved);
                    }
                }
            }
            UseTree::Path(u) => {
                // let mut first_segment = u.ident.to_string();
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
                // return Self::find_type_in_use_tree(
                //     loaded_files,
                //     loaded_dependancies,
                //     cur_file,
                //
                //
                //     &u.tree, self_module_path, Some(path), type_name
                // );
                println!("Use path : {:?}", u);
            }
            UseTree::Glob(_) => {}
        }
        None
    }
}