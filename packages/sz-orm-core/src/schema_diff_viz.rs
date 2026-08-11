//! Schema Diff 可视化模块（v4.1.0，`schema-diff-viz` feature gate）
//!
//! 提供 schema 差异可视化能力，复用既有 `SchemaDiff`/`DdlGenerator` 基础设施。
//! 支持 text/json/html 三种输出格式，自动标注破坏性变更。

use crate::schema_sync::SchemaDiff;
use serde::Serialize;

/// 变更类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ChangeType {
    /// 新增表
    AddedTable,
    /// 删除表（破坏性）
    DroppedTable,
    /// 新增列
    AddedColumn,
    /// 删除列（破坏性）
    DroppedColumn,
    /// 类型变更
    TypeChanged,
    /// 重命名列
    Renamed,
}

/// 变更严重级别
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Severity {
    /// 安全变更（可自动执行）
    Safe,
    /// 破坏性变更（需人工确认）
    Destructive,
}

/// 单条变更项
#[derive(Debug, Clone, Serialize)]
pub struct ChangeItem {
    /// 变更类型
    pub change_type: ChangeType,
    /// 严重级别
    pub severity: Severity,
    /// 表名
    pub table: String,
    /// 列名（表级变更时为 None）
    pub column: Option<String>,
    /// 变更描述
    pub description: String,
}

/// 差异报告
#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    /// 所有变更项
    pub changes: Vec<ChangeItem>,
    /// 新增表数
    pub added_tables_count: usize,
    /// 删除表数
    pub dropped_tables_count: usize,
    /// 新增列数
    pub added_columns_count: usize,
    /// 删除列数
    pub dropped_columns_count: usize,
    /// 类型变更数
    pub type_changed_count: usize,
    /// 重命名数
    pub renamed_count: usize,
    /// 是否包含破坏性变更
    pub has_destructive: bool,
}

/// 输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// 纯文本（终端友好）
    Text,
    /// JSON
    Json,
    /// HTML
    Html,
}

/// Schema 差异可视化器
pub struct SchemaDiffVisualizer;

impl SchemaDiffVisualizer {
    /// 从 SchemaDiff 生成差异报告
    pub fn analyze(diff: &SchemaDiff) -> DiffReport {
        let mut changes = Vec::new();

        for table in &diff.added_tables {
            changes.push(ChangeItem {
                change_type: ChangeType::AddedTable,
                severity: Severity::Safe,
                table: table.name.clone(),
                column: None,
                description: format!("新增表 {}（{} 列）", table.name, table.columns.len()),
            });
        }

        for table_name in &diff.dropped_tables {
            changes.push(ChangeItem {
                change_type: ChangeType::DroppedTable,
                severity: Severity::Destructive,
                table: table_name.clone(),
                column: None,
                description: format!("删除表 {}（破坏性，不会自动执行）", table_name),
            });
        }

        for (table, col) in &diff.added_columns {
            changes.push(ChangeItem {
                change_type: ChangeType::AddedColumn,
                severity: Severity::Safe,
                table: table.clone(),
                column: Some(col.name.clone()),
                description: format!(
                    "新增列 {}.{} 类型={} nullable={}",
                    table, col.name, col.sql_type, col.nullable
                ),
            });
        }

        for (table, col_name) in &diff.dropped_columns {
            changes.push(ChangeItem {
                change_type: ChangeType::DroppedColumn,
                severity: Severity::Destructive,
                table: table.clone(),
                column: Some(col_name.clone()),
                description: format!("删除列 {}.{}（破坏性，不会自动执行）", table, col_name),
            });
        }

        for (table, old_col, new_col) in &diff.type_changed_columns {
            changes.push(ChangeItem {
                change_type: ChangeType::TypeChanged,
                severity: Severity::Safe,
                table: table.clone(),
                column: Some(new_col.name.clone()),
                description: format!(
                    "类型变更 {}.{}: {} -> {} nullable: {} -> {}",
                    table,
                    old_col.name,
                    old_col.sql_type,
                    new_col.sql_type,
                    old_col.nullable,
                    new_col.nullable
                ),
            });
        }

        for (table, old_name, new_name) in &diff.renamed_columns {
            changes.push(ChangeItem {
                change_type: ChangeType::Renamed,
                severity: Severity::Safe,
                table: table.clone(),
                column: Some(new_name.clone()),
                description: format!("重命名列 {}.{} -> {}", table, old_name, new_name),
            });
        }

        DiffReport {
            has_destructive: diff.has_destructive_changes(),
            added_tables_count: diff.added_tables.len(),
            dropped_tables_count: diff.dropped_tables.len(),
            added_columns_count: diff.added_columns.len(),
            dropped_columns_count: diff.dropped_columns.len(),
            type_changed_count: diff.type_changed_columns.len(),
            renamed_count: diff.renamed_columns.len(),
            changes,
        }
    }

    /// 按指定格式渲染报告
    pub fn render(report: &DiffReport, format: OutputFormat) -> String {
        match format {
            OutputFormat::Text => Self::render_text(report),
            OutputFormat::Json => Self::render_json(report),
            OutputFormat::Html => Self::render_html(report),
        }
    }

    /// 渲染为纯文本（终端友好）
    pub fn render_text(report: &DiffReport) -> String {
        let mut out = String::new();
        out.push_str("=== Schema Diff Report ===\n\n");

        if report.changes.is_empty() {
            out.push_str("无变更。\n");
            return out;
        }

        out.push_str(&format!(
            "摘要: 新增表={} 删除表={} 新增列={} 删除列={} 类型变更={} 重命名={}\n",
            report.added_tables_count,
            report.dropped_tables_count,
            report.added_columns_count,
            report.dropped_columns_count,
            report.type_changed_count,
            report.renamed_count
        ));

        if report.has_destructive {
            out.push_str("\n⚠ 警告: 包含破坏性变更（删除表/列），不会自动执行\n");
        }

        out.push_str("\n变更明细:\n");
        for item in &report.changes {
            let icon = match item.severity {
                Severity::Safe => "✓",
                Severity::Destructive => "⚠",
            };
            out.push_str(&format!(
                "  {} [{}] {}\n",
                icon, item.table, item.description
            ));
        }

        out
    }

    /// 渲染为 JSON
    pub fn render_json(report: &DiffReport) -> String {
        serde_json::to_string_pretty(report).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    /// 渲染为 HTML
    pub fn render_html(report: &DiffReport) -> String {
        let mut html = String::new();
        html.push_str("<!DOCTYPE html>\n<html lang=\"zh\">\n<head>\n");
        html.push_str("<meta charset=\"UTF-8\">\n");
        html.push_str("<title>Schema Diff Report</title>\n");
        html.push_str("<style>\n");
        html.push_str("body { font-family: monospace; margin: 2em; }\n");
        html.push_str("h1 { color: #333; }\n");
        html.push_str(".summary { background: #f0f0f0; padding: 1em; border-radius: 4px; }\n");
        html.push_str(".destructive { color: #d32f2f; font-weight: bold; }\n");
        html.push_str(".safe { color: #388e3c; }\n");
        html.push_str("table { border-collapse: collapse; width: 100%; margin-top: 1em; }\n");
        html.push_str("th, td { border: 1px solid #ddd; padding: 0.5em; text-align: left; }\n");
        html.push_str("th { background: #f5f5f5; }\n");
        html.push_str("</style>\n</head>\n<body>\n");
        html.push_str("<h1>Schema Diff Report</h1>\n\n");

        if report.changes.is_empty() {
            html.push_str("<p>无变更。</p>\n");
            html.push_str("</body>\n</html>\n");
            return html;
        }

        html.push_str("<div class=\"summary\">\n");
        html.push_str(&format!(
            "<p>新增表: {} | 删除表: {} | 新增列: {} | 删除列: {} | 类型变更: {} | 重命名: {}</p>\n",
            report.added_tables_count,
            report.dropped_tables_count,
            report.added_columns_count,
            report.dropped_columns_count,
            report.type_changed_count,
            report.renamed_count
        ));
        if report.has_destructive {
            html.push_str("<p class=\"destructive\">⚠ 警告: 包含破坏性变更</p>\n");
        }
        html.push_str("</div>\n\n");

        html.push_str("<table>\n");
        html.push_str(
            "<thead><tr><th>严重级别</th><th>类型</th><th>表</th><th>列</th><th>描述</th></tr></thead>\n",
        );
        html.push_str("<tbody>\n");
        for item in &report.changes {
            let class = match item.severity {
                Severity::Safe => "safe",
                Severity::Destructive => "destructive",
            };
            let type_str = format!("{:?}", item.change_type);
            let col = item.column.as_deref().unwrap_or("-");
            let sev_icon = match item.severity {
                Severity::Safe => "✓",
                Severity::Destructive => "⚠",
            };
            html.push_str(&format!(
                "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                class, sev_icon, type_str, item.table, col, item.description
            ));
        }
        html.push_str("</tbody>\n</table>\n");
        html.push_str("</body>\n</html>\n");
        html
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_sync::{ColumnDef, SchemaDiff, TableDef};

    fn make_column(name: &str, sql_type: &str) -> ColumnDef {
        ColumnDef {
            name: name.to_string(),
            sql_type: sql_type.to_string(),
            nullable: true,
            primary_key: false,
            default: None,
        }
    }

    fn make_table(name: &str, columns: Vec<ColumnDef>) -> TableDef {
        TableDef {
            name: name.to_string(),
            columns,
        }
    }

    fn sample_diff() -> SchemaDiff {
        SchemaDiff {
            added_tables: vec![make_table("users", vec![make_column("id", "BIGINT")])],
            dropped_tables: vec!["old_table".to_string()],
            added_columns: vec![("orders".to_string(), make_column("status", "VARCHAR(20)"))],
            dropped_columns: vec![("orders".to_string(), "legacy_field".to_string())],
            type_changed_columns: vec![(
                "users".to_string(),
                make_column("age", "INT"),
                make_column("age", "BIGINT"),
            )],
            renamed_columns: vec![(
                "users".to_string(),
                "name".to_string(),
                "full_name".to_string(),
            )],
        }
    }

    #[test]
    fn test_analyze_empty_diff() {
        let diff = SchemaDiff::default();
        let report = SchemaDiffVisualizer::analyze(&diff);
        assert!(report.changes.is_empty());
        assert!(!report.has_destructive);
        assert_eq!(report.added_tables_count, 0);
    }

    #[test]
    fn test_analyze_full_diff() {
        let diff = sample_diff();
        let report = SchemaDiffVisualizer::analyze(&diff);
        assert_eq!(report.added_tables_count, 1);
        assert_eq!(report.dropped_tables_count, 1);
        assert_eq!(report.added_columns_count, 1);
        assert_eq!(report.dropped_columns_count, 1);
        assert_eq!(report.type_changed_count, 1);
        assert_eq!(report.renamed_count, 1);
        assert!(report.has_destructive);
        assert_eq!(report.changes.len(), 6);
    }

    #[test]
    fn test_destructive_annotation() {
        let diff = sample_diff();
        let report = SchemaDiffVisualizer::analyze(&diff);
        let destructive_count = report
            .changes
            .iter()
            .filter(|c| c.severity == Severity::Destructive)
            .count();
        assert_eq!(destructive_count, 2);
    }

    #[test]
    fn test_render_text_empty() {
        let diff = SchemaDiff::default();
        let report = SchemaDiffVisualizer::analyze(&diff);
        let text = SchemaDiffVisualizer::render_text(&report);
        assert!(text.contains("无变更"));
    }

    #[test]
    fn test_render_text_with_changes() {
        let diff = sample_diff();
        let report = SchemaDiffVisualizer::analyze(&diff);
        let text = SchemaDiffVisualizer::render_text(&report);
        assert!(text.contains("Schema Diff Report"));
        assert!(text.contains("摘要"));
        assert!(text.contains("破坏性"));
        assert!(text.contains("新增表 users"));
        assert!(text.contains("删除表 old_table"));
    }

    #[test]
    fn test_render_json() {
        let diff = sample_diff();
        let report = SchemaDiffVisualizer::analyze(&diff);
        let json = SchemaDiffVisualizer::render_json(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["added_tables_count"], 1);
        assert_eq!(parsed["dropped_tables_count"], 1);
        assert_eq!(parsed["has_destructive"], true);
        assert_eq!(parsed["changes"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn test_render_html() {
        let diff = sample_diff();
        let report = SchemaDiffVisualizer::analyze(&diff);
        let html = SchemaDiffVisualizer::render_html(&report);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Schema Diff Report"));
        assert!(html.contains("destructive"));
        assert!(html.contains("<table>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_render_dispatch() {
        let diff = sample_diff();
        let report = SchemaDiffVisualizer::analyze(&diff);
        let text = SchemaDiffVisualizer::render(&report, OutputFormat::Text);
        let json = SchemaDiffVisualizer::render(&report, OutputFormat::Json);
        let html = SchemaDiffVisualizer::render(&report, OutputFormat::Html);
        assert!(text.starts_with("=== Schema Diff"));
        assert!(json.starts_with('{'));
        assert!(html.starts_with("<!DOCTYPE"));
    }

    #[test]
    fn test_render_html_empty() {
        let diff = SchemaDiff::default();
        let report = SchemaDiffVisualizer::analyze(&diff);
        let html = SchemaDiffVisualizer::render_html(&report);
        assert!(html.contains("无变更"));
    }
}
