//! SQL 规则引擎：可组合的规则集合、规则匹配与违规报告
//!
//! 本模块提供基于规则的 SQL 质量检查框架，支持：
//!
//! - **规则定义**（[`SqlRule`] trait）：自定义检查逻辑，返回违规报告
//! - **内置规则**：禁止 `SELECT *`、DELETE/UPDATE 必须带 WHERE、表数量上限、
//!   JOIN 数量上限、禁止 UNION、禁止关键字、正则匹配规则
//! - **规则引擎**（[`RuleEngine`]）：组合多条规则，批量检查 SQL，生成汇总报告
//! - **违规报告**（[`RuleReport`] / [`RuleViolation`]）：按严重级别分类，
//!   支持位置定位与人类可读摘要
//!
//! ## 示例
//!
//! ```rust
//! use sz_orm_sql_validator::sql_rules::{RuleEngine, NoSelectStarRule, RequireWhereInDeleteRule};
//!
//! let mut engine = RuleEngine::new();
//! engine.add_rule(Box::new(NoSelectStarRule));
//! engine.add_rule(Box::new(RequireWhereInDeleteRule));
//!
//! let report = engine.check("DELETE FROM users");
//! assert!(report.has_violations());
//! ```

use crate::{detect_statement_type, tokenize, SqlStatementType, SqlToken};
use regex::Regex;
use std::fmt;

// ============================================================================
// 规则严重级别
// ============================================================================

/// 规则违规的严重级别
///
/// 从低到高依次为 `Info` < `Warning` < `Error` < `Critical`。
/// [`RuleReport::has_errors`] 将 `Error` 与 `Critical` 均视为阻断级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleSeverity {
    /// 信息级：仅提示，不阻断
    Info,
    /// 警告级：潜在问题，建议修正
    Warning,
    /// 错误级：明确违规，应阻断
    Error,
    /// 严重级：高危违规，必须阻断
    Critical,
}

impl RuleSeverity {
    /// 返回级别的英文小写标识，便于日志聚合与序列化
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleSeverity::Info => "info",
            RuleSeverity::Warning => "warning",
            RuleSeverity::Error => "error",
            RuleSeverity::Critical => "critical",
        }
    }

    /// 返回级别的中文描述
    pub fn description(&self) -> &'static str {
        match self {
            RuleSeverity::Info => "信息",
            RuleSeverity::Warning => "警告",
            RuleSeverity::Error => "错误",
            RuleSeverity::Critical => "严重",
        }
    }

    /// 是否为阻断级（Error 或 Critical）
    pub fn is_blocking(&self) -> bool {
        matches!(self, RuleSeverity::Error | RuleSeverity::Critical)
    }
}

impl fmt::Display for RuleSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// 规则违规报告
// ============================================================================

/// 单条规则违规报告
///
/// 由 [`SqlRule::check`] 返回，描述一条 SQL 在某规则下的违规详情。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleViolation {
    /// 触发违规的规则名称
    pub rule_name: String,
    /// 违规严重级别
    pub severity: RuleSeverity,
    /// 人类可读的违规说明
    pub message: String,
    /// 违规在 SQL 文本中的字节偏移（0-based），`None` 表示无法定位
    pub position: Option<usize>,
}

impl RuleViolation {
    /// 创建一条无位置信息的违规
    pub fn new(
        rule_name: impl Into<String>,
        severity: RuleSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule_name: rule_name.into(),
            severity,
            message: message.into(),
            position: None,
        }
    }

    /// 创建一条带位置信息的违规
    pub fn with_position(
        rule_name: impl Into<String>,
        severity: RuleSeverity,
        message: impl Into<String>,
        position: usize,
    ) -> Self {
        Self {
            rule_name: rule_name.into(),
            severity,
            message: message.into(),
            position: Some(position),
        }
    }

    /// 违规是否为阻断级
    pub fn is_blocking(&self) -> bool {
        self.severity.is_blocking()
    }
}

impl fmt::Display for RuleViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.position {
            Some(pos) => write!(
                f,
                "[{}] {} at position {}: {}",
                self.severity, self.rule_name, pos, self.message
            ),
            None => write!(
                f,
                "[{}] {}: {}",
                self.severity, self.rule_name, self.message
            ),
        }
    }
}

// ============================================================================
// 规则匹配上下文
// ============================================================================

/// 规则匹配上下文，封装 SQL 文本及其预计算信息
///
/// 由 [`RuleEngine::check`] 在执行规则前构建，避免每条规则重复词法分析。
#[derive(Debug, Clone)]
pub struct RuleContext {
    /// 原始 SQL 文本
    pub sql: String,
    /// 词法分析后的令牌序列
    pub tokens: Vec<SqlToken>,
    /// 语句类型
    pub statement_type: SqlStatementType,
    /// SQL 全大写形式（便于关键字匹配，避免重复计算）
    pub sql_upper: String,
}

impl RuleContext {
    /// 从 SQL 文本构建上下文
    pub fn from_sql(sql: &str) -> Self {
        Self {
            sql: sql.to_string(),
            tokens: tokenize(sql),
            statement_type: detect_statement_type(sql),
            sql_upper: sql.to_uppercase(),
        }
    }

    /// 统计指定关键字的个数（基于令牌序列，精确匹配关键字令牌）
    pub fn keyword_count(&self, keyword: &str) -> usize {
        let upper = keyword.to_uppercase();
        self.tokens
            .iter()
            .filter(|t| matches!(t, SqlToken::Keyword(k) if *k == upper))
            .count()
    }

    /// 判断是否包含指定关键字
    pub fn has_keyword(&self, keyword: &str) -> bool {
        self.keyword_count(keyword) > 0
    }

    /// 统计 FROM/JOIN/INTO/UPDATE 后的表标识符个数
    pub fn table_count(&self) -> usize {
        let mut count = 0;
        let mut expect_table = false;
        for token in &self.tokens {
            match token {
                SqlToken::Keyword(k)
                    if matches!(k.as_str(), "FROM" | "JOIN" | "INTO" | "UPDATE") =>
                {
                    expect_table = true;
                }
                SqlToken::Identifier(name) if expect_table && !name.starts_with('"') => {
                    // 跳过别名（单个字母后跟 ON 或 WHERE）
                    let _ = name;
                    count += 1;
                    expect_table = false;
                }
                SqlToken::Punctuation('.') if expect_table => {
                    // 保留点号用于限定名
                }
                _ if expect_table => {
                    // 遇到非标识符则取消等待
                    expect_table = false;
                }
                _ => {}
            }
        }
        count
    }
}

// ============================================================================
// 规则 trait
// ============================================================================

/// SQL 规则接口
///
/// 实现者定义一条具体的检查逻辑。规则应为无状态且线程安全（`Send + Sync`），
/// 以便在规则引擎中共享。
pub trait SqlRule: Send + Sync {
    /// 规则名称，需在引擎内唯一
    fn name(&self) -> &str;

    /// 规则的默认严重级别
    fn severity(&self) -> RuleSeverity;

    /// 检查 SQL 上下文，返回 `Some(violation)` 表示违规，`None` 表示通过
    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation>;
}

// ============================================================================
// 内置规则
// ============================================================================

/// 禁止 `SELECT *` 规则
///
/// 检测 `SELECT *` 模式，建议显式列出列名以避免列顺序依赖与全表扫描。
#[derive(Debug, Default)]
pub struct NoSelectStarRule;

impl SqlRule for NoSelectStarRule {
    fn name(&self) -> &str {
        "no_select_star"
    }

    fn severity(&self) -> RuleSeverity {
        RuleSeverity::Warning
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        if ctx.statement_type != SqlStatementType::Select {
            return None;
        }
        // 检测 SELECT 后紧跟 *（tokenize 将 * 归为 Operator）
        let mut after_select = false;
        for token in &ctx.tokens {
            match token {
                SqlToken::Keyword(k) if k == "SELECT" => {
                    after_select = true;
                }
                SqlToken::Operator(op) if after_select && op == "*" => {
                    let pos = ctx.sql.find('*');
                    return Some(RuleViolation::with_position(
                        self.name(),
                        self.severity(),
                        "SELECT * 不允许，请显式列出列名",
                        pos.unwrap_or(0),
                    ));
                }
                _ if after_select => {
                    // 遇到非 * 的令牌则不再跟踪
                    after_select = false;
                }
                _ => {}
            }
        }
        None
    }
}

/// DELETE 必须包含 WHERE 子句规则
///
/// 无 WHERE 的 DELETE 会清空全表，属高危操作。
#[derive(Debug, Default)]
pub struct RequireWhereInDeleteRule;

impl SqlRule for RequireWhereInDeleteRule {
    fn name(&self) -> &str {
        "require_where_in_delete"
    }

    fn severity(&self) -> RuleSeverity {
        RuleSeverity::Critical
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        if ctx.statement_type != SqlStatementType::Delete {
            return None;
        }
        if !ctx.has_keyword("WHERE") {
            Some(RuleViolation::new(
                self.name(),
                self.severity(),
                "DELETE 语句必须包含 WHERE 子句，否则将清空全表",
            ))
        } else {
            None
        }
    }
}

/// UPDATE 必须包含 WHERE 子句规则
///
/// 无 WHERE 的 UPDATE 会更新全表，属高危操作。
#[derive(Debug, Default)]
pub struct RequireWhereInUpdateRule;

impl SqlRule for RequireWhereInUpdateRule {
    fn name(&self) -> &str {
        "require_where_in_update"
    }

    fn severity(&self) -> RuleSeverity {
        RuleSeverity::Critical
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        if ctx.statement_type != SqlStatementType::Update {
            return None;
        }
        if !ctx.has_keyword("WHERE") {
            Some(RuleViolation::new(
                self.name(),
                self.severity(),
                "UPDATE 语句必须包含 WHERE 子句，否则将更新全表",
            ))
        } else {
            None
        }
    }
}

/// 表数量上限规则
///
/// 限制单条 SQL 涉及的表数量（FROM/JOIN/INTO/UPDATE 后的标识符），
/// 防止过度复杂的跨表查询。
#[derive(Debug)]
pub struct MaxTableCountRule {
    /// 允许的最大表数量
    pub max: usize,
}

impl MaxTableCountRule {
    /// 创建规则，指定最大表数量
    pub fn new(max: usize) -> Self {
        Self { max }
    }
}

impl SqlRule for MaxTableCountRule {
    fn name(&self) -> &str {
        "max_table_count"
    }

    fn severity(&self) -> RuleSeverity {
        RuleSeverity::Warning
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        let count = ctx.table_count();
        if count > self.max {
            Some(RuleViolation::new(
                self.name(),
                self.severity(),
                format!("涉及表数量 {} 超过上限 {}", count, self.max),
            ))
        } else {
            None
        }
    }
}

/// JOIN 数量上限规则
#[derive(Debug)]
pub struct MaxJoinCountRule {
    /// 允许的最大 JOIN 数量
    pub max: usize,
}

impl MaxJoinCountRule {
    /// 创建规则，指定最大 JOIN 数量
    pub fn new(max: usize) -> Self {
        Self { max }
    }
}

impl SqlRule for MaxJoinCountRule {
    fn name(&self) -> &str {
        "max_join_count"
    }

    fn severity(&self) -> RuleSeverity {
        RuleSeverity::Warning
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        let count = ctx.keyword_count("JOIN");
        if count > self.max {
            Some(RuleViolation::new(
                self.name(),
                self.severity(),
                format!("JOIN 数量 {} 超过上限 {}", count, self.max),
            ))
        } else {
            None
        }
    }
}

/// 禁止 UNION 规则
///
/// UNION 可能导致结果集不可预测与性能问题，部分场景需禁用。
#[derive(Debug, Default)]
pub struct NoUnionRule;

impl SqlRule for NoUnionRule {
    fn name(&self) -> &str {
        "no_union"
    }

    fn severity(&self) -> RuleSeverity {
        RuleSeverity::Error
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        if ctx.has_keyword("UNION") {
            let pos = ctx.sql_upper.find("UNION");
            Some(RuleViolation::with_position(
                self.name(),
                self.severity(),
                "UNION 操作不允许",
                pos.unwrap_or(0),
            ))
        } else {
            None
        }
    }
}

/// 禁止关键字规则
///
/// 检测 SQL 中是否包含指定的禁止关键字（如 `GRANT`、`REVOKE`、`EXEC`）。
#[derive(Debug)]
pub struct ForbiddenKeywordRule {
    /// 规则名称
    pub rule_name: String,
    /// 禁止的关键字列表（大写）
    pub keywords: Vec<String>,
    /// 违规严重级别
    pub severity: RuleSeverity,
}

impl ForbiddenKeywordRule {
    /// 创建规则，指定名称与关键字列表
    pub fn new(rule_name: impl Into<String>, keywords: &[&str]) -> Self {
        Self {
            rule_name: rule_name.into(),
            keywords: keywords.iter().map(|k| k.to_uppercase()).collect(),
            severity: RuleSeverity::Critical,
        }
    }

    /// 设置违规严重级别
    pub fn with_severity(mut self, severity: RuleSeverity) -> Self {
        self.severity = severity;
        self
    }
}

impl SqlRule for ForbiddenKeywordRule {
    fn name(&self) -> &str {
        &self.rule_name
    }

    fn severity(&self) -> RuleSeverity {
        self.severity
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        for keyword in &self.keywords {
            if ctx.has_keyword(keyword) {
                let pos = ctx.sql_upper.find(keyword.as_str());
                return Some(RuleViolation::with_position(
                    self.name(),
                    self.severity,
                    format!("禁止关键字 {}", keyword),
                    pos.unwrap_or(0),
                ));
            }
        }
        None
    }
}

/// 正则匹配规则
///
/// 使用正则表达式检测 SQL 文本，匹配则视为违规。
/// 适用于自定义模式检测，如敏感表访问、特定函数调用等。
#[derive(Debug)]
pub struct RegexRule {
    /// 规则名称
    pub rule_name: String,
    /// 编译后的正则表达式
    pub pattern: Regex,
    /// 违规严重级别
    pub severity: RuleSeverity,
    /// 违规说明模板
    pub message: String,
}

impl RegexRule {
    /// 创建正则规则，指定名称与正则字符串
    pub fn new(
        rule_name: impl Into<String>,
        pattern: &str,
        severity: RuleSeverity,
        message: impl Into<String>,
    ) -> Result<Self, regex::Error> {
        Ok(Self {
            rule_name: rule_name.into(),
            pattern: Regex::new(pattern)?,
            severity,
            message: message.into(),
        })
    }
}

impl SqlRule for RegexRule {
    fn name(&self) -> &str {
        &self.rule_name
    }

    fn severity(&self) -> RuleSeverity {
        self.severity
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        self.pattern.find(&ctx.sql).map(|m| {
            RuleViolation::with_position(
                self.name(),
                self.severity,
                self.message.clone(),
                m.start(),
            )
        })
    }
}

/// 限制结果集行数规则
///
/// 检测 SELECT 语句是否包含 LIMIT 子句，未包含则报告违规。
/// 适用于防止全表扫描返回过大结果集的场景。
#[derive(Debug, Default)]
pub struct RequireLimitRule;

impl SqlRule for RequireLimitRule {
    fn name(&self) -> &str {
        "require_limit"
    }

    fn severity(&self) -> RuleSeverity {
        RuleSeverity::Info
    }

    fn check(&self, ctx: &RuleContext) -> Option<RuleViolation> {
        if ctx.statement_type != SqlStatementType::Select {
            return None;
        }
        // 子查询中的 SELECT 不要求 LIMIT，仅检查顶层
        if !ctx.has_keyword("LIMIT") && !ctx.has_keyword("FETCH") {
            Some(RuleViolation::new(
                self.name(),
                self.severity(),
                "SELECT 语句建议包含 LIMIT 子句以限制结果集大小",
            ))
        } else {
            None
        }
    }
}

// ============================================================================
// 规则报告
// ============================================================================

/// 规则检查汇总报告
#[derive(Debug, Clone, Default)]
pub struct RuleReport {
    /// 所有违规项
    pub violations: Vec<RuleViolation>,
}

impl RuleReport {
    /// 是否无违规
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    /// 是否存在违规
    pub fn has_violations(&self) -> bool {
        !self.violations.is_empty()
    }

    /// 是否存在阻断级违规（Error 或 Critical）
    pub fn has_blocking(&self) -> bool {
        self.violations.iter().any(|v| v.is_blocking())
    }

    /// 是否存在 Error 级违规
    pub fn has_errors(&self) -> bool {
        self.violations
            .iter()
            .any(|v| v.severity == RuleSeverity::Error)
    }

    /// 是否存在 Critical 级违规
    pub fn has_critical(&self) -> bool {
        self.violations
            .iter()
            .any(|v| v.severity == RuleSeverity::Critical)
    }

    /// 返回指定级别的违规数量
    pub fn count_by_severity(&self, severity: RuleSeverity) -> usize {
        self.violations
            .iter()
            .filter(|v| v.severity == severity)
            .count()
    }

    /// 返回指定级别的违规引用列表
    pub fn violations_by_severity(&self, severity: RuleSeverity) -> Vec<&RuleViolation> {
        self.violations
            .iter()
            .filter(|v| v.severity == severity)
            .collect()
    }

    /// 返回阻断级违规引用列表
    pub fn blocking_violations(&self) -> Vec<&RuleViolation> {
        self.violations.iter().filter(|v| v.is_blocking()).collect()
    }

    /// 违规总数
    pub fn violation_count(&self) -> usize {
        self.violations.len()
    }

    /// 生成人类可读的摘要报告
    pub fn summary(&self) -> String {
        if self.is_clean() {
            return "规则检查通过，无违规".to_string();
        }
        let mut lines = Vec::with_capacity(self.violations.len() + 2);
        lines.push(format!(
            "规则检查完成：共 {} 条违规（{} 信息，{} 警告，{} 错误，{} 严重）",
            self.violation_count(),
            self.count_by_severity(RuleSeverity::Info),
            self.count_by_severity(RuleSeverity::Warning),
            self.count_by_severity(RuleSeverity::Error),
            self.count_by_severity(RuleSeverity::Critical),
        ));
        for v in &self.violations {
            lines.push(format!("  - {}", v));
        }
        lines.join("\n")
    }
}

impl fmt::Display for RuleReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

// ============================================================================
// 规则引擎
// ============================================================================

/// 规则引擎，管理规则集合并批量执行检查
///
/// 规则按添加顺序执行，所有规则均会运行（不短路），
/// 以便一次性收集全部违规。
pub struct RuleEngine {
    rules: Vec<Box<dyn SqlRule>>,
}

impl RuleEngine {
    /// 创建空规则引擎
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 创建包含默认规则集的引擎
    ///
    /// 默认规则集包含：
    /// - [`NoSelectStarRule`]
    /// - [`RequireWhereInDeleteRule`]
    /// - [`RequireWhereInUpdateRule`]
    /// - [`NoUnionRule`]
    /// - [`ForbiddenKeywordRule`]（禁止 GRANT/REVOKE/EXEC/EXECUTE）
    pub fn with_default_rules() -> Self {
        let mut engine = Self::new();
        engine.add_rule(Box::new(NoSelectStarRule));
        engine.add_rule(Box::new(RequireWhereInDeleteRule));
        engine.add_rule(Box::new(RequireWhereInUpdateRule));
        engine.add_rule(Box::new(NoUnionRule));
        engine.add_rule(Box::new(ForbiddenKeywordRule::new(
            "forbidden_privilege_ops",
            &["GRANT", "REVOKE", "EXEC", "EXECUTE"],
        )));
        engine
    }

    /// 添加一条规则
    pub fn add_rule(&mut self, rule: Box<dyn SqlRule>) -> &mut Self {
        self.rules.push(rule);
        self
    }

    /// 返回规则数量
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 对单条 SQL 执行所有规则，返回汇总报告
    pub fn check(&self, sql: &str) -> RuleReport {
        let ctx = RuleContext::from_sql(sql);
        let violations = self
            .rules
            .iter()
            .filter_map(|rule| rule.check(&ctx))
            .collect();
        RuleReport { violations }
    }

    /// 对多条 SQL 批量执行检查，返回每条 SQL 的报告
    pub fn check_batch<'a>(&self, sqls: impl IntoIterator<Item = &'a str>) -> Vec<RuleReport> {
        sqls.into_iter().map(|sql| self.check(sql)).collect()
    }

    /// 检查 SQL 是否通过所有规则（无阻断级违规）
    pub fn passes(&self, sql: &str) -> bool {
        !self.check(sql).has_blocking()
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 规则集预设
// ============================================================================

/// 规则集预设，提供常见场景的规则组合
pub struct RulePresets;

impl RulePresets {
    /// 生产环境严格规则集
    ///
    /// 包含默认规则外加：
    /// - 表数量上限 5
    /// - JOIN 数量上限 3
    /// - 要求 LIMIT
    pub fn strict() -> RuleEngine {
        let mut engine = RuleEngine::with_default_rules();
        engine.add_rule(Box::new(MaxTableCountRule::new(5)));
        engine.add_rule(Box::new(MaxJoinCountRule::new(3)));
        engine.add_rule(Box::new(RequireLimitRule));
        engine
    }

    /// 只读查询规则集
    ///
    /// 仅检查 SELECT 相关规则，不检查 DML。
    pub fn read_only() -> RuleEngine {
        let mut engine = RuleEngine::new();
        engine.add_rule(Box::new(NoSelectStarRule));
        engine.add_rule(Box::new(NoUnionRule));
        engine.add_rule(Box::new(MaxTableCountRule::new(10)));
        engine.add_rule(Box::new(MaxJoinCountRule::new(5)));
        engine
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- RuleSeverity 测试 ----

    #[test]
    fn test_rule_severity_ordering() {
        assert!(RuleSeverity::Info < RuleSeverity::Warning);
        assert!(RuleSeverity::Warning < RuleSeverity::Error);
        assert!(RuleSeverity::Error < RuleSeverity::Critical);
    }

    #[test]
    fn test_rule_severity_as_str() {
        assert_eq!(RuleSeverity::Info.as_str(), "info");
        assert_eq!(RuleSeverity::Warning.as_str(), "warning");
        assert_eq!(RuleSeverity::Error.as_str(), "error");
        assert_eq!(RuleSeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_rule_severity_description() {
        assert_eq!(RuleSeverity::Info.description(), "信息");
        assert_eq!(RuleSeverity::Critical.description(), "严重");
    }

    #[test]
    fn test_rule_severity_is_blocking() {
        assert!(!RuleSeverity::Info.is_blocking());
        assert!(!RuleSeverity::Warning.is_blocking());
        assert!(RuleSeverity::Error.is_blocking());
        assert!(RuleSeverity::Critical.is_blocking());
    }

    // ---- RuleViolation 测试 ----

    #[test]
    fn test_rule_violation_new() {
        let v = RuleViolation::new("test_rule", RuleSeverity::Warning, "test message");
        assert_eq!(v.rule_name, "test_rule");
        assert_eq!(v.severity, RuleSeverity::Warning);
        assert_eq!(v.message, "test message");
        assert!(v.position.is_none());
    }

    #[test]
    fn test_rule_violation_with_position() {
        let v = RuleViolation::with_position("test_rule", RuleSeverity::Error, "test message", 42);
        assert_eq!(v.position, Some(42));
        assert!(v.is_blocking());
    }

    #[test]
    fn test_rule_violation_display() {
        let v = RuleViolation::new("r", RuleSeverity::Warning, "msg");
        let s = format!("{}", v);
        assert!(s.contains("[warning]"));
        assert!(s.contains("r"));
        assert!(s.contains("msg"));

        let v2 = RuleViolation::with_position("r", RuleSeverity::Error, "msg", 10);
        let s2 = format!("{}", v2);
        assert!(s2.contains("position 10"));
    }

    // ---- RuleContext 测试 ----

    #[test]
    fn test_rule_context_from_sql() {
        let ctx = RuleContext::from_sql("SELECT id FROM users");
        assert_eq!(ctx.statement_type, SqlStatementType::Select);
        assert!(ctx.sql_upper.contains("SELECT"));
        assert!(!ctx.tokens.is_empty());
    }

    #[test]
    fn test_rule_context_keyword_count() {
        let ctx = RuleContext::from_sql("SELECT id FROM users JOIN orders ON 1=1");
        assert_eq!(ctx.keyword_count("JOIN"), 1);
        assert_eq!(ctx.keyword_count("SELECT"), 1);
        assert_eq!(ctx.keyword_count("DELETE"), 0);
    }

    #[test]
    fn test_rule_context_has_keyword() {
        let ctx = RuleContext::from_sql("SELECT id FROM users WHERE id = 1");
        assert!(ctx.has_keyword("WHERE"));
        assert!(!ctx.has_keyword("DELETE"));
    }

    #[test]
    fn test_rule_context_table_count() {
        let ctx = RuleContext::from_sql("SELECT * FROM users JOIN orders ON users.id = orders.uid");
        assert!(ctx.table_count() >= 2);
    }

    // ---- NoSelectStarRule 测试 ----

    #[test]
    fn test_no_select_star_rule_detects_star() {
        let rule = NoSelectStarRule;
        let ctx = RuleContext::from_sql("SELECT * FROM users");
        let violation = rule.check(&ctx);
        assert!(violation.is_some());
        let v = violation.unwrap();
        assert_eq!(v.severity, RuleSeverity::Warning);
        assert!(v.position.is_some());
    }

    #[test]
    fn test_no_select_star_rule_passes_explicit_columns() {
        let rule = NoSelectStarRule;
        let ctx = RuleContext::from_sql("SELECT id, name FROM users");
        assert!(rule.check(&ctx).is_none());
    }

    #[test]
    fn test_no_select_star_rule_ignores_non_select() {
        let rule = NoSelectStarRule;
        let ctx = RuleContext::from_sql("DELETE FROM users WHERE id = 1");
        assert!(rule.check(&ctx).is_none());
    }

    // ---- RequireWhereInDeleteRule 测试 ----

    #[test]
    fn test_require_where_in_delete_rule_detects_missing_where() {
        let rule = RequireWhereInDeleteRule;
        let ctx = RuleContext::from_sql("DELETE FROM users");
        let violation = rule.check(&ctx);
        assert!(violation.is_some());
        assert_eq!(violation.unwrap().severity, RuleSeverity::Critical);
    }

    #[test]
    fn test_require_where_in_delete_rule_passes_with_where() {
        let rule = RequireWhereInDeleteRule;
        let ctx = RuleContext::from_sql("DELETE FROM users WHERE id = 1");
        assert!(rule.check(&ctx).is_none());
    }

    #[test]
    fn test_require_where_in_delete_rule_ignores_non_delete() {
        let rule = RequireWhereInDeleteRule;
        let ctx = RuleContext::from_sql("SELECT * FROM users");
        assert!(rule.check(&ctx).is_none());
    }

    // ---- RequireWhereInUpdateRule 测试 ----

    #[test]
    fn test_require_where_in_update_rule_detects_missing_where() {
        let rule = RequireWhereInUpdateRule;
        let ctx = RuleContext::from_sql("UPDATE users SET name = 'a'");
        let violation = rule.check(&ctx);
        assert!(violation.is_some());
        assert_eq!(violation.unwrap().severity, RuleSeverity::Critical);
    }

    #[test]
    fn test_require_where_in_update_rule_passes_with_where() {
        let rule = RequireWhereInUpdateRule;
        let ctx = RuleContext::from_sql("UPDATE users SET name = 'a' WHERE id = 1");
        assert!(rule.check(&ctx).is_none());
    }

    // ---- MaxTableCountRule 测试 ----

    #[test]
    fn test_max_table_count_rule_passes_within_limit() {
        let rule = MaxTableCountRule::new(2);
        let ctx = RuleContext::from_sql("SELECT * FROM users JOIN orders ON 1=1");
        assert!(rule.check(&ctx).is_none());
    }

    #[test]
    fn test_max_table_count_rule_detects_exceed() {
        let rule = MaxTableCountRule::new(1);
        let ctx = RuleContext::from_sql("SELECT * FROM users JOIN orders ON 1=1");
        let violation = rule.check(&ctx);
        assert!(violation.is_some());
        assert!(violation.unwrap().message.contains("超过上限"));
    }

    // ---- MaxJoinCountRule 测试 ----

    #[test]
    fn test_max_join_count_rule_passes_within_limit() {
        let rule = MaxJoinCountRule::new(2);
        let ctx = RuleContext::from_sql("SELECT * FROM users JOIN orders ON 1=1 JOIN items ON 2=2");
        assert!(rule.check(&ctx).is_none());
    }

    #[test]
    fn test_max_join_count_rule_detects_exceed() {
        let rule = MaxJoinCountRule::new(1);
        let ctx = RuleContext::from_sql("SELECT * FROM users JOIN orders ON 1=1 JOIN items ON 2=2");
        assert!(rule.check(&ctx).is_some());
    }

    // ---- NoUnionRule 测试 ----

    #[test]
    fn test_no_union_rule_detects_union() {
        let rule = NoUnionRule;
        let ctx = RuleContext::from_sql("SELECT id FROM users UNION SELECT id FROM archived");
        let violation = rule.check(&ctx);
        assert!(violation.is_some());
        assert_eq!(violation.unwrap().severity, RuleSeverity::Error);
    }

    #[test]
    fn test_no_union_rule_passes_without_union() {
        let rule = NoUnionRule;
        let ctx = RuleContext::from_sql("SELECT id FROM users");
        assert!(rule.check(&ctx).is_none());
    }

    // ---- ForbiddenKeywordRule 测试 ----

    #[test]
    fn test_forbidden_keyword_rule_detects_keyword() {
        let rule = ForbiddenKeywordRule::new("no_grant", &["GRANT"]);
        let ctx = RuleContext::from_sql("GRANT ALL ON users TO hacker");
        let violation = rule.check(&ctx);
        assert!(violation.is_some());
        assert_eq!(violation.unwrap().severity, RuleSeverity::Critical);
    }

    #[test]
    fn test_forbidden_keyword_rule_passes_without_keyword() {
        let rule = ForbiddenKeywordRule::new("no_grant", &["GRANT"]);
        let ctx = RuleContext::from_sql("SELECT * FROM users");
        assert!(rule.check(&ctx).is_none());
    }

    #[test]
    fn test_forbidden_keyword_rule_with_severity() {
        let rule =
            ForbiddenKeywordRule::new("no_exec", &["EXEC"]).with_severity(RuleSeverity::Warning);
        let ctx = RuleContext::from_sql("EXEC sp_executesql 'DROP TABLE users'");
        let violation = rule.check(&ctx).unwrap();
        assert_eq!(violation.severity, RuleSeverity::Warning);
    }

    // ---- RegexRule 测试 ----

    #[test]
    fn test_regex_rule_detects_match() {
        let rule = RegexRule::new(
            "no_sys_tables",
            r"(?i)\bsys\.",
            RuleSeverity::Error,
            "禁止访问系统表",
        )
        .unwrap();
        let ctx = RuleContext::from_sql("SELECT * FROM sys.tables");
        let violation = rule.check(&ctx);
        assert!(violation.is_some());
        assert!(violation.unwrap().position.is_some());
    }

    #[test]
    fn test_regex_rule_passes_no_match() {
        let rule = RegexRule::new(
            "no_sys_tables",
            r"(?i)\bsys\.",
            RuleSeverity::Error,
            "禁止访问系统表",
        )
        .unwrap();
        let ctx = RuleContext::from_sql("SELECT * FROM users");
        assert!(rule.check(&ctx).is_none());
    }

    #[test]
    fn test_regex_rule_invalid_pattern() {
        let result = RegexRule::new("bad", r"[invalid", RuleSeverity::Error, "msg");
        assert!(result.is_err());
    }

    // ---- RequireLimitRule 测试 ----

    #[test]
    fn test_require_limit_rule_detects_missing_limit() {
        let rule = RequireLimitRule;
        let ctx = RuleContext::from_sql("SELECT id FROM users");
        let violation = rule.check(&ctx);
        assert!(violation.is_some());
        assert_eq!(violation.unwrap().severity, RuleSeverity::Info);
    }

    #[test]
    fn test_require_limit_rule_passes_with_limit() {
        let rule = RequireLimitRule;
        let ctx = RuleContext::from_sql("SELECT id FROM users LIMIT 10");
        assert!(rule.check(&ctx).is_none());
    }

    // ---- RuleReport 测试 ----

    #[test]
    fn test_rule_report_clean() {
        let report = RuleReport::default();
        assert!(report.is_clean());
        assert!(!report.has_violations());
        assert!(!report.has_blocking());
        assert_eq!(report.violation_count(), 0);
    }

    #[test]
    fn test_rule_report_with_violations() {
        let report = RuleReport {
            violations: vec![
                RuleViolation::new("r1", RuleSeverity::Warning, "w"),
                RuleViolation::new("r2", RuleSeverity::Error, "e"),
                RuleViolation::new("r3", RuleSeverity::Critical, "c"),
            ],
        };
        assert!(report.has_violations());
        assert!(report.has_blocking());
        assert!(report.has_errors());
        assert!(report.has_critical());
        assert_eq!(report.violation_count(), 3);
        assert_eq!(report.count_by_severity(RuleSeverity::Warning), 1);
        assert_eq!(report.count_by_severity(RuleSeverity::Error), 1);
        assert_eq!(report.count_by_severity(RuleSeverity::Critical), 1);
    }

    #[test]
    fn test_rule_report_summary() {
        let report = RuleReport::default();
        assert!(report.summary().contains("无违规"));

        let report2 = RuleReport {
            violations: vec![RuleViolation::new("r1", RuleSeverity::Error, "msg")],
        };
        let summary = report2.summary();
        assert!(summary.contains("1 条违规"));
        assert!(summary.contains("r1"));
    }

    #[test]
    fn test_rule_report_violations_by_severity() {
        let report = RuleReport {
            violations: vec![
                RuleViolation::new("r1", RuleSeverity::Info, "i"),
                RuleViolation::new("r2", RuleSeverity::Info, "i2"),
                RuleViolation::new("r3", RuleSeverity::Error, "e"),
            ],
        };
        assert_eq!(report.violations_by_severity(RuleSeverity::Info).len(), 2);
        assert_eq!(report.violations_by_severity(RuleSeverity::Error).len(), 1);
        assert_eq!(report.blocking_violations().len(), 1);
    }

    // ---- RuleEngine 测试 ----

    #[test]
    fn test_rule_engine_new_empty() {
        let engine = RuleEngine::new();
        assert_eq!(engine.rule_count(), 0);
        let report = engine.check("SELECT * FROM users");
        assert!(report.is_clean());
    }

    #[test]
    fn test_rule_engine_add_rule() {
        let mut engine = RuleEngine::new();
        engine.add_rule(Box::new(NoSelectStarRule));
        assert_eq!(engine.rule_count(), 1);
    }

    #[test]
    fn test_rule_engine_check_collects_violations() {
        let mut engine = RuleEngine::new();
        engine.add_rule(Box::new(NoSelectStarRule));
        engine.add_rule(Box::new(RequireWhereInDeleteRule));
        let report = engine.check("DELETE FROM users");
        // RequireWhereInDeleteRule 应触发
        assert!(report.has_violations());
        assert!(report.has_critical());
    }

    #[test]
    fn test_rule_engine_check_clean_sql() {
        let mut engine = RuleEngine::new();
        engine.add_rule(Box::new(NoSelectStarRule));
        engine.add_rule(Box::new(RequireWhereInDeleteRule));
        let report = engine.check("SELECT id, name FROM users WHERE id = 1");
        assert!(report.is_clean());
    }

    #[test]
    fn test_rule_engine_passes() {
        let mut engine = RuleEngine::new();
        engine.add_rule(Box::new(RequireWhereInDeleteRule));
        assert!(engine.passes("DELETE FROM users WHERE id = 1"));
        assert!(!engine.passes("DELETE FROM users"));
    }

    #[test]
    fn test_rule_engine_check_batch() {
        let mut engine = RuleEngine::new();
        engine.add_rule(Box::new(NoSelectStarRule));
        let reports = engine.check_batch(["SELECT * FROM users", "SELECT id FROM users"]);
        assert_eq!(reports.len(), 2);
        assert!(reports[0].has_violations());
        assert!(reports[1].is_clean());
    }

    #[test]
    fn test_rule_engine_with_default_rules() {
        let engine = RuleEngine::with_default_rules();
        assert!(engine.rule_count() >= 5);
        // DROP 不在默认规则中（由 firewall 处理），但 GRANT 应被禁止
        let report = engine.check("GRANT ALL ON users TO hacker");
        assert!(report.has_critical());
    }

    #[test]
    fn test_rule_engine_default_rules_select_star() {
        let engine = RuleEngine::with_default_rules();
        let report = engine.check("SELECT * FROM users");
        assert!(report.has_violations());
    }

    #[test]
    fn test_rule_engine_default_rules_clean_sql() {
        let engine = RuleEngine::with_default_rules();
        let report = engine.check("SELECT id, name FROM users WHERE id = 1 LIMIT 10");
        assert!(report.is_clean());
    }

    // ---- RulePresets 测试 ----

    #[test]
    fn test_rule_presets_strict() {
        let engine = RulePresets::strict();
        assert!(engine.rule_count() >= 8);
        // 缺少 LIMIT 应触发 Info 级违规
        let report = engine.check("SELECT id FROM users WHERE id = 1");
        assert!(report.has_violations());
    }

    #[test]
    fn test_rule_presets_read_only() {
        let engine = RulePresets::read_only();
        // DELETE 不受只读规则集限制（无 RequireWhereInDeleteRule）
        let report = engine.check("DELETE FROM users");
        assert!(report.is_clean());
        // SELECT * 应触发
        let report2 = engine.check("SELECT * FROM users");
        assert!(report2.has_violations());
    }

    #[test]
    fn test_rule_presets_strict_max_table_count() {
        let engine = RulePresets::strict();
        let sql =
            "SELECT * FROM a JOIN b ON 1=1 JOIN c ON 2=2 JOIN d ON 3=3 JOIN e ON 4=4 JOIN f ON 5=5";
        let report = engine.check(sql);
        assert!(report.has_violations());
    }
}
