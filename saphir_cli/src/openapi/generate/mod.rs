use crate::{
    openapi::{
        generate::{
            controller_info::ControllerInfo,
            crate_syn_browser::{Browser, Item, ItemKind, Module, UseScope},
            response_info::AnonymousType,
            type_info::TypeInfo,
            utils::{find_macro_attribute_flag, find_macro_attribute_named_value, get_serde_field},
        },
        schema::{
            OpenApi, OpenApiContent, OpenApiMimeType, OpenApiObjectType, OpenApiParameter, OpenApiParameterLocation, OpenApiPath, OpenApiPathMethod,
            OpenApiRequestBody, OpenApiResponse, OpenApiSchema, OpenApiTag, OpenApiType,
        },
    },
    Command, CommandResult,
};
use clap::{Args, ValueEnum};
use http::StatusCode;
use serde_derive::Deserialize;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
    fmt::{Display, Formatter},
    fs::File as FsFile,
    io::Read,
    path::PathBuf,
    str::FromStr,
    time::Instant,
};
use syn::{Fields, Item as SynItem, ItemEnum, ItemStruct, Signature};

mod controller_info;
mod crate_syn_browser;
mod handler_info;
mod response_info;
mod route_info;
mod type_info;
mod utils;

#[derive(Debug, Eq, PartialEq, Clone, ValueEnum)]
enum Case {
    #[value(name = "lowercase")]
    Lower,
    #[value(name = "UPPERCASE")]
    Upper,
    #[value(name = "PascalCase")]
    Pascal,
    #[value(name = "camelCase")]
    Camel,
    #[value(name = "snake_case")]
    Snake,
    #[value(name = "SCREAMING_SNAKE_CASE")]
    ScreamingSnake,
    #[value(name = "kebab-case")]
    Kebab,
    #[value(name = "SCREAMING-KEBAB-CASE", alias("COBOL-CASE"))]
    Cobol,
}

impl Default for Case {
    fn default() -> Self {
        Case::Camel
    }
}

impl From<&Case> for convert_case::Case {
    fn from(case: &Case) -> Self {
        match case {
            Case::Lower => convert_case::Case::Lower,
            Case::Upper => convert_case::Case::Upper,
            Case::Pascal => convert_case::Case::Pascal,
            Case::Camel => convert_case::Case::Camel,
            Case::Snake => convert_case::Case::Snake,
            Case::ScreamingSnake => convert_case::Case::ScreamingSnake,
            Case::Kebab => convert_case::Case::Kebab,
            Case::Cobol => convert_case::Case::Cobol,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, ValueEnum)]
enum SchemaGranularity {
    None,
    Top,
    All,
}

impl Default for SchemaGranularity {
    fn default() -> Self {
        SchemaGranularity::Top
    }
}

#[derive(Debug)]
enum SchemaGranularityError {
    UnknownValue,
}

impl Display for SchemaGranularityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Not an expected value")
    }
}

impl FromStr for SchemaGranularity {
    type Err = SchemaGranularityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(SchemaGranularity::None),
            "top" => Ok(SchemaGranularity::Top),
            "all" => Ok(SchemaGranularity::All),
            _ => Err(SchemaGranularityError::UnknownValue),
        }
    }
}

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
#[derive(Args, Debug, Default)]
pub(crate) struct GenArgs {
    /// (Optional) Limit doc generation to the URIs under this scope.
    ///
    /// For example, if you pass `/api/v1` and that your Saphir server had
    /// handlers for the following routes :
    /// - GET  /
    /// - GET  /about
    /// - GET  /api/v1/user
    /// - POST /api/v1/user
    /// , the generated doc would contain only the `/api/v2/user` endpoints.
    #[arg(short = 's', long = "scope", default_value = "/", verbatim_doc_comment)]
    scope: String,

    /// (Optional) Granularity of schema generation.
    ///
    /// By default, top-level objects will have a corresponding schema
    /// created for them, but nested objects will be inlined.
    ///
    /// Available values:
    /// - none :          all objects are inlined
    /// - top (default) : Top-level objects are described as a component schema,
    ///   nested are inlined
    /// - all :           All objects are described as a component schema
    #[arg(value_enum, short = 'g', long = "schema-granularity", default_value = "top", verbatim_doc_comment)]
    schema_granularity: SchemaGranularity,

    /// (Optional) path to the Saphir server's root
    #[arg(short = 'p', long = "project-path", default_value = ".")]
    project_path: PathBuf,

    /// (Optional) If running on a workspace, name of the package of the lucid
    /// server for which we want to build openapi doc
    #[arg(long = "package")]
    package_name: Option<String>,

    /// (Optional) Resulting output path. Either the path to the resulting yaml
    /// file, or a dir, which would then contain a openapi.yaml
    #[arg(default_value = ".")]
    output_file: PathBuf,

    /// (Optional) Casing of the operation names.
    /// Accepted case names matches serde's :
    ///
    /// - lowercase
    /// - UPPERCASE
    /// - PascalCase
    /// - camelCase
    /// - snake_case
    /// - SCREAMING_SNAKE_CASE
    /// - kebab-case
    /// - SCREAMING-KEBAB-CASE
    #[arg(short = 'c', long = "operation-name-case", default_value = "camelCase", value_enum, verbatim_doc_comment)]
    operation_name_case: Case,
}

pub(crate) struct Gen {
    pub args: <Gen as Command>::Args,
    pub doc: OpenApi,
    pub operation_ids: RefCell<HashSet<String>>,
    pub generated_schema_names: HashMap<String, HashMap<Option<String>, String>>, // Name -> path -> final name
}

impl Command for Gen {
    type Args = GenArgs;

    fn new(args: Self::Args) -> Self {
        Self {
            args,
            doc: OpenApi {
                openapi_version: "3.0.3".to_string(),
                ..OpenApi::default()
            },
            operation_ids: RefCell::new(Default::default()),
            generated_schema_names: Default::default(),
        }
    }

    fn run<'b>(mut self) -> CommandResult {
        let now = Instant::now();
        self.read_project_cargo_toml()?;
        let browser = Browser::new(self.args.project_path.clone()).map_err(|e| format!("{}", e))?;
        let browser = unsafe { &*(&browser as *const Browser) }; // FIXME: Definitely find a better way to handle the lifetime issue here
        let entrypoint = self.get_crate_entrypoint(self.args.package_name.as_ref(), browser)?;
        let controllers = self.load_controllers(entrypoint)?;
        self.fill_openapi_with_controllers(entrypoint, controllers);
        self.doc.sort_and_dedup_tags();
        let file = self.write_doc_file()?;
        println!("Succesfully created `{}` in {}ms", file, now.elapsed().as_millis());
        Ok(())
    }
}

impl Gen {
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
        let mut f = FsFile::open(cargo_path).map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer)
            .map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;
        let cargo: Cargo = toml::from_str(buffer.as_str()).map_err(|_| print_project_path_error!("Cargo.toml", self.args.project_path))?;

        self.doc.info.title = cargo.package.name;
        self.doc.info.version = cargo.package.version;

        Ok(())
    }

    fn load_controllers<'b>(&mut self, entrypoint: &'b Module<'b>) -> Result<Vec<ControllerInfo>, String> {
        let controllers: Vec<ControllerInfo> = entrypoint
            .all_items()
            .map_err(|e| format!("{}", e))?
            .iter()
            .filter_map(|i| match i.kind() {
                ItemKind::Impl(im) => self.extract_controller_info(im),
                _ => None,
            })
            .collect::<Vec<_>>();
        Ok(controllers)
    }

    fn fill_openapi_with_controllers<'b>(&mut self, entrypoint: &'b Module<'b>, controllers: Vec<ControllerInfo>) {
        for controller in controllers {
            let controller_name = controller.controller_name.as_str();
            let controller_model_name = controller.name.as_str();
            let mut cur_controller_schema = 0;
            for mut handler in controller.handlers {
                for route in handler.routes {
                    let path = route.uri;
                    let method = route.method;
                    let description = if handler.use_cookies {
                        Some("NOTE: This request consume cookies.".to_string())
                    } else {
                        None
                    };

                    let mut tags = Vec::new();
                    tags.push(OpenApiTag {
                        name: controller.name.clone(),
                        description: Some(format!("Endpoints under the {} controller (`{}`).", controller_model_name, controller_name)),
                    });
                    if let Some(version) = &controller.version {
                        tags.push(OpenApiTag {
                            name: format!("v{}", version),
                            description: Some(format!("Endpoints under the v{} controllers.", version)),
                        });
                        tags.push(OpenApiTag {
                            name: format!("{}-v{}", controller_model_name, version),
                            description: Some(format!(
                                "Endpoints under the {} controller v{} (`{}`).",
                                controller_model_name, version, controller_name
                            )),
                        });
                    }

                    let mut data = OpenApiPath {
                        parameters: handler.parameters.clone(),
                        description: description.clone(),
                        operation_id: route.operation_id.clone(),
                        operation_name: route.operation_name.clone(),
                        tags: tags.iter().map(|t| t.name.clone()).collect(),
                        ..Default::default()
                    };
                    self.doc.tags.extend(tags);

                    if let Some(body_info) = handler.body_info.as_mut() {
                        if method == OpenApiPathMethod::Get {
                            let parameters = self.get_open_api_parameters_from_body_info(entrypoint, body_info);
                            data.parameters.extend(parameters);
                        } else {
                            data.request_body = Some(self.get_open_api_body_param(entrypoint, body_info));
                        }
                    }

                    for response in &mut handler.responses {
                        let mut content = BTreeMap::new();
                        let as_ref = self.args.schema_granularity != SchemaGranularity::None;
                        if let Some(schema) = response
                            .anonymous_type
                            .as_ref()
                            .map(|anon| match &anon.schema {
                                OpenApiSchema::Inline(t) => self.get_schema(
                                    anon.name
                                        .clone()
                                        .unwrap_or_else(|| {
                                            cur_controller_schema += 1;
                                            format!("{}_response_{}", controller_model_name, &cur_controller_schema)
                                        })
                                        .as_str(),
                                    None,
                                    t.clone(),
                                    as_ref,
                                ),
                                s => s.clone(),
                            })
                            .or_else(|| {
                                response.type_info.as_mut().filter(|t| t.is_type_serializable).map(|t| {
                                    self.get_open_api_schema_from_type_info(entrypoint, t, as_ref).unwrap_or_else(|| {
                                        let raw_type = OpenApiType::from_rust_type_str(t.name.as_str()).unwrap_or_else(OpenApiType::string);
                                        OpenApiSchema::Inline(if t.is_array {
                                            OpenApiType::Array {
                                                items: Box::new(OpenApiSchema::Inline(raw_type)),
                                                min_items: t.min_array_len,
                                                max_items: t.max_array_len,
                                            }
                                        } else if t.is_dictionary {
                                            OpenApiType::Object {
                                                object: OpenApiObjectType::Dictionary {
                                                    properties: Default::default(),
                                                    additional_properties: Box::new(OpenApiSchema::Inline(raw_type)),
                                                },
                                            }
                                        } else {
                                            raw_type
                                        })
                                    })
                                })
                            })
                        {
                            content.insert(response.mime.clone(), OpenApiContent { schema });
                        }
                        let status = StatusCode::from_u16(response.code);
                        let description = response
                            .type_info
                            .as_ref()
                            .map(|t| t.name.clone())
                            .unwrap_or_else(|| status.map(|s| s.canonical_reason()).ok().flatten().map(|s| s.to_owned()).unwrap_or_default());
                        data.responses.insert(response.code.to_string(), OpenApiResponse { description, content });
                    }

                    if !self.doc.paths.contains_key(path.as_str()) {
                        self.doc.paths.insert(path.clone(), BTreeMap::new());
                    }
                    let path_map = self.doc.paths.get_mut(path.as_str()).expect("Should work because of previous statement");

                    path_map.insert(method, data);
                }
            }
        }
    }

    fn get_schema(&mut self, name: &str, full_path: Option<&str>, ty: OpenApiType, as_ref: bool) -> OpenApiSchema {
        if as_ref {
            self.get_schema_ref(name, full_path, ty)
        } else {
            OpenApiSchema::Inline(ty)
        }
    }

    fn get_schema_ref(&mut self, name: &str, full_path: Option<&str>, mut ty: OpenApiType) -> OpenApiSchema {
        let mut is_array = false;
        let mut min = None;
        let mut max = None;
        if let OpenApiType::Array { items, min_items, max_items } = &ty {
            is_array = true;
            min = *min_items;
            max = *max_items;
            match &**items {
                OpenApiSchema::Inline(t) => {
                    ty = t.clone();
                }
                OpenApiSchema::Ref { .. } => return OpenApiSchema::Inline(ty),
            }
        }

        let schema = if ty.is_primitive() {
            OpenApiSchema::Inline(ty)
        } else {
            let ref_name = if let Some(map) = self.generated_schema_names.get_mut(name) {
                if let Some(ref_name) = map.get(&full_path.map(|s| s.to_string())) {
                    ref_name.clone()
                } else {
                    let name = format!("{}_{}", name, map.len() + 1);
                    map.insert(full_path.map(|s| s.to_string()), name.clone());
                    name
                }
            } else {
                let mut map = HashMap::new();
                map.insert(full_path.map(|s| s.to_string()), name.to_string());
                self.generated_schema_names.insert(name.to_string(), map);
                name.to_string()
            };
            self.doc.components.schemas.insert(ref_name.clone(), OpenApiSchema::Inline(ty));
            OpenApiSchema::Ref {
                type_ref: format!("#/components/schemas/{}", ref_name.as_str()),
            }
        };
        if is_array {
            OpenApiSchema::Inline(OpenApiType::Array {
                items: Box::new(schema),
                min_items: min,
                max_items: max,
            })
        } else {
            schema
        }
    }

    fn get_open_api_body_param<'b>(&mut self, entrypoint: &'b Module<'b>, body_info: &mut BodyParamInfo) -> OpenApiRequestBody {
        let schema = if body_info.type_info.is_type_deserializable {
            let ty = &mut body_info.type_info;
            let name = ty.rename.as_deref().unwrap_or(ty.name.as_str());
            let as_ref = self.args.schema_granularity != SchemaGranularity::None;
            self.get_open_api_schema_from_type_info(entrypoint, ty, as_ref)
                .unwrap_or_else(|| self.get_schema(name, None, OpenApiType::anonymous_input_object(), as_ref))
        } else {
            OpenApiSchema::Inline(OpenApiType::anonymous_input_object())
        };
        let mut content: BTreeMap<OpenApiMimeType, OpenApiContent> = BTreeMap::new();
        content.insert(body_info.openapi_type.clone(), OpenApiContent { schema });
        OpenApiRequestBody {
            description: body_info.type_info.name.clone(),
            required: !body_info.type_info.is_optional,
            nullable: body_info.type_info.is_optional,
            content,
        }
    }

    fn get_open_api_parameters_from_body_info<'b>(&mut self, entrypoint: &'b Module<'b>, body_info: &BodyParamInfo) -> Vec<OpenApiParameter> {
        let mut parameters = Vec::new();
        let as_ref = self.args.schema_granularity != SchemaGranularity::None;
        if let Some(schema) = if body_info.type_info.is_type_deserializable {
            self.get_open_api_schema_from_type_info(entrypoint, &body_info.type_info, false)
        } else {
            None
        } {
            let t = match schema {
                OpenApiSchema::Inline(t) => t,
                _ => unreachable!(),
            };

            if let OpenApiType::Object {
                object:
                    OpenApiObjectType::Object {
                        properties,
                        required,
                        additional_properties: false,
                    },
            } = t
            {
                for (name, schema) in properties {
                    let schema = if as_ref {
                        let schema = *schema;
                        match schema {
                            OpenApiSchema::Inline(t) => self.get_schema_ref(body_info.type_info.name.as_str(), body_info.type_info.type_path.as_deref(), t),
                            r => r,
                        }
                    } else {
                        *schema
                    };

                    parameters.push(OpenApiParameter {
                        name: name.clone(),
                        location: OpenApiParameterLocation::Query,
                        required: required.contains(&name),
                        schema,
                        ..Default::default()
                    });
                }
            }
        }
        parameters
    }

    fn get_open_api_schema_from_type_info<'b>(&mut self, scope: &'b dyn UseScope<'b>, type_info: &TypeInfo, as_ref: bool) -> Option<OpenApiSchema> {
        let type_path = type_info.type_path.as_ref()?;
        let type_mod = scope.target().module_by_use_path(type_path).ok().flatten()?;
        let type_impl = type_mod.find_type_definition(type_info.name.as_str()).ok().flatten()?;
        let schema = match type_impl.item {
            SynItem::Struct(s) => self.get_open_api_type_from_struct(type_info.name.as_str(), type_impl, s, as_ref),
            SynItem::Enum(e) => self.get_open_api_type_from_enum(type_info.name.as_str(), type_impl, e, as_ref),
            _ => unreachable!(),
        };

        if type_info.is_array {
            Some(OpenApiSchema::Inline(OpenApiType::Array {
                items: Box::new(schema),
                min_items: type_info.min_array_len,
                max_items: type_info.max_array_len,
            }))
        } else if type_info.is_dictionary {
            Some(OpenApiSchema::Inline(OpenApiType::Object {
                object: OpenApiObjectType::Dictionary {
                    properties: Default::default(),
                    additional_properties: Box::new(schema),
                },
            }))
        } else {
            Some(schema)
        }
    }

    fn get_open_api_type_from_struct<'b>(&mut self, name: &str, item: &'b Item<'b>, s: &ItemStruct, as_ref: bool) -> OpenApiSchema {
        let mut properties = BTreeMap::new();
        let mut required = Vec::new();
        for field in &s.fields {
            if let Some(field_name) = field.ident.as_ref().and_then(|i| get_serde_field(i.to_string(), &field.attrs, &s.attrs)) {
                if let Some(field_type_info) = TypeInfo::new(item.scope, &field.ty) {
                    let field_as_ref = self.args.schema_granularity == SchemaGranularity::All && as_ref;
                    let field_schema = self
                        .get_open_api_schema_from_type_info(item.scope, &field_type_info, field_as_ref)
                        .unwrap_or_else(|| {
                            let type_name = field_type_info.rename.as_ref().unwrap_or(&field_type_info.name);
                            let raw_type = OpenApiType::from_rust_type_str(type_name.as_str()).unwrap_or_else(OpenApiType::string);
                            if field_type_info.is_array {
                                OpenApiSchema::Inline(OpenApiType::Array {
                                    items: Box::new(OpenApiSchema::Inline(raw_type)),
                                    min_items: field_type_info.min_array_len,
                                    max_items: field_type_info.max_array_len,
                                })
                            } else if field_type_info.is_dictionary {
                                OpenApiSchema::Inline(OpenApiType::Object {
                                    object: OpenApiObjectType::Dictionary {
                                        properties: Default::default(),
                                        additional_properties: Box::new(OpenApiSchema::Inline(raw_type)),
                                    },
                                })
                            } else {
                                OpenApiSchema::Inline(raw_type)
                            }
                        });
                    if !field_type_info.is_optional
                        && !find_macro_attribute_flag(&field.attrs, "serde", "default")
                        && find_macro_attribute_named_value(&field.attrs, "serde", "default").is_none()
                    {
                        required.push(field_name.clone());
                    }
                    properties.insert(field_name, Box::new(field_schema));
                } else {
                    println!("Unsupported type : {:?}", &field.ty);
                }
            }
        }

        let path = item.scope.path();
        let object = if !properties.is_empty() {
            OpenApiType::object(properties, required)
        } else {
            OpenApiType::anonymous_input_object()
        };
        self.get_schema(name, Some(path), object, as_ref)
    }

    fn get_open_api_type_from_enum<'b>(&mut self, name: &str, item: &Item<'b>, e: &'b ItemEnum, as_ref: bool) -> OpenApiSchema {
        let ty = if e.variants.iter().all(|v| v.fields == Fields::Unit) {
            let mut values: Vec<String> = Vec::new();
            for variant in &e.variants {
                if let Some(name) = get_serde_field(variant.ident.to_string(), &variant.attrs, &e.attrs) {
                    values.push(name);
                }
            }
            OpenApiType::enums(values)
        } else {
            // TODO: properly support tuple and struct enum variants.
            //       this will require the item param
            OpenApiType::anonymous_input_object()
        };

        let path = item.scope.path();
        self.get_schema(name, Some(path), ty, as_ref)
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

    pub(crate) fn openapitype_from_raw<'b>(&mut self, scope: &'b dyn UseScope<'b>, raw: &str) -> Option<AnonymousType> {
        self._openapitype_from_raw(scope, raw).map(|(schema, name, _)| AnonymousType { schema, name })
    }

    fn _openapitype_from_raw<'b>(&mut self, scope: &'b dyn UseScope<'b>, raw: &str) -> Option<(OpenApiSchema, Option<String>, usize)> {
        let raw = raw.trim();
        let len = raw.len();
        let mut chars = raw.chars();
        let first_char = chars.next()?;
        match first_char {
            '{' => {
                let mut cur_key: Option<&str> = None;
                let mut properties = BTreeMap::new();
                let mut required = Vec::new();

                let mut s = 1;
                let mut e = 1;
                for i in 1..len {
                    let char = chars.next()?;
                    if e > i {
                        continue;
                    } else {
                        e = i;
                    }
                    match char {
                        ':' => {
                            let key = &raw[s..e].trim();
                            cur_key = Some(key);
                            s = e + 1;
                        }
                        '{' | '[' => {
                            let (t, _, end) = self._openapitype_from_raw(scope, &raw[s..(len - 1)])?;
                            e += end + 1;
                            if let Some(key) = cur_key {
                                properties.insert(key.to_string(), Box::new(t));
                                required.push(key.to_string());
                                s = e + 1;
                                cur_key = None;
                            }
                        }
                        ',' | '}' => {
                            if let Some(key) = cur_key {
                                let value = &raw[s..e].trim();
                                let (t, ..) = self._openapitype_from_raw(scope, value)?;
                                properties.insert(key.to_string(), Box::new(t));
                                required.push(key.to_string());
                            }
                            s = e + 1;
                            if char == '}' {
                                return if !properties.is_empty() {
                                    Some((OpenApiSchema::Inline(OpenApiType::object(properties, required)), None, e))
                                } else {
                                    None
                                };
                            }
                        }
                        _ => {}
                    }
                }
                None
            }
            '[' => {
                if chars.last()? != ']' {
                    return None;
                }
                self._openapitype_from_raw(scope, &raw[1..(len - 1)]).map(|(t, name, size)| {
                    (
                        OpenApiSchema::Inline(OpenApiType::Array {
                            items: Box::new(t),
                            min_items: None,
                            max_items: None,
                        }),
                        name,
                        size,
                    )
                })
            }
            _ => syn::parse_str::<syn::Path>(raw)
                .ok()
                .and_then(|p| TypeInfo::new_from_path(scope, &p))
                .as_ref()
                .filter(|t| t.is_type_serializable)
                .and_then(|t| self.get_open_api_schema_from_type_info(scope, t, self.args.schema_granularity == SchemaGranularity::All))
                .or_else(|| OpenApiType::from_rust_type_str(raw).map(OpenApiSchema::Inline))
                .map(|schema| (schema, None, len)),
        }
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
