//! # 健康检查 HTTP 端点（`prod-health-endpoint` feature）
//!
//! 提供 [`HealthEndpointConfig`] 配置化暴露聚合健康状态 JSON，复用
//! [`HealthCheckCache`] TTL 缓存避免高频探活对后端造成压力。
//!
//! ## 使用示例
//!
//! ```no_run
//! use std::sync::Arc;
//! use std::time::Duration;
//! use sz_orm_health::{
//!     DefaultHealthChecker, DbHealthChecker, HealthSnapshot,
//!     endpoint::HealthEndpointConfig,
//! };
//!
//! # #[tokio::main]
//! # async fn main() -> std::io::Result<()> {
//! let checker = Arc::new(DefaultHealthChecker::new()) as Arc<dyn DbHealthChecker>;
//! let config = HealthEndpointConfig::new("/health", 18080, vec!["pool_mysql".into()], Duration::from_secs(5));
//! sz_orm_health::endpoint::start_health_endpoint(config, checker).await?;
//! # Ok(())
//! # }
//! ```

use crate::advanced::HealthCheckCache;
use crate::{DbHealthChecker, HealthReport, HealthStatus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// 健康检查 HTTP 端点配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEndpointConfig {
    /// HTTP 路径，默认 `/health`
    pub path: String,
    /// 监听端口
    pub port: u16,
    /// 检查的资源集合（pool 名称列表）
    pub resources: Vec<String>,
    /// 缓存 TTL
    pub cache_ttl: Duration,
}

impl HealthEndpointConfig {
    pub fn new(path: &str, port: u16, resources: Vec<String>, cache_ttl: Duration) -> Self {
        Self {
            path: path.to_string(),
            port,
            resources,
            cache_ttl,
        }
    }

    /// 默认配置：path=/health, port=8080, resources=[], ttl=5s
    pub fn default_for_port(port: u16) -> Self {
        Self {
            path: "/health".to_string(),
            port,
            resources: vec![],
            cache_ttl: Duration::from_secs(5),
        }
    }
}

/// 聚合健康状态 JSON 响应体
#[derive(Debug, Serialize)]
struct HealthResponse {
    overall: &'static str,
    reports: Vec<HealthReport>,
}

fn status_str(status: &HealthStatus) -> &'static str {
    match status {
        HealthStatus::Healthy => "healthy",
        HealthStatus::Unhealthy => "unhealthy",
        HealthStatus::Unknown => "unknown",
    }
}

fn overall_status(reports: &[HealthReport]) -> HealthStatus {
    let mut any_unknown = false;
    for r in reports {
        match r.status {
            HealthStatus::Unhealthy => return HealthStatus::Unhealthy,
            HealthStatus::Unknown => any_unknown = true,
            HealthStatus::Healthy => {}
        }
    }
    if any_unknown || reports.is_empty() {
        HealthStatus::Unknown
    } else {
        HealthStatus::Healthy
    }
}

/// 启动健康检查 HTTP 端点
///
/// TCP 监听 `config.port`，每连接独立 tokio task，
/// GET `config.path` 返回聚合健康状态 JSON。
/// - Healthy → HTTP 200
/// - Unhealthy → HTTP 503
/// - Unknown → HTTP 503
pub async fn start_health_endpoint(
    config: HealthEndpointConfig,
    checker: Arc<dyn DbHealthChecker>,
) -> std::io::Result<()> {
    let cache = Arc::new(HealthCheckCache::new(checker, config.cache_ttl));
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;

    let path = config.path.clone();
    let resources = Arc::new(config.resources);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let cache = Arc::clone(&cache);
        let path = path.clone();
        let resources = Arc::clone(&resources);

        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            let n = match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
                Ok(0) => return,
                Ok(n) => n,
                Err(_) => return,
            };

            let request = String::from_utf8_lossy(&buf[..n]);
            let first_line = request.lines().next().unwrap_or("");
            let parts: Vec<&str> = first_line.split_whitespace().collect();
            let method = parts.first().copied().unwrap_or("");
            let req_path = parts.get(1).copied().unwrap_or("");

            if method != "GET" || req_path != path {
                let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                return;
            }

            let reports: Vec<HealthReport> = resources.iter().map(|r| cache.check(r)).collect();
            let overall = overall_status(&reports);
            let body = HealthResponse {
                overall: status_str(&overall),
                reports,
            };
            let json = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());
            let status_code = match overall {
                HealthStatus::Healthy => 200,
                HealthStatus::Unhealthy | HealthStatus::Unknown => 503,
            };
            let resp = format!(
                "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status_code,
                json.len(),
                json
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DefaultHealthChecker;

    fn make_checker() -> Arc<dyn DbHealthChecker> {
        let checker = DefaultHealthChecker::new();
        checker.set_healthy("pool_mysql", 5, 0);
        Arc::new(checker)
    }

    #[test]
    fn test_health_endpoint_config_new() {
        let config = HealthEndpointConfig::new(
            "/health",
            18080,
            vec!["pool_mysql".into()],
            Duration::from_secs(5),
        );
        assert_eq!(config.path, "/health");
        assert_eq!(config.port, 18080);
        assert_eq!(config.resources.len(), 1);
        assert_eq!(config.cache_ttl, Duration::from_secs(5));
    }

    #[test]
    fn test_health_endpoint_config_default_for_port() {
        let config = HealthEndpointConfig::default_for_port(9090);
        assert_eq!(config.path, "/health");
        assert_eq!(config.port, 9090);
        assert!(config.resources.is_empty());
    }

    #[test]
    fn test_overall_status_healthy() {
        let reports = vec![
            HealthReport::new("a").set_healthy(),
            HealthReport::new("b").set_healthy(),
        ];
        assert_eq!(overall_status(&reports), HealthStatus::Healthy);
    }

    #[test]
    fn test_overall_status_unhealthy() {
        let reports = vec![
            HealthReport::new("a").set_healthy(),
            HealthReport::new("b").set_status(HealthStatus::Unhealthy),
        ];
        assert_eq!(overall_status(&reports), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_overall_status_unknown() {
        let reports = vec![
            HealthReport::new("a").set_healthy(),
            HealthReport::new("b").set_status(HealthStatus::Unknown),
        ];
        assert_eq!(overall_status(&reports), HealthStatus::Unknown);
    }

    #[test]
    fn test_overall_status_empty() {
        let reports: Vec<HealthReport> = vec![];
        assert_eq!(overall_status(&reports), HealthStatus::Unknown);
    }

    #[tokio::test]
    async fn test_health_endpoint_serves_healthy() {
        let checker = make_checker();
        let config = HealthEndpointConfig::new(
            "/health",
            0,
            vec!["pool_mysql".into()],
            Duration::from_secs(5),
        );

        let cache = Arc::new(HealthCheckCache::new(checker, config.cache_ttl));
        let resources = &config.resources;
        let reports: Vec<HealthReport> = resources.iter().map(|r| cache.check(r)).collect();
        let overall = overall_status(&reports);
        assert_eq!(overall, HealthStatus::Healthy);

        let json = serde_json::to_string(&HealthResponse {
            overall: status_str(&overall),
            reports,
        })
        .unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("pool_mysql"));
    }

    #[tokio::test]
    async fn test_health_endpoint_cache_hit() {
        let checker = make_checker();
        let cache = HealthCheckCache::new(checker, Duration::from_secs(60));
        let r1 = cache.check("pool_mysql");
        let r2 = cache.check("pool_mysql");
        assert_eq!(r1.status, r2.status);
        assert_eq!(r1.pool_name, r2.pool_name);
        let (hits, misses, _) = cache.stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }
}

// ============================================================================
// K8s readiness/liveness 探针端点（prod-probe-endpoint feature）
// ============================================================================

#[cfg(feature = "prod-probe-endpoint")]
mod probe {
    use crate::advanced::ProbeManager;
    use crate::HealthStatus;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    /// K8s readiness/liveness 探针端点配置
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProbeEndpointConfig {
        /// readiness 端点路径，默认 `/ready`
        pub ready_path: String,
        /// liveness 端点路径，默认 `/live`
        pub live_path: String,
        /// 监听端口
        pub port: u16,
        /// K8s initialDelaySeconds
        pub initial_delay_seconds: u32,
        /// K8s periodSeconds
        pub period_seconds: u32,
    }

    impl ProbeEndpointConfig {
        pub fn new(
            ready_path: &str,
            live_path: &str,
            port: u16,
            initial_delay_seconds: u32,
            period_seconds: u32,
        ) -> Self {
            Self {
                ready_path: ready_path.to_string(),
                live_path: live_path.to_string(),
                port,
                initial_delay_seconds,
                period_seconds,
            }
        }

        /// 默认配置：ready_path=/ready, live_path=/live, port=8080, delay=10, period=5
        pub fn default_for_port(port: u16) -> Self {
            Self {
                ready_path: "/ready".to_string(),
                live_path: "/live".to_string(),
                port,
                initial_delay_seconds: 10,
                period_seconds: 5,
            }
        }

        /// 生成 K8s livenessProbe/readinessProbe httpGet 配置片段
        pub fn to_k8s_yaml(&self) -> String {
            format!(
                "livenessProbe:\n  httpGet:\n    path: {}\n    port: {}\n  initialDelaySeconds: {}\n  periodSeconds: {}\nreadinessProbe:\n  httpGet:\n    path: {}\n    port: {}\n  initialDelaySeconds: {}\n  periodSeconds: {}",
                self.live_path,
                self.port,
                self.initial_delay_seconds,
                self.period_seconds,
                self.ready_path,
                self.port,
                self.initial_delay_seconds,
                self.period_seconds,
            )
        }
    }

    fn status_code_for(status: &HealthStatus) -> u16 {
        match status {
            HealthStatus::Healthy => 200,
            HealthStatus::Unhealthy | HealthStatus::Unknown => 503,
        }
    }

    fn status_str(status: &HealthStatus) -> &'static str {
        match status {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Unknown => "unknown",
        }
    }

    /// 启动 K8s readiness/liveness 探针端点
    ///
    /// TCP 监听 `config.port`，暴露两个独立 HTTP 端点：
    /// - GET `config.ready_path`：readiness 探针，反映依赖可用性
    /// - GET `config.live_path`：liveness 探针，仅反映进程级存活
    pub async fn start_probe_endpoint(
        config: ProbeEndpointConfig,
        probe_manager: Arc<ProbeManager>,
    ) -> std::io::Result<()> {
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;

        let ready_path = config.ready_path.clone();
        let live_path = config.live_path.clone();

        loop {
            let (mut stream, _) = listener.accept().await?;
            let pm = Arc::clone(&probe_manager);
            let ready_path = ready_path.clone();
            let live_path = live_path.clone();

            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
                    Ok(0) => return,
                    Ok(n) => n,
                    Err(_) => return,
                };

                let request = String::from_utf8_lossy(&buf[..n]);
                let first_line = request.lines().next().unwrap_or("");
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                let method = parts.first().copied().unwrap_or("");
                let req_path = parts.get(1).copied().unwrap_or("");

                if method != "GET" {
                    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                    return;
                }

                let (status, kind) = if req_path == ready_path {
                    (pm.overall_readiness(), "readiness")
                } else if req_path == live_path {
                    (pm.overall_liveness(), "liveness")
                } else {
                    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
                    return;
                };

                let code = status_code_for(&status);
                let body = format!(
                    "{{\"kind\":\"{}\",\"status\":\"{}\"}}",
                    kind,
                    status_str(&status)
                );
                let resp = format!(
                    "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    code,
                    body.len(),
                    body
                );
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, resp.as_bytes()).await;
            });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::HealthSnapshot;

        #[test]
        fn test_probe_endpoint_config_new() {
            let config = ProbeEndpointConfig::new("/ready", "/live", 18081, 10, 5);
            assert_eq!(config.ready_path, "/ready");
            assert_eq!(config.live_path, "/live");
            assert_eq!(config.port, 18081);
            assert_eq!(config.initial_delay_seconds, 10);
            assert_eq!(config.period_seconds, 5);
        }

        #[test]
        fn test_probe_endpoint_config_default_for_port() {
            let config = ProbeEndpointConfig::default_for_port(9090);
            assert_eq!(config.ready_path, "/ready");
            assert_eq!(config.live_path, "/live");
            assert_eq!(config.port, 9090);
        }

        #[test]
        fn test_to_k8s_yaml_contains_liveness() {
            let config = ProbeEndpointConfig::new("/ready", "/live", 18081, 10, 5);
            let yaml = config.to_k8s_yaml();
            assert!(yaml.contains("livenessProbe"));
            assert!(yaml.contains("readinessProbe"));
            assert!(yaml.contains("path: /live"));
            assert!(yaml.contains("path: /ready"));
            assert!(yaml.contains("port: 18081"));
            assert!(yaml.contains("initialDelaySeconds: 10"));
            assert!(yaml.contains("periodSeconds: 5"));
        }

        #[test]
        fn test_probe_manager_readiness_unhealthy_liveness_healthy() {
            let pm = ProbeManager::new();
            pm.set_liveness("app", HealthSnapshot::healthy());
            pm.set_readiness("db", HealthSnapshot::unhealthy("connection refused"));
            assert_eq!(pm.overall_liveness(), HealthStatus::Healthy);
            assert_eq!(pm.overall_readiness(), HealthStatus::Unhealthy);
        }

        #[test]
        fn test_probe_manager_both_healthy() {
            let pm = ProbeManager::new();
            pm.set_liveness("app", HealthSnapshot::healthy());
            pm.set_readiness("db", HealthSnapshot::healthy());
            assert_eq!(pm.overall_liveness(), HealthStatus::Healthy);
            assert_eq!(pm.overall_readiness(), HealthStatus::Healthy);
        }

        #[test]
        fn test_status_code_for_healthy() {
            assert_eq!(status_code_for(&HealthStatus::Healthy), 200);
            assert_eq!(status_code_for(&HealthStatus::Unhealthy), 503);
            assert_eq!(status_code_for(&HealthStatus::Unknown), 503);
        }
    }
}

#[cfg(feature = "prod-probe-endpoint")]
pub use probe::{start_probe_endpoint, ProbeEndpointConfig};
