//! EXPLAIN 执行计划解析模块
//!
//! 解析各数据库方言的 EXPLAIN 输出，提取查询性能信号（全表扫描、缺失索引、
//! 临时表、文件排序、索引扫描），为查询优化器提供决策依据。
//!
//! # 支持的方言
//!
//! | 方言 | EXPLAIN 命令 | 全表扫描信号 |
//! |------|-------------|-------------|
//! | MySQL | `EXPLAIN` | `type=ALL` |
//! | PostgreSQL | `EXPLAIN` | `Seq Scan` |
//! | SQLite | `EXPLAIN QUERY PLAN` | `SCAN` |
//! | Oracle | `EXPLAIN PLAN FOR` | `TABLE ACCESS FULL` |
//! | MSSQL | `SET SHOWPLAN_TEXT ON` | `Table Scan` |

use crate::error::AiError;
use serde::{Deserialize, Serialize};

/// EXPLAIN 解析出的性能信号
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExplainSignal {
    /// 全表扫描（无索引或未使用索引）
    FullTableScan,
    /// 缺失索引（WHERE 条件列无索引）
    MissingIndex,
    /// 使用临时表
    UsingTempTable,
    /// 使用文件排序
    UsingFilesort,
    /// 索引扫描
    IndexScan,
}

impl ExplainSignal {
    /// 转换为字符串描述
    pub fn as_str(&self) -> &'static str {
        match self {
            ExplainSignal::FullTableScan => "FullTableScan",
            ExplainSignal::MissingIndex => "MissingIndex",
            ExplainSignal::UsingTempTable => "UsingTempTable",
            ExplainSignal::UsingFilesort => "UsingFilesort",
            ExplainSignal::IndexScan => "IndexScan",
        }
    }
}

/// EXPLAIN 解析器 trait
///
/// 各方言实现此 trait，将 EXPLAIN 输出文本解析为 [`ExplainSignal`] 列表。
pub trait ExplainPlanParser: Send + Sync {
    /// 解析 EXPLAIN 输出，返回检测到的性能信号列表
    ///
    /// # 错误
    ///
    /// 解析失败时返回 [`AiError`]，而非静默忽略。
    fn parse(&self, explain_output: &str) -> Result<Vec<ExplainSignal>, AiError>;

    /// 返回方言名称
    fn dialect(&self) -> &'static str;
}

// ==================== MySQL ====================

/// MySQL EXPLAIN 解析器
///
/// MySQL EXPLAIN 输出为表格格式，关键列：
/// - `type`：`ALL` = 全表扫描，`index` = 索引扫描
/// - `possible_keys`：`NULL` = 无可用索引
/// - `Extra`：`Using temporary` = 临时表，`Using filesort` = 文件排序
pub struct MySqlExplainParser;

impl ExplainPlanParser for MySqlExplainParser {
    fn parse(&self, explain_output: &str) -> Result<Vec<ExplainSignal>, AiError> {
        let mut signals = Vec::new();
        let lines: Vec<&str> = explain_output.lines().collect();
        let has_pipe = lines.iter().any(|l| l.contains('|'));

        if !has_pipe {
            return Err(AiError::Config(
                "MySQL EXPLAIN 解析失败：输出格式不匹配，缺少管道符表格格式".to_string(),
            ));
        }

        for line in &lines {
            let fields: Vec<&str> = line.split('|').map(|f| f.trim()).collect();
            let lower_fields: Vec<String> = fields.iter().map(|f| f.to_lowercase()).collect();

            let has_all = lower_fields.iter().any(|f| f == "all");
            let has_index = lower_fields.iter().any(|f| f == "index");
            let has_possible_keys_null = lower_fields.iter().any(|f| f == "null")
                && lower_fields
                    .iter()
                    .any(|f| f == "possible_keys" || f.is_empty());

            if has_all {
                signals.push(ExplainSignal::FullTableScan);
                if has_possible_keys_null || line.to_lowercase().contains("possible_keys") {
                    signals.push(ExplainSignal::MissingIndex);
                }
            }
            if has_index {
                signals.push(ExplainSignal::IndexScan);
            }
            let lower = line.to_lowercase();
            if lower.contains("using temporary") {
                signals.push(ExplainSignal::UsingTempTable);
            }
            if lower.contains("using filesort") {
                signals.push(ExplainSignal::UsingFilesort);
            }
        }

        signals.dedup();
        Ok(signals)
    }

    fn dialect(&self) -> &'static str {
        "mysql"
    }
}

// ==================== PostgreSQL ====================

/// PostgreSQL EXPLAIN 解析器
///
/// PostgreSQL EXPLAIN 输出为缩进文本格式：
/// - `Seq Scan` = 全表扫描
/// - `Index Scan` / `Index Only Scan` = 索引扫描
/// - `Sort` = 排序（可能使用文件排序）
/// - `Temporary` = 临时表
pub struct PgExplainParser;

impl ExplainPlanParser for PgExplainParser {
    fn parse(&self, explain_output: &str) -> Result<Vec<ExplainSignal>, AiError> {
        let mut signals = Vec::new();
        let lower = explain_output.to_lowercase();

        if !lower.contains("scan") && !lower.contains("cost") && !lower.contains("plan") {
            return Err(AiError::Config(
                "PostgreSQL EXPLAIN 解析失败：输出格式不匹配".to_string(),
            ));
        }

        if lower.contains("seq scan") {
            signals.push(ExplainSignal::FullTableScan);
            signals.push(ExplainSignal::MissingIndex);
        }
        if lower.contains("index scan") || lower.contains("index only scan") {
            signals.push(ExplainSignal::IndexScan);
        }
        if lower.contains("sort") && !lower.contains("index sort") {
            signals.push(ExplainSignal::UsingFilesort);
        }
        if lower.contains("temporary") || lower.contains("temp table") {
            signals.push(ExplainSignal::UsingTempTable);
        }

        signals.dedup();
        Ok(signals)
    }

    fn dialect(&self) -> &'static str {
        "postgresql"
    }
}

// ==================== SQLite ====================

/// SQLite EXPLAIN QUERY PLAN 解析器
///
/// SQLite 输出格式：
/// - `SCAN` = 全表扫描
/// - `SEARCH` = 使用索引扫描
/// - `USE TEMP B-TREE` = 临时表/排序
pub struct SqliteExplainParser;

impl ExplainPlanParser for SqliteExplainParser {
    fn parse(&self, explain_output: &str) -> Result<Vec<ExplainSignal>, AiError> {
        let mut signals = Vec::new();
        let lower = explain_output.to_lowercase();

        if !lower.contains("scan") && !lower.contains("search") && !lower.contains("explain") {
            return Err(AiError::Config(
                "SQLite EXPLAIN 解析失败：输出格式不匹配".to_string(),
            ));
        }

        for line in lower.lines() {
            if line.contains("scan") && !line.contains("search") {
                signals.push(ExplainSignal::FullTableScan);
                signals.push(ExplainSignal::MissingIndex);
            }
            if line.contains("search") {
                signals.push(ExplainSignal::IndexScan);
            }
            if line.contains("temp") || line.contains("temporary") {
                signals.push(ExplainSignal::UsingTempTable);
            }
            if line.contains("b-tree") || line.contains("sort") {
                signals.push(ExplainSignal::UsingFilesort);
            }
        }

        signals.dedup();
        Ok(signals)
    }

    fn dialect(&self) -> &'static str {
        "sqlite"
    }
}

// ==================== Oracle ====================

/// Oracle EXPLAIN PLAN FOR 解析器
///
/// Oracle 输出格式（通过 PLAN_TABLE）：
/// - `TABLE ACCESS FULL` = 全表扫描
/// - `INDEX` / `INDEX RANGE SCAN` = 索引扫描
/// - `SORT` = 排序
/// - `TEMP TABLE` = 临时表
pub struct OracleExplainParser;

impl ExplainPlanParser for OracleExplainParser {
    fn parse(&self, explain_output: &str) -> Result<Vec<ExplainSignal>, AiError> {
        let mut signals = Vec::new();
        let lower = explain_output.to_lowercase();

        if !lower.contains("access") && !lower.contains("scan") && !lower.contains("operation") {
            return Err(AiError::Config(
                "Oracle EXPLAIN 解析失败：输出格式不匹配".to_string(),
            ));
        }

        if lower.contains("table access full") {
            signals.push(ExplainSignal::FullTableScan);
            signals.push(ExplainSignal::MissingIndex);
        }
        if lower.contains("index range scan")
            || lower.contains("index unique scan")
            || lower.contains("index full scan")
        {
            signals.push(ExplainSignal::IndexScan);
        }
        if lower.contains("sort") {
            signals.push(ExplainSignal::UsingFilesort);
        }
        if lower.contains("temp") {
            signals.push(ExplainSignal::UsingTempTable);
        }

        signals.dedup();
        Ok(signals)
    }

    fn dialect(&self) -> &'static str {
        "oracle"
    }
}

// ==================== MSSQL ====================

/// MSSQL SET SHOWPLAN_TEXT ON 解析器
///
/// MSSQL 输出格式：
/// - `Table Scan` = 全表扫描
/// - `Index Scan` / `Index Seek` = 索引扫描
/// - `Sort` = 排序
/// - `Hash Match` = 哈希匹配（可能使用临时表）
pub struct MssqlExplainParser;

impl ExplainPlanParser for MssqlExplainParser {
    fn parse(&self, explain_output: &str) -> Result<Vec<ExplainSignal>, AiError> {
        let mut signals = Vec::new();
        let lower = explain_output.to_lowercase();

        if !lower.contains("scan") && !lower.contains("seek") && !lower.contains("plan") {
            return Err(AiError::Config(
                "MSSQL EXPLAIN 解析失败：输出格式不匹配".to_string(),
            ));
        }

        if lower.contains("table scan") {
            signals.push(ExplainSignal::FullTableScan);
            signals.push(ExplainSignal::MissingIndex);
        }
        if lower.contains("index scan") || lower.contains("index seek") {
            signals.push(ExplainSignal::IndexScan);
        }
        if lower.contains("sort") {
            signals.push(ExplainSignal::UsingFilesort);
        }
        if lower.contains("hash match") || lower.contains("temp table") {
            signals.push(ExplainSignal::UsingTempTable);
        }

        signals.dedup();
        Ok(signals)
    }

    fn dialect(&self) -> &'static str {
        "mssql"
    }
}

/// 根据方言名称返回对应的解析器实例
pub fn get_parser(dialect: &str) -> Option<Box<dyn ExplainPlanParser>> {
    match dialect.to_lowercase().as_str() {
        "mysql" | "mariadb" => Some(Box::new(MySqlExplainParser)),
        "postgres" | "postgresql" | "pg" => Some(Box::new(PgExplainParser)),
        "sqlite" => Some(Box::new(SqliteExplainParser)),
        "oracle" => Some(Box::new(OracleExplainParser)),
        "mssql" | "sqlserver" | "sql_server" => Some(Box::new(MssqlExplainParser)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mysql_full_table_scan() {
        let output = "+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+\n| id | select_type | table | partitions | type | possible_keys | key  | key_len | ref  | rows | filtered | Extra |\n+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+\n|  1 | SIMPLE      | users | NULL       | ALL  | NULL          | NULL | NULL    | NULL |  100 |   100.00 | NULL  |\n+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+";
        let parser = MySqlExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::FullTableScan));
        assert!(signals.contains(&ExplainSignal::MissingIndex));
    }

    #[test]
    fn test_mysql_index_scan() {
        let output = "+----+-------------+-------+------------+-------+---------------+---------+---------+-------+------+----------+-------+\n| id | select_type | table | partitions | type  | possible_keys | key     | key_len | ref   | rows | filtered | Extra |\n+----+-------------+-------+------------+-------+---------------+---------+---------+-------+------+----------+-------+\n|  1 | SIMPLE      | users | NULL       | ref   | idx_email     | idx_email | 302     | const |    1 |   100.00 | NULL  |\n+----+-------------+-------+------------+-------+---------------+---------+---------+-------+------+----------+-------+";
        let parser = MySqlExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(!signals.contains(&ExplainSignal::FullTableScan));
    }

    #[test]
    fn test_mysql_using_temp_and_filesort() {
        let output = "|  1 | SIMPLE      | t1    | NULL       | ALL  | NULL          | NULL | NULL    | NULL |  100 |   100.00 | Using temporary; Using filesort |";
        let parser = MySqlExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::UsingTempTable));
        assert!(signals.contains(&ExplainSignal::UsingFilesort));
    }

    #[test]
    fn test_pg_seq_scan() {
        let output = "Seq Scan on users  (cost=0.00..3.00 rows=100 width=4)";
        let parser = PgExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::FullTableScan));
        assert!(signals.contains(&ExplainSignal::MissingIndex));
    }

    #[test]
    fn test_pg_index_scan() {
        let output = "Index Scan using idx_users_email on users  (cost=0.29..8.30 rows=1 width=4)";
        let parser = PgExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::IndexScan));
        assert!(!signals.contains(&ExplainSignal::FullTableScan));
    }

    #[test]
    fn test_pg_sort() {
        let output = "Sort  (cost=69.83..71.33 rows=600 width=4)\n  Sort Key: users.name\n  -> Seq Scan on users";
        let parser = PgExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::UsingFilesort));
    }

    #[test]
    fn test_sqlite_scan() {
        let output = "SCAN users";
        let parser = SqliteExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::FullTableScan));
    }

    #[test]
    fn test_sqlite_search() {
        let output = "SEARCH users USING INDEX idx_users_email (email=?)";
        let parser = SqliteExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::IndexScan));
    }

    #[test]
    fn test_oracle_full_scan() {
        let output = "TABLE ACCESS FULL | USERS";
        let parser = OracleExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::FullTableScan));
    }

    #[test]
    fn test_oracle_index_scan() {
        let output = "INDEX RANGE SCAN | IDX_USERS_EMAIL";
        let parser = OracleExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::IndexScan));
    }

    #[test]
    fn test_mssql_table_scan() {
        let output = "Table Scan\n  Object: users";
        let parser = MssqlExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::FullTableScan));
    }

    #[test]
    fn test_mssql_index_seek() {
        let output = "Index Seek\n  Object: idx_users_email";
        let parser = MssqlExplainParser;
        let signals = parser.parse(output).unwrap();
        assert!(signals.contains(&ExplainSignal::IndexScan));
    }

    #[test]
    fn test_get_parser() {
        assert!(get_parser("mysql").is_some());
        assert!(get_parser("postgresql").is_some());
        assert!(get_parser("sqlite").is_some());
        assert!(get_parser("oracle").is_some());
        assert!(get_parser("mssql").is_some());
        assert!(get_parser("unknown").is_none());
    }

    #[test]
    fn test_parse_error_on_invalid_input() {
        let parser = MySqlExplainParser;
        let result = parser.parse("not a valid explain output");
        assert!(result.is_err());
    }
}
