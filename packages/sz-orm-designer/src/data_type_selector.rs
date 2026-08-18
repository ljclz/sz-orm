//! 数据类型选择器
//!
//! 提供 [`DataTypeSelector`] 基于数据特征（范围、精度、长度、用途）
//! 推荐最合适的 SQL 数据类型。

use std::fmt;

/// 数据用途
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPurpose {
    /// 主键
    PrimaryKey,
    /// 外键
    ForeignKey,
    /// 普通索引列
    Indexed,
    /// 非索引列
    NonIndexed,
    /// 唯一约束
    Unique,
    /// 文本搜索
    FullTextSearch,
    /// 数值计算
    NumericCalculation,
    /// 时间比较
    TemporalComparison,
    /// JSON 存储
    JsonStorage,
    /// 二进制数据
    BinaryData,
}

impl DataPurpose {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            DataPurpose::PrimaryKey => "primary key",
            DataPurpose::ForeignKey => "foreign key",
            DataPurpose::Indexed => "indexed",
            DataPurpose::NonIndexed => "non-indexed",
            DataPurpose::Unique => "unique",
            DataPurpose::FullTextSearch => "full-text search",
            DataPurpose::NumericCalculation => "numeric calculation",
            DataPurpose::TemporalComparison => "temporal comparison",
            DataPurpose::JsonStorage => "JSON storage",
            DataPurpose::BinaryData => "binary data",
        }
    }
}

/// 数据特征
#[derive(Debug, Clone)]
pub struct DataCharacteristics {
    /// 用途
    pub purpose: DataPurpose,
    /// 最小值
    pub min_value: Option<f64>,
    /// 最大值
    pub max_value: Option<f64>,
    /// 最大长度（字符）
    pub max_length: Option<usize>,
    /// 精度（小数位数）
    pub decimal_places: Option<u8>,
    /// 是否需要时区
    pub needs_timezone: bool,
    /// 是否固定长度
    pub fixed_length: bool,
    /// 是否稀疏（NULL 多）
    pub sparse: bool,
    /// 预估基数（不同值数量）
    pub estimated_cardinality: Option<u64>,
    /// 预估总行数
    pub estimated_total_rows: Option<u64>,
}

impl Default for DataCharacteristics {
    fn default() -> Self {
        Self {
            purpose: DataPurpose::NonIndexed,
            min_value: None,
            max_value: None,
            max_length: None,
            decimal_places: None,
            needs_timezone: false,
            fixed_length: false,
            sparse: false,
            estimated_cardinality: None,
            estimated_total_rows: None,
        }
    }
}

impl DataCharacteristics {
    /// 创建新数据特征
    #[must_use]
    pub fn new(purpose: DataPurpose) -> Self {
        Self {
            purpose,
            ..Self::default()
        }
    }

    /// 设置值范围
    #[must_use]
    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.min_value = Some(min);
        self.max_value = Some(max);
        self
    }

    /// 设置最大长度
    #[must_use]
    pub fn with_max_length(mut self, length: usize) -> Self {
        self.max_length = Some(length);
        self
    }

    /// 设置小数位数
    #[must_use]
    pub fn with_decimal_places(mut self, places: u8) -> Self {
        self.decimal_places = Some(places);
        self
    }

    /// 需要时区
    #[must_use]
    pub fn with_timezone(mut self) -> Self {
        self.needs_timezone = true;
        self
    }

    /// 固定长度
    #[must_use]
    pub fn fixed_length(mut self) -> Self {
        self.fixed_length = true;
        self
    }

    /// 稀疏列
    #[must_use]
    pub fn sparse(mut self) -> Self {
        self.sparse = true;
        self
    }

    /// 设置基数
    #[must_use]
    pub fn with_cardinality(mut self, cardinality: u64, total: u64) -> Self {
        self.estimated_cardinality = Some(cardinality);
        self.estimated_total_rows = Some(total);
        self
    }

    /// 选择性（基数 / 总数）
    #[must_use]
    pub fn selectivity(&self) -> f64 {
        match (self.estimated_cardinality, self.estimated_total_rows) {
            (Some(c), Some(t)) if t > 0 => c as f64 / t as f64,
            _ => 1.0,
        }
    }
}

/// 类型推荐结果
#[derive(Debug, Clone)]
pub struct TypeRecommendation {
    /// 推荐类型名
    pub type_name: String,
    /// 类型参数（如长度、精度）
    pub type_params: Option<String>,
    /// 推荐理由
    pub reason: String,
    /// 备选类型
    pub alternatives: Vec<String>,
}

impl TypeRecommendation {
    /// 创建新推荐
    #[must_use]
    pub fn new(type_name: &str) -> Self {
        Self {
            type_name: type_name.to_string(),
            type_params: None,
            reason: String::new(),
            alternatives: Vec::new(),
        }
    }

    /// 设置参数
    #[must_use]
    pub fn with_params(mut self, params: &str) -> Self {
        self.type_params = Some(params.to_string());
        self
    }

    /// 设置理由
    #[must_use]
    pub fn with_reason(mut self, reason: &str) -> Self {
        self.reason = reason.to_string();
        self
    }

    /// 添加备选
    #[must_use]
    pub fn with_alternative(mut self, alt: &str) -> Self {
        self.alternatives.push(alt.to_string());
        self
    }

    /// 生成完整类型声明
    #[must_use]
    pub fn to_type_declaration(&self) -> String {
        match &self.type_params {
            Some(p) => format!("{}({})", self.type_name, p),
            None => self.type_name.clone(),
        }
    }
}

impl fmt::Display for TypeRecommendation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_type_declaration())
    }
}

/// 数据类型选择器
#[derive(Debug, Default)]
pub struct DataTypeSelector {
    /// 是否优先使用 BIGINT 主键
    prefer_bigint_pk: bool,
    /// 是否优先使用 UUID 主键
    prefer_uuid_pk: bool,
    /// 是否优先使用 TIMESTAMP WITH TIME ZONE
    prefer_timestamptz: bool,
}

impl DataTypeSelector {
    /// 创建新的选择器
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefer_bigint_pk: true,
            prefer_uuid_pk: false,
            prefer_timestamptz: false,
        }
    }

    /// 优先使用 UUID 主键
    #[must_use]
    pub fn prefer_uuid(mut self) -> Self {
        self.prefer_uuid_pk = true;
        self.prefer_bigint_pk = false;
        self
    }

    /// 优先使用 TIMESTAMPTZ
    #[must_use]
    pub fn prefer_timestamptz(mut self) -> Self {
        self.prefer_timestamptz = true;
        self
    }

    /// 推荐数据类型
    #[must_use]
    pub fn recommend(&self, chars: &DataCharacteristics) -> TypeRecommendation {
        match chars.purpose {
            DataPurpose::PrimaryKey => self.recommend_primary_key(chars),
            DataPurpose::ForeignKey => self.recommend_foreign_key(chars),
            DataPurpose::Indexed => self.recommend_indexed(chars),
            DataPurpose::Unique => self.recommend_unique(chars),
            DataPurpose::FullTextSearch => self.recommend_fulltext(chars),
            DataPurpose::NumericCalculation => self.recommend_numeric(chars),
            DataPurpose::TemporalComparison => self.recommend_temporal(chars),
            DataPurpose::JsonStorage => {
                TypeRecommendation::new("JSON").with_reason("JSON storage purpose")
            }
            DataPurpose::BinaryData => self.recommend_binary(chars),
            DataPurpose::NonIndexed => self.recommend_general(chars),
        }
    }

    fn recommend_primary_key(&self, _: &DataCharacteristics) -> TypeRecommendation {
        if self.prefer_uuid_pk {
            TypeRecommendation::new("UUID")
                .with_reason("UUID primary key for distributed systems")
                .with_alternative("BIGINT")
        } else {
            TypeRecommendation::new("BIGINT")
                .with_reason("BIGINT primary key for performance")
                .with_alternative("UUID")
        }
    }

    fn recommend_foreign_key(&self, _: &DataCharacteristics) -> TypeRecommendation {
        if self.prefer_uuid_pk {
            TypeRecommendation::new("UUID").with_reason("UUID foreign key matching primary key")
        } else {
            TypeRecommendation::new("BIGINT").with_reason("BIGINT foreign key matching primary key")
        }
    }

    fn recommend_indexed(&self, chars: &DataCharacteristics) -> TypeRecommendation {
        if let Some(max_len) = chars.max_length {
            if max_len <= 50 {
                TypeRecommendation::new("VARCHAR")
                    .with_params(&max_len.to_string())
                    .with_reason("short indexed string")
            } else {
                TypeRecommendation::new("VARCHAR")
                    .with_params(&max_len.to_string())
                    .with_reason("indexed string")
            }
        } else if let (Some(min), Some(max)) = (chars.min_value, chars.max_value) {
            self.recommend_integer_type(min, max)
        } else {
            TypeRecommendation::new("VARCHAR").with_params("255")
        }
    }

    fn recommend_unique(&self, chars: &DataCharacteristics) -> TypeRecommendation {
        if let Some(max_len) = chars.max_length {
            TypeRecommendation::new("VARCHAR")
                .with_params(&max_len.to_string())
                .with_reason("unique constraint string")
        } else {
            TypeRecommendation::new("VARCHAR")
                .with_params("255")
                .with_reason("unique constraint")
        }
    }

    fn recommend_fulltext(&self, chars: &DataCharacteristics) -> TypeRecommendation {
        if let Some(max_len) = chars.max_length {
            if max_len > 1000 {
                TypeRecommendation::new("TEXT").with_reason("full-text search on long text")
            } else {
                TypeRecommendation::new("VARCHAR")
                    .with_params(&max_len.to_string())
                    .with_reason("full-text search on short text")
            }
        } else {
            TypeRecommendation::new("TEXT").with_reason("full-text search default to TEXT")
        }
    }

    fn recommend_numeric(&self, chars: &DataCharacteristics) -> TypeRecommendation {
        if let Some(places) = chars.decimal_places {
            if places == 0 {
                self.recommend_integer_type(
                    chars.min_value.unwrap_or(i64::MIN as f64),
                    chars.max_value.unwrap_or(i64::MAX as f64),
                )
            } else {
                let precision = 38;
                TypeRecommendation::new("DECIMAL")
                    .with_params(&format!("{precision}, {places}"))
                    .with_reason("fixed-point decimal for financial calculations")
                    .with_alternative("NUMERIC")
            }
        } else if let (Some(min), Some(max)) = (chars.min_value, chars.max_value) {
            self.recommend_integer_type(min, max)
        } else {
            TypeRecommendation::new("DOUBLE").with_reason("general numeric default")
        }
    }

    fn recommend_temporal(&self, chars: &DataCharacteristics) -> TypeRecommendation {
        if chars.needs_timezone || self.prefer_timestamptz {
            TypeRecommendation::new("TIMESTAMPTZ")
                .with_reason("timestamp with time zone for global apps")
                .with_alternative("TIMESTAMP")
        } else {
            TypeRecommendation::new("TIMESTAMP")
                .with_reason("timestamp for local time")
                .with_alternative("TIMESTAMPTZ")
        }
    }

    fn recommend_binary(&self, chars: &DataCharacteristics) -> TypeRecommendation {
        if let Some(max_len) = chars.max_length {
            if max_len > 1000 {
                TypeRecommendation::new("BYTEA").with_reason("variable-length binary data")
            } else {
                TypeRecommendation::new("BYTEA").with_reason("short binary data")
            }
        } else {
            TypeRecommendation::new("BYTEA").with_reason("binary data default")
        }
    }

    fn recommend_general(&self, chars: &DataCharacteristics) -> TypeRecommendation {
        if let Some(max_len) = chars.max_length {
            if max_len > 1000 {
                TypeRecommendation::new("TEXT").with_reason("long text column")
            } else {
                TypeRecommendation::new("VARCHAR")
                    .with_params(&max_len.to_string())
                    .with_reason("variable-length text")
            }
        } else if let Some(places) = chars.decimal_places {
            if places == 0 {
                TypeRecommendation::new("INTEGER").with_reason("integer column")
            } else {
                TypeRecommendation::new("DECIMAL")
                    .with_params(&format!("38, {places}"))
                    .with_reason("decimal column")
            }
        } else if let (Some(min), Some(max)) = (chars.min_value, chars.max_value) {
            self.recommend_integer_type(min, max)
        } else {
            TypeRecommendation::new("VARCHAR")
                .with_params("255")
                .with_reason("general purpose default")
        }
    }

    fn recommend_integer_type(&self, min: f64, max: f64) -> TypeRecommendation {
        let i32_min = i32::MIN as f64;
        let i32_max = i32::MAX as f64;
        let i64_min = i64::MIN as f64;
        let i64_max = i64::MAX as f64;
        if (i32_min..=i32_max).contains(&min) && (i32_min..=i32_max).contains(&max) {
            TypeRecommendation::new("INTEGER")
                .with_reason("value range fits in INT32")
                .with_alternative("BIGINT")
        } else if (i64_min..=i64_max).contains(&min) && (i64_min..=i64_max).contains(&max) {
            TypeRecommendation::new("BIGINT")
                .with_reason("value range fits in INT64")
                .with_alternative("INTEGER")
        } else {
            TypeRecommendation::new("DECIMAL")
                .with_params("38, 0")
                .with_reason("value range exceeds INT64")
        }
    }

    /// 批量推荐
    #[must_use]
    pub fn recommend_batch<'a>(
        &self,
        columns: &[(&'a str, DataCharacteristics)],
    ) -> Vec<(&'a str, TypeRecommendation)> {
        columns
            .iter()
            .map(|(name, chars)| (*name, self.recommend(chars)))
            .collect()
    }
}

impl fmt::Display for DataTypeSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DataTypeSelector(bigint_pk={}, uuid_pk={})",
            self.prefer_bigint_pk, self.prefer_uuid_pk
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_purpose_description() {
        assert_eq!(DataPurpose::PrimaryKey.description(), "primary key");
        assert_eq!(DataPurpose::JsonStorage.description(), "JSON storage");
    }

    #[test]
    fn test_data_characteristics_new() {
        let c = DataCharacteristics::new(DataPurpose::PrimaryKey);
        assert_eq!(c.purpose, DataPurpose::PrimaryKey);
    }

    #[test]
    fn test_data_characteristics_selectivity() {
        let c = DataCharacteristics::new(DataPurpose::Indexed).with_cardinality(100, 1000);
        assert!((c.selectivity() - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_data_characteristics_selectivity_no_data() {
        let c = DataCharacteristics::new(DataPurpose::Indexed);
        assert_eq!(c.selectivity(), 1.0);
    }

    #[test]
    fn test_type_recommendation_new() {
        let r = TypeRecommendation::new("INT");
        assert_eq!(r.type_name, "INT");
    }

    #[test]
    fn test_type_recommendation_to_declaration() {
        let r = TypeRecommendation::new("VARCHAR").with_params("255");
        assert_eq!(r.to_type_declaration(), "VARCHAR(255)");
    }

    #[test]
    fn test_type_recommendation_no_params() {
        let r = TypeRecommendation::new("INT");
        assert_eq!(r.to_type_declaration(), "INT");
    }

    #[test]
    fn test_type_recommendation_display() {
        let r = TypeRecommendation::new("INT");
        assert_eq!(format!("{}", r), "INT");
    }

    #[test]
    fn test_selector_primary_key_bigint() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::PrimaryKey);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "BIGINT");
    }

    #[test]
    fn test_selector_primary_key_uuid() {
        let s = DataTypeSelector::new().prefer_uuid();
        let c = DataCharacteristics::new(DataPurpose::PrimaryKey);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "UUID");
    }

    #[test]
    fn test_selector_foreign_key() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::ForeignKey);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "BIGINT");
    }

    #[test]
    fn test_selector_indexed_string() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::Indexed).with_max_length(50);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "VARCHAR");
    }

    #[test]
    fn test_selector_numeric_decimal() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::NumericCalculation).with_decimal_places(2);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "DECIMAL");
    }

    #[test]
    fn test_selector_numeric_integer() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::NumericCalculation)
            .with_decimal_places(0)
            .with_range(0.0, 1000.0);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "INTEGER");
    }

    #[test]
    fn test_selector_temporal() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::TemporalComparison);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "TIMESTAMP");
    }

    #[test]
    fn test_selector_temporal_tz() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::TemporalComparison).with_timezone();
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "TIMESTAMPTZ");
    }

    #[test]
    fn test_selector_json() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::JsonStorage);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "JSON");
    }

    #[test]
    fn test_selector_binary() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::BinaryData);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "BYTEA");
    }

    #[test]
    fn test_selector_fulltext_long() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::FullTextSearch).with_max_length(2000);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "TEXT");
    }

    #[test]
    fn test_selector_general_text() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::NonIndexed).with_max_length(2000);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "TEXT");
    }

    #[test]
    fn test_selector_general_default() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::NonIndexed);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "VARCHAR");
    }

    #[test]
    fn test_selector_integer_type_i32() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::Indexed).with_range(0.0, 1000.0);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "INTEGER");
    }

    #[test]
    fn test_selector_integer_type_i64() {
        let s = DataTypeSelector::new();
        let c = DataCharacteristics::new(DataPurpose::Indexed).with_range(0.0, 1e18);
        let r = s.recommend(&c);
        assert_eq!(r.type_name, "BIGINT");
    }

    #[test]
    fn test_selector_batch() {
        let s = DataTypeSelector::new();
        let cols = [
            ("id", DataCharacteristics::new(DataPurpose::PrimaryKey)),
            (
                "name",
                DataCharacteristics::new(DataPurpose::NonIndexed).with_max_length(100),
            ),
        ];
        let results = s.recommend_batch(&cols);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_selector_display() {
        let s = DataTypeSelector::new();
        let str = format!("{}", s);
        assert!(str.contains("DataTypeSelector"));
    }
}
