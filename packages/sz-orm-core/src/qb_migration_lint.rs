//! QueryBuilder 迁移 lint 模块
//!
//! 通过 syn 解析 Rust AST，精确匹配 `sz_orm_query_builder::Query` 路径使用，
//! 输出告警和迁移建议，辅助从旧版 QueryBuilder 迁移到 `sz_orm_core::QueryBuilder`。
//!
//! # 启用方式
//!
//! ```bash
//! cargo build --features qb-migration-tool
//! ```
//!
//! # 示例
//!
//! ```ignore
//! use sz_orm_core::qb_migration_lint;
//!
//! let source = r#"
//! use sz_orm_query_builder::Query;
//! let q = Query::select().from("users");
//! "#;
//! let warnings = qb_migration_lint::lint_source(source);
//! assert!(!warnings.is_empty());
//! ```

use std::collections::HashSet;

use syn::visit::Visit;

/// 旧版 QueryBuilder crate 名
const DEPRECATED_CRATE: &str = "sz_orm_query_builder";

/// 旧版 QueryBuilder 类型名
const DEPRECATED_TYPE: &str = "Query";

/// 新版 QueryBuilder 推荐路径
const RECOMMENDED_PATH: &str = "sz_orm_core::QueryBuilder";

/// 迁移 lint 检查器
///
/// 遍历 AST 收集 `sz_orm_query_builder::Query` 的使用告警。
/// 通过 [`lint_source`] 函数入口执行检查，或实例化后多次调用 [`MigrationLint::lint`]。
#[derive(Debug, Default)]
pub struct MigrationLint {
    /// 收集到的告警列表
    pub warnings: Vec<LintWarning>,
}

impl MigrationLint {
    /// 创建新的 lint 检查器
    pub fn new() -> Self {
        Self {
            warnings: Vec::new(),
        }
    }

    /// 检查源码并收集告警
    ///
    /// 返回检查器内部告警列表的引用，便于链式调用。
    pub fn lint(&mut self, source: &str) -> &[LintWarning] {
        self.warnings = lint_source(source);
        &self.warnings
    }

    /// 获取告警列表
    pub fn warnings(&self) -> &[LintWarning] {
        &self.warnings
    }
}

/// 单条迁移告警
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintWarning {
    /// 文件路径（`lint_source` 函数中为 `<source>`）
    pub file: String,
    /// 行号（1-based）
    pub line: usize,
    /// 列号（1-based）
    pub col: usize,
    /// 告警消息
    pub message: String,
    /// 迁移建议
    pub suggestion: String,
}

impl LintWarning {
    /// 格式化告警为字符串
    ///
    /// 格式：`warning: <message> [<file>:<line>:<col>]\n  suggestion: <suggestion>`
    pub fn format(&self) -> String {
        format!(
            "warning: {} [{}:{}:{}]\n  suggestion: {}",
            self.message, self.file, self.line, self.col, self.suggestion
        )
    }
}

/// 检查路径段切片是否匹配 `sz_orm_query_builder::Query` 前缀
fn is_deprecated_query_segments(segments: &[String]) -> bool {
    segments.len() >= 2 && segments[0] == DEPRECATED_CRATE && segments[1] == DEPRECATED_TYPE
}

/// 检查 `syn::Path` 是否匹配 `sz_orm_query_builder::Query` 前缀
fn is_deprecated_query_path(path: &syn::Path) -> bool {
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    is_deprecated_query_segments(&segments)
}

/// 获取路径的字符串形式（用 `::` 连接）
fn path_to_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// 计算字节偏移对应的行号和列号（均 1-based）
fn line_col_at(source: &str, byte_idx: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut last_line_start = 0usize;
    for (i, b) in source.bytes().enumerate() {
        if i >= byte_idx {
            break;
        }
        if b == b'\n' {
            line += 1;
            last_line_start = i + 1;
        }
    }
    let col = byte_idx - last_line_start + 1;
    (line, col)
}

/// 在源码中查找下一个未使用的匹配位置
fn find_next_position(
    source: &str,
    needle: &str,
    used: &HashSet<(usize, usize)>,
) -> Option<(usize, usize)> {
    let mut search_from = 0usize;
    while let Some(idx) = source[search_from..].find(needle) {
        let abs_idx = search_from + idx;
        let pos = line_col_at(source, abs_idx);
        if !used.contains(&pos) {
            return Some(pos);
        }
        search_from = abs_idx + needle.len();
    }
    None
}

/// 根据路径字符串生成迁移建议
fn generate_suggestion(path_str: &str) -> String {
    let parts: Vec<&str> = path_str.split("::").collect();
    if parts.len() == 2 {
        format!(
            "将 `{}` 替换为 `{}`（需指定 Model 类型和 dialect）",
            path_str, RECOMMENDED_PATH
        )
    } else if parts.len() >= 3 {
        match parts[2] {
            "select" => format!(
                "将 `{}::{}::select()` 替换为 `{}::new(dialect).select(vec![...])`",
                parts[0], parts[1], RECOMMENDED_PATH
            ),
            "insert" => format!(
                "将 `{}::{}::insert()` 替换为 `{}::new(dialect)` 后调用 `.build_insert(&data)`",
                parts[0], parts[1], RECOMMENDED_PATH
            ),
            "update" => format!(
                "将 `{}::{}::update()` 替换为 `{}::new(dialect)` 后调用 `.build_update(&data)`",
                parts[0], parts[1], RECOMMENDED_PATH
            ),
            "delete" => format!(
                "将 `{}::{}::delete()` 替换为 `{}::new(dialect)` 后调用 `.build_delete()`",
                parts[0], parts[1], RECOMMENDED_PATH
            ),
            method => format!(
                "将 `{}::{}::{}` 替换为对应的 `{}` 方法",
                parts[0], parts[1], method, RECOMMENDED_PATH
            ),
        }
    } else {
        format!("请参考 `{}` 文档", RECOMMENDED_PATH)
    }
}

/// AST 访问者，收集旧版 QueryBuilder 使用告警
struct LintVisitor {
    /// 收集到的告警
    warnings: Vec<LintWarning>,
    /// 原始源码（用于 span 不可用时的位置回退搜索）
    source: String,
    /// 已使用的源码位置（避免重复）
    used_positions: HashSet<(usize, usize)>,
}

impl LintVisitor {
    /// 创建访问者
    fn new(source: &str) -> Self {
        Self {
            warnings: Vec::new(),
            source: source.to_string(),
            used_positions: HashSet::new(),
        }
    }

    /// 添加告警，通过源码搜索定位行号列号
    ///
    /// syn::parse_file 在非 proc-macro 上下文中 span 位置不可靠，
    /// 统一采用源码字符串搜索定位，保证位置准确。
    fn add_warning(&mut self, path_str: &str) {
        let (line, col) =
            find_next_position(&self.source, path_str, &self.used_positions).unwrap_or((1, 1));
        self.used_positions.insert((line, col));

        self.warnings.push(LintWarning {
            file: "<source>".to_string(),
            line,
            col,
            message: format!(
                "{}::{} 已废弃，请使用 {}",
                DEPRECATED_CRATE, DEPRECATED_TYPE, RECOMMENDED_PATH
            ),
            suggestion: generate_suggestion(path_str),
        });
    }

    /// 递归检查 use 树，收集 `sz_orm_query_builder::Query` 导入告警
    ///
    /// `prefix` 是当前已累积的路径前缀（如 `["sz_orm_query_builder"]`）。
    fn check_use_tree(&mut self, tree: &syn::UseTree, prefix: &[String]) {
        match tree {
            syn::UseTree::Path(p) => {
                // p.ident::p.tree，如 sz_orm_query_builder::Query
                let mut new_prefix = prefix.to_vec();
                new_prefix.push(p.ident.to_string());
                self.check_use_tree(&p.tree, &new_prefix);
            }
            syn::UseTree::Name(n) => {
                // 叶子节点，如 Query
                let mut full = prefix.to_vec();
                full.push(n.ident.to_string());
                if is_deprecated_query_segments(&full) {
                    let path_str = full.join("::");
                    self.add_warning(&path_str);
                }
            }
            syn::UseTree::Rename(r) => {
                // 叶子节点带别名，如 Query as OldQuery
                let mut full = prefix.to_vec();
                full.push(r.ident.to_string());
                if is_deprecated_query_segments(&full) {
                    let path_str = full.join("::");
                    self.add_warning(&path_str);
                }
            }
            syn::UseTree::Group(g) => {
                // 分组，如 {Query, SelectQuery}
                for item in &g.items {
                    self.check_use_tree(item, prefix);
                }
            }
            syn::UseTree::Glob(_) => {
                // 通配符，如 sz_orm_query_builder::*
                // 导入了所有公共项，包括 Query，标注告警
                if prefix.len() == 1 && prefix[0] == DEPRECATED_CRATE {
                    let path_str = format!("{}::*", prefix.join("::"));
                    self.add_warning(&path_str);
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for LintVisitor {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        // 匹配代码中的 sz_orm_query_builder::Query 路径引用（类型、方法调用等）
        // 注意：use 语句中的路径不会触发 visit_path（syn 2.0 的 visit_use_path 不调用 visit_path）
        if is_deprecated_query_path(path) {
            let path_str = path_to_string(path);
            self.add_warning(&path_str);
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_item_use(&mut self, i: &'ast syn::ItemUse) {
        // 处理 use 语句：递归检查 use 树
        self.check_use_tree(&i.tree, &[]);
        // 继续遍历（visit_use_tree 不会触发 visit_path，不会重复）
        syn::visit::visit_item_use(self, i);
    }
}

/// 检查源码中 `sz_orm_query_builder::Query` 的使用并返回告警列表
///
/// # 参数
///
/// - `source`：Rust 源码字符串
///
/// # 返回
///
/// 告警列表。如果源码语法错误，返回空列表。
///
/// # 精确匹配
///
/// 仅匹配 `sz_orm_query_builder::Query` 路径，不匹配其他库的 `Query` 类型。
///
/// # 示例
///
/// ```ignore
/// let source = "use sz_orm_query_builder::Query;";
/// let warnings = lint_source(source);
/// assert_eq!(warnings.len(), 1);
/// ```
pub fn lint_source(source: &str) -> Vec<LintWarning> {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let mut visitor = LintVisitor::new(source);
    visitor.visit_file(&file);
    // 按 (line, col) 去重，避免边界情况重复告警
    let mut seen = HashSet::new();
    visitor
        .warnings
        .into_iter()
        .filter(|w| seen.insert((w.line, w.col)))
        .collect()
}
