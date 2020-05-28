use crate::{
    docgen::{
        controller_info::ControllerInfo,
        crate_syn_browser::{Browser, Item, ItemKind, Module, UseScope},
        type_info::TypeInfo,
        utils::{find_macro_attribute_flag, find_macro_attribute_named_value, get_serde_field},
    },
    openapi::{
        OpenApi, OpenApiContent, OpenApiMimeType, OpenApiObjectType, OpenApiParameter, OpenApiParameterLocation, OpenApiPath, OpenApiPathMethod,
        OpenApiRequestBody, OpenApiResponse, OpenApiSchema, OpenApiType,
    },
    Command, CommandResult,
};
use serde_derive::Deserialize;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    fs::File as FsFile,
    io::Read,
    path::PathBuf,
    time::Instant,
};
use structopt::StructOpt;
use syn::{Attribute, Fields, Item as SynItem, ItemEnum, ItemStruct, Lit, Meta, NestedMeta, Signature};

mod controller_info;
mod crate_syn_browser;
mod handler_info;
mod response_info;
mod route_info;
mod type_info;
mod utils;

macro_rules! print_project_path_error {
    ($file:expr, $project_path:expr) => {{
        let project_path = $project_path.to_str().map(|s| s.to_owned()).unwrap_or_else(|| format!("{:?}", $project_path));
        format!(
            "Unable to find `{}` in project root `{}`.
Make sure that you are either running this command from your project's root,
or that the argument --project-path (-p) point to the project's root.
You can see help with the --help flag.",
            $file, project_path
        )
    }};
}

/// Generate OpenAPI v3 from a Saphir application.
///
/// See: https://github.com/OAI/OpenAPI-Specification/blob/master/versions/3.0.2.md
#[derive(StructOpt, Debug, Default)]
pub(crate) struct DocGenArgs {
    /// (Optional) Limit doc generation to the URIs under this scope.
    ///
    /// For example, if you pass `/api/v1` and that your Saphir server had
    /// handlers for the following routes :
    /// - GET  /
    /// - GET  /about
    /// - GET  /api/v1/user
    /// - POST /api/v1/user
    /// , the generated doc would contain only the `/api/v2/user` endpoints.
    #[structopt(short = "s", long = "scope", default_value = "/", verbatim_doc_comment)]
    scope: String,

    /// (Optional) path to the Saphir server's root
    #[structopt(parse(from_os_str), short = "p", long = "project-path", default_value = ".")]
    project_path: PathBuf,

    /// (Optional) If running on a workspace, name of the package of the lucid
    /// server for which we want            to build openapi doc
    #[structopt(long = "package")]
    package_name: Option<String>,

    /// (optionnal) Path to the `.cargo` directory. By default, read from the
    /// current executable's environment,             which work when
    /// running this command as a cargo sub-command.
    #[structopt(parse(from_os_str), long = "cargo-path", default_value = "~/.cargo")]
    cargo_path: PathBuf,

    /// (Optional) Resulting output path. Either the path to the resulting yaml
    /// file,            or a dir, which would then contain a openapi.yaml
    #[structopt(parse(from_os_str), default_value = ".")]
    output_file: PathBuf,
}

pub(crate) struct DocGen {
    pub args: <DocGen as Command>::Args,
    pub doc: OpenApi,
    pub operation_ids: RefCell<HashSet<String>>,
}

impl Command for DocGen {
    type Args = DocGenArgs;

    fn new(args: Self::Args) -> Self {
        let mut doc = OpenApi::default();
        doc.openapi_version = "3.0.3".to_string();
        Self {
            args,
            doc,
            operation_ids: RefCell::new(Default::default()),
        }
    }

    fn run<'b>(&mut self) -> CommandResult {
        let now = Instant::now();
        self.read_project_cargo_toml()?;
        let browser = Browser::new(self.args.project_path.clone()).map_err(|e| format!("{}", e))?;
        let browser = unsafe { &*(&browser as *const Browser) }; // FIXME: Definitely find a better way to handle the lifetime issue here
        let entrypoint = self.get_crate_entrypoint(self.args.package_name.as_ref(), browser)?;
        let controllers = self.load_controllers(entrypoint)?;
        self.fill_openapi_with_controllers(entrypoint, controllers)?;
        let file = self.write_doc_file()?;
        println!("Succesfully created `{}` in {}ms", file, now.elapsed().as_millis());
        Ok(())
    }
}

impl DocGen {
    fn get_crate_entrypoint<'s, 'r, 'b: 'r>(&'s self, package_name: Option<&'r String>, browser: &'b Browser<'b>) -> Result<&'b Module<'b>, String> {
        let main_package = if let Some(main_name) = package_name {
            let package = browser.package_by_name(main_name);
            package.unwrap_or_else(|| panic!("Crate does not include a member named `{}`.", main_name))
        } else if browser.packages().len() == 1 {
            browser.packages().first().expect("Condition ensure exactly 1 workspace member")
        } else {
            return Err("This crate is a workspace with multiple packages!
Please select the package for which you want to generate the openapi documentation
by using the --package flag."
                .to_string());
        };
        let bin_target = main_package.bin_target().expect("Crate does not have a Binary target.");
        let entrypoint = bin_target.entrypoint().map_err(|e| e.to_string())?;
        Ok(entrypoint)
    }

    fn write_doc_file(&self) -> Result<String, String> {
        let mut path = self.args.output_file.clone();
        if path.is_dir() {
            path = path.join("openapi.yaml");
        }
        match path.extension() {
            None => path = path.with_extension("yaml"),
            Some(ext) => {
                if ext.to_str() != Some("yaml") && ext.to_str() != Some("yml") {
                    return Err("output must be a yaml file.".to_string());
                }
            }
        }
        let f = FsFile::create(&path).map_err(|_| format!("Unable to create file `{:?}`", &path))?;
        serde_yaml::to_writer(f, &self.doc).map_err(|_| format!("Unable to write to `{:?}`", path))?;
        Ok(path.to_str().unwrap_or_default().to_string())
    }

    fn read_project_cargo_toml(&mut self) -> CommandResult {
        #[derive(Deserialize)]
        struct Cargo {
            pub package: Package,
        }
        #[derive(Deserialize)]
        struct Package {
            pub name: String,
            pub version: String,
        }
        let cargo_path = self.args.project_path.clone().join("Cargo.toml");
        let mut f = FsFile::open(&cargo_path).map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer)
            .map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;
        let cargo: Cargo = toml::from_str(buffer.as_str()).map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;

        self.doc.info.title = cargo.package.name;
        self.doc.info.version = cargo.package.version;

        Ok(())
    }

    fn load_controllers<'b>(&self, entrypoint: &'b Module<'b>) -> Result<Vec<ControllerInfo>, String> {
        let controllers: Vec<ControllerInfo> = entrypoint
            .all_items()
            .map_err(|e| format!("{}", e))?
            .iter()
            .filter_map(|i| match i.kind() {
                ItemKind::Impl(im) => self.extract_controller_info(im).transpose(),
                _ => None,
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(controllers)
    }

    fn fill_openapi_with_controllers<'b>(&mut self, entrypoint: &'b Module<'b>, controllers: Vec<ControllerInfo>) -> CommandResult {
        for controller in controllers {
            for handler in controller.handlers {
                for route in handler.routes {
                    let path = route.uri;
                    let method = route.method;
                    let description = if handler.use_cookies {
                        Some("NOTE: This request consume cookies.".to_string())
                    } else {
                        None
                    };
                    let mut data = OpenApiPath {
                        parameters: handler.parameters.clone(),
                        description,
                        operation_id: route.operation_id,
                        ..Default::default()
                    };

                    if let Some(body_info) = &handler.body_info {
                        if method == OpenApiPathMethod::Get {
                            let parameters = self.get_open_api_parameters_from_body_info(entrypoint, body_info);
                            data.parameters.extend(parameters);
                        } else {
                            data.request_body = self.get_open_api_body_param(entrypoint, body_info);
                        }
                    }

                    for response in &handler.responses {
                        let mut content = HashMap::new();
                        if let Some(openapi_type) = response
                            .type_info
                            .as_ref()
                            .filter(|t| t.is_type_serializable)
                            .map(|t| self.get_open_api_type_from_type_info(entrypoint, &t))
                            .flatten()
                        {
                            content.insert(
                                response.mime.clone(),
                                OpenApiContent {
                                    schema: OpenApiSchema::Inline(openapi_type),
                                },
                            );
                        }
                        data.responses.insert(
                            response.code,
                            OpenApiResponse {
                                // TODO: Status code name from StatusCode in http
                                description: response.type_info.as_ref().map(|t| t.name.clone()).unwrap_or_default(),
                                content,
                            },
                        );
                    }

                    if !self.doc.paths.contains_key(path.as_str()) {
                        self.doc.paths.insert(path.clone(), BTreeMap::new());
                    }
                    let path_map = self.doc.paths.get_mut(path.as_str()).expect("Should work because of previous statement");

                    path_map.insert(method, data);
                }
            }
        }
        Ok(())
    }

    fn get_open_api_body_param<'b>(&self, entrypoint: &'b Module<'b>, body_info: &BodyParamInfo) -> Option<OpenApiRequestBody> {
        let t = if body_info.type_info.is_type_deserializable {
            self.get_open_api_type_from_type_info(entrypoint, &body_info.type_info)?
        } else {
            OpenApiType::anonymous_input_object()
        };
        let mut content: HashMap<OpenApiMimeType, OpenApiContent> = HashMap::new();
        content.insert(
            body_info.openapi_type.clone(),
            OpenApiContent {
                schema: OpenApiSchema::Inline(t),
            },
        );
        Some(OpenApiRequestBody {
            description: body_info.type_info.name.clone(),
            required: !body_info.type_info.is_optional,
            content,
        })
    }

    fn get_open_api_parameters_from_body_info<'b>(&self, entrypoint: &'b Module<'b>, body_info: &BodyParamInfo) -> Vec<OpenApiParameter> {
        let mut parameters = Vec::new();
        if let Some(t) = if body_info.type_info.is_type_deserializable {
            self.get_open_api_type_from_type_info(entrypoint, &body_info.type_info)
        } else {
            None
        } {
            if let OpenApiType::Object {
                object: OpenApiObjectType::Object { properties, required },
            } = t
            {
                for (name, openapi_type) in &properties {
                    parameters.push(OpenApiParameter {
                        name: name.clone(),
                        location: OpenApiParameterLocation::Query,
                        required: required.contains(name),
                        schema: OpenApiSchema::Inline(openapi_type.as_ref().clone()),
                        ..Default::default()
                    });
                }
            }
        }
        parameters
    }

    fn get_open_api_type_from_type_info<'b>(&self, entrypoint: &'b Module<'b>, type_info: &TypeInfo) -> Option<OpenApiType> {
        let type_path = type_info.type_path.as_ref()?;
        let type_mod = entrypoint.target().module_by_use_path(type_path).ok().flatten()?;
        let type_impl = type_mod.find_type_definition(type_info.name.as_str()).ok().flatten()?;
        match type_impl.item {
            SynItem::Struct(s) => self.get_open_api_type_from_struct(type_impl, &s),
            SynItem::Enum(e) => self.get_open_api_type_from_enum(type_impl, e),
            _ => unreachable!(),
        }
    }

    fn get_open_api_type_from_struct<'b>(&self, item: &'b Item<'b>, s: &ItemStruct) -> Option<OpenApiType> {
        let mut properties = HashMap::new();
        let mut required = Vec::new();
        for field in &s.fields {
            if let Some(field_name) = field.ident.as_ref().map(|i| get_serde_field(i.to_string(), &field.attrs, &s.attrs)).flatten() {
                if let Some(field_type_info) = TypeInfo::new(item.scope, &field.ty) {
                    let field_type = self
                        .get_open_api_type_from_type_info(item.scope, &field_type_info)
                        .unwrap_or_else(|| OpenApiType::from_rust_type_str(field_type_info.name.as_str()));
                    if !field_type_info.is_optional
                        && !find_macro_attribute_flag(&field.attrs, "serde", "default")
                        && find_macro_attribute_named_value(&field.attrs, "serde", "default").is_none()
                    {
                        required.push(field_name.clone());
                    }
                    properties.insert(field_name, Box::new(field_type));
                } else {
                    println!("Unsupported type : {:?}", &field.ty);
                }
            }
        }
        if !properties.is_empty() {
            Some(OpenApiType::object(properties, required))
        } else {
            Some(OpenApiType::anonymous_input_object())
        }
    }

    fn get_open_api_type_from_enum<'b>(&self, _item: &Item<'b>, e: &'b ItemEnum) -> Option<OpenApiType> {
        if e.variants.iter().all(|v| v.fields == Fields::Unit) {
            let mut values: Vec<String> = Vec::new();
            for variant in &e.variants {
                if let Some(name) = get_serde_field(variant.ident.to_string(), &variant.attrs, &e.attrs) {
                    values.push(name);
                }
            }
            return Some(OpenApiType::enums(values));
        }

        // TODO: properly support tuple and struct enum variants.
        //       this will require the item param
        Some(OpenApiType::anonymous_input_object())
    }

    fn handler_operation_id_from_sig(&self, sig: &Signature) -> String {
        let method_name = sig.ident.to_string();
        let mut operation_id = method_name.clone();
        let mut i = 2;
        let mut operation_ids = self.operation_ids.borrow_mut();
        while operation_ids.contains(operation_id.as_str()) {
            operation_id = format!("{}_{}", &method_name, &i);
            i += 1;
        }
        operation_ids.insert(operation_id.clone());
        operation_id
    }

    fn handler_method_from_attr(&self, attr: &Attribute) -> Option<OpenApiPathMethod> {
        let ident = attr.path.get_ident()?;
        OpenApiPathMethod::from_str(ident.to_string().as_str())
    }

    fn handler_path_from_attr(&self, attr: &Attribute) -> Option<(String, Vec<String>)> {
        if let Ok(Meta::List(meta)) = attr.parse_meta() {
            if let Some(NestedMeta::Lit(Lit::Str(l))) = meta.nested.first() {
                let mut chars: Vec<char> = l.value().chars().collect();
                let mut params: Vec<String> = Vec::new();

                let mut i = 0;
                while i < chars.len() {
                    if chars[i] == '<' || chars[i] == '{' {
                        chars[i] = '{';
                        let start = i;
                        for j in start..chars.len() {
                            if chars[j] == '>' || chars[j] == '}' {
                                chars[j] = '}';
                                params.push((&chars[(i + 1)..j]).iter().collect());
                                i = j;
                                break;
                            }
                        }
                    }
                    i += 1;
                }

                return Some((chars.into_iter().collect(), params));
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BodyParamInfo {
    openapi_type: OpenApiMimeType,
    type_info: TypeInfo,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RouteParametersInfo {
    parameters: Vec<OpenApiParameter>,
    has_cookies_param: bool,
    body_info: Option<BodyParamInfo>,
}
