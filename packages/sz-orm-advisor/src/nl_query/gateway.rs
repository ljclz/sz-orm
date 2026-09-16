//! NL 查询网关
//!
//! 统一入口，编排 SQL 生成 → 安全校验 → 结果格式化。

use super::formatter::{FormattedResult, NlResultFormatter};
use super::intent_cache::IntentCache;
use super::safety_gate::{NlQuerySafetyGate, SafetyVerdict};

/// NL 查询处理结果
#[derive(Debug, Clone)]
pub struct NlQueryResult {
    /// 原始自然语言
    pub nl_query: String,
    /// 生成的 SQL
    pub sql: String,
    /// 安全判定
    pub verdict: SafetyVerdict,
    /// 是否通过安全校验
    pub is_safe: bool,
    /// 格式化后的结果
    pub formatted: Option<FormattedResult>,
}

/// NL 查询网关
pub struct NlQueryGateway {
    safety_gate: NlQuerySafetyGate,
    formatter: NlResultFormatter,
    intent_cache: IntentCache<String>,
}

impl Default for NlQueryGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl NlQueryGateway {
    /// 创建网关
    pub fn new() -> Self {
        Self {
            safety_gate: NlQuerySafetyGate::new(),
            formatter: NlResultFormatter::default(),
            intent_cache: IntentCache::default(),
        }
    }

    /// 允许 DDL 操作
    pub fn allow_ddl(mut self) -> Self {
        self.safety_gate = self.safety_gate.allow_ddl();
        self
    }

    /// 设置最大显示行数
    pub fn max_display_rows(mut self, rows: usize) -> Self {
        self.formatter = NlResultFormatter::new(rows);
        self
    }

    /// 处理已生成的 SQL（同步入口）
    ///
    /// 接受自然语言查询和对应的 SQL，执行安全校验和格式化。
    pub fn process_sql(&mut self, nl_query: &str, sql: &str) -> NlQueryResult {
        if let Some(cached) = self.intent_cache.get(nl_query) {
            let verdict = self.safety_gate.validate(&cached);
            let is_safe = matches!(verdict, SafetyVerdict::Pass);
            return NlQueryResult {
                nl_query: nl_query.to_string(),
                sql: cached,
                verdict,
                is_safe,
                formatted: None,
            };
        }

        let verdict = self.safety_gate.validate(sql);
        let is_safe = matches!(verdict, SafetyVerdict::Pass);

        self.intent_cache
            .insert(nl_query.to_string(), sql.to_string());

        NlQueryResult {
            nl_query: nl_query.to_string(),
            sql: sql.to_string(),
            verdict,
            is_safe,
            formatted: None,
        }
    }

    /// 处理查询结果（带格式化）
    pub fn process_with_result(
        &mut self,
        nl_query: &str,
        sql: &str,
        headers: &[String],
        rows: &[Vec<String>],
    ) -> NlQueryResult {
        let mut result = self.process_sql(nl_query, sql);
        if result.is_safe {
            result.formatted = Some(self.formatter.format(headers, rows));
        }
        result
    }

    /// 格式化结果为 Markdown
    pub fn to_markdown(&self, result: &NlQueryResult) -> Option<String> {
        result
            .formatted
            .as_ref()
            .map(|f| self.formatter.to_markdown(f))
    }

    /// 缓存命中率
    pub fn cache_hit_rate(&self) -> f64 {
        self.intent_cache.hit_rate()
    }

    /// 安全校验门引用
    pub fn safety_gate(&self) -> &NlQuerySafetyGate {
        &self.safety_gate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_safe_sql() {
        let mut gw = NlQueryGateway::new();
        let result = gw.process_sql("查询所有用户", "SELECT * FROM users WHERE id = ?");
        assert!(result.is_safe);
        assert_eq!(result.verdict, SafetyVerdict::Pass);
    }

    #[test]
    fn test_process_injection_blocked() {
        let mut gw = NlQueryGateway::new();
        let result = gw.process_sql(
            "查询用户",
            "SELECT * FROM users WHERE name = 'admin' OR '1'='1",
        );
        assert!(!result.is_safe);
        assert!(matches!(
            result.verdict,
            SafetyVerdict::InjectionDetected(_)
        ));
    }

    #[test]
    fn test_process_ddl_blocked() {
        let mut gw = NlQueryGateway::new();
        let result = gw.process_sql("建表", "CREATE TABLE evil (id INT)");
        assert!(!result.is_safe);
    }

    #[test]
    fn test_process_ddl_allowed() {
        let mut gw = NlQueryGateway::new().allow_ddl();
        let result = gw.process_sql("建表", "CREATE TABLE ok (id INT)");
        assert!(result.is_safe);
    }

    #[test]
    fn test_cache_hit() {
        let mut gw = NlQueryGateway::new();
        gw.process_sql("查询用户", "SELECT * FROM users WHERE id = ?");
        gw.process_sql("查询用户", "SELECT * FROM users WHERE id = ?");
        assert!(gw.cache_hit_rate() > 0.0);
    }

    #[test]
    fn test_process_with_result() {
        let mut gw = NlQueryGateway::new();
        let result = gw.process_with_result(
            "查询用户",
            "SELECT name FROM users WHERE id = ?",
            &["name".into()],
            &[vec!["Alice".into()]],
        );
        assert!(result.is_safe);
        assert!(result.formatted.is_some());
    }

    #[test]
    fn test_process_with_result_unsafe_no_format() {
        let mut gw = NlQueryGateway::new();
        let result = gw.process_with_result(
            "注入",
            "SELECT * FROM users; --",
            &["name".into()],
            &[vec!["Alice".into()]],
        );
        assert!(!result.is_safe);
        assert!(result.formatted.is_none());
    }

    #[test]
    fn test_to_markdown() {
        let mut gw = NlQueryGateway::new();
        let result = gw.process_with_result(
            "查询",
            "SELECT name FROM users WHERE id = ?",
            &["name".into()],
            &[vec!["Alice".into()]],
        );
        let md = gw.to_markdown(&result).unwrap();
        assert!(md.contains("Alice"));
    }
}
