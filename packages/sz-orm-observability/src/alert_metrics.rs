//! 告警与仪表盘指标注册表对齐（v10.13.0 ROI-4，2026-10-27 性能评测整改）
//!
//! 2026-10-27 性能评测发现：`monitoring/alerts.yml` 与 `grafana/sz-orm-dashboard.json`
//! 引用的 13 个 `sz_orm_*` 指标名此前在源码中**完全不存在**（纸面配置——即使部署
//! Prometheus + Grafana 也采集不到任何数据）。
//!
//! 本模块提供：
//! 1. [`register_alert_metrics`]：在 [`MetricsRegistry`] 中注册告警/面板引用的全部指标，
//!    返回 [`AlertMetricHandles`] 供生产侧（查询路径 / soak 运行方 / SLO 监控）写入数据。
//!    **这些指标默认值为 0，需要调用方手动接入数据源**（本模块不自动注入任何数据）。
//! 2. 契约测试（`metric_names_match_alerts_and_dashboard`）：从仓库内
//!    `monitoring/alerts.yml` 与 `grafana/sz-orm-dashboard.json` 提取全部 `sz_orm_*`
//!    引用，断言注册后的 `/metrics` 渲染输出包含每一个名字——今后任何一侧
//!    （告警新增指标、注册表删除指标）失配都会在 CI 直接失败。
//!
//! HTTP 端点：将注册后的 registry 交给 [`crate::start_metrics_server`] 即可暴露
//! `GET /metrics`（Prometheus 抓取格式）。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::{Gauge, Histogram, MetricsRegistry};

/// 告警/仪表盘引用的指标定义（名、帮助文本）
///
/// 注意：直方图注册名为 `sz_orm_latency_us`，渲染时产生
/// `sz_orm_latency_us_bucket{le=...}` / `_sum` / `_count`——与
/// alerts.yml 中 `histogram_quantile(... sz_orm_latency_us_bucket ...)` 的引用一致。
pub struct AlertMetricDef {
    /// 指标名（与 alerts.yml / grafana dashboard 引用一致）
    pub name: &'static str,
    /// HELP 文本
    pub help: &'static str,
    /// 指标类型
    pub kind: AlertMetricKind,
}

/// 指标类型（直方图单独处理，不在此枚举内）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertMetricKind {
    /// 累计计数器（`_total` 后缀名）
    Counter,
    /// 瞬时值仪表
    Gauge,
}

/// alerts.yml + grafana/sz-orm-dashboard.json 引用的全部指标（唯一真源）。
/// 契约测试据此双向校验：文件引用但未注册 → 失败；注册但无生产写入方 → 需在文档标注"需手动接入"。
pub const ALERT_METRIC_DEFS: &[AlertMetricDef] = &[
    AlertMetricDef {
        name: "sz_orm_errors_total",
        help: "Total query errors (for error-rate alerts)",
        kind: AlertMetricKind::Counter,
    },
    AlertMetricDef {
        name: "sz_orm_ops_total",
        help: "Total query operations (for error-rate alerts)",
        kind: AlertMetricKind::Counter,
    },
    AlertMetricDef {
        name: "sz_orm_slo_error_budget_remaining",
        help: "Remaining SLO error budget (0..1)",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_slo_long_burn_rate",
        help: "SLO long-window burn rate (6h budget)",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_slo_short_burn_rate",
        help: "SLO short-window burn rate (1h budget)",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_slo_alerting",
        help: "SLO alert state (1 = alerting)",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_soak_fd_count",
        help: "File descriptor count observed by soak monitor",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_soak_ops_per_sec",
        help: "Throughput observed by soak monitor",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_soak_pool_active",
        help: "Pool active connections observed by soak monitor",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_soak_pool_idle",
        help: "Pool idle connections observed by soak monitor",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_soak_pool_max",
        help: "Pool max connections observed by soak monitor",
        kind: AlertMetricKind::Gauge,
    },
    AlertMetricDef {
        name: "sz_orm_soak_rss_bytes",
        help: "RSS bytes observed by soak monitor",
        kind: AlertMetricKind::Gauge,
    },
];

/// 延迟直方图注册名（渲染为 `sz_orm_latency_us_bucket` 等，供 histogram_quantile 使用）
pub const ALERT_LATENCY_HISTOGRAM: &str = "sz_orm_latency_us";
/// 延迟直方图桶边界（μs）：覆盖亚微秒到秒级
pub const ALERT_LATENCY_BUCKETS: &[f64] = &[
    1.0,
    5.0,
    10.0,
    25.0,
    50.0,
    100.0,
    250.0,
    500.0,
    1_000.0,
    2_500.0,
    5_000.0,
    10_000.0,
    50_000.0,
    100_000.0,
    500_000.0,
    1_000_000.0,
];

/// 注册后的指标句柄：生产侧数据源通过这些句柄写入真实数据。
///
/// **接入说明（需手动接入）**：查询路径应在请求完成时 `record_latency` 并按结果
/// `inc_ops` / `inc_errors`；soak 运行方应周期性 `set_soak_*`；SLO 监控应周期性
/// `set_slo_*`。未接入时所有指标渲染为 0（注册 ≠ 有数据）。
#[derive(Clone)]
pub struct AlertMetricHandles {
    /// 错误计数器（查询路径按失败调用 `inc_errors`）
    pub errors_total: Arc<crate::Counter>,
    /// 操作计数器（查询路径按请求调用 `inc_ops`）
    pub ops_total: Arc<crate::Counter>,
    /// 延迟直方图（查询路径按请求调用 `record_latency_us`）
    pub latency_histogram: Arc<Histogram>,
    slo_error_budget_remaining: Arc<Gauge>,
    slo_long_burn_rate: Arc<Gauge>,
    slo_short_burn_rate: Arc<Gauge>,
    slo_alerting: Arc<Gauge>,
    soak_fd_count: Arc<Gauge>,
    soak_ops_per_sec: Arc<Gauge>,
    soak_pool_active: Arc<Gauge>,
    soak_pool_idle: Arc<Gauge>,
    soak_pool_max: Arc<Gauge>,
    soak_rss_bytes: Arc<Gauge>,
}

impl AlertMetricHandles {
    /// 记录一次查询延迟（μs）到延迟直方图
    pub fn record_latency_us(&self, latency_us: f64) {
        self.latency_histogram.observe(latency_us);
    }

    /// 操作计数 +1 / +n
    pub fn inc_ops(&self, n: f64) {
        self.ops_total.inc_by(n);
    }

    /// 错误计数 +1 / +n
    pub fn inc_errors(&self, n: f64) {
        self.errors_total.inc_by(n);
    }

    /// 写入 SLO 燃烧率与预算（由 SLO 监控周期调用）
    pub fn set_slo(&self, short_burn: f64, long_burn: f64, budget_remaining: f64, alerting: bool) {
        self.slo_short_burn_rate.set(short_burn);
        self.slo_long_burn_rate.set(long_burn);
        self.slo_error_budget_remaining.set(budget_remaining);
        self.slo_alerting.set(if alerting { 1.0 } else { 0.0 });
    }

    /// 写入 soak 监控快照（由 soak 运行方周期调用）
    pub fn set_soak_snapshot(
        &self,
        pool_idle: f64,
        pool_active: f64,
        pool_max: f64,
        rss_bytes: f64,
        fd_count: f64,
        ops_per_sec: f64,
    ) {
        self.soak_pool_idle.set(pool_idle);
        self.soak_pool_active.set(pool_active);
        self.soak_pool_max.set(pool_max);
        self.soak_rss_bytes.set(rss_bytes);
        self.soak_fd_count.set(fd_count);
        self.soak_ops_per_sec.set(ops_per_sec);
    }
}

/// 在 registry 中注册告警/仪表盘引用的全部指标并返回写入句柄。
/// 重复调用幂等（registry 按 name 去重）。
pub fn register_alert_metrics(registry: &MetricsRegistry) -> AlertMetricHandles {
    let mut counters: HashMap<&str, Arc<crate::Counter>> = HashMap::new();
    let mut gauges: HashMap<&str, Arc<Gauge>> = HashMap::new();
    for def in ALERT_METRIC_DEFS {
        match def.kind {
            AlertMetricKind::Counter => {
                counters.insert(def.name, registry.register_counter(def.name, def.help));
            }
            AlertMetricKind::Gauge => {
                gauges.insert(def.name, registry.register_gauge(def.name, def.help));
            }
        }
    }
    let latency_histogram = registry.register_histogram(
        ALERT_LATENCY_HISTOGRAM,
        "Query latency distribution (us)",
        ALERT_LATENCY_BUCKETS.to_vec(),
    );

    AlertMetricHandles {
        errors_total: counters["sz_orm_errors_total"].clone(),
        ops_total: counters["sz_orm_ops_total"].clone(),
        latency_histogram,
        slo_error_budget_remaining: gauges["sz_orm_slo_error_budget_remaining"].clone(),
        slo_long_burn_rate: gauges["sz_orm_slo_long_burn_rate"].clone(),
        slo_short_burn_rate: gauges["sz_orm_slo_short_burn_rate"].clone(),
        slo_alerting: gauges["sz_orm_slo_alerting"].clone(),
        soak_fd_count: gauges["sz_orm_soak_fd_count"].clone(),
        soak_ops_per_sec: gauges["sz_orm_soak_ops_per_sec"].clone(),
        soak_pool_active: gauges["sz_orm_soak_pool_active"].clone(),
        soak_pool_idle: gauges["sz_orm_soak_pool_idle"].clone(),
        soak_pool_max: gauges["sz_orm_soak_pool_max"].clone(),
        soak_rss_bytes: gauges["sz_orm_soak_rss_bytes"].clone(),
    }
}

// ===== 2026-10-28 架构评测 P1-3：进程级全局安装 + 查询路径钩子桥接 =====

/// 进程级全局告警指标（registry + 写入句柄）
pub struct GlobalAlertMetrics {
    /// 全局 [`MetricsRegistry`]（可直接交给 [`crate::start_metrics_server`] 暴露 `/metrics`）
    pub registry: Arc<MetricsRegistry>,
    /// 写入句柄（查询路径 / soak / SLO 侧使用）
    pub handles: AlertMetricHandles,
}

static GLOBAL_ALERT_METRICS: OnceLock<GlobalAlertMetrics> = OnceLock::new();

/// 安装进程级全局告警指标（幂等：重复调用返回同一实例）。
///
/// 安装后 [`global_alert_handles`] 供读取句柄，[`global_registry`] 供 HTTP 端点导出；
/// 配合 [`install_query_metrics`] 可把 sz-orm-sqlx 查询热路径的 ops/latency/errors
/// 自动写入本 registry（13 个告警指标中的 `sz_orm_errors_total`/`sz_orm_ops_total`/
/// `sz_orm_latency_us` 由此产生真实数据）。
pub fn install_global_alert_metrics() -> &'static GlobalAlertMetrics {
    GLOBAL_ALERT_METRICS.get_or_init(|| {
        let registry = Arc::new(MetricsRegistry::new());
        let handles = register_alert_metrics(&registry);
        GlobalAlertMetrics { registry, handles }
    })
}

/// 已安装的全局告警指标（未安装时返回 `None`）
pub fn global_alert_metrics() -> Option<&'static GlobalAlertMetrics> {
    GLOBAL_ALERT_METRICS.get()
}

/// 已安装的全局写入句柄（未安装时返回 `None`）
pub fn global_alert_handles() -> Option<&'static AlertMetricHandles> {
    GLOBAL_ALERT_METRICS.get().map(|g| &g.handles)
}

/// 已安装的全局 registry（未安装时返回 `None`）
pub fn global_registry() -> Option<Arc<MetricsRegistry>> {
    GLOBAL_ALERT_METRICS.get().map(|g| g.registry.clone())
}

/// 查询路径钩子：把 core `metrics_hook` 回调转发到全局 `AlertMetricHandles`
struct AlertMetricsHook;

impl sz_orm_core::metrics_hook::QueryMetricsHook for AlertMetricsHook {
    fn record(&self, latency_us: u64, is_error: bool) {
        let handles = &install_global_alert_metrics().handles;
        handles.record_latency_us(latency_us as f64);
        handles.inc_ops(1.0);
        if is_error {
            handles.inc_errors(1.0);
        }
    }
}

/// 安装查询路径指标钩子（幂等：重复安装返回 false）。
///
/// 成功后，sz-orm-sqlx 工厂创建的连接（三后端）每次语句执行都会向全局
/// `AlertMetricHandles` 上报延迟/操作数/错误数——告警与仪表盘从"No data"
/// 变为真实数据。
pub fn install_query_metrics() -> bool {
    sz_orm_core::metrics_hook::install_query_metrics_hook(Arc::new(AlertMetricsHook))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn shared_registry() -> &'static MetricsRegistry {
        static REGISTRY: OnceLock<MetricsRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let registry = MetricsRegistry::default();
            register_alert_metrics(&registry);
            registry
        })
    }

    /// 从 alerts.yml 与 grafana dashboard 提取的 `sz_orm_*` 引用集合
    fn referenced_metric_names() -> Vec<String> {
        let mut names = std::collections::BTreeSet::new();
        for path in [
            "../../monitoring/alerts.yml",
            "../../grafana/sz-orm-dashboard.json",
        ] {
            let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
                panic!("读取 {path} 失败（契约测试必须在仓库根目录结构下运行）: {e}")
            });
            for m in regex_like_iter(&text) {
                names.insert(m);
            }
        }
        names.into_iter().collect()
    }

    /// 轻量 `sz_orm_[a-z0-9_]+` 提取（避免引入 regex 依赖）
    fn regex_like_iter(text: &str) -> Vec<String> {
        let bytes = text.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i + 7 <= bytes.len() {
            if &bytes[i..i + 7] == b"sz_orm_" {
                let mut j = i + 7;
                while j < bytes.len()
                    && (bytes[j].is_ascii_lowercase()
                        || bytes[j].is_ascii_digit()
                        || bytes[j] == b'_')
                {
                    j += 1;
                }
                out.push(text[i..j].to_string());
                i = j;
            } else {
                i += 1;
            }
        }
        out
    }

    /// 契约测试：alerts.yml / grafana dashboard 引用的每个指标名，
    /// 必须能被注册后的 registry 渲染输出覆盖。
    /// 直方图引用（`*_bucket`）按 `<base>_bucket` 渲染匹配。
    #[test]
    fn metric_names_match_alerts_and_dashboard() {
        let registry = shared_registry();
        let rendered = registry.render();

        for name in referenced_metric_names() {
            assert!(
                rendered.contains(&name),
                "alerts.yml / grafana dashboard 引用的指标 `{name}` 未被 register_alert_metrics 注册。\n\
                 修复方式：在 ALERT_METRIC_DEFS 中补充该指标，并同步提供生产写入方。"
            );
        }
    }

    #[test]
    fn registered_metrics_render_with_help_and_type() {
        let rendered = shared_registry().render();
        for def in ALERT_METRIC_DEFS {
            assert!(rendered.contains(&format!("# HELP {} {}", def.name, def.help)));
            let type_str = match def.kind {
                AlertMetricKind::Counter => "counter",
                AlertMetricKind::Gauge => "gauge",
            };
            assert!(rendered.contains(&format!("# TYPE {} {}", def.name, type_str)));
        }
        // 直方图 bucket 暴露（histogram_quantile 依赖）
        assert!(rendered.contains("sz_orm_latency_us_bucket{le="));
        assert!(rendered.contains("sz_orm_latency_us_sum"));
        assert!(rendered.contains("sz_orm_latency_us_count"));
    }

    #[test]
    fn handles_update_values() {
        let registry = MetricsRegistry::default();
        let handles = register_alert_metrics(&registry);
        handles.inc_ops(3.0);
        handles.inc_errors(1.0);
        handles.record_latency_us(120.0);
        handles.set_slo(2.5, 1.5, 0.8, false);
        handles.set_soak_snapshot(4.0, 6.0, 10.0, 1024.0, 32.0, 9_500.0);

        let rendered = registry.render();
        assert!(rendered.contains("sz_orm_ops_total 3"));
        assert!(rendered.contains("sz_orm_errors_total 1"));
        assert!(rendered.contains("sz_orm_latency_us_bucket{le=\"250\"} 1"));
        assert!(rendered.contains("sz_orm_slo_short_burn_rate 2.5"));
        assert!(rendered.contains("sz_orm_soak_pool_idle 4"));
        assert!(rendered.contains("sz_orm_soak_ops_per_sec 9500"));
    }
}
