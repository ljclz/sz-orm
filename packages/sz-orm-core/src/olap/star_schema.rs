//! Star Schema 优化（`olap-vectorized` feature）
//!
//! 识别星型 schema（事实表 + 维度表），优化 JOIN 顺序：
//! 先过滤维度表，再 JOIN 事实表，减少事实表扫描量。

/// 维度表
#[derive(Debug, Clone)]
pub struct DimensionTable {
    /// 表名
    pub table: String,
    /// JOIN 条件
    pub join_on: String,
    /// 过滤条件（WHERE 子句，可选）
    pub filter: Option<String>,
}

/// 星型 Schema
#[derive(Debug, Clone)]
pub struct StarSchema {
    /// 事实表
    pub fact_table: String,
    /// 维度表列表
    pub dimensions: Vec<DimensionTable>,
    /// 是否识别为星型
    pub is_star: bool,
}

/// Star Schema 优化器
#[derive(Debug)]
pub struct StarSchemaOptimizer {
    /// 维度表最大数量（超过则不视为星型，默认 10）
    max_dimensions: usize,
}

impl StarSchemaOptimizer {
    pub fn new() -> Self {
        Self { max_dimensions: 10 }
    }

    pub fn with_max_dimensions(mut self, max: usize) -> Self {
        self.max_dimensions = max;
        self
    }

    /// 识别星型 schema
    ///
    /// 判定条件：1 个事实表 + 1~max_dimensions 个维度表
    pub fn identify(&self, fact_table: &str, dimensions: Vec<DimensionTable>) -> StarSchema {
        let is_star = !dimensions.is_empty() && dimensions.len() <= self.max_dimensions;
        StarSchema {
            fact_table: fact_table.to_string(),
            dimensions,
            is_star,
        }
    }

    /// 优化 JOIN 顺序：先过滤维度表，再 JOIN 事实表
    ///
    /// 返回优化后的 JOIN 顺序 SQL 片段
    pub fn optimize(&self, schema: &StarSchema) -> String {
        if !schema.is_star {
            return format!("SELECT * FROM {}", schema.fact_table);
        }
        let mut sql = format!("SELECT * FROM {}", schema.fact_table);
        let filtered: Vec<&DimensionTable> = schema
            .dimensions
            .iter()
            .filter(|d| d.filter.is_some())
            .collect();
        let unfiltered: Vec<&DimensionTable> = schema
            .dimensions
            .iter()
            .filter(|d| d.filter.is_none())
            .collect();
        for d in filtered.iter().chain(unfiltered.iter()) {
            sql.push_str(&format!(" JOIN {} ON {}", d.table, d.join_on));
        }
        sql
    }

    /// 获取带过滤条件的维度表数
    pub fn filtered_dimension_count(&self, schema: &StarSchema) -> usize {
        schema
            .dimensions
            .iter()
            .filter(|d| d.filter.is_some())
            .count()
    }
}

impl Default for StarSchemaOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dim(table: &str, on: &str) -> DimensionTable {
        DimensionTable {
            table: table.into(),
            join_on: on.into(),
            filter: None,
        }
    }

    fn dim_with_filter(table: &str, on: &str, filter: &str) -> DimensionTable {
        DimensionTable {
            table: table.into(),
            join_on: on.into(),
            filter: Some(filter.into()),
        }
    }

    #[test]
    fn identify_star_schema() {
        let opt = StarSchemaOptimizer::new();
        let schema = opt.identify(
            "sales",
            vec![
                dim("dept", "sales.dept_id = dept.id"),
                dim("region", "sales.region_id = region.id"),
            ],
        );
        assert!(schema.is_star);
        assert_eq!(schema.dimensions.len(), 2);
    }

    #[test]
    fn not_star_with_too_many_dimensions() {
        let opt = StarSchemaOptimizer::new().with_max_dimensions(2);
        let schema = opt.identify("fact", vec![dim("d1", "1"), dim("d2", "2"), dim("d3", "3")]);
        assert!(!schema.is_star);
    }

    #[test]
    fn not_star_with_no_dimensions() {
        let opt = StarSchemaOptimizer::new();
        let schema = opt.identify("fact", vec![]);
        assert!(!schema.is_star);
    }

    #[test]
    fn optimize_filters_first() {
        let opt = StarSchemaOptimizer::new();
        let schema = opt.identify(
            "sales",
            vec![
                dim("dept", "sales.dept_id = dept.id"),
                dim_with_filter(
                    "region",
                    "sales.region_id = region.id",
                    "region.name = 'East'",
                ),
            ],
        );
        let sql = opt.optimize(&schema);
        let region_pos = sql.find("JOIN region").unwrap();
        let dept_pos = sql.find("JOIN dept").unwrap();
        assert!(
            region_pos < dept_pos,
            "filtered dimension should come first"
        );
    }

    #[test]
    fn filtered_count() {
        let opt = StarSchemaOptimizer::new();
        let schema = opt.identify(
            "sales",
            vec![
                dim("d1", "1"),
                dim_with_filter("d2", "2", "x"),
                dim_with_filter("d3", "3", "y"),
            ],
        );
        assert_eq!(opt.filtered_dimension_count(&schema), 2);
    }
}
