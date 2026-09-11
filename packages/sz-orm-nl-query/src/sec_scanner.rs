//! NL2SQL 输出路径安全扫描（v6.8.0 TASK W1-1）
//!
//! 静态扫描 NL2SQL 输出路径，禁止 `format!`/字符串拼接构造 SQL WHERE 条件。
//! 确保所有 NL2SQL 生成的 SQL 使用参数化占位符（`?`/`$1` 等）。

use std::path::Path;

use sz_orm_ai::nl2sql::SqlQuery;

/// 安全违规类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecViolation {
    /// 检测到字符串拼接构造 WHERE 条件
    StringConcatenation { line: usize, snippet: String },
    /// 检测到 format! 宏构造 SQL
    FormatMacroUsage { line: usize, snippet: String },
    /// SQL 中包含未参数化的 WHERE 条件（直接嵌入值）
    UnparameterizedWhere { snippet: String },
    /// 源文件读取失败
    SourceReadFailed { path: String, reason: String },
}

impl std::fmt::Display for SecViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StringConcatenation { line, snippet } => {
                write!(f, "StringConcatenation at line {}: {}", line, snippet)
            }
            Self::FormatMacroUsage { line, snippet } => {
                write!(f, "FormatMacroUsage at line {}: {}", line, snippet)
            }
            Self::UnparameterizedWhere { snippet } => {
                write!(f, "UnparameterizedWhere: {}", snippet)
            }
            Self::SourceReadFailed { path, reason } => {
                write!(f, "SourceReadFailed: {} ({})", path, reason)
            }
        }
    }
}

/// 源代码扫描报告
#[derive(Debug, Clone)]
pub struct ScanReport {
    /// 扫描的文件数
    pub files_scanned: usize,
    /// 发现的违规列表
    pub violations: Vec<SecViolation>,
}

impl ScanReport {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

/// NL2SQL 安全扫描器
///
/// 静态扫描 NL2SQL 输出路径，确保不使用字符串拼接构造 SQL。
pub struct NL2SQLSecScanner;

impl NL2SQLSecScanner {
    pub fn new() -> Self {
        Self
    }

    /// 扫描 SqlQuery 输出路径
    ///
    /// 检查 SQL 中是否存在未参数化的 WHERE 条件（直接嵌入字面值而非占位符）。
    pub fn scan_output_path(output: &SqlQuery) -> Result<(), SecViolation> {
        let sql = &output.sql;
        let lower = sql.to_lowercase();
        if let Some(where_pos) = lower.find("where") {
            let where_clause = &sql[where_pos..];
            if Self::has_unparameterized_condition(where_clause) {
                return Err(SecViolation::UnparameterizedWhere {
                    snippet: where_clause.chars().take(80).collect(),
                });
            }
        }
        Ok(())
    }

    /// 扫描源代码目录
    ///
    /// 递归扫描 `.rs` 文件，检测 `format!` 宏和字符串拼接构造 SQL 的模式。
    pub fn scan_source_dir(dir: &Path) -> Result<ScanReport, SecViolation> {
        let mut report = ScanReport {
            files_scanned: 0,
            violations: Vec::new(),
        };
        Self::scan_dir_recursive(dir, &mut report)?;
        Ok(report)
    }

    fn scan_dir_recursive(dir: &Path, report: &mut ScanReport) -> Result<(), SecViolation> {
        let entries = std::fs::read_dir(dir).map_err(|e| SecViolation::SourceReadFailed {
            path: dir.display().to_string(),
            reason: e.to_string(),
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| SecViolation::SourceReadFailed {
                path: dir.display().to_string(),
                reason: e.to_string(),
            })?;
            let path = entry.path();
            if path.is_dir() {
                Self::scan_dir_recursive(&path, report)?;
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                report.files_scanned += 1;
                Self::scan_file(&path, report)?;
            }
        }
        Ok(())
    }

    fn scan_file(path: &Path, report: &mut ScanReport) -> Result<(), SecViolation> {
        let content =
            std::fs::read_to_string(path).map_err(|e| SecViolation::SourceReadFailed {
                path: path.display().to_string(),
                reason: e.to_string(),
            })?;
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("//!") {
                continue;
            }
            if Self::line_has_format_sql(trimmed) {
                report.violations.push(SecViolation::FormatMacroUsage {
                    line: line_num + 1,
                    snippet: trimmed.chars().take(80).collect(),
                });
            }
            if Self::line_has_string_concat_sql(trimmed) {
                report.violations.push(SecViolation::StringConcatenation {
                    line: line_num + 1,
                    snippet: trimmed.chars().take(80).collect(),
                });
            }
        }
        Ok(())
    }

    fn has_unparameterized_condition(where_clause: &str) -> bool {
        let lower = where_clause.to_lowercase();
        if lower.contains("= ?") || lower.contains("= $") || lower.contains("= :") {
            return false;
        }
        let patterns = [
            "= '", "= \"", "= 0", "= 1", "= 2", "= 3", "= 4", "= 5", "= 6", "= 7", "= 8", "= 9",
        ];
        for p in &patterns {
            if lower.contains(p) {
                return true;
            }
        }
        false
    }

    fn line_has_format_sql(line: &str) -> bool {
        let lower = line.to_lowercase();
        (lower.contains("format!") || lower.contains("format_args!"))
            && (lower.contains("where") || lower.contains("select") || lower.contains("insert"))
    }

    fn line_has_string_concat_sql(line: &str) -> bool {
        let lower = line.to_lowercase();
        (lower.contains("+ \"") || lower.contains("\" +"))
            && (lower.contains("where") || lower.contains("sql"))
    }
}

impl Default for NL2SQLSecScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_ai::nl2sql::SqlQuery;

    fn make_query(sql: &str) -> SqlQuery {
        SqlQuery {
            sql: sql.into(),
            explanation: "test".into(),
            confidence: 1.0,
            dialect: None,
            cache_hit: false,
        }
    }

    #[test]
    fn parameterized_query_passes_scan() {
        let q = make_query("SELECT * FROM users WHERE id = ?");
        assert!(NL2SQLSecScanner::scan_output_path(&q).is_ok());
    }

    #[test]
    fn unparameterized_where_detected() {
        let q = make_query("SELECT * FROM users WHERE name = 'admin'");
        let result = NL2SQLSecScanner::scan_output_path(&q);
        assert!(matches!(
            result,
            Err(SecViolation::UnparameterizedWhere { .. })
        ));
    }

    #[test]
    fn no_where_clause_passes() {
        let q = make_query("SELECT 1");
        assert!(NL2SQLSecScanner::scan_output_path(&q).is_ok());
    }

    #[test]
    fn numeric_literal_in_where_detected() {
        let q = make_query("SELECT * FROM orders WHERE amount = 100");
        let result = NL2SQLSecScanner::scan_output_path(&q);
        assert!(matches!(
            result,
            Err(SecViolation::UnparameterizedWhere { .. })
        ));
    }

    #[test]
    fn dollar_placeholder_passes() {
        let q = make_query("SELECT * FROM users WHERE id = $1 AND status = $2");
        assert!(NL2SQLSecScanner::scan_output_path(&q).is_ok());
    }

    #[test]
    fn colon_placeholder_passes() {
        let q = make_query("SELECT * FROM users WHERE id = :user_id");
        assert!(NL2SQLSecScanner::scan_output_path(&q).is_ok());
    }

    #[test]
    fn format_macro_in_source_detected() {
        let temp = std::env::temp_dir().join("sz_orm_sec_test_format");
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("test.rs");
        std::fs::write(
            &file,
            "fn main() {\n    let sql = format!(\"SELECT * FROM users WHERE id = {}\", id);\n}\n",
        )
        .unwrap();
        let report = NL2SQLSecScanner::scan_source_dir(&temp).unwrap();
        assert!(!report.is_clean());
        assert!(report
            .violations
            .iter()
            .any(|v| matches!(v, SecViolation::FormatMacroUsage { .. })));
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn clean_source_passes_scan() {
        let temp = std::env::temp_dir().join("sz_orm_sec_test_clean");
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("clean.rs");
        std::fs::write(
            &file,
            "fn query() -> String {\n    \"SELECT * FROM users WHERE id = ?\".to_string()\n}\n",
        )
        .unwrap();
        let report = NL2SQLSecScanner::scan_source_dir(&temp).unwrap();
        assert!(report.is_clean());
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
