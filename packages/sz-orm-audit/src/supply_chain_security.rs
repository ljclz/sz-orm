//! v10.11.0 T4.5: 供应链安全深化
//!
//! SBOM 自动生成 + 依赖漏洞持续监控 + 可重现构建验证。
//! 复用既有 cargo audit/cargo deny 工具链，不新增外部依赖。

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// SBOM 格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SbomFormat {
    #[default]
    CycloneDX,
    Spdx,
}

/// SBOM 组件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomComponent {
    pub name: String,
    pub version: String,
    pub package_url: String,
    pub checksum: Option<String>,
}

impl SbomComponent {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            package_url: format!("pkg:cargo/{}@{}", name, version),
            checksum: None,
        }
    }

    pub fn with_checksum(mut self, checksum: &str) -> Self {
        self.checksum = Some(checksum.to_string());
        self
    }
}

/// SBOM（软件物料清单）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sbom {
    pub format: SbomFormat,
    pub components: Vec<SbomComponent>,
    pub generated_at: u64,
    pub spec_version: String,
}

/// 漏洞严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VulnerabilitySeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl VulnerabilitySeverity {
    pub fn level(&self) -> u8 {
        match self {
            VulnerabilitySeverity::Info => 0,
            VulnerabilitySeverity::Low => 1,
            VulnerabilitySeverity::Medium => 2,
            VulnerabilitySeverity::High => 3,
            VulnerabilitySeverity::Critical => 4,
        }
    }
}

/// 漏洞
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vulnerability {
    pub id: String,
    pub package: String,
    pub severity: VulnerabilitySeverity,
    pub description: String,
    pub fixed_version: Option<String>,
}

impl Vulnerability {
    pub fn new(
        id: &str,
        package: &str,
        severity: VulnerabilitySeverity,
        description: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            package: package.to_string(),
            severity,
            description: description.to_string(),
            fixed_version: None,
        }
    }

    pub fn with_fixed_version(mut self, version: &str) -> Self {
        self.fixed_version = Some(version.to_string());
        self
    }

    pub fn is_critical(&self) -> bool {
        self.severity == VulnerabilitySeverity::Critical
    }

    pub fn is_high_or_critical(&self) -> bool {
        self.severity == VulnerabilitySeverity::Critical
            || self.severity == VulnerabilitySeverity::High
    }
}

/// 漏洞报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityReport {
    pub vulnerabilities: Vec<Vulnerability>,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    pub should_block_build: bool,
    pub scanned_at: u64,
}

/// 可重现构建验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproducibleResult {
    pub is_reproducible: bool,
    pub expected_hash: String,
    pub actual_hash: String,
    pub verified_at: u64,
}

/// SBOM 错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbomError {
    GenerationFailed(String),
    NoComponents,
    ParseFailed(String),
}

impl std::fmt::Display for SbomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SbomError::GenerationFailed(msg) => write!(f, "SBOM generation failed: {}", msg),
            SbomError::NoComponents => write!(f, "No components to generate SBOM"),
            SbomError::ParseFailed(msg) => write!(f, "Parse failed: {}", msg),
        }
    }
}

impl std::error::Error for SbomError {}

/// 监控错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorError {
    ScanFailed(String),
    NoSbom,
}

impl std::fmt::Display for MonitorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MonitorError::ScanFailed(msg) => write!(f, "Scan failed: {}", msg),
            MonitorError::NoSbom => write!(f, "No SBOM provided"),
        }
    }
}

impl std::error::Error for MonitorError {}

/// 验证错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    NoExpectedHash,
    HashMismatch(String),
    ComputeFailed(String),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::NoExpectedHash => write!(f, "No expected hash set"),
            VerifyError::HashMismatch(msg) => write!(f, "Hash mismatch: {}", msg),
            VerifyError::ComputeFailed(msg) => write!(f, "Compute failed: {}", msg),
        }
    }
}

impl std::error::Error for VerifyError {}

/// SBOM 生成器
///
/// 生成软件物料清单（SBOM），支持 CycloneDX 和 SPDX 格式。
/// 实际生产环境通过 `cargo cyclonedx` 生成，此处提供编程接口。
pub struct SbomGenerator {
    format: SbomFormat,
    generate_count: Mutex<u64>,
}

impl SbomGenerator {
    pub fn new(format: SbomFormat) -> Self {
        Self {
            format,
            generate_count: Mutex::new(0),
        }
    }

    pub fn format(&self) -> SbomFormat {
        self.format
    }

    /// 从组件列表生成 SBOM
    pub fn generate(&self, components: Vec<SbomComponent>) -> Result<Sbom, SbomError> {
        if components.is_empty() {
            return Err(SbomError::NoComponents);
        }
        *self.generate_count.lock().expect("generate_count lock") += 1;
        let spec_version = match self.format {
            SbomFormat::CycloneDX => "1.4".to_string(),
            SbomFormat::Spdx => "SPDX-2.2".to_string(),
        };
        Ok(Sbom {
            format: self.format,
            components,
            generated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            spec_version,
        })
    }

    /// 从依赖列表生成 SBOM
    pub fn generate_from_deps(&self, deps: &[(&str, &str)]) -> Result<Sbom, SbomError> {
        if deps.is_empty() {
            return Err(SbomError::NoComponents);
        }
        let components: Vec<SbomComponent> = deps
            .iter()
            .map(|(name, version)| SbomComponent::new(name, version))
            .collect();
        self.generate(components)
    }

    pub fn generate_count(&self) -> u64 {
        *self.generate_count.lock().expect("generate_count lock")
    }
}

impl Default for SbomGenerator {
    fn default() -> Self {
        Self::new(SbomFormat::CycloneDX)
    }
}

/// 漏洞监控器
///
/// 依赖漏洞持续监控，发现漏洞告警 + 评估影响 + 建议升级/缓解措施。
/// 高危漏洞阻断构建（should_block_build = true）。
pub struct VulnerabilityMonitor {
    known_vulnerabilities: Mutex<HashMap<String, Vec<Vulnerability>>>,
    scan_count: Mutex<u64>,
    total_vulns_found: Mutex<u64>,
}

impl VulnerabilityMonitor {
    pub fn new() -> Self {
        Self {
            known_vulnerabilities: Mutex::new(HashMap::new()),
            scan_count: Mutex::new(0),
            total_vulns_found: Mutex::new(0),
        }
    }

    /// 注册已知漏洞（生产接线点：从 cargo audit 数据库加载）
    pub fn register_vulnerability(&self, vuln: Vulnerability) {
        let mut map = self.known_vulnerabilities.lock().expect("vulns lock");
        map.entry(vuln.package.clone()).or_default().push(vuln);
    }

    /// 监控 SBOM 中的漏洞
    ///
    /// 扫描 SBOM 组件，匹配已知漏洞，生成漏洞报告。
    /// 高危漏洞（Critical/High）设置 should_block_build = true。
    pub fn monitor(&self, sbom: &Sbom) -> Result<VulnerabilityReport, MonitorError> {
        if sbom.components.is_empty() {
            return Err(MonitorError::NoSbom);
        }

        *self.scan_count.lock().expect("scan_count lock") += 1;

        let map = self.known_vulnerabilities.lock().expect("vulns lock");
        let mut vulnerabilities = Vec::new();

        for component in &sbom.components {
            if let Some(vulns) = map.get(&component.name) {
                for vuln in vulns {
                    if let Some(fixed) = &vuln.fixed_version {
                        if component.version < *fixed {
                            vulnerabilities.push(vuln.clone());
                        }
                    } else {
                        vulnerabilities.push(vuln.clone());
                    }
                }
            }
        }

        let critical_count = vulnerabilities
            .iter()
            .filter(|v| v.severity == VulnerabilitySeverity::Critical)
            .count();
        let high_count = vulnerabilities
            .iter()
            .filter(|v| v.severity == VulnerabilitySeverity::High)
            .count();
        let medium_count = vulnerabilities
            .iter()
            .filter(|v| v.severity == VulnerabilitySeverity::Medium)
            .count();
        let low_count = vulnerabilities
            .iter()
            .filter(|v| v.severity == VulnerabilitySeverity::Low)
            .count();

        let should_block_build = critical_count > 0 || high_count > 0;

        *self.total_vulns_found.lock().expect("total_vulns lock") += vulnerabilities.len() as u64;

        Ok(VulnerabilityReport {
            vulnerabilities,
            critical_count,
            high_count,
            medium_count,
            low_count,
            should_block_build,
            scanned_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    }

    pub fn scan_count(&self) -> u64 {
        *self.scan_count.lock().expect("scan_count lock")
    }

    pub fn total_vulns_found(&self) -> u64 {
        *self.total_vulns_found.lock().expect("total_vulns lock")
    }

    pub fn known_vuln_count(&self) -> usize {
        let map = self.known_vulnerabilities.lock().expect("vulns lock");
        map.values().map(|v| v.len()).sum()
    }
}

impl Default for VulnerabilityMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// 可重现构建验证器
///
/// 锁定文件哈希一致则可重现，不一致告警人工介入。
pub struct ReproducibleBuildVerifier {
    expected_hash: Mutex<Option<String>>,
    verify_count: Mutex<u64>,
    mismatch_count: Mutex<u64>,
}

impl ReproducibleBuildVerifier {
    pub fn new() -> Self {
        Self {
            expected_hash: Mutex::new(None),
            verify_count: Mutex::new(0),
            mismatch_count: Mutex::new(0),
        }
    }

    /// 设置期望的锁定文件哈希（生产接线点：从 CI/CD 基线加载）
    pub fn set_expected_hash(&self, hash: &str) {
        *self.expected_hash.lock().expect("expected_hash lock") = Some(hash.to_string());
    }

    /// 验证可重现构建
    ///
    /// 比较实际哈希与期望哈希，一致则可重现。
    pub fn verify(&self, actual_hash: &str) -> Result<ReproducibleResult, VerifyError> {
        let expected = self
            .expected_hash
            .lock()
            .expect("expected_hash lock")
            .clone()
            .ok_or(VerifyError::NoExpectedHash)?;

        *self.verify_count.lock().expect("verify_count lock") += 1;

        let is_reproducible = expected == actual_hash;
        if !is_reproducible {
            *self.mismatch_count.lock().expect("mismatch_count lock") += 1;
        }

        Ok(ReproducibleResult {
            is_reproducible,
            expected_hash: expected,
            actual_hash: actual_hash.to_string(),
            verified_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    }

    /// 计算锁定文件哈希（SHA-256）
    pub fn compute_hash(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn verify_count(&self) -> u64 {
        *self.verify_count.lock().expect("verify_count lock")
    }

    pub fn mismatch_count(&self) -> u64 {
        *self.mismatch_count.lock().expect("mismatch_count lock")
    }
}

impl Default for ReproducibleBuildVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sbom_generation() {
        let gen = SbomGenerator::new(SbomFormat::CycloneDX);
        let sbom = gen
            .generate_from_deps(&[("serde", "1.0"), ("tokio", "1.0")])
            .unwrap();
        assert_eq!(sbom.components.len(), 2);
        assert_eq!(sbom.format, SbomFormat::CycloneDX);
        assert_eq!(gen.generate_count(), 1);
    }

    #[test]
    fn test_sbom_empty_fails() {
        let gen = SbomGenerator::new(SbomFormat::CycloneDX);
        assert_eq!(gen.generate(vec![]).unwrap_err(), SbomError::NoComponents);
    }

    #[test]
    fn test_vulnerability_monitor() {
        let monitor = VulnerabilityMonitor::new();
        monitor.register_vulnerability(
            Vulnerability::new("CVE-2024-001", "serde", VulnerabilitySeverity::High, "test")
                .with_fixed_version("1.0.1"),
        );

        let gen = SbomGenerator::new(SbomFormat::CycloneDX);
        let sbom = gen.generate_from_deps(&[("serde", "1.0")]).unwrap();

        let report = monitor.monitor(&sbom).unwrap();
        assert_eq!(report.high_count, 1);
        assert!(report.should_block_build);
    }

    #[test]
    fn test_reproducible_verify() {
        let verifier = ReproducibleBuildVerifier::new();
        verifier.set_expected_hash("abc123");
        let result = verifier.verify("abc123").unwrap();
        assert!(result.is_reproducible);
    }

    #[test]
    fn test_reproducible_mismatch() {
        let verifier = ReproducibleBuildVerifier::new();
        verifier.set_expected_hash("abc123");
        let result = verifier.verify("xyz789").unwrap();
        assert!(!result.is_reproducible);
        assert_eq!(verifier.mismatch_count(), 1);
    }
}
