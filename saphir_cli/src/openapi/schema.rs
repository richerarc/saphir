use crate::openapi::generate::SchemaGranularity;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{
    de::{self, Visitor},
    Deserialize as ImplDeserialize, Deserializer, Serialize as ImplSerialize, Serializer,
};
use serde_derive::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeMap, fmt};

static VERSIONNED_TAG_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r".+_v\d+").expect("regex should be valid"));
static VERSION_TAG_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"v\d+").expect("regex should be valid"));

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum OpenApiParameterLocation {
    #[default]
    Path,
    Query,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Default)]
pub enum OpenApiMimeType {
    Json,
    Form,
    #[default]
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

impl Visitor<'_> for OpenApiMimeTypeVisitor {
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

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd, Default)]
#[serde(rename_all = "camelCase")]
pub enum OpenApiPathMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    #[default]
    Any,
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

    pub fn to_str(&self) -> &'static str {
        match self {
            OpenApiPathMethod::Get => "get",
            OpenApiPathMethod::Post => "post",
            OpenApiPathMethod::Put => "put",
            OpenApiPathMethod::Patch => "patch",
            OpenApiPathMethod::Delete => "delete",
            OpenApiPathMethod::Any => "any",
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
        properties: BTreeMap<String, Box<OpenApiSchema>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        required: Vec<String>,
        #[serde(rename = "additionalProperties", default)]
        additional_properties: bool,
    },
    Dictionary {
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        properties: BTreeMap<String, Box<OpenApiSchema>>,
        #[serde(rename = "additionalProperties")]
        additional_properties: Box<OpenApiSchema>,
    },
    Ref {
        #[serde(rename = "$ref")]
        schema_path: String,
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
pub enum OpenApiNumberFormat {
    Float,
    Double,
    Int32,
    Int64,
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
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<OpenApiNumberFormat>,
    },
    Integer {
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<OpenApiNumberFormat>,
    },
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
    pub fn is_primitive(&self, granularity: &SchemaGranularity) -> bool {
        match self {
            OpenApiType::Object { .. } | OpenApiType::Array { .. } => false,
            OpenApiType::String { enum_values } if !enum_values.is_empty() && *granularity == SchemaGranularity::All => false,
            _ => true,
        }
    }

    pub fn string() -> Self {
        OpenApiType::String { enum_values: Vec::default() }
    }

    pub fn enums(values: Vec<String>) -> Self {
        OpenApiType::String { enum_values: values }
    }

    pub fn object(properties: BTreeMap<String, Box<OpenApiSchema>>, required: Vec<String>) -> Self {
        OpenApiType::Object {
            object: OpenApiObjectType::Object {
                properties,
                required,
                additional_properties: false,
            },
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

    pub fn from_rust_type_str(s: &str) -> Option<OpenApiType> {
        match s {
            "u64" | "i64" => Some(OpenApiType::Integer {
                format: Some(OpenApiNumberFormat::Int64),
            }),
            "i32" => Some(OpenApiType::Integer {
                format: Some(OpenApiNumberFormat::Int32),
            }),
            "u8" | "u16" | "u32" | "u128" | "usize" | "i8" | "i16" | "i128" | "isize" => Some(OpenApiType::Integer { format: None }),
            "f32" => Some(OpenApiType::Number {
                format: Some(OpenApiNumberFormat::Float),
            }),
            "f64" => Some(OpenApiType::Number {
                format: Some(OpenApiNumberFormat::Double),
            }),
            "bool" | "Bool" | "Boolean" => Some(OpenApiType::Boolean),
            "string" | "String" => Some(OpenApiType::string()),
            _ => None,
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
    #[serde(skip_serializing_if = "OpenApiComponents::is_empty")]
    pub(crate) components: OpenApiComponents,
}

impl OpenApi {
    pub fn sort_and_dedup_tags(&mut self) {
        self.tags.sort_unstable();
        self.tags.dedup();
    }
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

impl OpenApiTag {
    fn type_ord(&self) -> u8 {
        if VERSION_TAG_REGEX.is_match(&self.name) {
            return 1;
        }
        if VERSIONNED_TAG_REGEX.is_match(&self.name) {
            return 2;
        }
        0
    }
}

impl Ord for OpenApiTag {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.type_ord();
        let b = other.type_ord();
        if a == b {
            self.name.cmp(&other.name)
        } else {
            a.cmp(&b)
        }
    }
}

impl PartialOrd for OpenApiTag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
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
    #[serde(rename = "x-operation-name")]
    pub(crate) operation_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) parameters: Vec<OpenApiParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_body: Option<OpenApiRequestBody>,
    #[serde(rename = "x-codegen-request-body-name", skip_serializing_if = "Option::is_none")]
    pub(crate) x_codegen_request_body_name: Option<String>,
    pub(crate) responses: BTreeMap<String, OpenApiResponse>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OpenApiParameter {
    pub(crate) name: String,
    #[serde(rename = "in")]
    pub(crate) location: OpenApiParameterLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    pub(crate) required: bool,
    pub(crate) nullable: bool,
    pub(crate) schema: OpenApiSchema,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenApiRequestBody {
    pub(crate) description: String,
    pub(crate) required: bool,
    pub(crate) nullable: bool,
    pub(crate) content: BTreeMap<OpenApiMimeType, OpenApiContent>,
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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) content: BTreeMap<OpenApiMimeType, OpenApiContent>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OpenApiComponents {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) schemas: BTreeMap<String, OpenApiSchema>,
}
impl OpenApiComponents {
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }
}
