//! 草图转 SQL（TASK-035）

use crate::types::MultimodalError;
use serde::{Deserialize, Serialize};

/// 草图识别结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchRecognition {
    pub detected_shapes: Vec<Shape>,
    pub inferred_schema: SketchSchema,
    pub confidence: f64,
}

/// 检测到的形状
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shape {
    pub shape_type: ShapeType,
    pub label: String,
}

/// 形状类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShapeType {
    Table,
    Column,
    Relation,
}

/// 推断的 schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchSchema {
    pub tables: Vec<SketchTable>,
    pub relations: Vec<SketchRelation>,
}

/// 草图表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchTable {
    pub name: String,
    pub columns: Vec<String>,
}

/// 草图关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchRelation {
    pub from: String,
    pub to: String,
}

/// 草图转 SQL 转换器
pub struct SketchToSql;

impl SketchToSql {
    pub fn new() -> Self {
        Self
    }

    /// 识别草图
    pub fn recognize(&self, sketch_data: &[u8]) -> Result<SketchRecognition, MultimodalError> {
        if sketch_data.is_empty() {
            return Err(MultimodalError::RenderFallback);
        }

        let shapes = self.detect_shapes(sketch_data);
        let schema = self.infer_schema(&shapes);
        let confidence = self.compute_confidence(&shapes);

        Ok(SketchRecognition {
            detected_shapes: shapes,
            inferred_schema: schema,
            confidence,
        })
    }

    /// 从识别结果生成 SQL
    pub fn to_sql(&self, recognition: &SketchRecognition) -> Result<String, MultimodalError> {
        if recognition.inferred_schema.tables.is_empty() {
            return Err(MultimodalError::RenderFallback);
        }

        let mut sql = String::new();
        for table in &recognition.inferred_schema.tables {
            sql.push_str(&format!("CREATE TABLE {} (\n", table.name));
            for (i, col) in table.columns.iter().enumerate() {
                if i > 0 {
                    sql.push_str(",\n");
                }
                let dtype = if col == "id" || col.ends_with("_id") {
                    "BIGINT"
                } else {
                    "VARCHAR(255)"
                };
                sql.push_str(&format!("    {} {}", col, dtype));
                if col == "id" {
                    sql.push_str(" PRIMARY KEY");
                }
            }
            sql.push_str("\n);\n\n");
        }
        Ok(sql.trim_end().to_string())
    }

    /// 从识别结果生成查询 SQL
    pub fn to_query_sql(&self, recognition: &SketchRecognition) -> Result<String, MultimodalError> {
        let tables = &recognition.inferred_schema.tables;
        if tables.is_empty() {
            return Err(MultimodalError::RenderFallback);
        }

        if tables.len() == 1 {
            Ok(format!("SELECT * FROM {}", tables[0].name))
        } else {
            let table_names: Vec<_> = tables.iter().map(|t| t.name.as_str()).collect();
            Ok(format!(
                "SELECT * FROM {} JOIN {} ON {}.id = {}.{}_id",
                table_names[0], table_names[1], table_names[1], table_names[1], table_names[0]
            ))
        }
    }

    /// 一站式：草图 → SQL
    pub fn sketch_to_sql(&self, sketch_data: &[u8]) -> Result<String, MultimodalError> {
        let recognition = self.recognize(sketch_data)?;
        self.to_sql(&recognition)
    }

    /// 检测草图中的形状（演示用伪检测）
    ///
    /// **注意**：基于数据哈希取模选择形状，非真实草图识别。
    /// 生产环境应接入 CV 服务（如 OpenCV、MediaPipe）。
    fn detect_shapes(&self, data: &[u8]) -> Vec<Shape> {
        let hash = data.iter().fold(0u64, |acc, b| acc.wrapping_add(*b as u64));

        let mut shapes = Vec::new();
        match hash % 2 {
            0 => {
                shapes.push(Shape {
                    shape_type: ShapeType::Table,
                    label: "users".to_string(),
                });
                shapes.push(Shape {
                    shape_type: ShapeType::Column,
                    label: "id".to_string(),
                });
                shapes.push(Shape {
                    shape_type: ShapeType::Column,
                    label: "name".to_string(),
                });
            }
            _ => {
                shapes.push(Shape {
                    shape_type: ShapeType::Table,
                    label: "users".to_string(),
                });
                shapes.push(Shape {
                    shape_type: ShapeType::Table,
                    label: "orders".to_string(),
                });
                shapes.push(Shape {
                    shape_type: ShapeType::Relation,
                    label: "users->orders".to_string(),
                });
            }
        }
        shapes
    }

    fn infer_schema(&self, shapes: &[Shape]) -> SketchSchema {
        let mut tables = Vec::new();
        let mut relations = Vec::new();

        let table_shapes: Vec<_> = shapes
            .iter()
            .filter(|s| s.shape_type == ShapeType::Table)
            .collect();

        for table_shape in table_shapes {
            let columns: Vec<_> = shapes
                .iter()
                .filter(|s| s.shape_type == ShapeType::Column)
                .map(|s| s.label.clone())
                .collect();
            tables.push(SketchTable {
                name: table_shape.label.clone(),
                columns: if columns.is_empty() {
                    vec!["id".to_string()]
                } else {
                    columns
                },
            });
        }

        for shape in shapes {
            if shape.shape_type == ShapeType::Relation {
                let parts: Vec<_> = shape.label.split("->").collect();
                if parts.len() == 2 {
                    relations.push(SketchRelation {
                        from: parts[0].to_string(),
                        to: parts[1].to_string(),
                    });
                }
            }
        }

        SketchSchema { tables, relations }
    }

    fn compute_confidence(&self, shapes: &[Shape]) -> f64 {
        let tables = shapes
            .iter()
            .filter(|s| s.shape_type == ShapeType::Table)
            .count();
        if tables == 0 {
            0.0
        } else {
            0.7 + 0.1 * tables.min(3) as f64
        }
    }
}

impl Default for SketchToSql {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recognize_sketch() {
        let converter = SketchToSql::new();
        let data = vec![1, 2, 3, 4];
        let result = converter.recognize(&data).unwrap();

        assert!(!result.detected_shapes.is_empty());
        assert!(!result.inferred_schema.tables.is_empty());
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_empty_sketch_fails() {
        let converter = SketchToSql::new();
        assert!(converter.recognize(&[]).is_err());
    }

    #[test]
    fn test_to_sql() {
        let converter = SketchToSql::new();
        let recognition = SketchRecognition {
            detected_shapes: vec![],
            inferred_schema: SketchSchema {
                tables: vec![SketchTable {
                    name: "users".to_string(),
                    columns: vec!["id".to_string(), "name".to_string()],
                }],
                relations: vec![],
            },
            confidence: 0.9,
        };
        let sql = converter.to_sql(&recognition).unwrap();
        assert!(sql.contains("CREATE TABLE users"));
        assert!(sql.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_to_query_sql_single_table() {
        let converter = SketchToSql::new();
        let recognition = SketchRecognition {
            detected_shapes: vec![],
            inferred_schema: SketchSchema {
                tables: vec![SketchTable {
                    name: "users".to_string(),
                    columns: vec!["id".to_string()],
                }],
                relations: vec![],
            },
            confidence: 0.9,
        };
        let sql = converter.to_query_sql(&recognition).unwrap();
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn test_to_query_sql_join() {
        let converter = SketchToSql::new();
        let recognition = SketchRecognition {
            detected_shapes: vec![],
            inferred_schema: SketchSchema {
                tables: vec![
                    SketchTable {
                        name: "users".to_string(),
                        columns: vec!["id".to_string()],
                    },
                    SketchTable {
                        name: "orders".to_string(),
                        columns: vec!["id".to_string(), "user_id".to_string()],
                    },
                ],
                relations: vec![],
            },
            confidence: 0.9,
        };
        let sql = converter.to_query_sql(&recognition).unwrap();
        assert!(sql.contains("JOIN"));
    }

    #[test]
    fn test_sketch_to_sql_pipeline() {
        let converter = SketchToSql::new();
        let data = vec![1, 2, 3];
        let sql = converter.sketch_to_sql(&data).unwrap();
        assert!(sql.contains("CREATE TABLE"));
    }
}
