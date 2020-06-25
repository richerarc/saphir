use serde::{
    de::{self, Visitor},
    Deserialize as ImplDeserialize, Deserializer, Serialize as ImplSerialize, Serializer,
};
use serde_derive::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
};

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

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum OpenApiMimeType {
    Json,
    Form,
    Any,
    Other(String),
}

impl From<String> for OpenApiMimeType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "json" | "application/json" => OpenApiMimeType::Json,
            "form" | "application/x-www-form-urlencoded" => OpenApiMimeType::Form,
            "any" | "*/*" => OpenApiMimeType::Any,
            _ => OpenApiMimeType::Other(s),
        }
    }
}

impl ImplSerialize for OpenApiMimeType {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            OpenApiMimeType::Json => "application/json",
            OpenApiMimeType::Form => "application/x-www-form-urlencoded",
            OpenApiMimeType::Any => "*/*",
            OpenApiMimeType::Other(s) => s.as_str(),
        })
    }
}

struct OpenApiMimeTypeVisitor;

impl<'de> Visitor<'de> for OpenApiMimeTypeVisitor {
    type Value = OpenApiMimeType;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string or str representing a mimetype")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(match value {
            "application/json" => OpenApiMimeType::Json,
            "application/x-www-form-urlencoded" => OpenApiMimeType::Form,
            "*/*" => OpenApiMimeType::Any,
            s => OpenApiMimeType::Other(s.to_owned()),
        })
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(match value.as_str() {
            "application/json" => OpenApiMimeType::Json,
            "application/x-www-form-urlencoded" => OpenApiMimeType::Form,
            "*/*" => OpenApiMimeType::Any,
            _ => OpenApiMimeType::Other(value),
        })
    }
}

impl<'de> ImplDeserialize<'de> for OpenApiMimeType {
    fn deserialize<D>(deserializer: D) -> Result<OpenApiMimeType, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(OpenApiMimeTypeVisitor)
    }
}

impl Default for OpenApiMimeType {
    fn default() -> Self {
        OpenApiMimeType::Any
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

fn serde_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum OpenApiObjectType {
    Object {
        properties: HashMap<String, Box<OpenApiType>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        required: Vec<String>,
    },
    Dictionary {
        #[serde(skip_serializing_if = "HashMap::is_empty")]
        properties: HashMap<String, Box<OpenApiType>>,
        #[serde(rename = "additionalProperties")]
        additional_properties: HashMap<String, Box<OpenApiType>>,
    },
    AnonymousInputObject {
        #[serde(rename = "additionalProperties", default = "serde_true")]
        additional_properties: bool,
    },
    AnonymousOutputObject,
}
impl Default for OpenApiObjectType {
    fn default() -> Self {
        OpenApiObjectType::AnonymousInputObject { additional_properties: true }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum OpenApiType {
    // this includes dates and files
    String {
        #[serde(rename = "enum", skip_serializing_if = "Vec::is_empty")]
        enum_values: Vec<String>,
    },
    Number,
    Integer,
    Boolean,
    Array {
        items: Box<OpenApiSchema>,
        #[serde(rename = "minItems", skip_serializing_if = "Option::is_none")]
        min_items: Option<u32>,
        #[serde(rename = "maxItems", skip_serializing_if = "Option::is_none")]
        max_items: Option<u32>,
    },
    Object {
        #[serde(flatten)]
        object: OpenApiObjectType,
    },
}
impl Default for OpenApiType {
    fn default() -> Self {
        Self::string()
    }
}
impl OpenApiType {
    pub fn string() -> Self {
        OpenApiType::String { enum_values: Vec::default() }
    }

    pub fn enums(values: Vec<String>) -> Self {
        OpenApiType::String { enum_values: values }
    }

    pub fn object(properties: HashMap<String, Box<OpenApiType>>, required: Vec<String>) -> Self {
        OpenApiType::Object {
            object: OpenApiObjectType::Object { properties, required },
        }
    }

    pub fn anonymous_input_object() -> Self {
        OpenApiType::Object {
            object: OpenApiObjectType::AnonymousInputObject { additional_properties: true },
        }
    }

    #[allow(dead_code)]
    pub fn anonymous_output_object() -> Self {
        OpenApiType::Object {
            object: OpenApiObjectType::AnonymousOutputObject,
        }
    }

    pub fn from_rust_type_str(s: &str) -> OpenApiType {
        match s {
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => OpenApiType::Integer,
            "f32" | "f64" => OpenApiType::Number,
            "bool" | "Bool" | "Boolean" => OpenApiType::Boolean,
            _ => OpenApiType::string(),
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
    #[serde(skip_serializing_if = "HashSet::is_empty")]
    pub(crate) tags: HashSet<OpenApiTag>,
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

#[derive(Clone, Default, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct OpenApiTag {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiPath {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<String>,
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
    pub(crate) responses: HashMap<String, OpenApiResponse>,
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
    pub(crate) required: bool,
    pub(crate) content: HashMap<OpenApiMimeType, OpenApiContent>,
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
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub(crate) content: HashMap<OpenApiMimeType, OpenApiContent>,
}
