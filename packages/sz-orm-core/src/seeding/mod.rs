//! 数据 seeding/fixture 管理模块（v4.1.0，`data-seeding` feature gate）
//!
//! 提供 `FakerGenerator`（faker 数据生成）、`FixtureLoader`（fixture 模板加载）、
//! `SeedManager`（种子版本管理 + 依赖排序 + 幂等执行 + 环境隔离）。

pub mod faker;
pub mod fixture;
pub mod manager;

pub use faker::FakerGenerator;
pub use fixture::{FixtureLoader, FixtureTemplate, Reference};
pub use manager::{SeedEnv, SeedFile, SeedManager, SeedMode, SeedReport};

use serde_json::Value;
use thiserror::Error;

/// 字段生成器 trait（扩展性：用户可注册自定义生成器）
pub trait FieldGenerator: Send + Sync {
    /// 生成一个字段值
    fn generate(&self, rng: &mut rand::rngs::StdRng) -> Value;
}

/// 字段类型（用于推断默认生成器）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// 字符串类型
    String,
    /// 32 位有符号整数
    I32,
    /// 32 位无符号整数
    U32,
    /// 64 位有符号整数
    I64,
    /// 64 位无符号整数
    U64,
    /// 64 位浮点数
    F64,
    /// 布尔类型
    Boolean,
    /// UUID 类型
    Uuid,
    /// 日期时间类型
    DateTime,
    /// JSON 类型
    Json,
    /// 枚举类型，包含所有可选值
    Enum(Vec<String>),
}

/// 模型字段定义
#[derive(Debug, Clone)]
pub struct FieldDef {
    /// 字段名
    pub name: String,
    /// 字段类型
    pub field_type: FieldType,
    /// 是否可为 NULL
    pub nullable: bool,
}

/// 模型定义（用于批量生成）
#[derive(Debug, Clone)]
pub struct ModelDef {
    /// 目标表名
    pub table: String,
    /// 字段列表
    pub fields: Vec<FieldDef>,
}

/// seeding 错误类型
#[derive(Debug, Error)]
pub enum SeedError {
    /// 生产环境 seeding 被禁止
    #[error("production seeding forbidden: set allow_production=true to override")]
    EnvForbidden,
    /// 检测到依赖循环
    #[error("dependency cycle detected: {chain}")]
    DependencyCycle {
        /// 循环链描述
        chain: String,
    },
    /// fixture 文件解析失败
    #[error("fixture parse failed at {path}: {reason}")]
    FixtureParseFailed {
        /// 文件路径
        path: String,
        /// 失败原因
        reason: String,
    },
    /// seed 执行失败
    #[error("seed execution failed at version {version}: {reason}")]
    SeedExecution {
        /// 种子版本
        version: String,
        /// 失败原因
        reason: String,
    },
    /// 配置无效
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    /// IO 错误
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// YAML 解析错误
    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    /// JSON 解析错误
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
}

/// 生成的记录（字段名 → 值）
pub type Record = serde_json::Map<String, Value>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_type_equality() {
        assert_eq!(FieldType::String, FieldType::String);
        assert_ne!(FieldType::I32, FieldType::U32);
    }

    #[test]
    fn test_seed_error_display() {
        let err = SeedError::EnvForbidden;
        assert!(err.to_string().contains("production seeding forbidden"));

        let err = SeedError::DependencyCycle {
            chain: "A<-B<-A".to_string(),
        };
        assert!(err.to_string().contains("A<-B<-A"));
    }
}
