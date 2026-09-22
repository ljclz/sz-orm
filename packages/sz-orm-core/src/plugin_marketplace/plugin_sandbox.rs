//! PluginSandbox — 插件沙箱实现
//!
//! CPU/内存/文件系统/网络限制 → 宿主资源保护。
//! `SandboxEnv` 含 `cpu_limit`/`memory_limit`/`fs_isolated`/`network_isolated`。

/// 沙箱配置
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// CPU 限制（核数，0 表示不限）
    pub cpu_limit: f64,
    /// 内存限制（字节，0 表示不限）
    pub memory_limit_bytes: u64,
    /// 文件系统隔离
    pub fs_isolated: bool,
    /// 网络隔离
    pub network_isolated: bool,
    /// 允许的文件系统路径白名单
    pub allowed_paths: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            cpu_limit: 1.0,
            memory_limit_bytes: 64 * 1024 * 1024,
            fs_isolated: true,
            network_isolated: true,
            allowed_paths: Vec::new(),
        }
    }
}

/// 沙箱环境
#[derive(Debug, Clone)]
pub struct SandboxEnv {
    /// CPU 限制
    pub cpu_limit: f64,
    /// 内存限制
    pub memory_limit: u64,
    /// 文件系统是否隔离
    pub fs_isolated: bool,
    /// 网络是否隔离
    pub network_isolated: bool,
    /// 允许的路径
    pub allowed_paths: Vec<String>,
    /// 沙箱 ID
    pub sandbox_id: u64,
}

/// 沙箱违规
#[derive(Debug, Clone)]
pub struct SandboxViolation {
    /// 违规类型
    pub kind: ViolationKind,
    /// 违规详情
    pub detail: String,
}

/// 违规类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationKind {
    /// 访问宿主敏感路径
    HostPathAccess,
    /// 网络访问被拒绝
    NetworkAccess,
    /// 内存超限
    MemoryExceeded,
    /// CPU 超限
    CpuExceeded,
}

/// 宿主敏感路径（沙箱禁止访问）
const HOST_SENSITIVE_PATHS: &[&str] = &[
    "/etc/passwd",
    "/etc/shadow",
    "/root",
    "/proc",
    "/sys",
    "C:\\Windows\\System32",
];

/// 插件沙箱
pub struct PluginSandbox {
    config: SandboxConfig,
    next_id: std::sync::atomic::AtomicU64,
}

impl PluginSandbox {
    /// 创建沙箱
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// 默认配置创建
    pub fn with_default() -> Self {
        Self::new(SandboxConfig::default())
    }

    /// 配置引用
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// 创建沙箱环境
    ///
    /// 生产入口：`PluginSandbox::create`。
    pub fn create(&self) -> SandboxEnv {
        let sandbox_id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        SandboxEnv {
            cpu_limit: self.config.cpu_limit,
            memory_limit: self.config.memory_limit_bytes,
            fs_isolated: self.config.fs_isolated,
            network_isolated: self.config.network_isolated,
            allowed_paths: self.config.allowed_paths.clone(),
            sandbox_id,
        }
    }

    /// 检查路径访问是否违规
    ///
    /// 返回 `Some(violation)` 表示拦截，`None` 表示放行。
    pub fn check_path_access(&self, path: &str) -> Option<SandboxViolation> {
        if !self.config.fs_isolated {
            return None;
        }
        let normalized = path.replace('\\', "/");
        for sensitive in HOST_SENSITIVE_PATHS {
            let sensitive_norm = sensitive.replace('\\', "/");
            if normalized.starts_with(&sensitive_norm) || normalized.contains(&sensitive_norm) {
                return Some(SandboxViolation {
                    kind: ViolationKind::HostPathAccess,
                    detail: format!(
                        "[PLUGIN_SANDBOX_VIOLATION] 插件尝试访问宿主敏感资源: {}",
                        path
                    ),
                });
            }
        }
        if !self.config.allowed_paths.is_empty() {
            let allowed = self
                .config
                .allowed_paths
                .iter()
                .any(|p| normalized.starts_with(&p.replace('\\', "/")));
            if !allowed {
                return Some(SandboxViolation {
                    kind: ViolationKind::HostPathAccess,
                    detail: format!("[PLUGIN_SANDBOX_VIOLATION] 路径 {} 不在白名单中", path),
                });
            }
        }
        None
    }

    /// 检查网络访问是否违规
    pub fn check_network_access(&self, _addr: &str) -> Option<SandboxViolation> {
        if self.config.network_isolated {
            Some(SandboxViolation {
                kind: ViolationKind::NetworkAccess,
                detail: "[PLUGIN_SANDBOX_VIOLATION] 网络访问被拒绝（沙箱隔离）".to_string(),
            })
        } else {
            None
        }
    }

    /// 检查内存使用是否超限
    pub fn check_memory_usage(&self, used_bytes: u64) -> Option<SandboxViolation> {
        if self.config.memory_limit_bytes > 0 && used_bytes > self.config.memory_limit_bytes {
            Some(SandboxViolation {
                kind: ViolationKind::MemoryExceeded,
                detail: format!(
                    "[PLUGIN_SANDBOX_VIOLATION] 内存使用 {} > 限制 {}",
                    used_bytes, self.config.memory_limit_bytes
                ),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let cfg = SandboxConfig::default();
        assert_eq!(cfg.cpu_limit, 1.0);
        assert_eq!(cfg.memory_limit_bytes, 64 * 1024 * 1024);
        assert!(cfg.fs_isolated);
        assert!(cfg.network_isolated);
    }

    #[test]
    fn test_sandbox_create_env() {
        let sandbox = PluginSandbox::with_default();
        let env = sandbox.create();
        assert_eq!(env.cpu_limit, 1.0);
        assert!(env.fs_isolated);
        assert!(env.network_isolated);
        assert!(env.sandbox_id > 0);
    }

    #[test]
    fn test_sandbox_create_unique_ids() {
        let sandbox = PluginSandbox::with_default();
        let env1 = sandbox.create();
        let env2 = sandbox.create();
        assert_ne!(env1.sandbox_id, env2.sandbox_id);
    }

    #[test]
    fn test_sandbox_blocks_host_sensitive_path() {
        let sandbox = PluginSandbox::with_default();
        let violation = sandbox.check_path_access("/etc/passwd");
        assert!(violation.is_some());
        let v = violation.unwrap();
        assert_eq!(v.kind, ViolationKind::HostPathAccess);
        assert!(v.detail.contains("PLUGIN_SANDBOX_VIOLATION"));
    }

    #[test]
    fn test_sandbox_blocks_windows_sensitive_path() {
        let sandbox = PluginSandbox::with_default();
        let violation = sandbox.check_path_access("C:\\Windows\\System32\\config");
        assert!(violation.is_some());
    }

    #[test]
    fn test_sandbox_allows_safe_path_when_no_whitelist() {
        let cfg = SandboxConfig {
            fs_isolated: true,
            allowed_paths: Vec::new(),
            ..SandboxConfig::default()
        };
        let sandbox = PluginSandbox::new(cfg);
        let violation = sandbox.check_path_access("/tmp/plugin/data");
        assert!(violation.is_none());
    }

    #[test]
    fn test_sandbox_enforces_path_whitelist() {
        let cfg = SandboxConfig {
            fs_isolated: true,
            allowed_paths: vec!["/tmp/plugin".to_string()],
            ..SandboxConfig::default()
        };
        let sandbox = PluginSandbox::new(cfg);
        assert!(sandbox.check_path_access("/tmp/plugin/data").is_none());
        assert!(sandbox.check_path_access("/other/path").is_some());
    }

    #[test]
    fn test_sandbox_fs_not_isolated_allows_all() {
        let cfg = SandboxConfig {
            fs_isolated: false,
            ..SandboxConfig::default()
        };
        let sandbox = PluginSandbox::new(cfg);
        assert!(sandbox.check_path_access("/etc/passwd").is_none());
    }

    #[test]
    fn test_sandbox_network_isolated() {
        let sandbox = PluginSandbox::with_default();
        let violation = sandbox.check_network_access("127.0.0.1:8080");
        assert!(violation.is_some());
        assert_eq!(violation.unwrap().kind, ViolationKind::NetworkAccess);
    }

    #[test]
    fn test_sandbox_network_not_isolated() {
        let cfg = SandboxConfig {
            network_isolated: false,
            ..SandboxConfig::default()
        };
        let sandbox = PluginSandbox::new(cfg);
        assert!(sandbox.check_network_access("127.0.0.1:8080").is_none());
    }

    #[test]
    fn test_sandbox_memory_exceeded() {
        let cfg = SandboxConfig {
            memory_limit_bytes: 1024,
            ..SandboxConfig::default()
        };
        let sandbox = PluginSandbox::new(cfg);
        assert!(sandbox.check_memory_usage(2048).is_some());
        assert!(sandbox.check_memory_usage(512).is_none());
    }

    #[test]
    fn test_sandbox_memory_unlimited() {
        let cfg = SandboxConfig {
            memory_limit_bytes: 0,
            ..SandboxConfig::default()
        };
        let sandbox = PluginSandbox::new(cfg);
        assert!(sandbox.check_memory_usage(u64::MAX).is_none());
    }
}
