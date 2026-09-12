//! API 覆盖率审计工具。
//!
//! 提供 sz-orm-core 公共 API 与各绑定层暴露方法的覆盖率审计功能。
//!
//! ## 核心概念
//!
//! - [`CoreApiRegistry`]：核心 API 方法注册表，包含 pool/query/transaction 等模块的全部公共方法
//! - [`BindingApiSnapshot`]：绑定层 API 快照，记录某绑定层当前暴露的方法（已映射到 core 命名空间）
//! - [`ApiCoverageReport`]：覆盖率审计报告，包含覆盖率百分比、已覆盖/未覆盖方法清单
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use sz_orm_core::api_coverage::*;
//!
//! let registry = CoreApiRegistry::new();
//! let snapshot = BindingApiSnapshot::python();
//! let report = audit_coverage(&registry, &snapshot);
//! println!("Python 覆盖率: {:.1}%", report.coverage_percent);
//! ```

use std::collections::BTreeSet;

// ────────────────────────────────────────────────────────────────────────────
//  枚举定义
// ────────────────────────────────────────────────────────────────────────────

/// Core API 方法所属模块。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ApiModule {
    /// 连接池
    Pool,
    /// 查询构造器
    Query,
    /// 事务管理
    Transaction,
    /// 模型 trait
    Model,
    /// 活跃模型
    ActiveModel,
    /// 仓储
    Repository,
    /// 钩子
    Hooks,
    /// 观察者
    Observer,
    /// 分页器
    Paginator,
    /// 迁移
    Migration,
}

impl ApiModule {
    /// 返回模块的字符串名称。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pool => "pool",
            Self::Query => "query",
            Self::Transaction => "transaction",
            Self::Model => "model",
            Self::ActiveModel => "active_model",
            Self::Repository => "repository",
            Self::Hooks => "hooks",
            Self::Observer => "observer",
            Self::Paginator => "paginator",
            Self::Migration => "migration",
        }
    }
}

/// 绑定层名称。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingLayer {
    /// Python 绑定 (PyO3)
    Python,
    /// Java 绑定 (JNI)
    Java,
    /// Go 绑定 (CGO)
    Go,
    /// C++ 绑定 (FFI)
    Cpp,
    /// Node.js 绑定 (napi-rs)
    NodeJs,
    /// WASM 绑定
    Wasm,
    /// C ABI 统一 FFI 层
    CAbi,
}

impl BindingLayer {
    /// 返回绑定层的字符串名称。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Python => "Python",
            Self::Java => "Java",
            Self::Go => "Go",
            Self::Cpp => "C++",
            Self::NodeJs => "Node.js",
            Self::Wasm => "WASM",
            Self::CAbi => "C ABI",
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
//  核心数据结构
// ────────────────────────────────────────────────────────────────────────────

/// Core API 方法描述。
#[derive(Debug, Clone)]
pub struct CoreApiMethod {
    /// 方法所属模块
    pub module: ApiModule,
    /// 方法名
    pub name: &'static str,
    /// 是否异步
    pub is_async: bool,
}

/// API 覆盖率报告。
#[derive(Debug, Clone)]
pub struct ApiCoverageReport {
    /// 绑定层名称
    pub binding_layer_name: String,
    /// 覆盖率百分比 (0.0 ~ 100.0)
    pub coverage_percent: f64,
    /// 已覆盖的方法列表（格式：`module::method`）
    pub covered_methods: Vec<String>,
    /// 未覆盖的方法列表（格式：`module::method`）
    pub uncovered_methods: Vec<String>,
    /// 核心方法总数
    pub total_methods: usize,
    /// 已覆盖方法数
    pub covered_count: usize,
}

impl ApiCoverageReport {
    /// 返回未覆盖方法数。
    pub fn uncovered_count(&self) -> usize {
        self.total_methods - self.covered_count
    }

    /// 是否达到目标覆盖率。
    pub fn meets_target(&self, target_percent: f64) -> bool {
        self.coverage_percent >= target_percent
    }
}

// ────────────────────────────────────────────────────────────────────────────
//  Core API 注册表
// ────────────────────────────────────────────────────────────────────────────

/// Core API 注册表，持有全部核心公共方法。
pub struct CoreApiRegistry {
    methods: Vec<CoreApiMethod>,
}

impl CoreApiRegistry {
    /// 创建包含所有核心 API 的注册表。
    pub fn new() -> Self {
        Self {
            methods: core_api_methods(),
        }
    }

    /// 返回所有方法名（格式：`module::method`）。
    pub fn method_names(&self) -> Vec<String> {
        self.methods
            .iter()
            .map(|m| format!("{}::{}", m.module.as_str(), m.name))
            .collect()
    }

    /// 返回方法总数。
    pub fn len(&self) -> usize {
        self.methods.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }

    /// 按模块获取方法引用列表。
    pub fn methods_by_module(&self, module: ApiModule) -> Vec<&CoreApiMethod> {
        self.methods.iter().filter(|m| m.module == module).collect()
    }

    /// 按模块统计方法数。
    pub fn count_by_module(&self) -> Vec<(ApiModule, usize)> {
        let mut counts = Vec::new();
        for module in [
            ApiModule::Pool,
            ApiModule::Query,
            ApiModule::Transaction,
            ApiModule::Model,
            ApiModule::ActiveModel,
            ApiModule::Repository,
            ApiModule::Hooks,
            ApiModule::Observer,
            ApiModule::Paginator,
            ApiModule::Migration,
        ] {
            let count = self.methods.iter().filter(|m| m.module == module).count();
            counts.push((module, count));
        }
        counts
    }
}

impl Default for CoreApiRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ────────────────────────────────────────────────────────────────────────────
//  绑定层 API 快照
// ────────────────────────────────────────────────────────────────────────────

/// 绑定层 API 快照，记录某绑定层当前暴露的方法。
#[derive(Debug, Clone)]
pub struct BindingApiSnapshot {
    /// 绑定层
    pub layer: BindingLayer,
    /// 暴露的方法名集合（格式：`module::method`，已映射到 core 命名空间）
    pub exposed_methods: BTreeSet<String>,
}

impl BindingApiSnapshot {
    /// 创建空的绑定层快照。
    pub fn new(layer: BindingLayer) -> Self {
        Self {
            layer,
            exposed_methods: BTreeSet::new(),
        }
    }

    /// 从方法名列表创建快照。
    pub fn from_methods(layer: BindingLayer, methods: &[&str]) -> Self {
        Self {
            layer,
            exposed_methods: methods.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// 添加暴露的方法。
    pub fn with_method(mut self, method: &str) -> Self {
        self.exposed_methods.insert(method.to_string());
        self
    }

    /// Python 绑定层当前 API 快照。
    pub fn python() -> Self {
        Self::from_methods(
            BindingLayer::Python,
            &[
                "pool::new",
                "pool::acquire",
                "pool::release",
                "pool::status",
                "pool::close_all",
                "query::new",
                "query::table",
                "query::select",
                "query::where_eq",
                "query::where_ne",
                "query::where_gt",
                "query::where_lt",
                "query::where_like",
                "query::where_in",
                "query::where_between",
                "query::where_null",
                "query::where_not_null",
                "query::order_by",
                "query::order_desc",
                "query::limit",
                "query::offset",
                "query::page",
                "query::join_inner",
                "query::join_left",
                "query::build_select",
                "query::build_insert",
                "query::build_update",
                "query::build_delete",
                "query::build_count",
                "transaction::new",
                "transaction::commit",
                "transaction::rollback",
                "transaction::execute",
                "transaction::query",
                "model::table_name",
                "model::pk",
                "model::set_pk",
                "active_model::from_model",
                "active_model::set",
                "active_model::get",
                "active_model::changed_fields",
                "repository::new",
                "hooks::new",
                "hooks::register",
                "hooks::dispatch",
                "hooks::clear",
                "hooks::count",
                "observer::new",
                "observer::subscribe",
                "observer::dispatch",
            ],
        )
    }

    /// Java 绑定层当前 API 快照。
    pub fn java() -> Self {
        Self::from_methods(
            BindingLayer::Java,
            &[
                "pool::new",
                "pool::acquire",
                "pool::release",
                "pool::status",
                "pool::close_all",
                "query::new",
                "query::table",
                "query::where_eq",
                "query::order_by",
                "query::limit",
                "query::build_select",
                "query::build_insert",
                "query::build_update",
                "query::build_delete",
                "transaction::new",
                "transaction::commit",
                "transaction::rollback",
                "transaction::execute",
                "model::table_name",
                "model::pk",
                "model::set_pk",
                "active_model::set",
                "active_model::get",
                "active_model::save",
            ],
        )
    }

    /// Go 绑定层当前 API 快照。
    pub fn go() -> Self {
        Self::from_methods(
            BindingLayer::Go,
            &[
                "pool::new",
                "pool::acquire",
                "pool::release",
                "pool::status",
                "pool::close_all",
                "query::new",
                "query::table",
                "query::where_eq",
                "query::order_by",
                "query::limit",
                "query::build_select",
                "query::build_insert",
                "query::build_update",
                "query::build_delete",
                "transaction::new",
                "transaction::commit",
                "transaction::rollback",
                "transaction::execute",
                "model::table_name",
                "model::pk",
                "model::set_pk",
                "active_model::set",
                "active_model::get",
                "active_model::save",
            ],
        )
    }

    /// C++ 绑定层当前 API 快照。
    pub fn cpp() -> Self {
        Self::from_methods(
            BindingLayer::Cpp,
            &[
                "pool::new",
                "pool::acquire",
                "pool::release",
                "pool::status",
                "pool::close_all",
                "query::new",
                "query::table",
                "query::where_eq",
                "query::order_by",
                "query::limit",
                "query::build_select",
                "query::build_insert",
                "query::build_update",
                "query::build_delete",
                "transaction::new",
                "transaction::commit",
                "transaction::rollback",
                "transaction::execute",
                "model::table_name",
                "model::pk",
                "model::set_pk",
                "active_model::set",
                "active_model::get",
                "active_model::save",
            ],
        )
    }

    /// Node.js 绑定层当前 API 快照。
    pub fn nodejs() -> Self {
        Self::from_methods(
            BindingLayer::NodeJs,
            &[
                "pool::new",
                "pool::acquire",
                "pool::release",
                "pool::status",
                "pool::close_all",
                "query::new",
                "query::table",
                "query::select",
                "query::where_eq",
                "query::where_ne",
                "query::where_gt",
                "query::where_lt",
                "query::where_like",
                "query::where_in",
                "query::where_between",
                "query::where_null",
                "query::where_not_null",
                "query::order_by",
                "query::order_desc",
                "query::limit",
                "query::offset",
                "query::page",
                "query::join_inner",
                "query::join_left",
                "query::build_select",
                "query::build_insert",
                "query::build_update",
                "query::build_delete",
                "query::build_count",
                "transaction::new",
                "transaction::commit",
                "transaction::rollback",
                "transaction::execute",
                "transaction::query",
                "model::table_name",
                "model::pk",
                "model::set_pk",
                "active_model::from_model",
                "active_model::set",
                "active_model::get",
                "active_model::changed_fields",
                "active_model::into_model",
                "migration::new",
                "migration::migrate",
                "migration::up",
                "migration::down",
                "migration::rollback",
                "migration::progress",
            ],
        )
    }

    /// WASM 绑定层当前 API 快照。
    pub fn wasm() -> Self {
        Self::from_methods(
            BindingLayer::Wasm,
            &[
                "query::new",
                "query::table",
                "query::select",
                "query::where_eq",
                "query::where_like",
                "query::where_in",
                "query::order_by",
                "query::limit",
                "query::offset",
                "query::build_select",
                "query::build_insert",
                "query::build_update",
                "query::build_delete",
                "dual_env::detect_environment",
                "dual_env::verify_sandbox_isolation",
                "dual_env::build_command",
                "proxy_server::handle_http_request",
                "proxy_server::error_to_status_code",
            ],
        )
    }

    /// C ABI 统一 FFI 层当前 API 快照。
    pub fn cabi() -> Self {
        Self::from_methods(
            BindingLayer::CAbi,
            &[
                "pool::new",
                "pool::acquire",
                "pool::release",
                "pool::status",
                "pool::close_all",
                "query::build_select",
                "query::build_insert",
                "query::build_update",
                "query::build_delete",
                "query::build_count",
                "transaction::new",
                "transaction::commit",
                "transaction::rollback",
                "transaction::execute",
                "model::table_name",
                "model::pk",
                "model::set_pk",
            ],
        )
    }

    /// 返回所有绑定层的快照。
    pub fn all_layers() -> Vec<Self> {
        vec![
            Self::python(),
            Self::java(),
            Self::go(),
            Self::cpp(),
            Self::nodejs(),
            Self::wasm(),
            Self::cabi(),
        ]
    }
}

// ────────────────────────────────────────────────────────────────────────────
//  审计函数
// ────────────────────────────────────────────────────────────────────────────

/// 审计单个绑定层的 API 覆盖率。
///
/// 对比核心 API 注册表与绑定层快照，生成覆盖率报告。
pub fn audit_coverage(
    registry: &CoreApiRegistry,
    snapshot: &BindingApiSnapshot,
) -> ApiCoverageReport {
    let all_methods: Vec<String> = registry.method_names();
    let total = all_methods.len();

    let mut covered = Vec::new();
    let mut uncovered = Vec::new();

    for method in &all_methods {
        if snapshot.exposed_methods.contains(method) {
            covered.push(method.clone());
        } else {
            uncovered.push(method.clone());
        }
    }

    let covered_count = covered.len();
    let coverage_percent = if total == 0 {
        100.0
    } else {
        (covered_count as f64 / total as f64) * 100.0
    };

    ApiCoverageReport {
        binding_layer_name: snapshot.layer.as_str().to_string(),
        coverage_percent,
        covered_methods: covered,
        uncovered_methods: uncovered,
        total_methods: total,
        covered_count,
    }
}

/// 审计所有绑定层的 API 覆盖率。
///
/// 返回每个绑定层的覆盖率报告。
pub fn audit_all_layers(registry: &CoreApiRegistry) -> Vec<ApiCoverageReport> {
    BindingApiSnapshot::all_layers()
        .iter()
        .map(|snapshot| audit_coverage(registry, snapshot))
        .collect()
}

/// 生成覆盖率摘要文本报告。
///
/// 输出各绑定层的覆盖率百分比和未覆盖方法数。
pub fn format_summary(reports: &[ApiCoverageReport]) -> String {
    let mut out = String::new();
    out.push_str("=== API 覆盖率审计报告 ===\n\n");
    out.push_str(&format!(
        "{:<12} {:>8} {:>10} {:>10} {:>10}\n",
        "绑定层", "覆盖率", "已覆盖", "未覆盖", "总数"
    ));
    out.push_str(&"-".repeat(55));
    out.push('\n');

    for report in reports {
        out.push_str(&format!(
            "{:<12} {:>7.1}% {:>10} {:>10} {:>10}\n",
            report.binding_layer_name,
            report.coverage_percent,
            report.covered_count,
            report.uncovered_count(),
            report.total_methods,
        ));
    }

    out
}

/// 生成详细覆盖率报告（含未覆盖方法清单）。
///
/// 按优先级排序未覆盖方法：Pool/Query/Transaction > Model/ActiveModel > Hooks/Observer > Repository/Paginator
pub fn format_detail(report: &ApiCoverageReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "=== {} 绑定层 API 覆盖率详细报告 ===\n\n",
        report.binding_layer_name
    ));
    out.push_str(&format!("覆盖率: {:.1}%\n", report.coverage_percent));
    out.push_str(&format!(
        "已覆盖: {}/{}\n\n",
        report.covered_count, report.total_methods
    ));

    if !report.uncovered_methods.is_empty() {
        out.push_str("未覆盖方法（按优先级排序）:\n");
        let mut sorted = report.uncovered_methods.clone();
        sorted.sort_by_key(|a| method_priority(a));
        for method in &sorted {
            out.push_str(&format!("  - {}\n", method));
        }
    }

    out
}

/// 计算方法的优先级排序权重。
///
/// Pool/Query/Transaction (0) > Model/ActiveModel (1) > Hooks/Observer (2) > Repository/Paginator (3) > Migration (4)
fn method_priority(method: &str) -> u8 {
    if method.starts_with("pool::")
        || method.starts_with("query::")
        || method.starts_with("transaction::")
    {
        0
    } else if method.starts_with("model::") || method.starts_with("active_model::") {
        1
    } else if method.starts_with("hooks::") || method.starts_with("observer::") {
        2
    } else if method.starts_with("repository::") || method.starts_with("paginator::") {
        3
    } else {
        4
    }
}

// ────────────────────────────────────────────────────────────────────────────
//  Core API 方法清单（从 core_api_baseline.txt 生成）
// ────────────────────────────────────────────────────────────────────────────

/// 返回核心 API 方法清单。
fn core_api_methods() -> Vec<CoreApiMethod> {
    vec![
        // ── Pool ──
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "new",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "new_async",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "acquire",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "release",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "status",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "close_all",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "reap_idle",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "health_check",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "shutdown",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "warmup",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "config",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "max_size",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "resize",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "set_max_size",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Pool,
            name: "prewarm",
            is_async: true,
        },
        // ── Query ──
        CoreApiMethod {
            module: ApiModule::Query,
            name: "new",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "table",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "select",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_eq",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_ne",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_gt",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_ge",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_lt",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_le",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_like",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_in",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_not_in",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_between",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_not_between",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_null",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "where_not_null",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "or_where_eq",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "or_where_ne",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "or_where_gt",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "or_where_like",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "order_by",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "order_desc",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "group_by",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "having",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "limit",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "offset",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "page",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "join_inner",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "join_left",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "join_right",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_select",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_insert",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_update",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_delete",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_count",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_exists",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_max",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_min",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_sum",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "build_avg",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "validate",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Query,
            name: "sql",
            is_async: false,
        },
        // ── Transaction ──
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "new",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "commit",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "rollback",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "execute",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "query",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "savepoint",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "rollback_to_savepoint",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "release_savepoint",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "state",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "is_active",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "with_isolation",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "read_only",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Transaction,
            name: "with_timeout",
            is_async: false,
        },
        // ── Model ──
        CoreApiMethod {
            module: ApiModule::Model,
            name: "table_name",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Model,
            name: "pk_name",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Model,
            name: "pk",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Model,
            name: "set_pk",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Model,
            name: "foreign_key",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Model,
            name: "timestamp_fields",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Model,
            name: "soft_delete_field",
            is_async: false,
        },
        // ── ActiveModel ──
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "from_model",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "set",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "unset",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "get",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "changed_fields",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "into_model",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "as_model",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "save",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::ActiveModel,
            name: "update",
            is_async: true,
        },
        // ── Repository ──
        CoreApiMethod {
            module: ApiModule::Repository,
            name: "new",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Repository,
            name: "from_vec",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Repository,
            name: "len",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Repository,
            name: "is_empty",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Repository,
            name: "clear",
            is_async: false,
        },
        // ── Hooks ──
        CoreApiMethod {
            module: ApiModule::Hooks,
            name: "new",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Hooks,
            name: "register",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Hooks,
            name: "dispatch",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Hooks,
            name: "clear",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Hooks,
            name: "clear_all",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Hooks,
            name: "count",
            is_async: false,
        },
        // ── Observer ──
        CoreApiMethod {
            module: ApiModule::Observer,
            name: "new",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Observer,
            name: "add_observer",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Observer,
            name: "subscribe",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Observer,
            name: "dispatch",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Observer,
            name: "clear",
            is_async: false,
        },
        // ── Paginator ──
        CoreApiMethod {
            module: ApiModule::Paginator,
            name: "fetch_page",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Paginator,
            name: "set_total",
            is_async: false,
        },
        // ── Migration ──
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "new",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "add_migrations",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "migrate",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "up",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "down",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "rollback",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "reset",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "refresh",
            is_async: true,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "progress",
            is_async: false,
        },
        CoreApiMethod {
            module: ApiModule::Migration,
            name: "build",
            is_async: false,
        },
    ]
}

// ────────────────────────────────────────────────────────────────────────────
//  单元测试
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_api_registry_non_empty() {
        let registry = CoreApiRegistry::new();
        assert!(!registry.is_empty());
        assert!(
            registry.len() >= 50,
            "核心 API 应有 50+ 方法，实际: {}",
            registry.len()
        );
    }

    #[test]
    fn test_core_api_methods_by_module() {
        let registry = CoreApiRegistry::new();
        let pool_methods = registry.methods_by_module(ApiModule::Pool);
        assert!(!pool_methods.is_empty());
        assert!(pool_methods.iter().all(|m| m.module == ApiModule::Pool));

        let query_methods = registry.methods_by_module(ApiModule::Query);
        assert!(!query_methods.is_empty());
        assert!(query_methods.iter().all(|m| m.module == ApiModule::Query));
    }

    #[test]
    fn test_core_api_count_by_module() {
        let registry = CoreApiRegistry::new();
        let counts = registry.count_by_module();
        assert_eq!(counts.len(), 10);

        let total: usize = counts.iter().map(|(_, c)| c).sum();
        assert_eq!(total, registry.len());
    }

    #[test]
    fn test_audit_coverage_python() {
        let registry = CoreApiRegistry::new();
        let snapshot = BindingApiSnapshot::python();
        let report = audit_coverage(&registry, &snapshot);

        assert_eq!(report.binding_layer_name, "Python");
        assert!(report.coverage_percent > 0.0);
        assert!(report.coverage_percent < 100.0);
        assert_eq!(report.covered_count, report.covered_methods.len());
        assert_eq!(report.total_methods, registry.len());
    }

    #[test]
    fn test_audit_coverage_all_layers() {
        let registry = CoreApiRegistry::new();
        let reports = audit_all_layers(&registry);

        assert_eq!(reports.len(), 7);
        for report in &reports {
            assert!(report.coverage_percent >= 0.0);
            assert!(report.coverage_percent <= 100.0);
            assert_eq!(report.total_methods, registry.len());
        }
    }

    #[test]
    fn test_audit_coverage_nodejs_higher_than_java() {
        let registry = CoreApiRegistry::new();
        let nodejs_report = audit_coverage(&registry, &BindingApiSnapshot::nodejs());
        let java_report = audit_coverage(&registry, &BindingApiSnapshot::java());

        assert!(
            nodejs_report.coverage_percent > java_report.coverage_percent,
            "Node.js 覆盖率 ({:.1}%) 应高于 Java ({:.1}%)",
            nodejs_report.coverage_percent,
            java_report.coverage_percent
        );
    }

    #[test]
    fn test_audit_coverage_full_coverage() {
        let registry = CoreApiRegistry::new();
        let owned = registry.method_names();
        let all_methods: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let snapshot = BindingApiSnapshot::from_methods(BindingLayer::CAbi, &all_methods);
        let report = audit_coverage(&registry, &snapshot);

        assert_eq!(report.coverage_percent, 100.0);
        assert!(report.uncovered_methods.is_empty());
    }

    #[test]
    fn test_audit_coverage_zero_coverage() {
        let registry = CoreApiRegistry::new();
        let snapshot = BindingApiSnapshot::new(BindingLayer::Wasm);
        let report = audit_coverage(&registry, &snapshot);

        assert_eq!(report.coverage_percent, 0.0);
        assert!(report.covered_methods.is_empty());
        assert_eq!(report.uncovered_count(), registry.len());
    }

    #[test]
    fn test_format_summary() {
        let registry = CoreApiRegistry::new();
        let reports = audit_all_layers(&registry);
        let summary = format_summary(&reports);

        assert!(summary.contains("API 覆盖率审计报告"));
        assert!(summary.contains("Python"));
        assert!(summary.contains("Node.js"));
    }

    #[test]
    fn test_format_detail() {
        let registry = CoreApiRegistry::new();
        let report = audit_coverage(&registry, &BindingApiSnapshot::python());
        let detail = format_detail(&report);

        assert!(detail.contains("Python"));
        assert!(detail.contains("覆盖率"));
    }

    #[test]
    fn test_method_priority_ordering() {
        assert_eq!(method_priority("pool::new"), 0);
        assert_eq!(method_priority("query::select"), 0);
        assert_eq!(method_priority("transaction::commit"), 0);
        assert_eq!(method_priority("model::table_name"), 1);
        assert_eq!(method_priority("active_model::set"), 1);
        assert_eq!(method_priority("hooks::register"), 2);
        assert_eq!(method_priority("observer::dispatch"), 2);
        assert_eq!(method_priority("repository::new"), 3);
        assert_eq!(method_priority("paginator::fetch_page"), 3);
        assert_eq!(method_priority("migration::up"), 4);
    }

    #[test]
    fn test_api_module_as_str() {
        assert_eq!(ApiModule::Pool.as_str(), "pool");
        assert_eq!(ApiModule::Query.as_str(), "query");
        assert_eq!(ApiModule::Transaction.as_str(), "transaction");
        assert_eq!(ApiModule::Migration.as_str(), "migration");
    }

    #[test]
    fn test_binding_layer_as_str() {
        assert_eq!(BindingLayer::Python.as_str(), "Python");
        assert_eq!(BindingLayer::Java.as_str(), "Java");
        assert_eq!(BindingLayer::NodeJs.as_str(), "Node.js");
        assert_eq!(BindingLayer::CAbi.as_str(), "C ABI");
    }

    #[test]
    fn test_report_meets_target() {
        let registry = CoreApiRegistry::new();
        let report = audit_coverage(&registry, &BindingApiSnapshot::python());

        assert!(!report.meets_target(90.0), "Python 当前覆盖率不应达到 90%");
        assert!(report.meets_target(0.0));
    }

    /// v6.9.0：验证 Python 绑定层覆盖率从基线 32.5% 提升。
    #[test]
    fn test_python_coverage_v690_enhanced() {
        let registry = CoreApiRegistry::new();
        let report = audit_coverage(&registry, &BindingApiSnapshot::python());

        println!(
            "\nPython 绑定层 v6.9.0 覆盖率: {:.1}% ({}/{} 方法)",
            report.coverage_percent, report.covered_count, report.total_methods
        );
        println!("已覆盖模块: active_model, repository, hooks, observer (v6.9.0 新增)");
        println!("未覆盖方法数: {}", report.uncovered_count());

        assert!(
            report.coverage_percent > 40.0,
            "v6.9.0 Python 覆盖率应 > 40%，实际: {:.1}%",
            report.coverage_percent
        );

        let has_active_model = report
            .covered_methods
            .iter()
            .any(|m| m.starts_with("active_model::"));
        let has_hooks = report
            .covered_methods
            .iter()
            .any(|m| m.starts_with("hooks::"));
        let has_observer = report
            .covered_methods
            .iter()
            .any(|m| m.starts_with("observer::"));
        let has_repository = report
            .covered_methods
            .iter()
            .any(|m| m.starts_with("repository::"));

        assert!(has_active_model, "应覆盖 active_model 模块");
        assert!(has_hooks, "应覆盖 hooks 模块");
        assert!(has_observer, "应覆盖 observer 模块");
        assert!(has_repository, "应覆盖 repository 模块");
    }

    /// 输出所有绑定层覆盖率基线报告（--nocapture 时可见）。
    #[test]
    fn test_coverage_baseline_report() {
        let registry = CoreApiRegistry::new();
        let reports = audit_all_layers(&registry);

        println!("\n{}", format_summary(&reports));

        let target_layers = ["Python", "Java", "Go", "C++", "Node.js"];
        for report in &reports {
            if target_layers.contains(&report.binding_layer_name.as_str()) {
                println!("{}", format_detail(report));
            }
        }

        for report in &reports {
            assert!(report.coverage_percent >= 0.0);
            assert!(report.coverage_percent <= 100.0);
        }
    }
}
