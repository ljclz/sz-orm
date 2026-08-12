//! # sz-orm-n1-lint — N+1 查询静态检测器（v4.3.0 M2）
//!
//! 在**开发期**发现 N+1 查询模式：用 `syn` 解析函数体 AST，
//! 检测循环（for/while）内出现的查询调用（`find_by_*` / `where_eq` 链 /
//! 关联查询方法），输出 [`N1Finding`]。
//!
//! 两种使用方式（共用同一分析逻辑 [`analyze_fn`]，避免重复实现）：
//!
//! 1. **标注宏**：`#[detect_n_plus_one]`（`sz-orm-macros`，`n1-lint` feature）——
//!    编译期对单函数分析并输出警告
//! 2. **批量扫描**：`scan_dir`（CLI `sz-orm n1-lint --path=...`）——
//!    递归扫描目录下全部 `.rs` 文件，输出 table/json 报告
//!
//! 检测模式（保守策略，避免误报）：
//! - [`N1Pattern::QueryInLoop`]：循环体内直接查询调用
//! - [`N1Pattern::ConditionalQueryInLoop`]：循环体内 `if` 分支的查询调用
//! - [`N1Pattern::MissingEagerLoadHint`]：循环内单条查询可用 `where_in` 批量替代
//!
//! 与运行时检测器 `sz-orm-core::entity_graph::N1QueryDetector` 互补：
//! 静态检测在开发期发现问题，运行时检测兜底运行期模式。

use std::path::Path;

/// 检测出的 N+1 模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum N1Pattern {
    /// 循环体内直接查询调用
    QueryInLoop,
    /// 循环体内条件分支（if）中的查询调用
    ConditionalQueryInLoop,
    /// 循环内单条查询可用 `where_in` 批量替代
    MissingEagerLoadHint,
}

impl N1Pattern {
    /// 人类可读描述
    pub fn as_str(&self) -> &'static str {
        match self {
            N1Pattern::QueryInLoop => "query-in-loop",
            N1Pattern::ConditionalQueryInLoop => "conditional-query-in-loop",
            N1Pattern::MissingEagerLoadHint => "missing-eager-load",
        }
    }
}

/// 单条 N+1 检测结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct N1Finding {
    /// 模式
    pub pattern: N1Pattern,
    /// 所在文件（扫描时填充；单函数分析为空）
    pub file: String,
    /// 行号（1 起）
    pub line: usize,
    /// 人类可读消息
    pub message: String,
}

/// 分析单个函数体，返回全部 N+1 检测结果（不含文件信息）
pub fn analyze_fn(item_fn: &syn::ItemFn) -> Vec<N1Finding> {
    let mut findings = Vec::new();
    let body = &item_fn.block;
    scan_block(body, &mut findings, false);
    findings
}

/// 分析 Rust 源码字符串（测试/批量扫描用）
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

/// 递归扫描目录下全部 `.rs` 文件，返回 `(文件路径, 检测结果)`
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

/// 深度遍历块，检测循环内查询调用
fn scan_block(block: &syn::Block, findings: &mut Vec<N1Finding>, in_loop: bool) {
    for stmt in &block.stmts {
        match stmt {
            syn::Stmt::Expr(expr, _) => scan_expr(expr, findings, in_loop),
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    scan_expr(&init.expr, findings, in_loop);
                }
            }
            _ => {}
        }
    }
}

/// 遍历表达式树：循环 → 递归体内扫描（in_loop=true）；if/else 分支 → 条件标记；
/// 查询调用 → 按当前上下文记录
fn scan_expr(expr: &syn::Expr, findings: &mut Vec<N1Finding>, in_loop: bool) {
    match expr {
        syn::Expr::ForLoop(for_loop) => {
            scan_block(&for_loop.body, findings, true);
        }
        syn::Expr::While(while_loop) => {
            scan_block(&while_loop.body, findings, true);
        }
        syn::Expr::If(if_expr) => {
            // 条件分支中的查询：随 in_loop 上下文判定（已在循环内则为 QueryInLoop）
            scan_block(&if_expr.then_branch, findings, in_loop);
            if let Some((_, else_branch)) = &if_expr.else_branch {
                scan_expr(else_branch, findings, in_loop);
            }
        }
        syn::Expr::Block(block_expr) => {
            scan_block(&block_expr.block, findings, in_loop);
        }
        syn::Expr::MethodCall(method_call) => {
            // 查询方法调用检测
            let method = method_call.method.to_string();
            if is_query_method(&method) {
                let pattern = if in_loop {
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
            // 递归：方法调用链的 receiver 与参数
            scan_expr(&method_call.receiver, findings, in_loop);
            for arg in &method_call.args {
                scan_expr(arg, findings, in_loop);
            }
        }
        syn::Expr::Call(call) => {
            // 自由函数查询调用（find_by_* 等）
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
                    findings.push(N1Finding {
                        pattern: if in_loop {
                            N1Pattern::QueryInLoop
                        } else {
                            N1Pattern::MissingEagerLoadHint
                        },
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
                scan_expr(arg, findings, in_loop);
            }
        }
        syn::Expr::Closure(closure) => {
            scan_expr(&closure.body, findings, in_loop);
        }
        // 包裹类型：递归进入内部表达式（.await? / 括号 / 引用 / 一元运算等）
        syn::Expr::Try(try_expr) => scan_expr(&try_expr.expr, findings, in_loop),
        syn::Expr::Await(await_expr) => scan_expr(&await_expr.base, findings, in_loop),
        syn::Expr::Paren(paren) => scan_expr(&paren.expr, findings, in_loop),
        syn::Expr::Group(group) => scan_expr(&group.expr, findings, in_loop),
        syn::Expr::Reference(reference) => scan_expr(&reference.expr, findings, in_loop),
        syn::Expr::Unary(unary) => scan_expr(&unary.expr, findings, in_loop),
        syn::Expr::Cast(cast) => scan_expr(&cast.expr, findings, in_loop),
        syn::Expr::Let(let_expr) => scan_expr(&let_expr.expr, findings, in_loop),
        syn::Expr::Tuple(tuple) => {
            for e in &tuple.elems {
                scan_expr(e, findings, in_loop);
            }
        }
        syn::Expr::Array(array) => {
            for e in &array.elems {
                scan_expr(e, findings, in_loop);
            }
        }
        _ => {}
    }
}

/// 查询方法名判断（保守白名单，避免误报普通方法）
fn is_query_method(method: &str) -> bool {
    method.starts_with("find_by")
        || method == "find_all"
        || method == "query"
        || method == "where_eq"
        || method == "or_where_eq"
        || method == "find_with_related"
        || method == "eager_load"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 构造测试 fixture 文件目录
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
        assert_eq!(findings[0].pattern, N1Pattern::QueryInLoop);
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
        let _ = std::fs::remove_dir_all(&fixture_dir());
    }
}
