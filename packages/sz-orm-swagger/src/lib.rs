//! # SZ-ORM Swagger — OpenAPI/Swagger 规范生成
//!
//! 提供 OpenAPI 3.0 规范的构建与序列化，支持路径、方法、参数、响应、
//! Schema 定义、SecurityScheme、Tags、Servers 与示例生成，输出可被
//! Swagger UI 等工具直接消费。
//!
//! ## 主要类型
//!
//! - [`OpenAPISpec`] — 规范根对象
//! - [`PathInfo`] — 单个 (path, method) 操作描述
//! - [`Schema`] / [`SchemaRef`] / [`ObjectType`] / [`ArrayType`] — Schema 定义
//! - [`SecurityScheme`] — 认证方案（Basic/Bearer/ApiKey/OAuth2）
//! - [`Tag`] — 接口分组标签
//! - [`Server`] — 环境服务器配置
//! - [`ExampleBuilder`] — 基于 Schema 生成示例

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// 根对象 OpenAPISpec
// ============================================================================

/// OpenAPI 3.0 规范根对象
///
/// 完整结构包含 paths、components（schemas/securitySchemes）、tags、servers、info。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAPISpec {
    pub openapi: String,
    pub info: serde_json::Value,
    pub paths: HashMap<String, serde_json::Value>,
    /// 组件表（schemas、securitySchemes 等）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
    /// 接口分组标签列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    /// 环境服务器列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,
    /// 全局安全要求
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<SecurityRequirement>,
}

impl OpenAPISpec {
    /// 序列化为格式化 JSON 字符串
    pub fn to_json_string(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// 组件表，存放可复用的 Schema 与 SecurityScheme
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Components {
    /// 可复用的 Schema 定义
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub schemas: HashMap<String, Schema>,
    /// 可复用的 SecurityScheme 定义
    #[serde(rename = "securitySchemes", default, skip_serializing_if = "HashMap::is_empty")]
    pub security_schemes: HashMap<String, SecurityScheme>,
}

/// 安全要求：所需认证方案的键值对映射
///
/// 例：`{"bearerAuth": []}` 表示需要 bearerAuth 方案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRequirement {
    #[serde(flatten)]
    pub requirements: HashMap<String, Vec<String>>,
}

impl SecurityRequirement {
    pub fn new(scheme: &str) -> Self {
        let mut requirements = HashMap::new();
        requirements.insert(scheme.to_string(), vec![]);
        Self { requirements }
    }

    pub fn with_scopes(mut self, scheme: &str, scopes: Vec<String>) -> Self {
        self.requirements.insert(scheme.to_string(), scopes);
        self
    }
}

// ============================================================================
// Path 描述
// ============================================================================

/// Describes a single (path, method) operation in an OpenAPI spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathInfo {
    pub method: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,
    pub responses: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<SecurityRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
}

impl PathInfo {
    pub fn new(method: &str, summary: &str) -> Self {
        Self {
            method: method.to_string(),
            summary: summary.to_string(),
            description: None,
            tags: vec![],
            parameters: vec![],
            request_body: None,
            responses: HashMap::new(),
            security: vec![],
            operation_id: None,
            deprecated: None,
        }
    }

    pub fn with_response(mut self, code: &str, desc: &str) -> Self {
        self.responses
            .insert(code.to_string(), serde_json::json!({ "description": desc }));
        self
    }

    /// 添加带 Schema 的响应
    pub fn with_response_schema(mut self, code: &str, desc: &str, schema_ref: &str) -> Self {
        self.responses.insert(
            code.to_string(),
            serde_json::json!({
                "description": desc,
                "content": {
                    "application/json": {
                        "schema": { "$ref": schema_ref }
                    }
                }
            }),
        );
        self
    }

    pub fn with_parameter(mut self, param: serde_json::Value) -> Self {
        self.parameters.push(param);
        self
    }

    /// 添加路径参数
    pub fn with_path_param(self, name: &str, desc: &str, required: bool) -> Self {
        self.with_parameter(serde_json::json!({
            "name": name,
            "in": "path",
            "description": desc,
            "required": required,
            "schema": { "type": "string" }
        }))
    }

    /// 添加查询参数
    pub fn with_query_param(self, name: &str, desc: &str, required: bool) -> Self {
        self.with_parameter(serde_json::json!({
            "name": name,
            "in": "query",
            "description": desc,
            "required": required,
            "schema": { "type": "string" }
        }))
    }

    /// 添加标签
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// 设置请求体
    pub fn with_request_body(mut self, body: RequestBody) -> Self {
        self.request_body = Some(body);
        self
    }

    /// 设置请求体（简化版：引用 Schema）
    pub fn with_request_body_ref(self, desc: &str, schema_ref: &str, required: bool) -> Self {
        self.with_request_body(RequestBody::new(desc, schema_ref, required))
    }

    /// 设置操作 ID
    pub fn with_operation_id(mut self, id: &str) -> Self {
        self.operation_id = Some(id.to_string());
        self
    }

    /// 设置安全要求
    pub fn with_security(mut self, req: SecurityRequirement) -> Self {
        self.security.push(req);
        self
    }

    /// 标记为已弃用
    pub fn deprecated(mut self) -> Self {
        self.deprecated = Some(true);
        self
    }

    /// 设置描述
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}

/// 请求体定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    pub description: String,
    pub content: HashMap<String, MediaType>,
    pub required: bool,
}

impl RequestBody {
    pub fn new(desc: &str, schema_ref: &str, required: bool) -> Self {
        let mut content = HashMap::new();
        content.insert(
            "application/json".to_string(),
            MediaType::with_schema_ref(schema_ref),
        );
        Self {
            description: desc.to_string(),
            content,
            required,
        }
    }

    pub fn with_content(mut self, content_type: &str, media: MediaType) -> Self {
        self.content.insert(content_type.to_string(), media);
        self
    }
}

/// 媒体类型（包含 Schema 与示例）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub examples: HashMap<String, Example>,
}

impl MediaType {
    pub fn with_schema_ref(schema_ref: &str) -> Self {
        Self {
            schema: Some(Schema::ref_to(schema_ref)),
            example: None,
            examples: HashMap::new(),
        }
    }

    pub fn with_schema(mut self, schema: Schema) -> Self {
        self.schema = Some(schema);
        self
    }

    pub fn with_example(mut self, example: serde_json::Value) -> Self {
        self.example = Some(example);
        self
    }
}

/// 示例（OpenAPI 3.0 example 对象）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_value: Option<String>,
}

impl Example {
    pub fn new(value: serde_json::Value) -> Self {
        Self {
            summary: None,
            description: None,
            value: Some(value),
            external_value: None,
        }
    }

    pub fn with_summary(mut self, summary: &str) -> Self {
        self.summary = Some(summary.to_string());
        self
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}

// ============================================================================
// Schema 定义（SchemaRef / ObjectType / ArrayType）
// ============================================================================

/// Schema 定义，支持引用、对象、数组、基本类型
///
/// - `Ref`：通过 `$ref` 引用 components.schemas 中定义的 Schema
/// - `Object`：对象类型，包含 properties 和 required
/// - `Array`：数组类型，包含 items
/// - `Primitive`：基本类型（string/integer/number/boolean）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Schema {
    /// 引用其他已定义的 Schema
    Ref {
        #[serde(rename = "$ref")]
        ref_path: String,
    },
    /// 对象类型
    Object(ObjectType),
    /// 数组类型
    Array(ArrayType),
    /// 基本类型（string/integer/number/boolean）
    Primitive(PrimitiveSchema),
}

impl Schema {
    /// 创建一个引用 Schema
    pub fn ref_to(name: &str) -> Self {
        Self::Ref {
            ref_path: format!("#/components/schemas/{}", name),
        }
    }

    /// 创建一个对象 Schema
    pub fn object(obj: ObjectType) -> Self {
        Self::Object(obj)
    }

    /// 创建一个数组 Schema
    pub fn array(arr: ArrayType) -> Self {
        Self::Array(arr)
    }

    /// 创建一个字符串 Schema
    pub fn string() -> Self {
        Self::Primitive(PrimitiveSchema::string())
    }

    /// 创建一个整数 Schema
    pub fn integer() -> Self {
        Self::Primitive(PrimitiveSchema::integer())
    }

    /// 创建一个数字 Schema
    pub fn number() -> Self {
        Self::Primitive(PrimitiveSchema::number())
    }

    /// 创建一个布尔 Schema
    pub fn boolean() -> Self {
        Self::Primitive(PrimitiveSchema::boolean())
    }

    /// 判断是否为引用类型
    pub fn is_ref(&self) -> bool {
        matches!(self, Schema::Ref { .. })
    }

    /// 判断是否为对象类型
    pub fn is_object(&self) -> bool {
        matches!(self, Schema::Object(_))
    }

    /// 判断是否为数组类型
    pub fn is_array(&self) -> bool {
        matches!(self, Schema::Array(_))
    }

    /// 设置 format（仅对 Primitive 类型有效，其他类型返回原值）
    pub fn with_format(self, format: &str) -> Self {
        match self {
            Self::Primitive(p) => Self::Primitive(p.with_format(format)),
            _ => self,
        }
    }

    /// 设置描述（仅对 Primitive 类型有效）
    pub fn with_description(self, desc: &str) -> Self {
        match self {
            Self::Primitive(p) => Self::Primitive(p.with_description(desc)),
            _ => self,
        }
    }

    /// 设置示例值（仅对 Primitive 类型有效）
    pub fn with_example(self, value: serde_json::Value) -> Self {
        match self {
            Self::Primitive(p) => Self::Primitive(p.with_example(value)),
            _ => self,
        }
    }

    /// 设置默认值（仅对 Primitive 类型有效）
    pub fn with_default(self, value: serde_json::Value) -> Self {
        match self {
            Self::Primitive(p) => Self::Primitive(p.with_default(value)),
            _ => self,
        }
    }
}

/// 对象类型 Schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectType {
    #[serde(rename = "type")]
    pub schema_type: String,
    /// 字段定义：name -> Schema
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, Schema>,
    /// 必填字段列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<Box<Schema>>,
}

impl ObjectType {
    pub fn new() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: HashMap::new(),
            required: vec![],
            description: None,
            additional_properties: None,
        }
    }

    /// 添加字段（可选）
    pub fn with_property(mut self, name: &str, schema: Schema) -> Self {
        self.properties.insert(name.to_string(), schema);
        self
    }

    /// 添加必填字段
    pub fn with_required_property(mut self, name: &str, schema: Schema) -> Self {
        self.properties.insert(name.to_string(), schema);
        self.required.push(name.to_string());
        self
    }

    /// 设置描述
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// 设置额外属性（用于 Map 类型）
    pub fn with_additional_properties(mut self, schema: Schema) -> Self {
        self.additional_properties = Some(Box::new(schema));
        self
    }
}

impl Default for ObjectType {
    fn default() -> Self {
        Self::new()
    }
}

/// 数组类型 Schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayType {
    #[serde(rename = "type")]
    pub schema_type: String,
    /// 数组元素 Schema
    pub items: Box<Schema>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_items: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ArrayType {
    pub fn new(items: Schema) -> Self {
        Self {
            schema_type: "array".to_string(),
            items: Box::new(items),
            min_items: None,
            max_items: None,
            unique_items: None,
            description: None,
        }
    }

    pub fn with_min_items(mut self, min: u32) -> Self {
        self.min_items = Some(min);
        self
    }

    pub fn with_max_items(mut self, max: u32) -> Self {
        self.max_items = Some(max);
        self
    }

    pub fn unique_items(mut self) -> Self {
        self.unique_items = Some(true);
        self
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}

/// 基本类型 Schema（string/integer/number/boolean）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimitiveSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

impl PrimitiveSchema {
    pub fn string() -> Self {
        Self {
            schema_type: "string".to_string(),
            format: None,
            description: None,
            default: None,
            example: None,
            enum_values: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
        }
    }

    pub fn integer() -> Self {
        Self {
            schema_type: "integer".to_string(),
            format: Some("int64".to_string()),
            description: None,
            default: None,
            example: None,
            enum_values: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
        }
    }

    pub fn number() -> Self {
        Self {
            schema_type: "number".to_string(),
            format: Some("double".to_string()),
            description: None,
            default: None,
            example: None,
            enum_values: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
        }
    }

    pub fn boolean() -> Self {
        Self {
            schema_type: "boolean".to_string(),
            format: None,
            description: None,
            default: None,
            example: None,
            enum_values: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
        }
    }

    pub fn with_format(mut self, format: &str) -> Self {
        self.format = Some(format.to_string());
        self
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default = Some(value);
        self
    }

    pub fn with_example(mut self, value: serde_json::Value) -> Self {
        self.example = Some(value);
        self
    }

    pub fn with_enum(mut self, values: Vec<serde_json::Value>) -> Self {
        self.enum_values = Some(values);
        self
    }

    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.minimum = Some(min);
        self.maximum = Some(max);
        self
    }

    pub fn with_length_range(mut self, min: u32, max: u32) -> Self {
        self.min_length = Some(min);
        self.max_length = Some(max);
        self
    }

    pub fn with_pattern(mut self, pattern: &str) -> Self {
        self.pattern = Some(pattern.to_string());
        self
    }
}

// ============================================================================
// SecurityScheme 认证方案
// ============================================================================

/// 认证方案，支持 HTTP（Basic/Bearer）、ApiKey、OAuth2
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SecurityScheme {
    /// HTTP 认证（Basic 或 Bearer）
    ///
    /// OpenAPI 3.0 中 Basic 和 Bearer 都使用 `type: http`，
    /// 通过 `scheme` 字段区分（`basic` 或 `bearer`）。
    #[serde(rename = "http")]
    Http {
        /// 认证方案：`basic` 或 `bearer`
        scheme: String,
        /// Bearer 格式（仅 scheme=bearer 时使用，如 `JWT`）
        #[serde(rename = "bearerFormat", default, skip_serializing_if = "Option::is_none")]
        bearer_format: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// API Key 认证（Header / Query / Cookie）
    #[serde(rename = "apiKey")]
    ApiKey {
        name: String,
        #[serde(rename = "in")]
        location: ApiKeyLocation,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// OAuth2 认证
    #[serde(rename = "oauth2")]
    OAuth2 {
        flows: Box<OAuth2Flows>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

impl SecurityScheme {
    /// 创建 Basic 认证方案
    pub fn basic() -> Self {
        Self::Http {
            scheme: "basic".to_string(),
            bearer_format: None,
            description: None,
        }
    }

    /// 创建 Bearer Token 认证方案
    pub fn bearer(jwt_format: bool) -> Self {
        Self::Http {
            scheme: "bearer".to_string(),
            bearer_format: if jwt_format {
                Some("JWT".to_string())
            } else {
                None
            },
            description: None,
        }
    }

    /// 创建 API Key 认证方案
    pub fn api_key(name: &str, location: ApiKeyLocation) -> Self {
        Self::ApiKey {
            name: name.to_string(),
            location,
            description: None,
        }
    }

    /// 创建 OAuth2 认证方案
    pub fn oauth2(flows: OAuth2Flows) -> Self {
        Self::OAuth2 {
            flows: Box::new(flows),
            description: None,
        }
    }

    /// 判断是否为 Basic 认证
    pub fn is_basic(&self) -> bool {
        matches!(self, Self::Http { scheme, .. } if scheme == "basic")
    }

    /// 判断是否为 Bearer 认证
    pub fn is_bearer(&self) -> bool {
        matches!(self, Self::Http { scheme, .. } if scheme == "bearer")
    }

    /// 设置描述
    pub fn with_description(self, desc: &str) -> Self {
        match self {
            Self::Http {
                scheme,
                bearer_format,
                ..
            } => Self::Http {
                scheme,
                bearer_format,
                description: Some(desc.to_string()),
            },
            Self::ApiKey {
                name,
                location,
                ..
            } => Self::ApiKey {
                name,
                location,
                description: Some(desc.to_string()),
            },
            Self::OAuth2 { flows, .. } => Self::OAuth2 {
                flows,
                description: Some(desc.to_string()),
            },
        }
    }
}

/// API Key 位置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    Query,
    Header,
    Cookie,
}

/// OAuth2 流程集合
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuth2Flows {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implicit: Option<ImplicitFlow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordFlow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_credentials: Option<ClientCredentialsFlow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_code: Option<AuthorizationCodeFlow>,
}

impl OAuth2Flows {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_implicit(mut self, flow: ImplicitFlow) -> Self {
        self.implicit = Some(flow);
        self
    }

    pub fn with_password(mut self, flow: PasswordFlow) -> Self {
        self.password = Some(flow);
        self
    }

    pub fn with_client_credentials(mut self, flow: ClientCredentialsFlow) -> Self {
        self.client_credentials = Some(flow);
        self
    }

    pub fn with_authorization_code(mut self, flow: AuthorizationCodeFlow) -> Self {
        self.authorization_code = Some(flow);
        self
    }
}

/// Implicit 流程
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplicitFlow {
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
    #[serde(rename = "refreshUrl", default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    pub scopes: HashMap<String, String>,
}

/// Password 流程
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordFlow {
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    #[serde(rename = "refreshUrl", default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    pub scopes: HashMap<String, String>,
}

/// Client Credentials 流程
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCredentialsFlow {
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    #[serde(rename = "refreshUrl", default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    pub scopes: HashMap<String, String>,
}

/// Authorization Code 流程
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationCodeFlow {
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    #[serde(rename = "refreshUrl", default, skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
    pub scopes: HashMap<String, String>,
}

// ============================================================================
// Tags 接口分组
// ============================================================================

/// 接口分组标签
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocs>,
}

impl Tag {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            external_docs: None,
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn with_external_docs(mut self, docs: ExternalDocs) -> Self {
        self.external_docs = Some(docs);
        self
    }
}

/// 外部文档链接
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDocs {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ExternalDocs {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            description: None,
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}

// ============================================================================
// Servers 环境配置
// ============================================================================

/// 环境服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, ServerVariable>,
}

impl Server {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            description: None,
            variables: HashMap::new(),
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn with_variable(mut self, name: &str, var: ServerVariable) -> Self {
        self.variables.insert(name.to_string(), var);
        self
    }
}

/// 服务器变量（用于 URL 模板替换）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerVariable {
    #[serde(rename = "default")]
    pub default_value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<String>,
}

impl ServerVariable {
    pub fn new(default: &str) -> Self {
        Self {
            default_value: default.to_string(),
            description: None,
            enum_values: vec![],
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn with_enum(mut self, values: Vec<String>) -> Self {
        self.enum_values = values;
        self
    }
}

// ============================================================================
// 示例生成器（基于 Schema 自动生成示例值）
// ============================================================================

/// 基于 Schema 生成示例值的生成器
pub struct ExampleBuilder;

impl ExampleBuilder {
    /// 根据 Schema 生成示例值
    ///
    /// - 对象：递归生成所有字段的示例
    /// - 数组：生成单元素数组
    /// - 引用：返回 null（需调用方解析引用）
    /// - 基本类型：根据 type/format 生成合理的默认值
    pub fn from_schema(schema: &Schema) -> serde_json::Value {
        match schema {
            Schema::Ref { .. } => serde_json::Value::Null,
            Schema::Object(obj) => Self::from_object(obj),
            Schema::Array(arr) => {
                let item = Self::from_schema(&arr.items);
                serde_json::Value::Array(vec![item])
            }
            Schema::Primitive(p) => Self::from_primitive(p),
        }
    }

    /// 根据对象 Schema 生成示例对象
    pub fn from_object(obj: &ObjectType) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (name, schema) in &obj.properties {
            map.insert(name.clone(), Self::from_schema(schema));
        }
        serde_json::Value::Object(map)
    }

    /// 根据基本类型 Schema 生成示例值
    pub fn from_primitive(p: &PrimitiveSchema) -> serde_json::Value {
        // 优先返回 example 字段
        if let Some(example) = &p.example {
            return example.clone();
        }
        // 其次返回 default 字段
        if let Some(default) = &p.default {
            return default.clone();
        }
        // 再次返回 enum 的第一个值
        if let Some(enum_vals) = &p.enum_values {
            if let Some(first) = enum_vals.first() {
                return first.clone();
            }
        }
        // 根据 type 生成默认示例
        match p.schema_type.as_str() {
            "string" => match p.format.as_deref() {
                Some("date") => serde_json::Value::String("2026-01-01".to_string()),
                Some("date-time") => {
                    serde_json::Value::String("2026-01-01T00:00:00Z".to_string())
                }
                Some("email") => {
                    serde_json::Value::String("user@example.com".to_string())
                }
                Some("uuid") => serde_json::Value::String(
                    "550e8400-e29b-41d4-a716-446655440000".to_string(),
                ),
                Some("uri") => serde_json::Value::String("https://example.com".to_string()),
                _ => serde_json::Value::String("string".to_string()),
            },
            "integer" => serde_json::Value::Number(
                serde_json::Number::from(p.minimum.unwrap_or(0.0) as i64),
            ),
            "number" => serde_json::json!(p.minimum.unwrap_or(0.0)),
            "boolean" => serde_json::Value::Bool(false),
            _ => serde_json::Value::Null,
        }
    }
}

// ============================================================================
// OpenAPIGenerator 生成器
// ============================================================================

pub struct OpenAPIGenerator {
    paths: Vec<(String, PathInfo)>,
    info: serde_json::Value,
    schemas: HashMap<String, Schema>,
    security_schemes: HashMap<String, SecurityScheme>,
    tags: Vec<Tag>,
    servers: Vec<Server>,
    security: Vec<SecurityRequirement>,
}

impl OpenAPIGenerator {
    pub fn new() -> Self {
        Self {
            paths: vec![],
            info: serde_json::json!({
                "title": "API",
                "version": "1.0.0",
                "description": "Generated by sz-orm-swagger"
            }),
            schemas: HashMap::new(),
            security_schemes: HashMap::new(),
            tags: vec![],
            servers: vec![],
            security: vec![],
        }
    }

    pub fn with_info(mut self, info: serde_json::Value) -> Self {
        self.info = info;
        self
    }

    /// 注册 (path, method) 操作。同一 path 的多个 method 会合并到同一个 paths 条目。
    pub fn register_path(&mut self, path: &str, info: PathInfo) -> &mut Self {
        self.paths.push((path.to_string(), info));
        self
    }

    /// 注册可复用的 Schema 定义
    pub fn register_schema(&mut self, name: &str, schema: Schema) -> &mut Self {
        self.schemas.insert(name.to_string(), schema);
        self
    }

    /// 注册可复用的 SecurityScheme
    pub fn register_security_scheme(&mut self, name: &str, scheme: SecurityScheme) -> &mut Self {
        self.security_schemes.insert(name.to_string(), scheme);
        self
    }

    /// 添加接口分组标签
    pub fn add_tag(&mut self, tag: Tag) -> &mut Self {
        self.tags.push(tag);
        self
    }

    /// 添加环境服务器
    pub fn add_server(&mut self, server: Server) -> &mut Self {
        self.servers.push(server);
        self
    }

    /// 设置全局安全要求
    pub fn with_global_security(mut self, req: SecurityRequirement) -> Self {
        self.security.push(req);
        self
    }

    /// 生成 OpenAPISpec
    pub fn generate(&self) -> OpenAPISpec {
        let mut paths: HashMap<String, serde_json::Value> = HashMap::new();
        for (path, info) in &self.paths {
            let method = info.method.to_lowercase();
            let entry = paths
                .entry(path.clone())
                .or_insert_with(|| serde_json::json!({}));
            let mut op = serde_json::json!({
                "summary": info.summary,
                "parameters": info.parameters,
                "responses": info.responses
            });
            if !info.tags.is_empty() {
                op["tags"] = serde_json::json!(info.tags);
            }
            if let Some(desc) = &info.description {
                op["description"] = serde_json::json!(desc);
            }
            if let Some(body) = &info.request_body {
                op["requestBody"] = serde_json::json!(body);
            }
            if !info.security.is_empty() {
                op["security"] = serde_json::json!(info.security);
            }
            if let Some(op_id) = &info.operation_id {
                op["operationId"] = serde_json::json!(op_id);
            }
            if let Some(deprecated) = info.deprecated {
                op["deprecated"] = serde_json::json!(deprecated);
            }
            entry[method] = op;
        }

        let components = if self.schemas.is_empty() && self.security_schemes.is_empty() {
            None
        } else {
            Some(Components {
                schemas: self.schemas.clone(),
                security_schemes: self.security_schemes.clone(),
            })
        };

        OpenAPISpec {
            openapi: "3.0.0".to_string(),
            paths,
            info: self.info.clone(),
            components,
            tags: self.tags.clone(),
            servers: self.servers.clone(),
            security: self.security.clone(),
        }
    }
}

impl Default for OpenAPIGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SwaggerUi 渲染器
// ============================================================================

pub struct SwaggerUi {
    mount_path: String,
    spec: Option<OpenAPISpec>,
}

impl SwaggerUi {
    pub fn new(path: &str) -> Self {
        Self {
            mount_path: path.to_string(),
            spec: None,
        }
    }

    pub fn with_spec(mut self, spec: OpenAPISpec) -> Self {
        self.spec = Some(spec);
        self
    }

    pub fn mount(&self) -> String {
        format!("{}docs", self.mount_path)
    }

    /// 渲染自包含的 Swagger UI HTML 页面，内嵌规范 JSON
    pub fn render_html(&self) -> String {
        let spec_json = match &self.spec {
            Some(s) => s.to_json_string(),
            None => serde_json::json!({
                "openapi": "3.0.0",
                "info": { "title": "API", "version": "1.0.0" },
                "paths": {}
            })
            .to_string(),
        };
        let mount = self.mount();
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>Swagger UI</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@4.19.0/swagger-ui.css">
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@4.19.0/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@4.19.0/swagger-ui-standalone-preset.js"></script>
    <script>
        const spec = {spec};
        window.onload = () => {{
            SwaggerUIBundle({{
                spec: spec,
                dom_id: '#swagger-ui',
                url: '{mount}/openapi.json',
                presets: [SwaggerUIBundle.presets.apis, SwaggerUIStandalonePreset],
                layout: 'StandaloneLayout'
            }});
        }};
    </script>
</body>
</html>"#,
            spec = spec_json,
            mount = mount
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_empty_has_no_paths() {
        let s = OpenAPIGenerator::new().generate();
        assert!(s.paths.is_empty());
        assert_eq!(s.info["title"], "API");
        assert_eq!(s.openapi, "3.0.0");
    }

    #[test]
    fn test_register_and_generate_single_path() {
        let mut g = OpenAPIGenerator::new();
        g.register_path(
            "/users",
            PathInfo::new("GET", "List users").with_response("200", "OK"),
        );
        let spec = g.generate();
        let users = spec.paths.get("/users").expect("/users should exist");
        let get = users.get("get").expect("GET method should exist");
        assert_eq!(get["summary"], "List users");
        assert!(get["responses"]["200"].is_object());
    }

    #[test]
    fn test_register_multiple_methods_same_path() {
        let mut g = OpenAPIGenerator::new();
        g.register_path(
            "/users",
            PathInfo::new("GET", "List users").with_response("200", "OK"),
        );
        g.register_path(
            "/users",
            PathInfo::new("POST", "Create user").with_response("201", "Created"),
        );
        let spec = g.generate();
        assert_eq!(spec.paths.len(), 1, "only one /users path key");
        let users = spec.paths.get("/users").unwrap();
        assert!(users.get("get").is_some());
        assert!(users.get("post").is_some());
        assert_eq!(users["get"]["summary"], "List users");
        assert_eq!(users["post"]["summary"], "Create user");
        assert_eq!(users["post"]["responses"]["201"]["description"], "Created");
    }

    #[test]
    fn test_register_multiple_paths() {
        let mut g = OpenAPIGenerator::new();
        g.register_path("/users", PathInfo::new("GET", "List users"));
        g.register_path("/orders", PathInfo::new("GET", "List orders"));
        g.register_path(
            "/items/{id}",
            PathInfo::new("GET", "Get item").with_response("404", "Not found"),
        );
        let spec = g.generate();
        assert_eq!(spec.paths.len(), 3);
        assert!(spec.paths.contains_key("/users"));
        assert!(spec.paths.contains_key("/orders"));
        assert!(spec.paths.contains_key("/items/{id}"));
    }

    #[test]
    fn test_ui_mount() {
        let ui = SwaggerUi::new("/api");
        assert_eq!(ui.mount(), "/apidocs");
    }

    #[test]
    fn test_ui_html_contains_cdn_and_bundle() {
        let ui = SwaggerUi::new("/api").with_spec(OpenAPIGenerator::new().generate());
        let html = ui.render_html();
        assert!(html.contains("swagger-ui-dist"));
        assert!(html.contains("swagger-ui.css"));
        assert!(html.contains("swagger-ui-bundle.js"));
        assert!(html.contains("SwaggerUIBundle"));
        assert!(html.contains("id=\"swagger-ui\""));
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_ui_html_embeds_spec_content() {
        let mut g = OpenAPIGenerator::new();
        g.register_path(
            "/items",
            PathInfo::new("GET", "List items").with_response("200", "OK"),
        );
        let ui = SwaggerUi::new("/api").with_spec(g.generate());
        let html = ui.render_html();
        assert!(html.contains("/items"));
        assert!(html.contains("List items"));
        assert!(html.contains("\"get\""));
    }

    #[test]
    fn test_ui_html_without_spec_uses_default_spec() {
        let ui = SwaggerUi::new("/api");
        let html = ui.render_html();
        assert!(html.contains("swagger-ui"));
        assert!(html.contains("\"openapi\""));
    }

    #[test]
    fn test_spec_to_json_string_is_valid_json() {
        let mut g = OpenAPIGenerator::new();
        g.register_path("/users", PathInfo::new("GET", "List users"));
        let spec = g.generate();
        let json = spec.to_json_string();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");
        assert!(parsed["paths"]["/users"]["get"].is_object());
        assert_eq!(parsed["openapi"], "3.0.0");
    }

    #[test]
    fn test_path_info_builder() {
        let p = PathInfo::new("PUT", "Update user")
            .with_response("200", "OK")
            .with_response("404", "Not found")
            .with_parameter(serde_json::json!({"name": "id", "in": "path"}));
        assert_eq!(p.method, "PUT");
        assert_eq!(p.responses.len(), 2);
        assert_eq!(p.parameters.len(), 1);
    }

    // ===== Schema 定义测试 =====

    #[test]
    fn test_schema_ref_to() {
        let s = Schema::ref_to("User");
        match &s {
            Schema::Ref { ref_path } => {
                assert_eq!(ref_path, "#/components/schemas/User");
            }
            _ => panic!("Expected Ref variant"),
        }
        assert!(s.is_ref());
        assert!(!s.is_object());
        assert!(!s.is_array());
    }

    #[test]
    fn test_schema_primitive_string() {
        let s = Schema::string();
        match &s {
            Schema::Primitive(p) => {
                assert_eq!(p.schema_type, "string");
            }
            _ => panic!("Expected Primitive variant"),
        }
    }

    #[test]
    fn test_schema_primitive_integer_with_format() {
        let s = Schema::integer();
        match &s {
            Schema::Primitive(p) => {
                assert_eq!(p.schema_type, "integer");
                assert_eq!(p.format.as_deref(), Some("int64"));
            }
            _ => panic!("Expected Primitive variant"),
        }
    }

    #[test]
    fn test_schema_primitive_number() {
        let s = Schema::number();
        match &s {
            Schema::Primitive(p) => {
                assert_eq!(p.schema_type, "number");
                assert_eq!(p.format.as_deref(), Some("double"));
            }
            _ => panic!("Expected Primitive variant"),
        }
    }

    #[test]
    fn test_schema_primitive_boolean() {
        let s = Schema::boolean();
        match &s {
            Schema::Primitive(p) => {
                assert_eq!(p.schema_type, "boolean");
            }
            _ => panic!("Expected Primitive variant"),
        }
    }

    #[test]
    fn test_object_type_builder() {
        let obj = ObjectType::new()
            .with_required_property("id", Schema::integer())
            .with_property("name", Schema::string())
            .with_description("User object");
        assert_eq!(obj.schema_type, "object");
        assert_eq!(obj.properties.len(), 2);
        assert!(obj.required.contains(&"id".to_string()));
        assert!(!obj.required.contains(&"name".to_string()));
        assert_eq!(obj.description.as_deref(), Some("User object"));
    }

    #[test]
    fn test_object_type_additional_properties() {
        let obj = ObjectType::new()
            .with_description("String map")
            .with_additional_properties(Schema::string());
        assert!(obj.additional_properties.is_some());
    }

    #[test]
    fn test_array_type_builder() {
        let arr = ArrayType::new(Schema::string())
            .with_min_items(1)
            .with_max_items(100)
            .unique_items()
            .with_description("List of names");
        assert_eq!(arr.schema_type, "array");
        assert_eq!(arr.min_items, Some(1));
        assert_eq!(arr.max_items, Some(100));
        assert_eq!(arr.unique_items, Some(true));
        assert_eq!(arr.description.as_deref(), Some("List of names"));
    }

    #[test]
    fn test_schema_is_object_array() {
        let obj_schema = Schema::object(ObjectType::new());
        assert!(obj_schema.is_object());
        assert!(!obj_schema.is_array());

        let arr_schema = Schema::array(ArrayType::new(Schema::string()));
        assert!(!arr_schema.is_object());
        assert!(arr_schema.is_array());
    }

    #[test]
    fn test_primitive_schema_with_format() {
        let p = PrimitiveSchema::string()
            .with_format("email")
            .with_description("Email address");
        assert_eq!(p.format.as_deref(), Some("email"));
        assert_eq!(p.description.as_deref(), Some("Email address"));
    }

    #[test]
    fn test_primitive_schema_with_default() {
        let p = PrimitiveSchema::integer().with_default(serde_json::json!(42));
        assert_eq!(p.default, Some(serde_json::json!(42)));
    }

    #[test]
    fn test_primitive_schema_with_example() {
        let p = PrimitiveSchema::string().with_example(serde_json::json!("John Doe"));
        assert_eq!(p.example, Some(serde_json::json!("John Doe")));
    }

    #[test]
    fn test_primitive_schema_with_enum() {
        let p = PrimitiveSchema::string().with_enum(vec![
            serde_json::json!("active"),
            serde_json::json!("inactive"),
        ]);
        assert!(p.enum_values.is_some());
        assert_eq!(p.enum_values.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_primitive_schema_with_range() {
        let p = PrimitiveSchema::integer().with_range(0.0, 100.0);
        assert_eq!(p.minimum, Some(0.0));
        assert_eq!(p.maximum, Some(100.0));
    }

    #[test]
    fn test_primitive_schema_with_length_range() {
        let p = PrimitiveSchema::string().with_length_range(3, 50);
        assert_eq!(p.min_length, Some(3));
        assert_eq!(p.max_length, Some(50));
    }

    #[test]
    fn test_primitive_schema_with_pattern() {
        let p = PrimitiveSchema::string().with_pattern("^[a-z]+$");
        assert_eq!(p.pattern.as_deref(), Some("^[a-z]+$"));
    }

    #[test]
    fn test_schema_serialization() {
        let obj = ObjectType::new()
            .with_required_property("id", Schema::integer())
            .with_property("name", Schema::string());
        let schema = Schema::object(obj);
        let json = serde_json::to_string(&schema).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "object");
        assert!(parsed["properties"]["id"].is_object());
        assert!(parsed["required"].is_array());
    }

    // ===== SecurityScheme 测试 =====

    #[test]
    fn test_security_scheme_basic() {
        let s = SecurityScheme::basic();
        assert!(s.is_basic());
        assert!(!s.is_bearer());
        match &s {
            SecurityScheme::Http { scheme, .. } => {
                assert_eq!(scheme, "basic");
            }
            _ => panic!("Expected Http variant"),
        }
    }

    #[test]
    fn test_security_scheme_bearer_with_jwt() {
        let s = SecurityScheme::bearer(true);
        assert!(s.is_bearer());
        assert!(!s.is_basic());
        match &s {
            SecurityScheme::Http {
                scheme,
                bearer_format,
                ..
            } => {
                assert_eq!(scheme, "bearer");
                assert_eq!(bearer_format.as_deref(), Some("JWT"));
            }
            _ => panic!("Expected Http variant"),
        }
    }

    #[test]
    fn test_security_scheme_bearer_without_format() {
        let s = SecurityScheme::bearer(false);
        match &s {
            SecurityScheme::Http { bearer_format, .. } => {
                assert!(bearer_format.is_none());
            }
            _ => panic!("Expected Http variant"),
        }
    }

    #[test]
    fn test_security_scheme_api_key_header() {
        let s = SecurityScheme::api_key("X-API-Key", ApiKeyLocation::Header);
        match &s {
            SecurityScheme::ApiKey { name, location, .. } => {
                assert_eq!(name, "X-API-Key");
                assert!(matches!(location, ApiKeyLocation::Header));
            }
            _ => panic!("Expected ApiKey variant"),
        }
    }

    #[test]
    fn test_security_scheme_api_key_query() {
        let s = SecurityScheme::api_key("api_key", ApiKeyLocation::Query);
        match &s {
            SecurityScheme::ApiKey { location, .. } => {
                assert!(matches!(location, ApiKeyLocation::Query));
            }
            _ => panic!("Expected ApiKey variant"),
        }
    }

    #[test]
    fn test_security_scheme_api_key_cookie() {
        let s = SecurityScheme::api_key("session", ApiKeyLocation::Cookie);
        match &s {
            SecurityScheme::ApiKey { location, .. } => {
                assert!(matches!(location, ApiKeyLocation::Cookie));
            }
            _ => panic!("Expected ApiKey variant"),
        }
    }

    #[test]
    fn test_security_scheme_oauth2() {
        let mut scopes = HashMap::new();
        scopes.insert("read".to_string(), "Read access".to_string());
        scopes.insert("write".to_string(), "Write access".to_string());
        let flow = AuthorizationCodeFlow {
            authorization_url: "https://example.com/oauth/authorize".to_string(),
            token_url: "https://example.com/oauth/token".to_string(),
            refresh_url: None,
            scopes,
        };
        let flows = OAuth2Flows::new().with_authorization_code(flow);
        let s = SecurityScheme::oauth2(flows);
        match &s {
            SecurityScheme::OAuth2 { flows, .. } => {
                assert!(flows.authorization_code.is_some());
                assert!(flows.implicit.is_none());
            }
            _ => panic!("Expected OAuth2 variant"),
        }
    }

    #[test]
    fn test_security_scheme_with_description() {
        let s = SecurityScheme::bearer(true).with_description("JWT auth");
        match &s {
            SecurityScheme::Http { description, .. } => {
                assert_eq!(description.as_deref(), Some("JWT auth"));
            }
            _ => panic!("Expected Http variant"),
        }
    }

    #[test]
    fn test_oauth2_flows_builder() {
        let mut scopes = HashMap::new();
        scopes.insert("read".to_string(), "Read access".to_string());
        let implicit = ImplicitFlow {
            authorization_url: "https://example.com/oauth/authorize".to_string(),
            refresh_url: None,
            scopes,
        };
        let flows = OAuth2Flows::new()
            .with_implicit(implicit)
            .with_password(PasswordFlow {
                token_url: "https://example.com/oauth/token".to_string(),
                refresh_url: None,
                scopes: HashMap::new(),
            })
            .with_client_credentials(ClientCredentialsFlow {
                token_url: "https://example.com/oauth/token".to_string(),
                refresh_url: None,
                scopes: HashMap::new(),
            });
        assert!(flows.implicit.is_some());
        assert!(flows.password.is_some());
        assert!(flows.client_credentials.is_some());
        assert!(flows.authorization_code.is_none());
    }

    // ===== Tags 测试 =====

    #[test]
    fn test_tag_builder() {
        let tag = Tag::new("users")
            .with_description("User management endpoints")
            .with_external_docs(ExternalDocs::new("https://example.com/docs/users")
                .with_description("User docs"));
        assert_eq!(tag.name, "users");
        assert_eq!(tag.description.as_deref(), Some("User management endpoints"));
        assert!(tag.external_docs.is_some());
        assert_eq!(
            tag.external_docs.as_ref().unwrap().url,
            "https://example.com/docs/users"
        );
    }

    #[test]
    fn test_external_docs_builder() {
        let docs = ExternalDocs::new("https://example.com/docs")
            .with_description("External documentation");
        assert_eq!(docs.url, "https://example.com/docs");
        assert_eq!(docs.description.as_deref(), Some("External documentation"));
    }

    // ===== Servers 测试 =====

    #[test]
    fn test_server_builder() {
        let server = Server::new("https://{env}.example.com")
            .with_description("Environment-specific server")
            .with_variable(
                "env",
                ServerVariable::new("api")
                    .with_description("Environment name")
                    .with_enum(vec!["api".to_string(), "staging".to_string(), "prod".to_string()]),
            );
        assert_eq!(server.url, "https://{env}.example.com");
        assert_eq!(
            server.description.as_deref(),
            Some("Environment-specific server")
        );
        assert_eq!(server.variables.len(), 1);
        let env_var = server.variables.get("env").unwrap();
        assert_eq!(env_var.default_value, "api");
        assert_eq!(env_var.enum_values.len(), 3);
    }

    #[test]
    fn test_server_variable_builder() {
        let var = ServerVariable::new("v1")
            .with_description("API version")
            .with_enum(vec!["v1".to_string(), "v2".to_string()]);
        assert_eq!(var.default_value, "v1");
        assert_eq!(var.description.as_deref(), Some("API version"));
        assert_eq!(var.enum_values, vec!["v1".to_string(), "v2".to_string()]);
    }

    // ===== 示例生成测试 =====

    #[test]
    fn test_example_builder_primitive_string() {
        let p = PrimitiveSchema::string();
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::Value::String("string".to_string()));
    }

    #[test]
    fn test_example_builder_primitive_string_with_example() {
        let p = PrimitiveSchema::string().with_example(serde_json::json!("hello"));
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!("hello"));
    }

    #[test]
    fn test_example_builder_primitive_string_with_default() {
        let p = PrimitiveSchema::string().with_default(serde_json::json!("default_val"));
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!("default_val"));
    }

    #[test]
    fn test_example_builder_primitive_email_format() {
        let p = PrimitiveSchema::string().with_format("email");
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!("user@example.com"));
    }

    #[test]
    fn test_example_builder_primitive_date_format() {
        let p = PrimitiveSchema::string().with_format("date");
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!("2026-01-01"));
    }

    #[test]
    fn test_example_builder_primitive_datetime_format() {
        let p = PrimitiveSchema::string().with_format("date-time");
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn test_example_builder_primitive_uuid_format() {
        let p = PrimitiveSchema::string().with_format("uuid");
        let v = ExampleBuilder::from_primitive(&p);
        assert!(v.as_str().unwrap().contains("-"));
    }

    #[test]
    fn test_example_builder_primitive_integer() {
        let p = PrimitiveSchema::integer();
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!(0));
    }

    #[test]
    fn test_example_builder_primitive_integer_with_minimum() {
        let p = PrimitiveSchema::integer().with_range(10.0, 100.0);
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!(10));
    }

    #[test]
    fn test_example_builder_primitive_number() {
        let p = PrimitiveSchema::number();
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!(0.0));
    }

    #[test]
    fn test_example_builder_primitive_boolean() {
        let p = PrimitiveSchema::boolean();
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!(false));
    }

    #[test]
    fn test_example_builder_primitive_with_enum() {
        let p = PrimitiveSchema::string().with_enum(vec![
            serde_json::json!("active"),
            serde_json::json!("inactive"),
        ]);
        let v = ExampleBuilder::from_primitive(&p);
        assert_eq!(v, serde_json::json!("active"));
    }

    #[test]
    fn test_example_builder_object() {
        let obj = ObjectType::new()
            .with_required_property("id", Schema::integer())
            .with_property("name", Schema::string());
        let v = ExampleBuilder::from_object(&obj);
        assert!(v.is_object());
        assert_eq!(v["id"], serde_json::json!(0));
        assert_eq!(v["name"], serde_json::json!("string"));
    }

    #[test]
    fn test_example_builder_array() {
        let arr = ArrayType::new(Schema::string());
        let schema = Schema::array(arr);
        let v = ExampleBuilder::from_schema(&schema);
        assert!(v.is_array());
        assert_eq!(v[0], serde_json::json!("string"));
    }

    #[test]
    fn test_example_builder_ref_returns_null() {
        let schema = Schema::ref_to("User");
        let v = ExampleBuilder::from_schema(&schema);
        assert!(v.is_null());
    }

    // ===== 集成测试：Generator 与 Schema/SecurityScheme/Tags/Servers =====

    #[test]
    fn test_generator_with_schema() {
        let mut g = OpenAPIGenerator::new();
        let user_schema = Schema::object(
            ObjectType::new()
                .with_required_property("id", Schema::integer())
                .with_required_property("name", Schema::string())
                .with_property("email", Schema::string().with_format("email")),
        );
        g.register_schema("User", user_schema);
        let spec = g.generate();
        let components = spec.components.expect("components should exist");
        assert!(components.schemas.contains_key("User"));
    }

    #[test]
    fn test_generator_with_security_scheme() {
        let mut g = OpenAPIGenerator::new();
        g.register_security_scheme("bearerAuth", SecurityScheme::bearer(true));
        let spec = g.generate();
        let components = spec.components.expect("components should exist");
        assert!(components.security_schemes.contains_key("bearerAuth"));
    }

    #[test]
    fn test_generator_with_tags() {
        let mut g = OpenAPIGenerator::new();
        g.add_tag(Tag::new("users").with_description("User endpoints"));
        g.add_tag(Tag::new("orders").with_description("Order endpoints"));
        let spec = g.generate();
        assert_eq!(spec.tags.len(), 2);
        assert_eq!(spec.tags[0].name, "users");
        assert_eq!(spec.tags[1].name, "orders");
    }

    #[test]
    fn test_generator_with_servers() {
        let mut g = OpenAPIGenerator::new();
        g.add_server(Server::new("https://api.example.com").with_description("Production"));
        g.add_server(Server::new("https://staging.example.com").with_description("Staging"));
        let spec = g.generate();
        assert_eq!(spec.servers.len(), 2);
        assert_eq!(spec.servers[0].url, "https://api.example.com");
        assert_eq!(spec.servers[1].url, "https://staging.example.com");
    }

    #[test]
    fn test_generator_with_global_security() {
        let g = OpenAPIGenerator::new()
            .with_global_security(SecurityRequirement::new("bearerAuth"));
        let spec = g.generate();
        assert_eq!(spec.security.len(), 1);
    }

    #[test]
    fn test_path_info_with_request_body() {
        let p = PathInfo::new("POST", "Create user")
            .with_request_body_ref("User payload", "#/components/schemas/User", true);
        assert!(p.request_body.is_some());
        let body = p.request_body.unwrap();
        assert!(body.required);
        assert!(body.content.contains_key("application/json"));
    }

    #[test]
    fn test_path_info_with_tags_and_security() {
        let p = PathInfo::new("GET", "List users")
            .with_tag("users")
            .with_tag("admin")
            .with_security(SecurityRequirement::new("bearerAuth"));
        assert_eq!(p.tags, vec!["users", "admin"]);
        assert_eq!(p.security.len(), 1);
    }

    #[test]
    fn test_path_info_with_path_param() {
        let p = PathInfo::new("GET", "Get user").with_path_param("id", "User ID", true);
        assert_eq!(p.parameters.len(), 1);
        assert_eq!(p.parameters[0]["in"], "path");
        assert_eq!(p.parameters[0]["name"], "id");
        assert_eq!(p.parameters[0]["required"], true);
    }

    #[test]
    fn test_path_info_with_query_param() {
        let p = PathInfo::new("GET", "List users")
            .with_query_param("page", "Page number", false)
            .with_query_param("limit", "Items per page", false);
        assert_eq!(p.parameters.len(), 2);
        assert_eq!(p.parameters[0]["in"], "query");
        assert_eq!(p.parameters[1]["in"], "query");
    }

    #[test]
    fn test_path_info_with_operation_id() {
        let p = PathInfo::new("GET", "Get user").with_operation_id("getUserById");
        assert_eq!(p.operation_id.as_deref(), Some("getUserById"));
    }

    #[test]
    fn test_path_info_deprecated() {
        let p = PathInfo::new("GET", "Old endpoint").deprecated();
        assert_eq!(p.deprecated, Some(true));
    }

    #[test]
    fn test_path_info_with_response_schema() {
        let p = PathInfo::new("GET", "Get user")
            .with_response_schema("200", "OK", "#/components/schemas/User");
        let resp = &p.responses["200"];
        assert_eq!(resp["description"], "OK");
        assert_eq!(resp["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/User");
    }

    #[test]
    fn test_security_requirement_with_scopes() {
        let req = SecurityRequirement::new("oauth2").with_scopes(
            "oauth2",
            vec!["read".to_string(), "write".to_string()],
        );
        assert_eq!(req.requirements.len(), 1);
        let scopes = req.requirements.get("oauth2").unwrap();
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn test_media_type_builder() {
        let media = MediaType::with_schema_ref("#/components/schemas/User")
            .with_example(serde_json::json!({"id": 1, "name": "John"}));
        assert!(media.schema.is_some());
        assert!(media.example.is_some());
    }

    #[test]
    fn test_request_body_with_multiple_content_types() {
        let mut body = RequestBody::new("User data", "#/components/schemas/User", true);
        body = body.with_content(
            "application/xml",
            MediaType::with_schema_ref("#/components/schemas/User"),
        );
        assert_eq!(body.content.len(), 2);
        assert!(body.content.contains_key("application/json"));
        assert!(body.content.contains_key("application/xml"));
    }

    #[test]
    fn test_example_builder() {
        let ex = Example::new(serde_json::json!({"id": 1, "name": "John"}))
            .with_summary("Sample user")
            .with_description("A typical user object");
        assert_eq!(ex.summary.as_deref(), Some("Sample user"));
        assert_eq!(ex.description.as_deref(), Some("A typical user object"));
        assert!(ex.value.is_some());
    }

    #[test]
    fn test_full_spec_generation_with_all_components() {
        let mut g = OpenAPIGenerator::new();

        // 注册 Schema
        let user_schema = Schema::object(
            ObjectType::new()
                .with_required_property("id", Schema::integer())
                .with_required_property("name", Schema::string())
                .with_property("email", Schema::string().with_format("email")),
        );
        g.register_schema("User", user_schema);

        // 注册 SecurityScheme
        g.register_security_scheme("bearerAuth", SecurityScheme::bearer(true));

        // 添加 Tags
        g.add_tag(Tag::new("users").with_description("User management"));

        // 添加 Servers
        g.add_server(Server::new("https://api.example.com").with_description("Production"));

        // 注册路径
        g.register_path(
            "/users",
            PathInfo::new("GET", "List users")
                .with_tag("users")
                .with_response_schema("200", "OK", "#/components/schemas/User")
                .with_security(SecurityRequirement::new("bearerAuth"))
                .with_operation_id("listUsers"),
        );
        g.register_path(
            "/users",
            PathInfo::new("POST", "Create user")
                .with_tag("users")
                .with_request_body_ref("User to create", "#/components/schemas/User", true)
                .with_response_schema("201", "Created", "#/components/schemas/User")
                .with_operation_id("createUser"),
        );

        let spec = g.generate();
        let json = spec.to_json_string();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["openapi"], "3.0.0");
        assert!(parsed["components"]["schemas"]["User"].is_object());
        assert!(parsed["components"]["securitySchemes"]["bearerAuth"].is_object());
        assert_eq!(parsed["tags"][0]["name"], "users");
        assert_eq!(parsed["servers"][0]["url"], "https://api.example.com");
        assert!(parsed["paths"]["/users"]["get"].is_object());
        assert!(parsed["paths"]["/users"]["post"].is_object());
        assert_eq!(parsed["paths"]["/users"]["get"]["operationId"], "listUsers");
        assert_eq!(parsed["paths"]["/users"]["post"]["operationId"], "createUser");
    }
}
