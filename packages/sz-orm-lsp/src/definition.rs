//! # 定义跳转 provider（v6.9.0 REQ-DEV-002）
//!
//! 解析 Model 字段引用 → 跳转到 Model 结构体定义行。

use crate::server::{LspPosition, LspRange};
use serde::{Deserialize, Serialize};

/// LSP Location（uri + range）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: LspRange,
}

/// Model 定义信息
#[derive(Debug, Clone)]
pub struct ModelDefinition {
    /// Model 名称
    pub name: String,
    /// 定义所在 URI
    pub uri: String,
    /// 定义所在行号
    pub line: u32,
    /// 字段列表
    pub fields: Vec<ModelField>,
}

/// Model 字段
#[derive(Debug, Clone)]
pub struct ModelField {
    pub name: String,
    pub field_type: String,
    pub line: u32,
}

/// 定义跳转 provider
pub struct DefinitionProvider {
    models: Vec<ModelDefinition>,
}

impl Default for DefinitionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DefinitionProvider {
    /// 创建定义跳转 provider
    pub fn new() -> Self {
        Self { models: vec![] }
    }

    /// 注册 Model 定义
    pub fn register_model(&mut self, model: ModelDefinition) {
        self.models.push(model);
    }

    /// 从源码解析 Model 定义
    ///
    /// 查找 `struct XxxModel` 或 `#[derive(Model)]` 标注的结构体。
    pub fn parse_from_source(uri: &str, source: &str) -> Vec<ModelDefinition> {
        let mut models = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            if let Some(name) = extract_struct_name(line) {
                let mut fields = Vec::new();
                for (fidx, fline) in lines.iter().enumerate().skip(idx + 1) {
                    if fline.trim().starts_with('}') {
                        break;
                    }
                    if let Some(field) = extract_field(fline, fidx as u32) {
                        fields.push(field);
                    }
                }
                models.push(ModelDefinition {
                    name,
                    uri: uri.to_string(),
                    line: idx as u32,
                    fields,
                });
            }
        }
        models
    }

    /// textDocument/definition handler
    ///
    /// 查找光标位置所在单词对应的 Model 定义。
    pub fn goto_definition(&self, uri: &str, position: &LspPosition) -> Vec<Location> {
        let target = self.find_model_by_position(uri, position);
        target
            .into_iter()
            .map(|m| Location {
                uri: m.uri.clone(),
                range: LspRange {
                    start: LspPosition {
                        line: m.line,
                        character: 0,
                    },
                    end: LspPosition {
                        line: m.line,
                        character: 80,
                    },
                },
            })
            .collect()
    }

    /// 根据光标位置查找模型定义
    fn find_model_by_position(
        &self,
        uri: &str,
        position: &LspPosition,
    ) -> Option<&ModelDefinition> {
        self.models
            .iter()
            .find(|m| m.uri == uri && m.line <= position.line)
    }

    /// 查找指定名称的 Model 定义
    pub fn find_model(&self, name: &str) -> Option<&ModelDefinition> {
        self.models.iter().find(|m| m.name == name)
    }

    /// 查找字段定义位置
    pub fn find_field(&self, model_name: &str, field_name: &str) -> Option<Location> {
        let model = self.find_model(model_name)?;
        let field = model.fields.iter().find(|f| f.name == field_name)?;
        Some(Location {
            uri: model.uri.clone(),
            range: LspRange {
                start: LspPosition {
                    line: field.line,
                    character: 0,
                },
                end: LspPosition {
                    line: field.line,
                    character: 80,
                },
            },
        })
    }
}

fn extract_struct_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("pub struct ") {
        let name = rest
            .split(|c: char| c.is_whitespace() || c == '{' || c == '<')
            .next()?;
        if name.ends_with("Model") {
            return Some(name.to_string());
        }
    }
    if let Some(rest) = trimmed.strip_prefix("struct ") {
        let name = rest
            .split(|c: char| c.is_whitespace() || c == '{' || c == '<')
            .next()?;
        if name.ends_with("Model") {
            return Some(name.to_string());
        }
    }
    None
}

fn extract_field(line: &str, line_num: u32) -> Option<ModelField> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('}') {
        return None;
    }
    let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
    if parts.len() != 2 {
        return None;
    }
    let name = parts[0].trim().trim_start_matches("pub ").trim();
    let field_type = parts[1].trim().trim_end_matches(',');
    if name.is_empty() || field_type.is_empty() {
        return None;
    }
    Some(ModelField {
        name: name.to_string(),
        field_type: field_type.to_string(),
        line: line_num,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_struct_name() {
        assert_eq!(
            extract_struct_name("pub struct UserModel {"),
            Some("UserModel".to_string())
        );
        assert_eq!(
            extract_struct_name("struct ProductModel {"),
            Some("ProductModel".to_string())
        );
        assert_eq!(extract_struct_name("pub fn foo()"), None);
    }

    #[test]
    fn test_extract_field() {
        let field = extract_field("    pub name: String,", 5).unwrap();
        assert_eq!(field.name, "name");
        assert_eq!(field.field_type, "String");
        assert_eq!(field.line, 5);
    }

    #[test]
    fn test_parse_from_source() {
        let source = r#"
pub struct UserModel {
    pub id: i64,
    pub name: String,
    pub email: String,
}

pub struct ProductModel {
    pub id: i64,
    pub price: f64,
}
"#;
        let models = DefinitionProvider::parse_from_source("file:///test.rs", source);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "UserModel");
        assert_eq!(models[0].fields.len(), 3);
        assert_eq!(models[1].name, "ProductModel");
        assert_eq!(models[1].fields.len(), 2);
    }

    #[test]
    fn test_find_model() {
        let mut provider = DefinitionProvider::new();
        provider.register_model(ModelDefinition {
            name: "UserModel".to_string(),
            uri: "file:///test.rs".to_string(),
            line: 1,
            fields: vec![ModelField {
                name: "id".to_string(),
                field_type: "i64".to_string(),
                line: 2,
            }],
        });
        assert!(provider.find_model("UserModel").is_some());
        assert!(provider.find_model("UnknownModel").is_none());
    }

    #[test]
    fn test_find_field() {
        let mut provider = DefinitionProvider::new();
        provider.register_model(ModelDefinition {
            name: "UserModel".to_string(),
            uri: "file:///test.rs".to_string(),
            line: 1,
            fields: vec![
                ModelField {
                    name: "id".to_string(),
                    field_type: "i64".to_string(),
                    line: 2,
                },
                ModelField {
                    name: "name".to_string(),
                    field_type: "String".to_string(),
                    line: 3,
                },
            ],
        });

        let loc = provider.find_field("UserModel", "name").unwrap();
        assert_eq!(loc.uri, "file:///test.rs");
        assert_eq!(loc.range.start.line, 3);
    }

    #[test]
    fn test_goto_definition() {
        let mut provider = DefinitionProvider::new();
        provider.register_model(ModelDefinition {
            name: "UserModel".to_string(),
            uri: "file:///test.rs".to_string(),
            line: 5,
            fields: vec![],
        });

        let locations = provider.goto_definition(
            "file:///test.rs",
            &LspPosition {
                line: 10,
                character: 0,
            },
        );
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, 5);
    }
}
