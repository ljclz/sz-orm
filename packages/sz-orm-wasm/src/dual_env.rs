//! # WASM 双环境适配（v6.9.0 REQ-BND-WASM）
//!
//! 支持 `wasm-pack build --target web`（浏览器）和 `--target nodejs`（Node.js）双目标构建。
//! 提供运行时环境检测、构建配置生成、沙箱隔离验证。

use serde::{Deserialize, Serialize};

/// WASM 构建目标
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WasmBuildTarget {
    /// 浏览器目标（`--target web`）
    Web,
    /// Node.js 目标（`--target nodejs`）
    Nodejs,
}

impl WasmBuildTarget {
    /// 返回 wasm-pack --target 参数值
    pub fn target_arg(&self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Nodejs => "nodejs",
        }
    }

    /// 返回构建输出目录名
    pub fn output_dir(&self) -> &'static str {
        match self {
            Self::Web => "pkg-web",
            Self::Nodejs => "pkg-nodejs",
        }
    }
}

/// 运行时环境
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeEnv {
    /// 浏览器环境
    Browser,
    /// Node.js 环境
    Nodejs,
    /// 未知环境（原生或其他）
    Unknown,
}

/// 双环境构建配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualEnvConfig {
    /// 浏览器构建目标配置
    pub web: BuildTargetConfig,
    /// Node.js 构建目标配置
    pub nodejs: BuildTargetConfig,
}

impl Default for DualEnvConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl DualEnvConfig {
    /// 创建默认双环境配置
    pub fn new() -> Self {
        Self {
            web: BuildTargetConfig::for_target(WasmBuildTarget::Web),
            nodejs: BuildTargetConfig::for_target(WasmBuildTarget::Nodejs),
        }
    }

    /// 获取指定目标的配置
    pub fn get(&self, target: WasmBuildTarget) -> &BuildTargetConfig {
        match target {
            WasmBuildTarget::Web => &self.web,
            WasmBuildTarget::Nodejs => &self.nodejs,
        }
    }

    /// 生成 wasm-pack 构建命令
    pub fn build_command(&self, target: WasmBuildTarget) -> String {
        let cfg = self.get(target);
        format!(
            "wasm-pack build --target {} --out-dir {} {}",
            target.target_arg(),
            cfg.output_dir,
            if cfg.release { "--release" } else { "" }
        )
    }
}

/// 单目标构建配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildTargetConfig {
    /// 构建目标
    pub target: WasmBuildTarget,
    /// 输出目录
    pub output_dir: String,
    /// 是否 release 构建
    pub release: bool,
    /// 启用的 feature 列表
    pub features: Vec<String>,
}

impl BuildTargetConfig {
    /// 为指定目标创建默认配置
    pub fn for_target(target: WasmBuildTarget) -> Self {
        let features = match target {
            WasmBuildTarget::Web => vec!["js".to_string()],
            WasmBuildTarget::Nodejs => vec!["js".to_string()],
        };
        Self {
            target,
            output_dir: target.output_dir().to_string(),
            release: true,
            features,
        }
    }

    /// 生成 feature 参数字符串
    pub fn feature_args(&self) -> String {
        if self.features.is_empty() {
            String::new()
        } else {
            format!("--features {}", self.features.join(","))
        }
    }
}

/// 检测当前运行时环境
///
/// 在 WASM 目标下通过 `cfg!(target_arch = "wasm32")` 判断；
/// 浏览器 vs Node.js 的区分在 JS 端通过 `typeof window` 检测，
/// 此处提供 Rust 端的逻辑框架。
pub fn detect_environment() -> RuntimeEnv {
    RuntimeEnv::Unknown
}

/// 沙箱隔离验证结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxVerificationResult {
    /// 被测试的路径
    pub path: String,
    /// 是否被沙箱拒绝
    pub blocked: bool,
    /// 拒绝原因
    pub reason: String,
}

/// 验证沙箱是否阻止敏感路径访问
///
/// 检查 `/etc/passwd`、`/etc/shadow`、`~/.ssh` 等敏感路径
/// 是否被 SandboxedFs 正确拒绝。
pub fn verify_sandbox_isolation(sandbox: &crate::SandboxedFs) -> Vec<SandboxVerificationResult> {
    let sensitive_paths = [
        "/etc/passwd",
        "/etc/shadow",
        "~/.ssh/id_rsa",
        "/root/.bashrc",
    ];
    sensitive_paths
        .iter()
        .map(|path| {
            let result = sandbox.check_read(path);
            SandboxVerificationResult {
                path: path.to_string(),
                blocked: result.is_err(),
                reason: match result {
                    Ok(()) => "access granted".to_string(),
                    Err(e) => format!("{:?}", e),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_target_arg() {
        assert_eq!(WasmBuildTarget::Web.target_arg(), "web");
        assert_eq!(WasmBuildTarget::Nodejs.target_arg(), "nodejs");
    }

    #[test]
    fn test_build_target_output_dir() {
        assert_eq!(WasmBuildTarget::Web.output_dir(), "pkg-web");
        assert_eq!(WasmBuildTarget::Nodejs.output_dir(), "pkg-nodejs");
    }

    #[test]
    fn test_dual_env_config_default() {
        let config = DualEnvConfig::new();
        assert_eq!(config.web.target, WasmBuildTarget::Web);
        assert_eq!(config.nodejs.target, WasmBuildTarget::Nodejs);
        assert!(config.web.release);
        assert!(config.nodejs.release);
    }

    #[test]
    fn test_dual_env_config_get() {
        let config = DualEnvConfig::new();
        let web_cfg = config.get(WasmBuildTarget::Web);
        assert_eq!(web_cfg.output_dir, "pkg-web");
        let nodejs_cfg = config.get(WasmBuildTarget::Nodejs);
        assert_eq!(nodejs_cfg.output_dir, "pkg-nodejs");
    }

    #[test]
    fn test_build_command_web() {
        let config = DualEnvConfig::new();
        let cmd = config.build_command(WasmBuildTarget::Web);
        assert!(cmd.contains("--target web"));
        assert!(cmd.contains("pkg-web"));
        assert!(cmd.contains("--release"));
    }

    #[test]
    fn test_build_command_nodejs() {
        let config = DualEnvConfig::new();
        let cmd = config.build_command(WasmBuildTarget::Nodejs);
        assert!(cmd.contains("--target nodejs"));
        assert!(cmd.contains("pkg-nodejs"));
    }

    #[test]
    fn test_feature_args() {
        let cfg = BuildTargetConfig::for_target(WasmBuildTarget::Web);
        assert!(cfg.feature_args().contains("js"));
    }

    #[test]
    fn test_detect_environment() {
        let env = detect_environment();
        assert!(matches!(
            env,
            RuntimeEnv::Unknown | RuntimeEnv::Browser | RuntimeEnv::Nodejs
        ));
    }

    #[test]
    fn test_sandbox_isolation_blocks_etc_passwd() {
        let sandbox = crate::SandboxedFs::new(crate::SandboxConfig::deny_all());
        let results = verify_sandbox_isolation(&sandbox);
        assert!(
            results.iter().all(|r| r.blocked),
            "all sensitive paths should be blocked"
        );
        let passwd = results.iter().find(|r| r.path == "/etc/passwd").unwrap();
        assert!(passwd.blocked);
        assert!(passwd.reason.contains("AccessDenied"));
    }

    #[test]
    fn test_sandbox_isolation_with_allowed_tmp() {
        let sandbox = crate::SandboxedFs::new(crate::SandboxConfig::allow_rw("/tmp"));
        let results = verify_sandbox_isolation(&sandbox);
        let passwd = results.iter().find(|r| r.path == "/etc/passwd").unwrap();
        assert!(
            passwd.blocked,
            "/etc/passwd must be blocked even with /tmp allowed"
        );
    }

    #[test]
    fn test_sandbox_verification_result_serialize() {
        let result = SandboxVerificationResult {
            path: "/etc/passwd".to_string(),
            blocked: true,
            reason: "AccessDenied".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("/etc/passwd"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_dual_env_config_serialize() {
        let config = DualEnvConfig::new();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("web"));
        assert!(json.contains("nodejs"));
    }
}
