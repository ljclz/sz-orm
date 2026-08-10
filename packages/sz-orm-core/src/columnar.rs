//! v3.2.0 零拷贝序列化 — 列式结果集
//!
//! `ColumnarResultSet` 按列连续存储数据（`Vec<Vec<Value>>` 外层每元素为一列），
//! 相比行式存储（`Vec<RowData>`），列式布局对批量遍历单列场景缓存友好。

use std::collections::HashMap;

use crate::result_map::RowData;
use crate::value::Value;

/// 列式 schema（列名 + 类型）
#[derive(Debug, Clone)]
pub struct ColumnarSchema {
    /// 列名列表
    pub names: Vec<String>,
    /// 列类型列表
    pub types: Vec<String>,
}

impl ColumnarSchema {
    /// 创建新的列式 schema
    pub fn new(names: Vec<String>, types: Vec<String>) -> Self {
        Self { names, types }
    }

    /// 返回列数
    pub fn column_count(&self) -> usize {
        self.names.len()
    }

    /// 按列名查找列索引
    pub fn name_index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }
}

/// 列式结果集
///
/// 数据按列连续存储：`columns[i]` 是第 i 列的所有行值。
/// 对单列批量遍历（如聚合、过滤）缓存友好。
pub struct ColumnarResultSet {
    columns: Vec<Vec<Value>>,
    schema: ColumnarSchema,
    row_count: usize,
}

impl ColumnarResultSet {
    /// 创建空的列式结果集
    pub fn new(schema: ColumnarSchema) -> Self {
        let column_count = schema.column_count();
        Self {
            columns: vec![Vec::new(); column_count],
            schema,
            row_count: 0,
        }
    }

    /// 从行式数据转换为列式
    pub fn from_row_data(rows: &[RowData], schema: ColumnarSchema) -> Self {
        let column_count = schema.column_count();
        let mut columns = vec![Vec::with_capacity(rows.len()); column_count];

        for row in rows {
            for (i, name) in schema.names.iter().enumerate() {
                let value = row.get(name).cloned().unwrap_or(Value::Null);
                columns[i].push(value);
            }
        }

        Self {
            columns,
            schema,
            row_count: rows.len(),
        }
    }

    /// 转换回行式数据
    pub fn to_row_data(&self) -> Vec<RowData> {
        let mut rows = Vec::with_capacity(self.row_count);

        for row_idx in 0..self.row_count {
            let mut map = HashMap::new();
            for (col_idx, name) in self.schema.names.iter().enumerate() {
                if col_idx < self.columns.len() && row_idx < self.columns[col_idx].len() {
                    map.insert(name.clone(), self.columns[col_idx][row_idx].clone());
                }
            }
            rows.push(RowData::new(map));
        }

        rows
    }

    /// 按列名取列数据
    pub fn column(&self, name: &str) -> Option<&Vec<Value>> {
        self.schema.name_index(name).map(|i| &self.columns[i])
    }

    /// 按列索引取列数据
    pub fn column_by_index(&self, idx: usize) -> Option<&Vec<Value>> {
        self.columns.get(idx)
    }

    /// 行数
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// 列数
    pub fn column_count(&self) -> usize {
        self.schema.column_count()
    }

    /// schema 引用
    pub fn schema(&self) -> &ColumnarSchema {
        &self.schema
    }

    /// 获取指定行指定列的值
    pub fn get(&self, row_idx: usize, col_name: &str) -> Option<&Value> {
        let col_idx = self.schema.name_index(col_name)?;
        self.columns.get(col_idx)?.get(row_idx)
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_schema() -> ColumnarSchema {
        ColumnarSchema::new(
            vec!["id".into(), "name".into(), "age".into()],
            vec!["INTEGER".into(), "VARCHAR".into(), "INTEGER".into()],
        )
    }

    fn make_rows() -> Vec<RowData> {
        vec![
            RowData::new(HashMap::from([
                ("id".into(), Value::I64(1)),
                ("name".into(), Value::String("Alice".into())),
                ("age".into(), Value::I32(30)),
            ])),
            RowData::new(HashMap::from([
                ("id".into(), Value::I64(2)),
                ("name".into(), Value::String("Bob".into())),
                ("age".into(), Value::I32(25)),
            ])),
            RowData::new(HashMap::from([
                ("id".into(), Value::I64(3)),
                ("name".into(), Value::String("Charlie".into())),
                ("age".into(), Value::I32(35)),
            ])),
        ]
    }

    #[test]
    fn test_columnar_schema() {
        let schema = make_schema();
        assert_eq!(schema.column_count(), 3);
        assert_eq!(schema.name_index("id"), Some(0));
        assert_eq!(schema.name_index("name"), Some(1));
        assert_eq!(schema.name_index("nonexistent"), None);
    }

    #[test]
    fn test_columnar_result_set_new() {
        let schema = make_schema();
        let result = ColumnarResultSet::new(schema);
        assert_eq!(result.row_count(), 0);
        assert_eq!(result.column_count(), 3);
    }

    #[test]
    fn test_from_row_data() {
        let rows = make_rows();
        let schema = make_schema();
        let result = ColumnarResultSet::from_row_data(&rows, schema);

        assert_eq!(result.row_count(), 3);
        assert_eq!(result.column_count(), 3);
    }

    #[test]
    fn test_column_access() {
        let rows = make_rows();
        let schema = make_schema();
        let result = ColumnarResultSet::from_row_data(&rows, schema);

        let id_col = result.column("id").expect("id column");
        assert_eq!(id_col.len(), 3);
        assert_eq!(id_col[0], Value::I64(1));
        assert_eq!(id_col[1], Value::I64(2));
        assert_eq!(id_col[2], Value::I64(3));

        let name_col = result.column("name").expect("name column");
        assert_eq!(name_col[0], Value::String("Alice".into()));

        assert!(result.column("nonexistent").is_none());
    }

    #[test]
    fn test_get_value() {
        let rows = make_rows();
        let schema = make_schema();
        let result = ColumnarResultSet::from_row_data(&rows, schema);

        assert_eq!(result.get(0, "name"), Some(&Value::String("Alice".into())));
        assert_eq!(result.get(1, "name"), Some(&Value::String("Bob".into())));
        assert_eq!(result.get(2, "age"), Some(&Value::I32(35)));
        assert_eq!(result.get(3, "id"), None);
        assert_eq!(result.get(0, "nonexistent"), None);
    }

    #[test]
    fn test_roundtrip_row_to_columnar_to_row() {
        let rows = make_rows();
        let schema = make_schema();
        let result = ColumnarResultSet::from_row_data(&rows, schema);
        let back = result.to_row_data();

        assert_eq!(back.len(), rows.len());

        for (original, converted) in rows.iter().zip(back.iter()) {
            for name in ["id", "name", "age"] {
                assert_eq!(
                    original.get(name).cloned(),
                    converted.get(name).cloned(),
                    "列 {} 往返应一致",
                    name
                );
            }
        }
    }

    #[test]
    fn test_empty_rows() {
        let schema = make_schema();
        let result = ColumnarResultSet::from_row_data(&[], schema);
        assert_eq!(result.row_count(), 0);
        assert_eq!(result.to_row_data().len(), 0);
    }

    #[test]
    fn test_column_lengths_equal_row_count() {
        let rows = make_rows();
        let schema = make_schema();
        let result = ColumnarResultSet::from_row_data(&rows, schema);

        for name in ["id", "name", "age"] {
            let col = result.column(name).expect("column");
            assert_eq!(
                col.len(),
                result.row_count(),
                "列 {} 长度应等于 row_count",
                name
            );
        }
    }

    #[test]
    fn test_column_order_matches_schema() {
        let rows = make_rows();
        let schema = make_schema();
        let result = ColumnarResultSet::from_row_data(&rows, schema);

        let id_col = result.column_by_index(0).expect("index 0");
        let name_col = result.column_by_index(1).expect("index 1");
        let age_col = result.column_by_index(2).expect("index 2");

        assert_eq!(id_col[0], Value::I64(1));
        assert_eq!(name_col[0], Value::String("Alice".into()));
        assert_eq!(age_col[0], Value::I32(30));
    }

    #[test]
    fn test_large_dataset_roundtrip() {
        let n = 10000;
        let rows: Vec<RowData> = (0..n)
            .map(|i| {
                RowData::new(HashMap::from([
                    ("id".into(), Value::I64(i as i64)),
                    ("name".into(), Value::String(format!("user_{}", i))),
                    ("age".into(), Value::I32((i % 100) as i32 + 20)),
                ]))
            })
            .collect();

        let schema = ColumnarSchema::new(
            vec!["id".into(), "name".into(), "age".into()],
            vec!["i64".into(), "string".into(), "i32".into()],
        );
        let result = ColumnarResultSet::from_row_data(&rows, schema);
        assert_eq!(result.row_count(), n);

        let back = result.to_row_data();
        assert_eq!(back.len(), n);

        for (original, converted) in rows.iter().zip(back.iter()) {
            assert_eq!(original.get("id"), converted.get("id"));
            assert_eq!(original.get("name"), converted.get("name"));
            assert_eq!(original.get("age"), converted.get("age"));
        }
    }

    #[test]
    fn test_columnar_batch_aggregation() {
        let n = 1000;
        let rows: Vec<RowData> = (0..n)
            .map(|i| {
                RowData::new(HashMap::from([
                    ("id".into(), Value::I64(i as i64)),
                    ("value".into(), Value::I64((i % 10) as i64)),
                ]))
            })
            .collect();

        let schema = ColumnarSchema::new(
            vec!["id".into(), "value".into()],
            vec!["i64".into(), "i64".into()],
        );
        let result = ColumnarResultSet::from_row_data(&rows, schema);

        let value_col = result.column("value").expect("value column");
        let sum: i64 = value_col
            .iter()
            .filter_map(|v| match v {
                Value::I64(n) => Some(*n),
                _ => None,
            })
            .sum();

        let expected: i64 = (0..n).map(|i| (i % 10) as i64).sum();
        assert_eq!(sum, expected);
    }
}
