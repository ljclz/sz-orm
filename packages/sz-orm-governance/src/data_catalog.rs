//! 数据目录自动生成（TASK-021）

use serde::{Deserialize, Serialize};

/// 数据目录条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub table: String,
    pub columns: Vec<ColumnCatalog>,
    pub business_description: String,
    pub quality_score: f64,
}

/// 列目录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnCatalog {
    pub name: String,
    pub data_type: String,
    pub business_meaning: String,
    pub quality_score: f64,
}

/// 数据目录构建器
pub struct DataCatalogBuilder;

impl DataCatalogBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build(&self, table: &str, columns: &[(&str, &str)]) -> CatalogEntry {
        let column_catalogs: Vec<ColumnCatalog> = columns
            .iter()
            .map(|(name, dtype)| ColumnCatalog {
                name: name.to_string(),
                data_type: dtype.to_string(),
                business_meaning: Self::infer_meaning(name),
                quality_score: Self::infer_quality(name, dtype),
            })
            .collect();

        let avg_quality = if column_catalogs.is_empty() {
            0.0
        } else {
            column_catalogs.iter().map(|c| c.quality_score).sum::<f64>()
                / column_catalogs.len() as f64
        };

        CatalogEntry {
            table: table.to_string(),
            columns: column_catalogs,
            business_description: format!("{table} 表存储业务数据"),
            quality_score: avg_quality,
        }
    }

    fn infer_meaning(field: &str) -> String {
        let f = field.to_lowercase();
        if f.contains("id") {
            "唯一标识符".to_string()
        } else if f.contains("name") {
            "名称".to_string()
        } else if f.contains("time") || f.contains("date") {
            "时间戳".to_string()
        } else if f.contains("amount") || f.contains("price") {
            "金额".to_string()
        } else {
            "业务字段".to_string()
        }
    }

    /// 基于列名和数据类型推断质量分
    ///
    /// 主键/标识列: 0.95（高置信）
    /// 时间戳列: 0.90（通常完整）
    /// 非空列: 0.85
    /// 可空列: 0.75
    fn infer_quality(field: &str, dtype: &str) -> f64 {
        let f = field.to_lowercase();
        if f.contains("id") || f == "id" {
            0.95
        } else if f.contains("time") || f.contains("date") {
            0.90
        } else if dtype.to_uppercase().contains("NOT NULL") {
            0.85
        } else {
            0.75
        }
    }
}

impl Default for DataCatalogBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_catalog() {
        let builder = DataCatalogBuilder::new();
        let catalog = builder.build("orders", &[("id", "BIGINT"), ("amount", "DECIMAL")]);
        assert_eq!(catalog.table, "orders");
        assert_eq!(catalog.columns.len(), 2);
        assert!(catalog.quality_score > 0.0);
    }

    #[test]
    fn test_infer_meaning() {
        let builder = DataCatalogBuilder::new();
        let catalog = builder.build("users", &[("user_id", "BIGINT"), ("user_name", "VARCHAR")]);
        assert_eq!(catalog.columns[0].business_meaning, "唯一标识符");
        assert_eq!(catalog.columns[1].business_meaning, "名称");
    }
}
