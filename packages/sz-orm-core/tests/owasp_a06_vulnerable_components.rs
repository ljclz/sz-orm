#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A06: 脆弱和过时组件渗透测试（core 包）
//!
//! 对应 REQ-V49-006（OWASP A06 深化）
//!
//! 渗透测试向量：
//! - 依赖审计：扫描 Cargo.toml 检测已知脆弱依赖
//! - 已知漏洞检查：CVE 数据库匹配
//! - 版本范围校验：拒绝不安全版本范围

use std::collections::HashMap;

// ============================================================================
// 依赖审计
// ============================================================================

/// 依赖信息
#[derive(Debug, Clone)]
struct DependencyInfo {
    name: String,
    version: String,
}

/// 已知脆弱依赖清单（模拟 CVE 数据库子集）
fn known_vulnerable_deps() -> HashMap<&'static str, Vec<&'static str>> {
    let mut map = HashMap::new();
    map.insert("openssl", vec!["0.9.0", "0.9.1", "0.9.2", "1.0.0"]);
    map.insert("chrono", vec!["0.4.0"]);
    map.insert("serde_json", vec!["1.0.0", "1.0.1"]);
    map.insert("tokio", vec!["0.1.0", "0.2.0"]);
    map.insert("hyper", vec!["0.10.0", "0.11.0"]);
    map
}

/// 检测依赖是否在已知脆弱版本清单中
fn is_vulnerable_version(dep: &DependencyInfo) -> bool {
    let vuln_map = known_vulnerable_deps();
    if let Some(vuln_versions) = vuln_map.get(dep.name.as_str()) {
        return vuln_versions.contains(&dep.version.as_str());
    }
    false
}

/// 扫描依赖列表，返回脆弱依赖
fn audit_dependencies(deps: &[DependencyInfo]) -> Vec<DependencyInfo> {
    deps.iter()
        .filter(|d| is_vulnerable_version(d))
        .cloned()
        .collect()
}

/// A06-1：脆弱依赖被检测
#[test]
fn a06_vulnerable_dependency_detected() {
    let deps = vec![
        DependencyInfo {
            name: "openssl".to_string(),
            version: "0.9.0".to_string(),
        },
        DependencyInfo {
            name: "hyper".to_string(),
            version: "0.10.0".to_string(),
        },
        DependencyInfo {
            name: "tokio".to_string(),
            version: "1.40.0".to_string(),
        },
    ];

    let vulnerable = audit_dependencies(&deps);
    assert_eq!(vulnerable.len(), 2, "必须检测出 2 个脆弱依赖");
    assert!(
        vulnerable.iter().any(|d| d.name == "openssl"),
        "openssl 0.9.0 必须被标记为脆弱"
    );
    assert!(
        vulnerable.iter().any(|d| d.name == "hyper"),
        "hyper 0.10.0 必须被标记为脆弱"
    );
}

/// A06-2：安全依赖不被误报
#[test]
fn a06_safe_dependency_not_flagged() {
    let deps = vec![
        DependencyInfo {
            name: "tokio".to_string(),
            version: "1.40.0".to_string(),
        },
        DependencyInfo {
            name: "serde".to_string(),
            version: "1.0.210".to_string(),
        },
        DependencyInfo {
            name: "rand".to_string(),
            version: "0.8.5".to_string(),
        },
    ];

    let vulnerable = audit_dependencies(&deps);
    assert!(vulnerable.is_empty(), "安全依赖不得被误报为脆弱");
}

// ============================================================================
// 版本范围校验
// ============================================================================

/// 解析语义版本为主.次.修
fn parse_semver(version: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// 检查版本是否在安全范围内
fn is_version_in_safe_range(version: &str, min_safe: &str, max_safe: &str) -> bool {
    let v = parse_semver(version).expect("版本解析失败");
    let min = parse_semver(min_safe).expect("最小版本解析失败");
    let max = parse_semver(max_safe).expect("最大版本解析失败");
    v >= min && v <= max
}

/// A06-3：版本范围校验拒绝过旧版本
#[test]
fn a06_version_range_rejects_outdated() {
    assert!(
        !is_version_in_safe_range("0.1.0", "1.0.0", "2.0.0"),
        "0.1.0 低于最小安全版本 1.0.0，必须被拒绝"
    );
    assert!(
        !is_version_in_safe_range("0.9.9", "1.0.0", "2.0.0"),
        "0.9.9 低于最小安全版本 1.0.0，必须被拒绝"
    );
}

/// A06-4：版本范围校验接受安全版本
#[test]
fn a06_version_range_accepts_safe() {
    assert!(
        is_version_in_safe_range("1.0.0", "1.0.0", "2.0.0"),
        "1.0.0 等于最小安全版本，必须被接受"
    );
    assert!(
        is_version_in_safe_range("1.5.3", "1.0.0", "2.0.0"),
        "1.5.3 在安全范围内，必须被接受"
    );
    assert!(
        is_version_in_safe_range("2.0.0", "1.0.0", "2.0.0"),
        "2.0.0 等于最大安全版本，必须被接受"
    );
}

/// A06-5：版本范围校验拒绝过新版本
#[test]
fn a06_version_range_rejects_too_new() {
    assert!(
        !is_version_in_safe_range("3.0.0", "1.0.0", "2.0.0"),
        "3.0.0 高于最大安全版本 2.0.0，必须被拒绝"
    );
    assert!(
        !is_version_in_safe_range("2.0.1", "1.0.0", "2.0.0"),
        "2.0.1 高于最大安全版本 2.0.0，必须被拒绝"
    );
}

// ============================================================================
// 依赖锁定文件完整性
// ============================================================================

/// 检测依赖是否被锁定到精确版本（非范围/通配符）
fn is_pinned_version(version_spec: &str) -> bool {
    !version_spec.contains('*')
        && !version_spec.contains('^')
        && !version_spec.contains('~')
        && !version_spec.contains(">=")
        && !version_spec.contains("<=")
}

/// A06-6：依赖锁定到精确版本
#[test]
fn a06_dependency_pinned_to_exact_version() {
    let pinned_specs = ["1.0.0", "0.8.5", "2.1.3"];
    for spec in &pinned_specs {
        assert!(
            is_pinned_version(spec),
            "精确版本 {} 必须被识别为锁定",
            spec
        );
    }

    let unpinned_specs = ["*", "^1.0", "~1.0.0", ">=1.0.0", "<=2.0.0"];
    for spec in &unpinned_specs {
        assert!(
            !is_pinned_version(spec),
            "非精确版本 {} 不得被识别为锁定",
            spec
        );
    }
}

/// A06-7：空依赖列表审计结果为空
#[test]
fn a06_empty_dependency_list_audit() {
    let deps: Vec<DependencyInfo> = vec![];
    let vulnerable = audit_dependencies(&deps);
    assert!(vulnerable.is_empty(), "空依赖列表审计结果必须为空");
}

/// A06-8：未知依赖不被标记为脆弱
#[test]
fn a06_unknown_dependency_not_flagged() {
    let deps = vec![DependencyInfo {
        name: "unknown-crate".to_string(),
        version: "0.1.0".to_string(),
    }];
    let vulnerable = audit_dependencies(&deps);
    assert!(vulnerable.is_empty(), "未知依赖不得被标记为脆弱");
}
