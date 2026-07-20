//! SQL 安全工具：标识符与时间桶校验
//!
//! v0.2.2 引入：为 `stub` / `memory` / `real_timescale` 提供统一的 SQL 注入防护原语。
//! 所有需要拼接 SQL 标识符（表名/列名/视图名/指标名）的位置必须先经
//! `validate_identifier` 校验；时间桶（time_bucket）参数必须经
//! `validate_time_bucket` 校验。

use crate::error::TimescaleError;

/// 校验 SQL 标识符（表名/列名/视图名/指标名）
///
/// 仅允许 ASCII 字母数字 + 下划线，不以数字开头，长度 1-63（PostgreSQL 限制）。
/// 拒绝任何 SQL 元字符（引号、分号、空格、注释、引号转义等），杜绝 SQL 注入。
pub fn validate_identifier(name: &str, kind: &str) -> Result<(), TimescaleError> {
    if name.is_empty() || name.len() > 63 {
        return Err(TimescaleError::InvalidConfig(format!(
            "invalid {}: empty or too long (max 63 chars): {:?}",
            kind, name
        )));
    }
    let mut chars = name.chars();
    let first = chars.next().expect("non-empty checked above");
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(TimescaleError::InvalidConfig(format!(
            "invalid {}: must start with ASCII letter or underscore, got {:?}",
            kind, name
        )));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(TimescaleError::InvalidConfig(format!(
            "invalid {}: only ASCII alphanumeric and underscore allowed, got {:?}",
            kind, name
        )));
    }
    Ok(())
}

/// 校验 time_bucket 参数（如 "5m" / "1h" / "1d"）
///
/// 仅允许 `<正整数><单位>` 格式，单位仅支持 TimescaleDB 标准单位：
/// s/m/h/d/w。拒绝任何其他字符，防止 SQL 注入。
pub fn validate_time_bucket(bucket: &str) -> Result<(), TimescaleError> {
    if bucket.is_empty() || bucket.len() > 16 {
        return Err(TimescaleError::InvalidConfig(format!(
            "invalid time_bucket: empty or too long (max 16 chars): {:?}",
            bucket
        )));
    }
    let mut chars = bucket.chars();
    // 必须以数字开头
    let first = chars.next().expect("non-empty checked above");
    if !first.is_ascii_digit() {
        return Err(TimescaleError::InvalidConfig(format!(
            "invalid time_bucket: must start with digit, got {:?}",
            bucket
        )));
    }
    // 中间必须是数字
    let digits_end = bucket
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(bucket.len());
    let _digits = &bucket[..digits_end];
    // 剩余部分必须是单个合法单位字符
    let unit = &bucket[digits_end..];
    const ALLOWED_UNITS: &[&str] = &["s", "m", "h", "d", "w"];
    if !ALLOWED_UNITS.contains(&unit) {
        return Err(TimescaleError::InvalidConfig(format!(
            "invalid time_bucket: unit {:?} not allowed, allowed: {:?} (got {:?})",
            unit, ALLOWED_UNITS, bucket
        )));
    }
    Ok(())
}

/// 校验 `CREATE MATERIALIZED VIEW ... AS <query>` 中的 query 参数
///
/// v0.2.2 修复 P0-9（第二次审查发现）：原实现直接拼接 query 到 SQL，存在 SQL 注入风险。
/// 本函数对 query 做严格白名单式校验：
///
/// 1. 长度限制：1..=4096 字节（足够覆盖连续聚合的 SELECT 语句）
/// 2. 必须以 `SELECT` 关键字开头（大小写不敏感，前后允许空白）
/// 3. 禁止分号 `;`（防止多语句注入）
/// 4. 禁止行注释 `--` 和块注释 `/*` `*/`
/// 5. 禁止危险 DDL/DML 关键字（DROP/DELETE/UPDATE/INSERT/ALTER/TRUNCATE/CREATE/GRANT/REVOKE/EXEC/MERGE）
///    —— 在 word boundary 处匹配，避免误伤合法列名（如 `updated_at`）
///
/// 注意：本函数是**防御性校验**，不替代完整 SQL 解析器。对于复杂的连续聚合 query
/// （含子查询、JOIN、CTE 等），建议调用方在应用层使用 `sqlparser` crate 做 AST 级校验。
pub fn validate_continuous_aggregate_query(query: &str) -> Result<(), TimescaleError> {
    if query.is_empty() {
        return Err(TimescaleError::InvalidConfig(
            "invalid continuous aggregate query: empty".to_string(),
        ));
    }
    if query.len() > 4096 {
        return Err(TimescaleError::InvalidConfig(format!(
            "invalid continuous aggregate query: too long (max 4096 chars, got {})",
            query.len()
        )));
    }
    let trimmed = query.trim_start();
    let upper_trimmed = trimmed.to_ascii_uppercase();
    // 必须以 SELECT 或 WITH（CTE）开头（大小写不敏感）
    // SELECT 后必须跟空白或 (；WITH 后必须跟空白
    let starts_with_select = upper_trimmed.starts_with("SELECT")
        && trimmed
            .as_bytes()
            .get(6)
            .map(|&b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'(')
            .unwrap_or(true);
    let starts_with_with = upper_trimmed.starts_with("WITH")
        && trimmed
            .as_bytes()
            .get(4)
            .map(|&b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r')
            .unwrap_or(true);
    if !starts_with_select && !starts_with_with {
        return Err(TimescaleError::InvalidConfig(format!(
            "invalid continuous aggregate query: must start with SELECT or WITH keyword, got {:?}",
            &query[..query.len().min(40)]
        )));
    }
    // 禁止分号（多语句注入防护）
    if query.contains(';') {
        return Err(TimescaleError::InvalidConfig(
            "invalid continuous aggregate query: semicolon ';' is forbidden (multi-statement injection prevention)".to_string(),
        ));
    }
    // 禁止行注释和块注释
    if query.contains("--") || query.contains("/*") || query.contains("*/") {
        return Err(TimescaleError::InvalidConfig(
            "invalid continuous aggregate query: SQL comments (--, /*, */) are forbidden"
                .to_string(),
        ));
    }
    // 禁止危险 DDL/DML 关键字（word boundary 匹配，大小写不敏感）
    let upper = query.to_ascii_uppercase();
    const FORBIDDEN_KEYWORDS: &[&str] = &[
        "DROP", "DELETE", "UPDATE", "INSERT", "ALTER", "TRUNCATE", "CREATE", "GRANT", "REVOKE",
        "EXEC", "MERGE", "CALL", "VACUUM", "REINDEX", "CLUSTER", "ATTACH", "DETACH",
    ];
    for kw in FORBIDDEN_KEYWORDS {
        if contains_keyword_word_boundary(&upper, kw) {
            return Err(TimescaleError::InvalidConfig(format!(
                "invalid continuous aggregate query: forbidden SQL keyword {:?} detected (DDL/DML in continuous aggregate query is not allowed)",
                kw
            )));
        }
    }
    Ok(())
}

/// 在 `haystack`（已 upper-case）中检查是否包含 `keyword`（已 upper-case）作为独立单词
///
/// 使用 word boundary 判定：关键字前后字符必须是非字母数字下划线（或字符串边界）。
/// 这样 `updated_at` 不会被误判为含 `UPDATE`，`inserted_at` 不会被误判为含 `INSERT`。
fn contains_keyword_word_boundary(haystack: &str, keyword: &str) -> bool {
    let bytes = haystack.as_bytes();
    let kw_bytes = keyword.as_bytes();
    if kw_bytes.is_empty() || bytes.len() < kw_bytes.len() {
        return false;
    }
    let mut i = 0usize;
    while i + kw_bytes.len() <= bytes.len() {
        if &bytes[i..i + kw_bytes.len()] == kw_bytes {
            let before_ok = i == 0 || {
                let b = bytes[i - 1];
                !(b.is_ascii_alphanumeric() || b == b'_')
            };
            let after_pos = i + kw_bytes.len();
            let after_ok = after_pos >= bytes.len() || {
                let b = bytes[after_pos];
                !(b.is_ascii_alphanumeric() || b == b'_')
            };
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("cpu_usage", "metric").is_ok());
        assert!(validate_identifier("_internal", "metric").is_ok());
        assert!(validate_identifier("m_2026", "metric").is_ok());
    }

    #[test]
    fn test_validate_identifier_injection_attempts() {
        assert!(validate_identifier("cpu; DROP TABLE", "metric").is_err());
        assert!(validate_identifier("cpu'--", "metric").is_err());
        assert!(validate_identifier("cpu\"x", "metric").is_err());
        assert!(validate_identifier("cpu OR 1=1", "metric").is_err());
        assert!(validate_identifier("1cpu", "metric").is_err());
        assert!(validate_identifier("", "metric").is_err());
        assert!(validate_identifier(&"a".repeat(64), "metric").is_err());
    }

    #[test]
    fn test_validate_time_bucket_valid() {
        assert!(validate_time_bucket("5m").is_ok());
        assert!(validate_time_bucket("1h").is_ok());
        assert!(validate_time_bucket("1d").is_ok());
        assert!(validate_time_bucket("30s").is_ok());
        assert!(validate_time_bucket("2w").is_ok());
        assert!(validate_time_bucket("60m").is_ok());
    }

    #[test]
    fn test_validate_time_bucket_injection_attempts() {
        assert!(validate_time_bucket("5m; DROP TABLE").is_err());
        assert!(validate_time_bucket("' OR '1'='1").is_err());
        assert!(validate_time_bucket("m").is_err()); // 缺数字
        assert!(validate_time_bucket("5x").is_err()); // 非法单位
        assert!(validate_time_bucket("").is_err());
        assert!(validate_time_bucket("5").is_err()); // 缺单位
        assert!(validate_time_bucket("5mm").is_err()); // 多字符单位
        assert!(validate_time_bucket("5m'--").is_err());
    }

    // ===== v0.2.2 P0-9 修复（第二次审查发现）=====

    #[test]
    fn test_validate_continuous_aggregate_query_valid_basic() {
        assert!(validate_continuous_aggregate_query(
            "SELECT time_bucket('1h', ts) AS bucket, avg(value) FROM metrics GROUP BY bucket"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_valid_lowercase_select() {
        assert!(validate_continuous_aggregate_query(
            "select time_bucket('1h', ts) AS bucket, max(value) from metrics group by bucket"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_valid_leading_whitespace() {
        assert!(validate_continuous_aggregate_query(
            "  \n\tSELECT time_bucket('1h', ts), min(value) FROM metrics GROUP BY 1"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_valid_with_subquery() {
        // 合法子查询（不含禁止关键字）
        assert!(
            validate_continuous_aggregate_query(
                "SELECT time_bucket('1h', ts) AS b, avg(v) FROM (SELECT ts, v FROM raw_metrics) sub GROUP BY b"
            )
            .is_ok()
        );
    }

    #[test]
    fn test_validate_continuous_aggregate_query_valid_with_cte() {
        // 合法 WITH ... AS（不是 WITH CHECK，是 CTE）
        // 注：本函数禁止的是 DDL/DML 关键字，WITH 不在禁列
        assert!(
            validate_continuous_aggregate_query(
                "WITH agg AS (SELECT ts, v FROM metrics) SELECT time_bucket('1h', ts), avg(v) FROM agg GROUP BY 1"
            )
            .is_ok()
        );
    }

    #[test]
    fn test_validate_continuous_aggregate_query_rejects_not_select() {
        // 不以 SELECT/WITH 开头
        assert!(
            validate_continuous_aggregate_query("WITH x AS (SELECT 1) SELECT * FROM x").is_ok()
        ); // WITH 开头（CTE）合法
        assert!(validate_continuous_aggregate_query("FROM metrics SELECT *").is_err());
        assert!(validate_continuous_aggregate_query("SHOW TABLES").is_err());
        assert!(validate_continuous_aggregate_query("EXPLAIN SELECT 1").is_err());
        assert!(validate_continuous_aggregate_query("WITHIN").is_err()); // WITHIN 不是 WITH
        assert!(validate_continuous_aggregate_query("SELECTED_COLS").is_err()); // SELECTED 不是 SELECT
    }

    #[test]
    fn test_validate_continuous_aggregate_query_rejects_select_as_substring() {
        // SELECT 必须是独立关键字，不是列名的一部分
        assert!(validate_continuous_aggregate_query("SELECTED_COLS").is_err());
        assert!(validate_continuous_aggregate_query("SELECTOR").is_err());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_rejects_semicolon() {
        // 多语句注入
        assert!(
            validate_continuous_aggregate_query("SELECT * FROM metrics; DROP TABLE metrics")
                .is_err()
        );
        assert!(validate_continuous_aggregate_query("SELECT 1;").is_err());
        assert!(validate_continuous_aggregate_query("; SELECT 1").is_err());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_rejects_line_comment() {
        assert!(validate_continuous_aggregate_query("SELECT 1 -- comment").is_err());
        assert!(validate_continuous_aggregate_query("SELECT 1--").is_err());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_rejects_block_comment() {
        assert!(validate_continuous_aggregate_query("SELECT /* comment */ 1").is_err());
        assert!(validate_continuous_aggregate_query("SELECT 1 /* inline */").is_err());
        assert!(validate_continuous_aggregate_query("SELECT 1 */").is_err());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_rejects_ddl_keywords() {
        // 禁止 DDL/DML 关键字（word boundary）
        assert!(
            validate_continuous_aggregate_query("SELECT * FROM metrics WHERE x = DROP").is_err()
        );
        assert!(validate_continuous_aggregate_query(
            "SELECT * FROM metrics UNION DELETE FROM other"
        )
        .is_err());
        assert!(validate_continuous_aggregate_query(
            "SELECT * FROM metrics WHERE v > 0 UPDATE metrics SET v=1"
        )
        .is_err());
        assert!(validate_continuous_aggregate_query("SELECT INSERT(1,2,3)").is_err());
        assert!(validate_continuous_aggregate_query(
            "SELECT * FROM metrics ALTER COLUMN v TYPE BIGINT"
        )
        .is_err());
        assert!(validate_continuous_aggregate_query("SELECT * FROM metrics TRUNCATE").is_err());
        assert!(validate_continuous_aggregate_query("SELECT * FROM metrics CREATE INDEX").is_err());
        assert!(validate_continuous_aggregate_query("SELECT * FROM metrics GRANT ALL").is_err());
        assert!(validate_continuous_aggregate_query("SELECT * FROM metrics REVOKE ALL").is_err());
        assert!(validate_continuous_aggregate_query("SELECT * FROM metrics EXEC fn()").is_err());
        assert!(validate_continuous_aggregate_query("SELECT * FROM metrics MERGE INTO x").is_err());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_allows_safe_column_names() {
        // 列名包含关键字子串但非独立关键字，应通过
        assert!(validate_continuous_aggregate_query("SELECT updated_at FROM metrics").is_ok());
        assert!(validate_continuous_aggregate_query("SELECT inserted_at FROM metrics").is_ok());
        assert!(validate_continuous_aggregate_query("SELECT deleted_count FROM metrics").is_ok());
        assert!(validate_continuous_aggregate_query("SELECT created_by FROM metrics").is_ok());
        assert!(validate_continuous_aggregate_query("SELECT truncate_flag FROM metrics").is_ok());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_rejects_empty() {
        assert!(validate_continuous_aggregate_query("").is_err());
    }

    #[test]
    fn test_validate_continuous_aggregate_query_rejects_too_long() {
        let long = format!("SELECT {}", "a, ".repeat(3000));
        assert!(validate_continuous_aggregate_query(&long).is_err());
    }

    #[test]
    fn test_contains_keyword_word_boundary() {
        assert!(contains_keyword_word_boundary("SELECT DROP TABLE", "DROP"));
        assert!(contains_keyword_word_boundary("SELECT DROP", "DROP"));
        assert!(contains_keyword_word_boundary("DROP SELECT", "DROP"));
        assert!(!contains_keyword_word_boundary("SELECTED", "SELECT")); // SELECTED 不是 SELECT
        assert!(!contains_keyword_word_boundary("updated_at", "UPDATE"));
        assert!(!contains_keyword_word_boundary("inserted_at", "INSERT"));
        assert!(!contains_keyword_word_boundary("deleted_col", "DELETE"));
        assert!(!contains_keyword_word_boundary("truncate_flag", "TRUNCATE"));
        assert!(contains_keyword_word_boundary("x DROP y", "DROP"));
        assert!(contains_keyword_word_boundary("x(DROP)y", "DROP"));
        assert!(!contains_keyword_word_boundary("", "DROP"));
        assert!(!contains_keyword_word_boundary("SELECT 1", "DROP"));
    }
}
