//! # 数据验证框架（`data-validation` feature）
//!
//! 提供 `Validate` trait + `ValidationError` + 8 种字段级校验规则，
//! 支持 `#[derive(Validate)]` 自动生成验证代码。

pub mod rules;

#[cfg(feature = "validate-on-write")]
pub mod model_integration;

#[cfg(test)]
mod derive_tests;

/// 验证错误
///
/// 字段级含义由各 variant 的 `#[error(...)]` 文案描述（field/min/max/value 等）。
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    /// 必填字段为空
    #[error("field `{field}` is required but empty")]
    Required { field: String },
    /// 长度超出范围
    #[error("field `{field}` length {actual} not in [{min}, {max}]")]
    Length {
        field: String,
        min: usize,
        max: usize,
        actual: usize,
    },
    /// 数值超出范围
    #[error("field `{field}` value {actual} not in [{min}, {max}]")]
    Range {
        field: String,
        min: String,
        max: String,
        actual: String,
    },
    /// 邮箱格式无效
    #[error("field `{field}` value `{value}` is not a valid email")]
    Email { field: String, value: String },
    /// 正则不匹配
    #[error("field `{field}` value `{value}` does not match pattern `{pattern}`")]
    Regex {
        field: String,
        pattern: String,
        value: String,
    },
    /// 不包含所需子串
    #[error("field `{field}` does not contain `{substring}`")]
    Contains { field: String, substring: String },
    /// 包含禁止子串
    #[error("field `{field}` contains forbidden `{substring}`")]
    DoesNotContain { field: String, substring: String },
    /// 自定义校验失败
    #[error("field `{field}` custom validation failed: {reason}")]
    Custom { field: String, reason: String },
    /// 聚合错误（非短路，收集全部失败）
    #[error("validation failed with {count} error(s)")]
    Aggregate {
        errors: Vec<ValidationError>,
        count: usize,
    },
}

/// 验证 trait
pub trait Validate {
    /// 执行验证，返回 Ok 或聚合错误
    fn validate(&self) -> Result<(), ValidationError>;
}

/// 聚合多个验证结果（非短路，收集全部错误）
pub fn aggregate(results: Vec<Result<(), ValidationError>>) -> Result<(), ValidationError> {
    let errors: Vec<ValidationError> = results.into_iter().filter_map(|r| r.err()).collect();
    match errors.len() {
        0 => Ok(()),
        1 => Err(errors.into_iter().next().unwrap()),
        n => Err(ValidationError::Aggregate { errors, count: n }),
    }
}
