//! QueryBuilder 迁移 fix 模块
//!
//! 自动将旧版 `sz_orm_query_builder::Query` API 转换为 `sz_orm_core::QueryBuilder` 等价代码。
//!
//! # 启用方式
//!
//! ```bash
//! cargo build --features qb-migration-tool
//! ```
//!
//! # 支持的自动转换
//!
//! | 旧 API | 新 API |
//! |--------|--------|
//! | `Query::select()` | `QueryBuilder::<Model>::new(dialect).select(vec![])` |
//! | `Query::insert()` | `QueryBuilder::<Model>::new(dialect)` |
//! | `Query::update()` | `QueryBuilder::<Model>::new(dialect)` |
//! | `Query::delete()` | `QueryBuilder::<Model>::new(dialect)` |
//! | `.from("t")` | `.table("t")` |
//! | `.into_table("t")` | `.table("t")` |
//! | `.from_table("t")` | `.table("t")` |
//! | `.order_by("c", true)` | `.order_by("c")` |
//! | `.order_by("c", false)` | `.order_desc("c")` |
//!
//! # 需人工审查的场景
//!
//! 以下场景标注 `needs_review = true`，不自动替换：
//!
//! - `UNION` / `UNION ALL`
//! - CTE（`with_cte` / `with_recursive_cte`）
//! - 窗口函数（`OVER`）
//! - `.where_clause("...")` — 字符串条件需改为参数化 `.where_eq()`
//! - `.column("c")` — 需合并为 `.select(vec![...])`

/// 迁移 fix 配置
#[derive(Debug, Clone)]
pub struct MigrationFix {
    /// 是否仅显示 diff 不修改（dry-run 模式）
    pub dry_run: bool,
}

impl MigrationFix {
    /// 创建 fix 配置
    ///
    /// - `dry_run = true`：仅生成变更列表，不修改源码
    /// - `dry_run = false`：执行修改并返回修复后源码
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    /// 执行迁移
    ///
    /// 等价于 `fix_source(source, self.dry_run)`。
    pub fn fix(&self, source: &str) -> FixResult {
        fix_source(source, self.dry_run)
    }
}

/// 单个修复变更
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixChange {
    /// 行号（1-based）
    pub line: usize,
    /// 原始代码片段
    pub original: String,
    /// 替换代码片段
    pub replacement: String,
    /// 是否自动修复（`false` 表示需人工审查）
    pub auto: bool,
}

/// 修复结果
#[derive(Debug, Clone)]
pub struct FixResult {
    /// 原始源码
    pub original: String,
    /// 修复后源码（dry-run 模式下与 original 相同）
    pub fixed: String,
    /// 变更列表
    pub changes: Vec<FixChange>,
    /// 是否需要人工审查
    pub needs_review: bool,
}

impl FixResult {
    /// 生成 diff 格式的变更摘要
    ///
    /// - `-` 前缀：自动修复
    /// - `?` 前缀：需人工审查
    /// - `!` 前缀：复杂场景全局标记
    pub fn diff(&self) -> String {
        let mut output = String::new();
        for change in &self.changes {
            let prefix = if change.auto { '-' } else { '?' };
            output.push_str(&format!(
                "{} L{}: {} -> {}\n",
                prefix, change.line, change.original, change.replacement
            ));
        }
        if self.needs_review {
            output.push_str("! 需人工审查：检测到复杂场景（UNION/CTE/窗口函数/字符串条件）\n");
        }
        output
    }
}

/// 检测源码中是否包含复杂构造（UNION/CTE/窗口函数）
fn has_complex_constructs(source: &str) -> bool {
    source.contains(".union(")
        || source.contains(".union_all(")
        || source.contains(".with_cte(")
        || source.contains(".with_recursive_cte(")
        || source.contains("OVER")
        || source.contains(".over(")
}

/// 从 `(` 之后的位置开始，查找匹配的右括号位置
///
/// `after_open` 应为 `(` 字符的下一个位置。返回 `)` 字符的位置。
fn find_close_paren(s: &str, after_open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    // depth 初始为 1，对应外层已遇到的 `(`
    let mut depth = 1i32;
    for (i, &b) in bytes.iter().enumerate().skip(after_open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// 替换 `Query::select()` 等构造调用
///
/// 优先匹配完整路径 `sz_orm_query_builder::Query::xxx()`，
/// 再匹配短路径 `Query::xxx()`，避免误替换。
fn replace_query_constructs(line: &str, line_no: usize) -> (String, Vec<FixChange>) {
    let mut changes = Vec::new();
    let mut result = line.to_string();

    // 按模式长度降序排列，优先匹配长模式
    let replacements: &[(&str, &str)] = &[
        (
            "sz_orm_query_builder::Query::select()",
            "QueryBuilder::<Model>::new(dialect).select(vec![])",
        ),
        (
            "sz_orm_query_builder::Query::insert()",
            "QueryBuilder::<Model>::new(dialect)",
        ),
        (
            "sz_orm_query_builder::Query::update()",
            "QueryBuilder::<Model>::new(dialect)",
        ),
        (
            "sz_orm_query_builder::Query::delete()",
            "QueryBuilder::<Model>::new(dialect)",
        ),
        (
            "Query::select()",
            "QueryBuilder::<Model>::new(dialect).select(vec![])",
        ),
        ("Query::insert()", "QueryBuilder::<Model>::new(dialect)"),
        ("Query::update()", "QueryBuilder::<Model>::new(dialect)"),
        ("Query::delete()", "QueryBuilder::<Model>::new(dialect)"),
    ];

    for (old, new) in replacements {
        if result.contains(old) {
            result = result.replace(old, new);
            changes.push(FixChange {
                line: line_no,
                original: old.to_string(),
                replacement: new.to_string(),
                auto: true,
            });
        }
    }

    (result, changes)
}

/// 替换表名方法：`.from("t")` / `.into_table("t")` / `.from_table("t")` → `.table("t")`
fn replace_table_methods(line: &str, line_no: usize) -> (String, Vec<FixChange>) {
    let mut changes = Vec::new();
    let mut result = line.to_string();

    let replacements: &[(&str, &str)] = &[
        (".from_table(", ".table("),
        (".into_table(", ".table("),
        (".from(", ".table("),
    ];

    for (old, new) in replacements {
        if result.contains(old) {
            result = result.replace(old, new);
            changes.push(FixChange {
                line: line_no,
                original: old.to_string(),
                replacement: new.to_string(),
                auto: true,
            });
        }
    }

    (result, changes)
}

/// 替换 `.order_by("c", bool)` → `.order_by("c")` 或 `.order_desc("c")`
fn replace_order_by(line: &str, line_no: usize) -> (String, Vec<FixChange>) {
    let mut changes = Vec::new();
    let mut result = line.to_string();

    let pattern = ".order_by(";
    let mut search_from = 0usize;
    while let Some(idx) = result[search_from..].find(pattern) {
        let start = search_from + idx;
        let args_start = start + pattern.len();
        let Some(args_end) = find_close_paren(&result, args_start) else {
            break;
        };
        let args = &result[args_start..args_end];
        let original = result[start..=args_end].to_string();

        let (new_call, auto) = if args.contains(", true") {
            let col = args.split(", true").next().unwrap_or("").trim();
            (format!(".order_by({})", col), true)
        } else if args.contains(", false") {
            let col = args.split(", false").next().unwrap_or("").trim();
            (format!(".order_desc({})", col), true)
        } else {
            // 单参数 order_by，无需替换
            (original.clone(), true)
        };

        if new_call != original {
            let new_len = new_call.len();
            result = format!(
                "{}{}{}",
                &result[..start],
                new_call,
                &result[args_end + 1..]
            );
            changes.push(FixChange {
                line: line_no,
                original,
                replacement: new_call,
                auto,
            });
            search_from = start + new_len;
        } else {
            search_from = args_end + 1;
        }
    }

    (result, changes)
}

/// 标注需人工审查的方法（`.where_clause()` / `.column()`）
fn mark_review_methods(line: &str, line_no: usize) -> Vec<FixChange> {
    let mut changes = Vec::new();
    let review_patterns: &[&str] = &[".where_clause(", ".column("];

    for pattern in review_patterns {
        if let Some(idx) = line.find(pattern) {
            let args_start = idx + pattern.len();
            if let Some(args_end) = find_close_paren(line, args_start) {
                let original = line[idx..=args_end].to_string();
                changes.push(FixChange {
                    line: line_no,
                    original,
                    replacement: "/* 需人工审查：迁移到 sz_orm_core::QueryBuilder 等价方法 */"
                        .to_string(),
                    auto: false,
                });
            }
        }
    }

    changes
}

/// 修复源码中的旧版 QueryBuilder API
///
/// # 参数
///
/// - `source`：Rust 源码字符串
/// - `dry_run`：`true` 仅生成变更列表不修改源码；`false` 执行修改
///
/// # 返回
///
/// [`FixResult`] 包含变更列表和是否需人工审查。
///
/// # 示例
///
/// ```ignore
/// let source = r#"let q = Query::select().from("users");"#;
/// let result = fix_source(source, false);
/// assert!(result.fixed.contains("QueryBuilder"));
/// ```
pub fn fix_source(source: &str, dry_run: bool) -> FixResult {
    let mut changes = Vec::new();
    let mut needs_review = false;

    // 检测复杂场景
    if has_complex_constructs(source) {
        needs_review = true;
    }

    // 按行处理
    let lines: Vec<&str> = source.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());

    for (i, line) in lines.iter().enumerate() {
        let line_no = i + 1;
        let mut current = line.to_string();
        let mut line_changes = Vec::new();

        // 1. 替换 Query::select() 等构造调用
        let (new_line, mut c) = replace_query_constructs(&current, line_no);
        current = new_line;
        line_changes.append(&mut c);

        // 2. 替换表名方法
        let (new_line, mut c) = replace_table_methods(&current, line_no);
        current = new_line;
        line_changes.append(&mut c);

        // 3. 替换 order_by
        let (new_line, mut c) = replace_order_by(&current, line_no);
        current = new_line;
        line_changes.append(&mut c);

        // 4. 标注需人工审查的方法
        let c = mark_review_methods(&current, line_no);
        line_changes.extend(c);

        // 如果有非自动变更，标记需审查
        if line_changes.iter().any(|c| !c.auto) {
            needs_review = true;
        }

        changes.extend(line_changes);
        result_lines.push(current);
    }

    let fixed = if dry_run {
        source.to_string()
    } else {
        let joined = result_lines.join("\n");
        if source.ends_with('\n') {
            format!("{}\n", joined)
        } else {
            joined
        }
    };

    FixResult {
        original: source.to_string(),
        fixed,
        changes,
        needs_review,
    }
}
