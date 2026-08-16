//! gRPC 健康检查
//!
//! 实现 gRPC 健康检查协议（grpc.health.v1.Health），
//! 支持服务级健康状态管理和探针。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServingStatus {
    /// 服务未知
    Unknown,
    /// 正在服务
    Serving,
    /// 不在服务
    NotServing,
}

impl ServingStatus {
    /// 状态名
    pub fn as_str(&self) -> &'static str {
        match self {
            ServingStatus::Unknown => "UNKNOWN",
            ServingStatus::Serving => "SERVING",
            ServingStatus::NotServing => "NOT_SERVING",
        }
    }

    /// 是否正在服务
    pub fn is_serving(&self) -> bool {
        matches!(self, ServingStatus::Serving)
    }
}

/// 健康检查服务
pub struct HealthCheckService {
    /// 各服务的健康状态
    statuses: Mutex<HashMap<String, ServiceHealthInfo>>,
    /// 全局服务名（空字符串表示整体）
    overall_name: String,
}

/// 单个服务的健康信息
#[derive(Debug, Clone)]
struct ServiceHealthInfo {
    status: ServingStatus,
    last_check: Instant,
    check_count: u64,
    consecutive_failures: u32,
}

impl ServiceHealthInfo {
    fn new() -> Self {
        Self {
            status: ServingStatus::Unknown,
            last_check: Instant::now(),
            check_count: 0,
            consecutive_failures: 0,
        }
    }
}

impl Default for HealthCheckService {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthCheckService {
    /// 创建健康检查服务
    pub fn new() -> Self {
        Self {
            statuses: Mutex::new(HashMap::new()),
            overall_name: String::new(),
        }
    }

    /// 注册一个服务
    pub fn register(&self, service: &str) {
        if let Ok(mut statuses) = self.statuses.lock() {
            statuses
                .entry(service.to_string())
                .or_insert_with(ServiceHealthInfo::new);
        }
    }

    /// 设置服务状态
    pub fn set_status(&self, service: &str, status: ServingStatus) {
        if let Ok(mut statuses) = self.statuses.lock() {
            let info = statuses
                .entry(service.to_string())
                .or_insert_with(ServiceHealthInfo::new);
            info.status = status;
            info.last_check = Instant::now();
            info.check_count += 1;
            if status == ServingStatus::NotServing {
                info.consecutive_failures += 1;
            } else {
                info.consecutive_failures = 0;
            }
        }
    }

    /// 查询服务状态
    pub fn check(&self, service: &str) -> ServingStatus {
        match self.statuses.lock() {
            Ok(statuses) => statuses
                .get(service)
                .map(|i| i.status)
                .unwrap_or(ServingStatus::Unknown),
            Err(_) => ServingStatus::Unknown,
        }
    }

    /// 检查整体健康状态
    pub fn check_overall(&self) -> ServingStatus {
        self.check(&self.overall_name)
    }

    /// 设置整体健康状态
    pub fn set_overall(&self, status: ServingStatus) {
        self.set_status(&self.overall_name, status);
    }

    /// 列出所有正在服务的服务
    pub fn serving_services(&self) -> Vec<String> {
        match self.statuses.lock() {
            Ok(statuses) => statuses
                .iter()
                .filter(|(_, i)| i.status == ServingStatus::Serving)
                .map(|(k, _)| k.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 列出所有不在服务的服务
    pub fn not_serving_services(&self) -> Vec<String> {
        match self.statuses.lock() {
            Ok(statuses) => statuses
                .iter()
                .filter(|(_, i)| i.status == ServingStatus::NotServing)
                .map(|(k, _)| k.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 获取服务的检查次数
    pub fn check_count(&self, service: &str) -> u64 {
        match self.statuses.lock() {
            Ok(statuses) => statuses.get(service).map(|i| i.check_count).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// 获取服务的连续失败次数
    pub fn consecutive_failures(&self, service: &str) -> u32 {
        match self.statuses.lock() {
            Ok(statuses) => statuses
                .get(service)
                .map(|i| i.consecutive_failures)
                .unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// 距离上次检查的时间
    pub fn time_since_last_check(&self, service: &str) -> Option<Duration> {
        match self.statuses.lock() {
            Ok(statuses) => statuses.get(service).map(|i| i.last_check.elapsed()),
            Err(_) => None,
        }
    }

    /// 执行健康探针
    ///
    /// 调用 `check_fn` 检查服务是否健康，并更新状态。
    pub fn probe<F>(&self, service: &str, check_fn: F) -> ServingStatus
    where
        F: FnOnce() -> bool,
    {
        let healthy = check_fn();
        let status = if healthy {
            ServingStatus::Serving
        } else {
            ServingStatus::NotServing
        };
        self.set_status(service, status);
        status
    }

    /// 生成健康报告
    pub fn report(&self) -> String {
        let mut out = String::from("gRPC Health Report:\n");
        match self.statuses.lock() {
            Ok(statuses) => {
                let mut services: Vec<_> = statuses.keys().collect();
                services.sort();
                for svc in services {
                    let info = &statuses[svc];
                    let name = if svc.is_empty() { "(overall)" } else { svc };
                    out.push_str(&format!(
                        "  {}: {} (checks={}, failures={})\n",
                        name,
                        info.status.as_str(),
                        info.check_count,
                        info.consecutive_failures
                    ));
                }
            }
            Err(_) => out.push_str("  (unable to acquire lock)\n"),
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_serving_status_is_serving() {
        assert!(ServingStatus::Serving.is_serving());
        assert!(!ServingStatus::NotServing.is_serving());
        assert!(!ServingStatus::Unknown.is_serving());
    }

    #[test]
    fn test_serving_status_as_str() {
        assert_eq!(ServingStatus::Unknown.as_str(), "UNKNOWN");
        assert_eq!(ServingStatus::Serving.as_str(), "SERVING");
        assert_eq!(ServingStatus::NotServing.as_str(), "NOT_SERVING");
    }

    #[test]
    fn test_check_unknown_service() {
        let hcs = HealthCheckService::new();
        assert_eq!(hcs.check("unknown"), ServingStatus::Unknown);
    }

    #[test]
    fn test_register_and_check() {
        let hcs = HealthCheckService::new();
        hcs.register("svc1");
        assert_eq!(hcs.check("svc1"), ServingStatus::Unknown);
    }

    #[test]
    fn test_set_and_check_status() {
        let hcs = HealthCheckService::new();
        hcs.set_status("svc1", ServingStatus::Serving);
        assert_eq!(hcs.check("svc1"), ServingStatus::Serving);
        hcs.set_status("svc1", ServingStatus::NotServing);
        assert_eq!(hcs.check("svc1"), ServingStatus::NotServing);
    }

    #[test]
    fn test_overall_status() {
        let hcs = HealthCheckService::new();
        hcs.set_overall(ServingStatus::Serving);
        assert_eq!(hcs.check_overall(), ServingStatus::Serving);
    }

    #[test]
    fn test_serving_services() {
        let hcs = HealthCheckService::new();
        hcs.set_status("a", ServingStatus::Serving);
        hcs.set_status("b", ServingStatus::NotServing);
        hcs.set_status("c", ServingStatus::Serving);
        let mut serving = hcs.serving_services();
        serving.sort();
        assert_eq!(serving, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn test_not_serving_services() {
        let hcs = HealthCheckService::new();
        hcs.set_status("a", ServingStatus::Serving);
        hcs.set_status("b", ServingStatus::NotServing);
        assert_eq!(hcs.not_serving_services(), vec!["b".to_string()]);
    }

    #[test]
    fn test_check_count() {
        let hcs = HealthCheckService::new();
        hcs.set_status("a", ServingStatus::Serving);
        hcs.set_status("a", ServingStatus::Serving);
        hcs.set_status("a", ServingStatus::NotServing);
        assert_eq!(hcs.check_count("a"), 3);
    }

    #[test]
    fn test_consecutive_failures() {
        let hcs = HealthCheckService::new();
        hcs.set_status("a", ServingStatus::NotServing);
        hcs.set_status("a", ServingStatus::NotServing);
        assert_eq!(hcs.consecutive_failures("a"), 2);
        hcs.set_status("a", ServingStatus::Serving);
        assert_eq!(hcs.consecutive_failures("a"), 0);
    }

    #[test]
    fn test_time_since_last_check() {
        let hcs = HealthCheckService::new();
        hcs.set_status("a", ServingStatus::Serving);
        thread::sleep(Duration::from_millis(10));
        let elapsed = hcs.time_since_last_check("a").unwrap();
        assert!(elapsed >= Duration::from_millis(10));
    }

    #[test]
    fn test_probe_healthy() {
        let hcs = HealthCheckService::new();
        let status = hcs.probe("svc", || true);
        assert_eq!(status, ServingStatus::Serving);
        assert_eq!(hcs.check("svc"), ServingStatus::Serving);
    }

    #[test]
    fn test_probe_unhealthy() {
        let hcs = HealthCheckService::new();
        let status = hcs.probe("svc", || false);
        assert_eq!(status, ServingStatus::NotServing);
        assert_eq!(hcs.check("svc"), ServingStatus::NotServing);
    }

    #[test]
    fn test_report() {
        let hcs = HealthCheckService::new();
        hcs.set_status("a", ServingStatus::Serving);
        hcs.set_status("b", ServingStatus::NotServing);
        let r = hcs.report();
        assert!(r.contains("gRPC Health Report"));
        assert!(r.contains("SERVING"));
        assert!(r.contains("NOT_SERVING"));
    }
}
