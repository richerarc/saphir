use std::collections::HashMap;
use serde_derive::{Serialize, Deserialize};
use http::StatusCode;

#[derive(Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OpenApiParameterLocation {
    Path,
    Query,
}
impl Default for OpenApiParameterLocation { fn default() -> Self { OpenApiParameterLocation::Path } }

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum OpenApiPathMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Any,
    Custom(String),
}
impl Default for OpenApiPathMethod { fn default() -> Self { OpenApiPathMethod::Any } }
impl OpenApiPathMethod {
    pub fn from_str(s: &str) -> Self {
        match s {
            "get" => OpenApiPathMethod::Get,
            "post" => OpenApiPathMethod::Post,
            "put" => OpenApiPathMethod::Put,
            "patch" => OpenApiPathMethod::Patch,
            "delete" => OpenApiPathMethod::Delete,
            "any" => OpenApiPathMethod::Any,
            s => OpenApiPathMethod::Custom(s.to_owned()),
        }
    }
}

#[derive(Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OpenApiType {
    String,
    Integer,
    Object,
}
impl Default for OpenApiType { fn default() -> Self { OpenApiType::Object } }


#[derive(Default, Serialize, Deserialize)]
pub struct OpenApi {
    #[serde(rename = "openapi")]
    pub(crate) openapi_version: String,
    pub(crate) info: OpenApiInfo,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) servers: Vec<OpenApiServer>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<OpenApiTag>,
    // Key: api path / method / definition
    pub(crate) paths: HashMap<String, HashMap<OpenApiPathMethod, OpenApiPath>>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OpenApiInfo {
    pub(crate) title: String,
    pub(crate) version: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OpenApiServer {
    pub(crate) url: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OpenApiTag {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiPath {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<OpenApiTag>,
    pub(crate) summary: String,
    pub(crate) operation_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) parameters: Vec<OpenApiParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_body: Option<OpenApiRequestBody>,
    #[serde(rename = "x-codegen-request-body-name", skip_serializing_if = "Option::is_none")]
    pub(crate) x_codegen_request_body_name: Option<String>,
    pub(crate) responses: HashMap<u16, OpenApiResponse>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OpenApiParameter {
    pub(crate) name: String,
    #[serde(rename = "in")]
    pub(crate) location: OpenApiParameterLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    pub(crate) required: bool,
    pub(crate) schema: OpenApiSchema,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OpenApiRequestBody {
    pub(crate) description: String,
    pub(crate) content: HashMap<String, OpenApiContent>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OpenApiContent {
    pub(crate) schema: OpenApiSchema,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OpenApiSchema {
    #[serde(rename = "type")]
    pub(crate) openapi_type: OpenApiType,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub(crate) properties: HashMap<String, OpenApiType>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct OpenApiResponse {
    pub(crate) description: String,
    pub(crate) content: HashMap<String, OpenApiContent>,
}