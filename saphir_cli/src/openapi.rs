use serde_derive::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OpenApiParameterLocation {
    Path,
    Query,
}
impl Default for OpenApiParameterLocation {
    fn default() -> Self {
        OpenApiParameterLocation::Path
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum OpenApiMimeTypes {
    #[serde(rename = "application/json")]
    Json,
    #[serde(rename = "application/x-www-form-urlencoded")]
    Form,
    #[serde(rename = "*/*")]
    Any,
}
impl Default for OpenApiMimeTypes {
    fn default() -> Self {
        OpenApiMimeTypes::Any
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum OpenApiPathMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Any,
}
impl Default for OpenApiPathMethod {
    fn default() -> Self {
        OpenApiPathMethod::Any
    }
}
impl OpenApiPathMethod {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "get" => Some(OpenApiPathMethod::Get),
            "post" => Some(OpenApiPathMethod::Post),
            "put" => Some(OpenApiPathMethod::Put),
            "patch" => Some(OpenApiPathMethod::Patch),
            "delete" => Some(OpenApiPathMethod::Delete),
            "any" => Some(OpenApiPathMethod::Any),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum OpenApiType {
    String, // this includes dates and files
    Number,
    Integer,
    Boolean,
    Array {
        items: Box<OpenApiSchema>,
        #[serde(rename = "min_items", skip_serializing_if = "Option::is_none")]
        min_items: Option<u32>,
        #[serde(rename = "max_items", skip_serializing_if = "Option::is_none")]
        max_items: Option<u32>,
    },
    Object {
        properties: HashMap<String, Box<OpenApiType>>,
    }
}
impl Default for OpenApiType {
    fn default() -> Self {
        OpenApiType::String
    }
}
impl OpenApiType {
    pub fn from_rust_type_str(s: &str) -> OpenApiType {
        match s {
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" |
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => OpenApiType::Integer,
            "f32" | "f64" => OpenApiType::Number,
            "bool" | "Bool" | "Boolean" => OpenApiType::Boolean,
            _ => OpenApiType::String
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenApi {
    #[serde(rename = "openapi")]
    pub(crate) openapi_version: String,
    pub(crate) info: OpenApiInfo,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) servers: Vec<OpenApiServer>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<OpenApiTag>,
    // Key: api path / method / definition
    pub(crate) paths: BTreeMap<String, BTreeMap<OpenApiPathMethod, OpenApiPath>>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenApiInfo {
    pub(crate) title: String,
    pub(crate) version: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenApiServer {
    pub(crate) url: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenApiTag {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiPath {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<OpenApiTag>,
    pub(crate) summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    pub(crate) operation_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) parameters: Vec<OpenApiParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_body: Option<OpenApiRequestBody>,
    #[serde(rename = "x-codegen-request-body-name", skip_serializing_if = "Option::is_none")]
    pub(crate) x_codegen_request_body_name: Option<String>,
    pub(crate) responses: HashMap<u16, OpenApiResponse>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OpenApiParameter {
    pub(crate) name: String,
    #[serde(rename = "in")]
    pub(crate) location: OpenApiParameterLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    pub(crate) required: bool,
    pub(crate) schema: OpenApiSchema,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenApiRequestBody {
    pub(crate) description: String,
    pub(crate) content: HashMap<String, OpenApiContent>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OpenApiContent {
    pub(crate) schema: OpenApiSchema,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum OpenApiSchema {
    Ref {
        #[serde(rename = "$ref")]
        type_ref: String,
    },
    Inline(OpenApiType),
}
impl Default for OpenApiSchema {
    fn default() -> Self {
        OpenApiSchema::Inline(OpenApiType::default())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OpenApiResponse {
    pub(crate) description: String,
    pub(crate) content: HashMap<String, OpenApiContent>,
}
