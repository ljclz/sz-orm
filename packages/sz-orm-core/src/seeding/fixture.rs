//! FixtureLoader — fixture 模板加载器
//!
//! 从 YAML/JSON 文件加载静态测试数据模板，解析关联引用，支持模板继承与覆盖。

use super::{Record, SeedError};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// 关联引用（如 `${user.0.id}`）
#[derive(Debug, Clone)]
pub struct Reference {
    /// 当前记录中引用字段名
    pub field: String,
    /// 目标表名
    pub target_table: String,
    /// 目标记录索引
    pub target_index: usize,
    /// 目标字段名
    pub target_field: String,
}

/// Fixture 模板
#[derive(Debug, Clone)]
pub struct FixtureTemplate {
    /// 目标表名
    pub table: String,
    /// 静态记录列表
    pub records: Vec<Record>,
    /// 生成数量
    pub count: usize,
    /// 关联引用列表
    pub references: Vec<Reference>,
    /// 继承的模板名
    pub extends: Option<String>,
}

/// Fixture 加载器
pub struct FixtureLoader;

impl FixtureLoader {
    /// 从文件加载 fixture 模板
    pub fn load(path: &str) -> Result<FixtureTemplate, SeedError> {
        let content = std::fs::read_to_string(path)?;
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match ext {
            "yaml" | "yml" => Self::parse_yaml(&content, path),
            "json" => Self::parse_json(&content, path),
            _ => Err(SeedError::FixtureParseFailed {
                path: path.to_string(),
                reason: format!("unsupported file extension: {}", ext),
            }),
        }
    }

    fn parse_yaml(content: &str, path: &str) -> Result<FixtureTemplate, SeedError> {
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(content).map_err(|e| SeedError::FixtureParseFailed {
                path: path.to_string(),
                reason: e.to_string(),
            })?;
        Self::build_template(yaml, path)
    }

    fn parse_json(content: &str, path: &str) -> Result<FixtureTemplate, SeedError> {
        let json: serde_json::Value =
            serde_json::from_str(content).map_err(|e| SeedError::FixtureParseFailed {
                path: path.to_string(),
                reason: e.to_string(),
            })?;
        let yaml = serde_yaml::to_value(&json).map_err(|e| SeedError::FixtureParseFailed {
            path: path.to_string(),
            reason: e.to_string(),
        })?;
        Self::build_template(yaml, path)
    }

    fn build_template(yaml: serde_yaml::Value, path: &str) -> Result<FixtureTemplate, SeedError> {
        let map = yaml
            .as_mapping()
            .ok_or_else(|| SeedError::FixtureParseFailed {
                path: path.to_string(),
                reason: "root must be a mapping".to_string(),
            })?;
        let table = map
            .get(serde_yaml::Value::String("table".to_string()))
            .and_then(|v| v.as_str())
            .ok_or_else(|| SeedError::FixtureParseFailed {
                path: path.to_string(),
                reason: "missing 'table' field".to_string(),
            })?
            .to_string();
        let count = map
            .get(serde_yaml::Value::String("count".to_string()))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let extends = map
            .get(serde_yaml::Value::String("extends".to_string()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let records = Self::extract_records(map, path)?;
        let references = Self::extract_references(map);
        Ok(FixtureTemplate {
            table,
            records,
            count,
            references,
            extends,
        })
    }

    fn extract_records(map: &serde_yaml::Mapping, path: &str) -> Result<Vec<Record>, SeedError> {
        let fields = map.get(serde_yaml::Value::String("fields".to_string()));
        let count = map
            .get(serde_yaml::Value::String("count".to_string()))
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;
        match fields {
            Some(serde_yaml::Value::Mapping(field_map)) => {
                let mut records = Vec::with_capacity(count);
                for _ in 0..count {
                    let mut record = serde_json::Map::new();
                    for (k, v) in field_map {
                        let key = k.as_str().unwrap_or("").to_string();
                        let value = serde_json::to_value(v).unwrap_or(Value::Null);
                        record.insert(key, value);
                    }
                    records.push(record);
                }
                Ok(records)
            }
            Some(serde_yaml::Value::Sequence(items)) => {
                let mut records = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(item_map) = item.as_mapping() {
                        let mut record = serde_json::Map::new();
                        for (k, v) in item_map {
                            let key = k.as_str().unwrap_or("").to_string();
                            let value = serde_json::to_value(v).unwrap_or(Value::Null);
                            record.insert(key, value);
                        }
                        records.push(record);
                    }
                }
                Ok(records)
            }
            None => Ok(Vec::new()),
            _ => Err(SeedError::FixtureParseFailed {
                path: path.to_string(),
                reason: "fields must be a mapping or sequence".to_string(),
            }),
        }
    }

    fn extract_references(map: &serde_yaml::Mapping) -> Vec<Reference> {
        let refs = map.get(serde_yaml::Value::String("references".to_string()));
        match refs {
            Some(serde_yaml::Value::Sequence(items)) => items
                .iter()
                .filter_map(|item| {
                    let m = item.as_mapping()?;
                    let field = m
                        .get(serde_yaml::Value::String("field".to_string()))?
                        .as_str()?
                        .to_string();
                    let target_table = m
                        .get(serde_yaml::Value::String("target".to_string()))?
                        .as_str()?
                        .to_string();
                    let target_index = m
                        .get(serde_yaml::Value::String("index".to_string()))?
                        .as_u64()? as usize;
                    let target_field = m
                        .get(serde_yaml::Value::String("target_field".to_string()))?
                        .as_str()?
                        .to_string();
                    Some(Reference {
                        field,
                        target_table,
                        target_index,
                        target_field,
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// 解析关联引用 `${table.index.field}`
    pub fn resolve_references(
        template: &mut FixtureTemplate,
        resolved: &HashMap<String, Vec<Record>>,
    ) -> Result<(), SeedError> {
        for reference in &template.references {
            let target_records = resolved.get(&reference.target_table).ok_or_else(|| {
                SeedError::InvalidConfig(format!(
                    "reference target table '{}' not found",
                    reference.target_table
                ))
            })?;
            let target_record = target_records.get(reference.target_index).ok_or_else(|| {
                SeedError::InvalidConfig(format!(
                    "reference target index {} out of range",
                    reference.target_index
                ))
            })?;
            let target_value = target_record
                .get(&reference.target_field)
                .cloned()
                .unwrap_or(Value::Null);
            for record in &mut template.records {
                record.insert(reference.field.clone(), target_value.clone());
            }
        }
        Ok(())
    }

    /// 加载目录下所有 fixture 文件
    pub fn load_dir(dir: &str) -> Result<Vec<FixtureTemplate>, SeedError> {
        let mut templates = Vec::new();
        let path = Path::new(dir);
        if !path.exists() {
            return Err(SeedError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("directory not found: {}", dir),
            )));
        }
        let mut entries: Vec<_> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext == "yaml" || ext == "yml" || ext == "json")
            })
            .collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path_str = entry.path().to_string_lossy().to_string();
            templates.push(Self::load(&path_str)?);
        }
        Ok(templates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn write_temp_file(name: &str, content: &str) -> String {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("sz_orm_fixture_{}", name));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path.to_string_lossy().to_string()
    }

    fn cleanup(path: &str) {
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_load_yaml_fixture() {
        let content = r#"
table: users
count: 3
fields:
  name: "张三"
  email: "zhangsan@example.com"
  age: 30
"#;
        let path = write_temp_file("test1.yaml", content);
        let template = FixtureLoader::load(&path).unwrap();
        assert_eq!(template.table, "users");
        assert_eq!(template.count, 3);
        assert_eq!(template.records.len(), 3);
        assert_eq!(template.records[0]["name"], "张三");
        cleanup(&path);
    }

    #[test]
    fn test_load_json_fixture() {
        let content =
            r#"{"table": "orders", "count": 2, "fields": {"order_id": 1001, "amount": 99.9}}"#;
        let path = write_temp_file("test2.json", content);
        let template = FixtureLoader::load(&path).unwrap();
        assert_eq!(template.table, "orders");
        assert_eq!(template.records.len(), 2);
        cleanup(&path);
    }

    #[test]
    fn test_resolve_references() {
        let mut template = FixtureTemplate {
            table: "orders".to_string(),
            records: vec![serde_json::Map::new()],
            count: 1,
            references: vec![Reference {
                field: "user_id".to_string(),
                target_table: "users".to_string(),
                target_index: 0,
                target_field: "id".to_string(),
            }],
            extends: None,
        };
        let mut user_record = serde_json::Map::new();
        user_record.insert("id".to_string(), json!(42));
        let resolved: HashMap<String, Vec<Record>> = vec![("users".to_string(), vec![user_record])]
            .into_iter()
            .collect();
        FixtureLoader::resolve_references(&mut template, &resolved).unwrap();
        assert_eq!(template.records[0]["user_id"], json!(42));
    }

    #[test]
    fn test_parse_error() {
        let path = write_temp_file("test3.yaml", "invalid: yaml: content: [");
        let result = FixtureLoader::load(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SeedError::FixtureParseFailed { .. }));
        cleanup(&path);
    }

    #[test]
    fn test_unsupported_extension() {
        let path = write_temp_file("test4.txt", "content");
        let result = FixtureLoader::load(&path);
        assert!(result.is_err());
        cleanup(&path);
    }
}
