//! OLAP 资源限制（`olap-vectorized` feature）
//!
//! 限制 OLAP 查询的扫描行数、执行时间、内存使用，
//! 超限时拒绝执行，防止分析查询影响 OLTP 业务。

use std::time::Duration;

/// 资源限制配置
#[derive(Debug, Clone)]
pub struct ResourceLimit {
    /// 最大扫描行数
    pub max_scan_rows: u64,
    /// 最大执行时间
    pub max_execution_time: Duration,
    /// 最大内存使用（MB）
    max_memory_mb: u64,
}

impl Default for ResourceLimit {
    fn default() -> Self {
        Self {
            max_scan_rows: 10_000_000,
            max_execution_time: Duration::from_secs(300),
            max_memory_mb: 4096,
        }
    }
}

impl ResourceLimit {
    /// 不限制（用于内部维护查询）
    pub fn unlimited() -> Self {
        Self {
            max_scan_rows: u64::MAX,
            max_execution_time: Duration::from_secs(u64::MAX),
            max_memory_mb: u64::MAX,
        }
    }
}

/// 资源超限类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceExceeded {
    /// 扫描行数超限
    ScanRowsExceeded { actual: u64, limit: u64 },
    /// 执行时间超限
    ExecutionTimeExceeded { actual_ms: u64, limit_ms: u64 },
    /// 内存使用超限
    MemoryExceeded { actual_mb: u64, limit_mb: u64 },
}

/// 资源检查结果
#[derive(Debug, Clone)]
pub enum ResourceCheckResult {
    /// 通过
    Ok,
    /// 超限
    Exceeded(ResourceExceeded),
}

/// OLAP 资源守卫
#[derive(Debug)]
pub struct OlapResourceGuard {
    limit: ResourceLimit,
    /// 是否允许无限资源（olap-unlimited feature）
    allow_unlimited: bool,
}

impl OlapResourceGuard {
    pub fn new(limit: ResourceLimit) -> Self {
        Self {
            limit,
            allow_unlimited: false,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(ResourceLimit::default())
    }

    pub fn allow_unlimited(mut self) -> Self {
        self.allow_unlimited = true;
        self
    }

    pub fn limit(&self) -> &ResourceLimit {
        &self.limit
    }

    /// 检查资源是否在限制内
    pub fn check(
        &self,
        scan_rows: u64,
        execution_time: Duration,
        memory_mb: u64,
    ) -> ResourceCheckResult {
        if self.allow_unlimited {
            return ResourceCheckResult::Ok;
        }
        if scan_rows > self.limit.max_scan_rows {
            return ResourceCheckResult::Exceeded(ResourceExceeded::ScanRowsExceeded {
                actual: scan_rows,
                limit: self.limit.max_scan_rows,
            });
        }
        if execution_time > self.limit.max_execution_time {
            return ResourceCheckResult::Exceeded(ResourceExceeded::ExecutionTimeExceeded {
                actual_ms: execution_time.as_millis() as u64,
                limit_ms: self.limit.max_execution_time.as_millis() as u64,
            });
        }
        if memory_mb > self.limit.max_memory_mb {
            return ResourceCheckResult::Exceeded(ResourceExceeded::MemoryExceeded {
                actual_mb: memory_mb,
                limit_mb: self.limit.max_memory_mb,
            });
        }
        ResourceCheckResult::Ok
    }

    /// 预检查扫描行数（在执行前调用）
    pub fn pre_check_scan_rows(&self, estimated_rows: u64) -> ResourceCheckResult {
        if self.allow_unlimited || estimated_rows <= self.limit.max_scan_rows {
            ResourceCheckResult::Ok
        } else {
            ResourceCheckResult::Exceeded(ResourceExceeded::ScanRowsExceeded {
                actual: estimated_rows,
                limit: self.limit.max_scan_rows,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_limits_passes() {
        let guard = OlapResourceGuard::with_defaults();
        let result = guard.check(1000, Duration::from_secs(10), 100);
        assert!(matches!(result, ResourceCheckResult::Ok));
    }

    #[test]
    fn scan_rows_exceeded() {
        let guard = OlapResourceGuard::with_defaults();
        let result = guard.check(20_000_000, Duration::from_secs(10), 100);
        assert!(matches!(
            result,
            ResourceCheckResult::Exceeded(ResourceExceeded::ScanRowsExceeded { .. })
        ));
    }

    #[test]
    fn execution_time_exceeded() {
        let guard = OlapResourceGuard::with_defaults();
        let result = guard.check(1000, Duration::from_secs(600), 100);
        assert!(matches!(
            result,
            ResourceCheckResult::Exceeded(ResourceExceeded::ExecutionTimeExceeded { .. })
        ));
    }

    #[test]
    fn unlimited_allows_everything() {
        let guard = OlapResourceGuard::with_defaults().allow_unlimited();
        let result = guard.check(u64::MAX, Duration::from_secs(u64::MAX), u64::MAX);
        assert!(matches!(result, ResourceCheckResult::Ok));
    }

    #[test]
    fn pre_check_scan_rows() {
        let guard = OlapResourceGuard::with_defaults();
        assert!(matches!(
            guard.pre_check_scan_rows(5000),
            ResourceCheckResult::Ok
        ));
        assert!(matches!(
            guard.pre_check_scan_rows(20_000_000),
            ResourceCheckResult::Exceeded(_)
        ));
    }
}
