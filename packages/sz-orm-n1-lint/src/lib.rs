//! # sz-orm-n1-lint — Static N+1 Query Detector (v4.3.0 M2)
//!
//! Detects N+1 query patterns at **development time**: parses function body AST with `syn`,
//! detects query calls (`find_by_*` / `where_eq` chains / association query methods)
//! appearing inside loops (for/while), and outputs [`N1Finding`].
//!
//! Two usage modes (sharing the same analysis logic [`analyze_fn`], avoiding duplicate implementation):
//!
//! 1. **Attribute macro**: `#[detect_n_plus_one]` (`sz-orm-macros`, `n1-lint` feature) —
//!    analyzes a single function at compile time and emits a warning
//! 2. **Batch scanning**: `scan_dir` (CLI `sz-orm n1-lint --path=...`) —
//!    recursively scans all `.rs` files under a directory, outputs table/json report
//!
//! Detection patterns (conservative strategy to avoid false positives):
//! - [`N1Pattern::QueryInLoop`]: direct query call inside a loop body
//! - [`N1Pattern::ConditionalQueryInLoop`]: query call inside an `if` branch within a loop body
//! - [`N1Pattern::MissingEagerLoadHint`]: single query inside a loop that can be replaced with `where_in` batch loading
//!
//! Complementary to the runtime detector `sz-orm-core::entity_graph::N1QueryDetector`:
//! static detection finds problems at development time, runtime detection covers runtime patterns.

use std::path::Path;

pub mod association;
pub mod formatter;
pub mod rule_engine;

/// Detected N+1 pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum N1Pattern {
    /// Direct query call inside a loop body
    QueryInLoop,
    /// Query call inside a conditional branch (if) within a loop body
    ConditionalQueryInLoop,
    /// Single query inside a loop that can be replaced with `where_in` batch loading
    MissingEagerLoadHint,
}

impl N1Pattern {
    /// Human-readable description
    pub fn as_str(&self) -> &'static str {
        match self {
            N1Pattern::QueryInLoop => "query-in-loop",
            N1Pattern::ConditionalQueryInLoop => "conditional-query-in-loop",
            N1Pattern::MissingEagerLoadHint => "missing-eager-load",
        }
    }
}

/// Single N+1 detection result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct N1Finding {
    /// Pattern
    pub pattern: N1Pattern,
    /// Source file (populated during scanning; empty for single-function analysis)
    pub file: String,
    /// Line number (1-based)
    pub line: usize,
    /// Human-readable message
    pub message: String,
}

/// Analyze a single function body, returning all N+1 detection results (without file information)
pub fn analyze_fn(item_fn: &syn::ItemFn) -> Vec<N1Finding> {
    let mut findings = Vec::new();
    let body = &item_fn.block;
    scan_block(body, &mut findings, false);
    findings
}

/// Analyze a Rust source code string (for testing / batch scanning)
pub fn analyze_str(code: &str, file: &str) -> Vec<N1Finding> {
    let Ok(file_syntax) = syn::parse_file(code) else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for item in &file_syntax.items {
        if let syn::Item::Fn(item_fn) = item {
            findings.extend(analyze_fn(item_fn));
        }
    }
    for f in &mut findings {
        f.file = file.to_string();
    }
    findings
}

/// Recursively scan all `.rs` files under a directory, returning `(file path, detection results)`
pub fn scan_dir(path: &Path) -> Vec<(String, Vec<N1Finding>)> {
    let mut results = Vec::new();
    if path.is_file() {
        if path.extension().map(|e| e == "rs").unwrap_or(false) {
            if let Ok(code) = std::fs::read_to_string(path) {
                let findings = analyze_str(&code, &path.display().to_string());
                if !findings.is_empty() {
                    results.push((path.display().to_string(), findings));
                }
            }
        }
        return results;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return results;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            // 跳过 target 目录（构建产物）
            if entry_path
                .file_name()
                .map(|n| n == "target")
                .unwrap_or(false)
            {
                continue;
            }
            results.extend(scan_dir(&entry_path));
        } else if entry_path.extension().map(|e| e == "rs").unwrap_or(false) {
            if let Ok(code) = std::fs::read_to_string(&entry_path) {
                let findings = analyze_str(&code, &entry_path.display().to_string());
                if !findings.is_empty() {
                    results.push((entry_path.display().to_string(), findings));
                }
            }
        }
    }
    results
}

/// Traverse block depth-first, detecting query calls inside loops
fn scan_block(block: &syn::Block, findings: &mut Vec<N1Finding>, in_loop: bool) {
    scan_block_impl(block, findings, in_loop, false);
}

/// Traverse block depth-first (with conditional branch flag)
fn scan_block_impl(
    block: &syn::Block,
    findings: &mut Vec<N1Finding>,
    in_loop: bool,
    in_conditional: bool,
) {
    for stmt in &block.stmts {
        match stmt {
            syn::Stmt::Expr(expr, _) => scan_expr_impl(expr, findings, in_loop, in_conditional),
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    scan_expr_impl(&init.expr, findings, in_loop, in_conditional);
                }
            }
            _ => {}
        }
    }
}

/// Traverse expression tree (with conditional branch flag)
fn scan_expr_impl(
    expr: &syn::Expr,
    findings: &mut Vec<N1Finding>,
    in_loop: bool,
    in_conditional: bool,
) {
    match expr {
        syn::Expr::ForLoop(for_loop) => {
            scan_block_impl(&for_loop.body, findings, true, false);
        }
        syn::Expr::While(while_loop) => {
            scan_block_impl(&while_loop.body, findings, true, false);
        }
        syn::Expr::If(if_expr) => {
            scan_block_impl(&if_expr.then_branch, findings, in_loop, true);
            if let Some((_, else_branch)) = &if_expr.else_branch {
                scan_expr_impl(else_branch, findings, in_loop, true);
            }
        }
        syn::Expr::Block(block_expr) => {
            scan_block_impl(&block_expr.block, findings, in_loop, in_conditional);
        }
        syn::Expr::MethodCall(method_call) => {
            let method = method_call.method.to_string();
            if is_query_method(&method) {
                let pattern = if in_loop && in_conditional {
                    N1Pattern::ConditionalQueryInLoop
                } else if in_loop {
                    N1Pattern::QueryInLoop
                } else {
                    N1Pattern::MissingEagerLoadHint
                };
                let span = method_call.method.span();
                findings.push(N1Finding {
                    pattern,
                    file: String::new(),
                    line: span.start().line,
                    message: format!(
                        "query method '{method}' called {}: consider batch loading",
                        if in_loop {
                            "inside a loop (N+1)"
                        } else {
                            "outside a loop"
                        }
                    ),
                });
            }
            scan_expr_impl(&method_call.receiver, findings, in_loop, in_conditional);
            for arg in &method_call.args {
                scan_expr_impl(arg, findings, in_loop, in_conditional);
            }
        }
        syn::Expr::Call(call) => {
            if let syn::Expr::Path(path_expr) = &*call.func {
                let name = path_expr
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                if name.starts_with("find_by") || name == "query" || name == "find_all" {
                    let span = path_expr
                        .path
                        .segments
                        .last()
                        .map(|s| s.ident.span())
                        .unwrap_or_else(proc_macro2::Span::call_site);
                    let pattern = if in_loop && in_conditional {
                        N1Pattern::ConditionalQueryInLoop
                    } else if in_loop {
                        N1Pattern::QueryInLoop
                    } else {
                        N1Pattern::MissingEagerLoadHint
                    };
                    findings.push(N1Finding {
                        pattern,
                        file: String::new(),
                        line: span.start().line,
                        message: format!(
                            "query function '{name}' called {}",
                            if in_loop {
                                "inside a loop (N+1)"
                            } else {
                                "outside a loop"
                            }
                        ),
                    });
                }
            }
            for arg in &call.args {
                scan_expr_impl(arg, findings, in_loop, in_conditional);
            }
        }
        syn::Expr::Closure(closure) => {
            scan_expr_impl(&closure.body, findings, in_loop, in_conditional);
        }
        syn::Expr::Try(try_expr) => {
            scan_expr_impl(&try_expr.expr, findings, in_loop, in_conditional)
        }
        syn::Expr::Await(await_expr) => {
            scan_expr_impl(&await_expr.base, findings, in_loop, in_conditional)
        }
        syn::Expr::Paren(paren) => scan_expr_impl(&paren.expr, findings, in_loop, in_conditional),
        syn::Expr::Group(group) => scan_expr_impl(&group.expr, findings, in_loop, in_conditional),
        syn::Expr::Reference(reference) => {
            scan_expr_impl(&reference.expr, findings, in_loop, in_conditional)
        }
        syn::Expr::Unary(unary) => scan_expr_impl(&unary.expr, findings, in_loop, in_conditional),
        syn::Expr::Cast(cast) => scan_expr_impl(&cast.expr, findings, in_loop, in_conditional),
        syn::Expr::Let(let_expr) => {
            scan_expr_impl(&let_expr.expr, findings, in_loop, in_conditional)
        }
        syn::Expr::Tuple(tuple) => {
            for e in &tuple.elems {
                scan_expr_impl(e, findings, in_loop, in_conditional);
            }
        }
        syn::Expr::Array(array) => {
            for e in &array.elems {
                scan_expr_impl(e, findings, in_loop, in_conditional);
            }
        }
        _ => {}
    }
}

/// Query method name check (conservative whitelist to avoid false positives on ordinary methods)
fn is_query_method(method: &str) -> bool {
    method.starts_with("find_by")
        || method == "find_all"
        || method == "query"
        || method == "where_eq"
        || method == "or_where_eq"
        || method == "find_with_related"
        || method == "eager_load"
}

// ---------------------------------------------------------------------------
// N1Severity — 严重度等级
// ---------------------------------------------------------------------------

/// Severity of N+1 detection results
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum N1Severity {
    /// Info (can be replaced with batch loading)
    Info,
    /// Warning (conditional query inside a loop)
    Warning,
    /// Error (direct query inside a loop)
    Error,
}

impl N1Severity {
    /// Human-readable name
    pub fn as_str(&self) -> &'static str {
        match self {
            N1Severity::Info => "info",
            N1Severity::Warning => "warning",
            N1Severity::Error => "error",
        }
    }

    /// Infer severity from pattern
    pub fn from_pattern(pattern: N1Pattern) -> Self {
        match pattern {
            N1Pattern::QueryInLoop => N1Severity::Error,
            N1Pattern::ConditionalQueryInLoop => N1Severity::Warning,
            N1Pattern::MissingEagerLoadHint => N1Severity::Info,
        }
    }
}

// ---------------------------------------------------------------------------
// N1Report — 检测报告聚合
// ---------------------------------------------------------------------------

/// N+1 detection report: aggregates multiple detection results
#[derive(Debug, Clone, Default)]
pub struct N1Report {
    findings: Vec<N1Finding>,
}

impl N1Report {
    /// Create empty report
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a detection result
    pub fn add_finding(&mut self, finding: N1Finding) {
        self.findings.push(finding);
    }

    /// Reference to all detection results
    pub fn findings(&self) -> &[N1Finding] {
        &self.findings
    }

    /// Total number of detection results
    pub fn count(&self) -> usize {
        self.findings.len()
    }

    /// Count by pattern
    pub fn count_by_pattern(&self, pattern: N1Pattern) -> usize {
        self.findings
            .iter()
            .filter(|f| f.pattern == pattern)
            .count()
    }

    /// Whether there are no detection results (clean code)
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// List of unique patterns found
    pub fn patterns_found(&self) -> Vec<N1Pattern> {
        let mut seen = Vec::new();
        for f in &self.findings {
            if !seen.contains(&f.pattern) {
                seen.push(f.pattern);
            }
        }
        seen
    }

    /// Summary string
    pub fn to_summary_string(&self) -> String {
        let mut out = format!("N+1 Query Report: {} finding(s)\n", self.count());
        for f in &self.findings {
            out.push_str(&format!(
                "  [{}] {}:{} — {}\n",
                f.pattern.as_str(),
                f.file,
                f.line,
                f.message
            ));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// N1Config — 检测配置
// ---------------------------------------------------------------------------

/// N+1 detection config: can ignore specific patterns
#[derive(Debug, Clone, Default)]
pub struct N1Config {
    ignored_patterns: Vec<N1Pattern>,
}

impl N1Config {
    /// Create default config (ignores no patterns)
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an ignore pattern (chainable)
    pub fn with_ignore_pattern(&mut self, pattern: N1Pattern) -> &mut Self {
        if !self.ignored_patterns.contains(&pattern) {
            self.ignored_patterns.push(pattern);
        }
        self
    }

    /// Check whether a pattern is ignored
    pub fn is_ignored(&self, pattern: N1Pattern) -> bool {
        self.ignored_patterns.contains(&pattern)
    }

    /// Number of ignored patterns
    pub fn ignored_count(&self) -> usize {
        self.ignored_patterns.len()
    }
}

// ---------------------------------------------------------------------------
// QueryMethod — 已知查询方法名
// ---------------------------------------------------------------------------

/// Known query method enum
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryMethod {
    /// `find_by_*` family
    FindById,
    /// `find_all`
    FindAll,
    /// `query`
    Query,
    /// `where_eq`
    WhereEq,
    /// `or_where_eq`
    OrWhereEq,
    /// `find_with_related`
    FindWithRelated,
    /// `eager_load`
    EagerLoad,
    /// Custom method name
    Custom(String),
}

impl QueryMethod {
    /// Method name string
    pub fn as_str(&self) -> &str {
        match self {
            QueryMethod::FindById => "find_by_id",
            QueryMethod::FindAll => "find_all",
            QueryMethod::Query => "query",
            QueryMethod::WhereEq => "where_eq",
            QueryMethod::OrWhereEq => "or_where_eq",
            QueryMethod::FindWithRelated => "find_with_related",
            QueryMethod::EagerLoad => "eager_load",
            QueryMethod::Custom(name) => name,
        }
    }

    /// Whether it can be replaced with batch loading (`where_in` / `eager_load` can replace)
    pub fn is_batchable(&self) -> bool {
        matches!(
            self,
            QueryMethod::FindById
                | QueryMethod::FindAll
                | QueryMethod::WhereEq
                | QueryMethod::FindWithRelated
        )
    }
}

// ---------------------------------------------------------------------------
// LoopType — 循环类型
// ---------------------------------------------------------------------------

/// Detected loop type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopType {
    /// `for` loop
    For,
    /// `while` loop
    While,
    /// `loop` loop
    Loop,
}

impl LoopType {
    /// Human-readable name
    pub fn as_str(&self) -> &'static str {
        match self {
            LoopType::For => "for",
            LoopType::While => "while",
            LoopType::Loop => "loop",
        }
    }
}

// ---------------------------------------------------------------------------
// N1Suggestion — 修复建议
// ---------------------------------------------------------------------------

/// N+1 fix suggestion
#[derive(Debug, Clone)]
pub struct N1Suggestion {
    pattern: N1Pattern,
}

impl N1Suggestion {
    /// Create suggestion from detection pattern
    pub fn new(pattern: N1Pattern) -> Self {
        Self { pattern }
    }

    /// Suggestion description
    pub fn description(&self) -> &'static str {
        match self.pattern {
            N1Pattern::QueryInLoop => {
                "replace loop+query with batch loading (where_in / eager_load)"
            }
            N1Pattern::ConditionalQueryInLoop => {
                "extract conditional query outside loop, or pre-load all related data"
            }
            N1Pattern::MissingEagerLoadHint => {
                "consider using eager_load to avoid potential N+1 when called in a loop"
            }
        }
    }

    /// Fix type
    pub fn fix_type(&self) -> &'static str {
        match self.pattern {
            N1Pattern::QueryInLoop => "batch-load",
            N1Pattern::ConditionalQueryInLoop => "pre-load",
            N1Pattern::MissingEagerLoadHint => "eager-load-hint",
        }
    }
}

// ---------------------------------------------------------------------------
// 模块 re-export
// ---------------------------------------------------------------------------

pub use association::{
    AssociationType, BatchLoadAdvisor, BatchLoadStrategy, BatchLoadSuggestion, QueryAssociation,
    QueryAssociationAnalyzer,
};
pub use formatter::{FalsePositiveFilter, N1DetectionConfig, N1ReportFormatter, ReportFormat};
pub use rule_engine::{DetectionRule, N1DetectionRuleEngine, RuleEngineStats, RuleMatchResult};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build test fixture file directory
    fn fixture_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("sz-orm-n1-lint-fixtures");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn detects_query_in_for_loop() {
        let code = r#"
fn process_users(users: Vec<User>) {
    for user in users {
        let orders = Order::find_by_user(user.id); // N+1！
    }
}
"#;
        let findings = analyze_str(code, "test.rs");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, N1Pattern::QueryInLoop);
        assert!(findings[0].message.contains("inside a loop"));
        assert!(findings[0].line >= 3);
    }

    #[test]
    fn detects_method_call_in_loop() {
        let code = r#"
fn process(ids: Vec<i64>) {
    for id in ids {
        let user = User::query().where_eq("id", id).first().await?;
    }
}
"#;
        let findings = analyze_str(code, "test.rs");
        // where_eq 在循环内 → QueryInLoop
        assert!(findings.iter().any(|f| f.pattern == N1Pattern::QueryInLoop));
    }

    #[test]
    fn detects_conditional_query_in_loop() {
        let code = r#"
fn process(users: Vec<User>) {
    for user in users {
        if user.is_vip {
            let orders = Order::find_by_user(user.id);
        }
    }
}
"#;
        let findings = analyze_str(code, "test.rs");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, N1Pattern::ConditionalQueryInLoop);
    }

    #[test]
    fn flags_batchable_outside_loop_as_hint() {
        let code = r#"
fn find_one(id: i64) {
    let user = User::find_by_id(id);
}
"#;
        let findings = analyze_str(code, "test.rs");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, N1Pattern::MissingEagerLoadHint);
    }

    #[test]
    fn no_false_positive_on_unrelated_methods() {
        let code = r#"
fn compute(list: Vec<i32>) -> i32 {
    let mut sum = 0;
    for v in list {
        sum += v.saturating_mul(2); // 非查询方法，不误报
    }
    sum
}
"#;
        let findings = analyze_str(code, "test.rs");
        assert!(findings.is_empty());
    }

    #[test]
    fn scan_dir_finds_rs_files() {
        let dir = fixture_dir();
        let file = dir.join("sample.rs");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(
            f,
            "fn p(us: Vec<User>) {{ for u in us {{ let o = Order::find_by_user(u.id); }} }}"
        )
        .unwrap();
        let results = scan_dir(&dir);
        assert!(
            results.iter().any(|(path, findings)| {
                path.ends_with("sample.rs")
                    && findings.iter().any(|f| f.pattern == N1Pattern::QueryInLoop)
            }),
            "scan_dir should find query-in-loop in sample.rs"
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn scan_dir_skips_target_dir() {
        let dir = fixture_dir().join("target");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("generated.rs");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(
            f,
            "fn p(us: Vec<User>) {{ for u in us {{ let o = Order::find_by_user(u.id); }} }}"
        )
        .unwrap();
        let results = scan_dir(&fixture_dir());
        assert!(
            !results.iter().any(|(path, _)| path.contains("target")),
            "target dir must be skipped"
        );
        let _ = std::fs::remove_dir_all(fixture_dir());
    }

    // --- N1Severity tests ---

    #[test]
    fn severity_from_pattern_query_in_loop() {
        assert_eq!(
            N1Severity::from_pattern(N1Pattern::QueryInLoop),
            N1Severity::Error
        );
    }

    #[test]
    fn severity_from_pattern_conditional() {
        assert_eq!(
            N1Severity::from_pattern(N1Pattern::ConditionalQueryInLoop),
            N1Severity::Warning
        );
    }

    #[test]
    fn severity_from_pattern_missing_eager_load() {
        assert_eq!(
            N1Severity::from_pattern(N1Pattern::MissingEagerLoadHint),
            N1Severity::Info
        );
    }

    #[test]
    fn severity_as_str_nonempty() {
        assert!(!N1Severity::Info.as_str().is_empty());
        assert!(!N1Severity::Warning.as_str().is_empty());
        assert!(!N1Severity::Error.as_str().is_empty());
    }

    #[test]
    fn severity_ordering() {
        assert!(N1Severity::Info < N1Severity::Warning);
        assert!(N1Severity::Warning < N1Severity::Error);
    }

    // --- N1Report tests ---

    #[test]
    fn report_new_empty() {
        let r = N1Report::new();
        assert_eq!(r.count(), 0);
        assert!(r.is_clean());
        assert!(r.findings().is_empty());
    }

    #[test]
    fn report_add_finding() {
        let mut r = N1Report::new();
        r.add_finding(N1Finding {
            pattern: N1Pattern::QueryInLoop,
            file: "test.rs".to_string(),
            line: 10,
            message: "test".to_string(),
        });
        assert_eq!(r.count(), 1);
        assert!(!r.is_clean());
    }

    #[test]
    fn report_count_by_pattern() {
        let mut r = N1Report::new();
        for _ in 0..3 {
            r.add_finding(N1Finding {
                pattern: N1Pattern::QueryInLoop,
                file: String::new(),
                line: 0,
                message: String::new(),
            });
        }
        r.add_finding(N1Finding {
            pattern: N1Pattern::MissingEagerLoadHint,
            file: String::new(),
            line: 0,
            message: String::new(),
        });
        assert_eq!(r.count_by_pattern(N1Pattern::QueryInLoop), 3);
        assert_eq!(r.count_by_pattern(N1Pattern::MissingEagerLoadHint), 1);
        assert_eq!(r.count_by_pattern(N1Pattern::ConditionalQueryInLoop), 0);
    }

    #[test]
    fn report_to_summary_string() {
        let mut r = N1Report::new();
        r.add_finding(N1Finding {
            pattern: N1Pattern::QueryInLoop,
            file: "a.rs".to_string(),
            line: 5,
            message: "test finding".to_string(),
        });
        let summary = r.to_summary_string();
        assert!(summary.contains("1 finding(s)"));
        assert!(summary.contains("a.rs"));
        assert!(summary.contains("test finding"));
    }

    #[test]
    fn report_patterns_found() {
        let mut r = N1Report::new();
        r.add_finding(N1Finding {
            pattern: N1Pattern::QueryInLoop,
            file: String::new(),
            line: 0,
            message: String::new(),
        });
        r.add_finding(N1Finding {
            pattern: N1Pattern::QueryInLoop,
            file: String::new(),
            line: 0,
            message: String::new(),
        });
        r.add_finding(N1Finding {
            pattern: N1Pattern::MissingEagerLoadHint,
            file: String::new(),
            line: 0,
            message: String::new(),
        });
        let patterns = r.patterns_found();
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn report_patterns_found_empty() {
        let r = N1Report::new();
        assert!(r.patterns_found().is_empty());
    }

    #[test]
    fn report_is_clean_with_findings() {
        let mut r = N1Report::new();
        r.add_finding(N1Finding {
            pattern: N1Pattern::QueryInLoop,
            file: String::new(),
            line: 0,
            message: String::new(),
        });
        assert!(!r.is_clean());
    }

    // --- N1Config tests ---

    #[test]
    fn config_new_no_ignored() {
        let c = N1Config::new();
        assert_eq!(c.ignored_count(), 0);
        assert!(!c.is_ignored(N1Pattern::QueryInLoop));
    }

    #[test]
    fn config_with_ignore_pattern() {
        let mut c = N1Config::new();
        c.with_ignore_pattern(N1Pattern::QueryInLoop);
        assert!(c.is_ignored(N1Pattern::QueryInLoop));
        assert_eq!(c.ignored_count(), 1);
    }

    #[test]
    fn config_is_ignored_false() {
        let mut c = N1Config::new();
        c.with_ignore_pattern(N1Pattern::QueryInLoop);
        assert!(!c.is_ignored(N1Pattern::ConditionalQueryInLoop));
    }

    #[test]
    fn config_duplicate_ignore_not_counted() {
        let mut c = N1Config::new();
        c.with_ignore_pattern(N1Pattern::QueryInLoop);
        c.with_ignore_pattern(N1Pattern::QueryInLoop);
        assert_eq!(c.ignored_count(), 1);
    }

    #[test]
    fn config_ignore_all_patterns() {
        let mut c = N1Config::new();
        c.with_ignore_pattern(N1Pattern::QueryInLoop);
        c.with_ignore_pattern(N1Pattern::ConditionalQueryInLoop);
        c.with_ignore_pattern(N1Pattern::MissingEagerLoadHint);
        assert_eq!(c.ignored_count(), 3);
    }

    // --- QueryMethod tests ---

    #[test]
    fn query_method_as_str_find_by_id() {
        assert_eq!(QueryMethod::FindById.as_str(), "find_by_id");
    }

    #[test]
    fn query_method_as_str_query() {
        assert_eq!(QueryMethod::Query.as_str(), "query");
    }

    #[test]
    fn query_method_as_str_custom() {
        let m = QueryMethod::Custom("my_query".to_string());
        assert_eq!(m.as_str(), "my_query");
    }

    #[test]
    fn query_method_is_batchable_find_all() {
        assert!(QueryMethod::FindAll.is_batchable());
    }

    #[test]
    fn query_method_is_batchable_where_eq() {
        assert!(QueryMethod::WhereEq.is_batchable());
    }

    #[test]
    fn query_method_is_batchable_eager_load() {
        assert!(!QueryMethod::EagerLoad.is_batchable());
    }

    #[test]
    fn query_method_is_batchable_or_where_eq() {
        assert!(!QueryMethod::OrWhereEq.is_batchable());
    }

    // --- LoopType tests ---

    #[test]
    fn loop_type_as_str_for() {
        assert_eq!(LoopType::For.as_str(), "for");
    }

    #[test]
    fn loop_type_as_str_while() {
        assert_eq!(LoopType::While.as_str(), "while");
    }

    #[test]
    fn loop_type_as_str_loop() {
        assert_eq!(LoopType::Loop.as_str(), "loop");
    }

    #[test]
    fn loop_type_all_variants_distinct() {
        assert_ne!(LoopType::For, LoopType::While);
        assert_ne!(LoopType::While, LoopType::Loop);
        assert_ne!(LoopType::For, LoopType::Loop);
    }

    // --- N1Suggestion tests ---

    #[test]
    fn suggestion_query_in_loop() {
        let s = N1Suggestion::new(N1Pattern::QueryInLoop);
        assert!(s.description().contains("batch"));
        assert_eq!(s.fix_type(), "batch-load");
    }

    #[test]
    fn suggestion_conditional() {
        let s = N1Suggestion::new(N1Pattern::ConditionalQueryInLoop);
        assert!(s.description().contains("pre-load"));
        assert_eq!(s.fix_type(), "pre-load");
    }

    #[test]
    fn suggestion_missing_eager_load() {
        let s = N1Suggestion::new(N1Pattern::MissingEagerLoadHint);
        assert!(s.description().contains("eager_load"));
        assert_eq!(s.fix_type(), "eager-load-hint");
    }

    #[test]
    fn suggestion_description_nonempty() {
        for pattern in [
            N1Pattern::QueryInLoop,
            N1Pattern::ConditionalQueryInLoop,
            N1Pattern::MissingEagerLoadHint,
        ] {
            assert!(!N1Suggestion::new(pattern).description().is_empty());
        }
    }

    #[test]
    fn suggestion_fix_type_nonempty() {
        for pattern in [
            N1Pattern::QueryInLoop,
            N1Pattern::ConditionalQueryInLoop,
            N1Pattern::MissingEagerLoadHint,
        ] {
            assert!(!N1Suggestion::new(pattern).fix_type().is_empty());
        }
    }

    // --- Additional integration tests ---

    #[test]
    fn analyze_str_invalid_syntax() {
        let findings = analyze_str("this is not valid rust", "bad.rs");
        assert!(findings.is_empty());
    }

    #[test]
    fn analyze_str_empty() {
        let findings = analyze_str("", "empty.rs");
        assert!(findings.is_empty());
    }

    #[test]
    fn analyze_str_no_functions() {
        let code = "const X: i32 = 42;";
        let findings = analyze_str(code, "const.rs");
        assert!(findings.is_empty());
    }

    #[test]
    fn scan_dir_nonexistent() {
        let results = scan_dir(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(results.is_empty());
    }

    #[test]
    fn scan_dir_empty_dir() {
        let dir = fixture_dir().join("empty_subdir");
        let _ = std::fs::create_dir_all(&dir);
        let results = scan_dir(&dir);
        assert!(results.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn analyze_fn_directly() {
        let code =
            "fn test_fn(items: Vec<i64>) { for item in items { let q = User::find_by_id(item); } }";
        let file_syntax = syn::parse_file(code).unwrap();
        if let syn::Item::Fn(item_fn) = &file_syntax.items[0] {
            let findings = analyze_fn(item_fn);
            assert!(findings.iter().any(|f| f.pattern == N1Pattern::QueryInLoop));
        } else {
            panic!("expected Item::Fn");
        }
    }

    #[test]
    fn report_findings_ref_returns_slice() {
        let mut r = N1Report::new();
        r.add_finding(N1Finding {
            pattern: N1Pattern::QueryInLoop,
            file: String::new(),
            line: 1,
            message: String::new(),
        });
        assert_eq!(r.findings().len(), 1);
    }

    #[test]
    fn query_method_all_known_variants() {
        let methods = [
            QueryMethod::FindById,
            QueryMethod::FindAll,
            QueryMethod::Query,
            QueryMethod::WhereEq,
            QueryMethod::OrWhereEq,
            QueryMethod::FindWithRelated,
            QueryMethod::EagerLoad,
        ];
        for i in 0..methods.len() {
            for j in (i + 1)..methods.len() {
                assert_ne!(methods[i], methods[j]);
            }
        }
    }

    #[test]
    fn suggestion_fix_type_distinct_per_pattern() {
        let types: Vec<&str> = [
            N1Suggestion::new(N1Pattern::QueryInLoop).fix_type(),
            N1Suggestion::new(N1Pattern::ConditionalQueryInLoop).fix_type(),
            N1Suggestion::new(N1Pattern::MissingEagerLoadHint).fix_type(),
        ]
        .into();
        assert_eq!(types.len(), 3);
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                assert_ne!(types[i], types[j]);
            }
        }
    }

    #[test]
    fn severity_all_variants() {
        let variants = [N1Severity::Info, N1Severity::Warning, N1Severity::Error];
        for i in 0..variants.len() {
            for j in (i + 1)..variants.len() {
                assert_ne!(variants[i], variants[j]);
            }
        }
    }
}
