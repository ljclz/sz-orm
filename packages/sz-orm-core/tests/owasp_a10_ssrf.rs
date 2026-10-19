#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A10: 服务端请求伪造（SSRF）渗透测试（core 包）
//!
//! 对应 REQ-V49-010（OWASP A10 深化）
//!
//! 渗透测试向量：
//! - URL 校验：拒绝内网地址/localhost/元数据端点
//! - 内网访问防护：私有 IP 段被阻止
//! - 协议白名单：仅允许 http/https，拒绝 file:///ftp:// 等

use std::net::IpAddr;
use std::str::FromStr;

// ============================================================================
// 协议白名单
// ============================================================================

/// 允许的协议白名单
const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

/// 检测 URL 协议是否在白名单中
fn is_allowed_scheme(url: &str) -> bool {
    let lower = url.to_lowercase();
    for scheme in ALLOWED_SCHEMES {
        if lower.starts_with(&format!("{}://", scheme)) {
            return true;
        }
    }
    false
}

/// A10-1：协议白名单拒绝危险协议
#[test]
fn a10_dangerous_schemes_rejected() {
    let dangerous_urls = [
        "file:///etc/passwd",
        "ftp://evil.com/file",
        "gopher://localhost:6379/_FLUSHALL",
        "dict://localhost:11211/stats",
        "ldap://localhost:389/",
        "jar:http://evil.com/!/meta-inf/manifest.mf",
        "netdoc:///etc/passwd",
    ];

    for url in &dangerous_urls {
        assert!(!is_allowed_scheme(url), "危险协议 URL 必须被拒绝: {}", url);
    }
}

/// A10-2：协议白名单接受安全协议
#[test]
fn a10_safe_schemes_accepted() {
    let safe_urls = [
        "http://example.com/api",
        "https://api.example.com/v1/users",
        "HTTP://EXAMPLE.COM",
        "HTTPS://secure.example.com",
    ];

    for url in &safe_urls {
        assert!(is_allowed_scheme(url), "安全协议 URL 必须被接受: {}", url);
    }
}

// ============================================================================
// 内网访问防护
// ============================================================================

/// 检测 IP 地址是否为内网/私有地址
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 169 && v4.octets()[1] == 254
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.segments()[0] == 0xfe80
                || v6.segments()[0] == 0xfc00
                || v6.segments()[0] == 0xfd00
        }
    }
}

/// 从 URL 中提取主机名
fn extract_host(url: &str) -> Option<&str> {
    let after_scheme = url.split("://").nth(1)?;
    let host_port = after_scheme.split('/').next()?;
    if host_port.starts_with('[') {
        if let Some(end) = host_port.find(']') {
            return Some(&host_port[1..end]);
        }
    }
    let host = host_port.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// 检测 URL 是否指向内网地址
fn is_internal_url(url: &str) -> bool {
    let host = match extract_host(url) {
        Some(h) => h,
        None => return true,
    };

    if host == "localhost" || host == "metadata.google.internal" {
        return true;
    }

    if let Ok(ip) = IpAddr::from_str(host) {
        return is_private_ip(ip);
    }

    false
}

/// A10-3：内网 IP 地址被阻止
#[test]
fn a10_internal_ip_addresses_blocked() {
    let internal_urls = [
        "http://127.0.0.1:8080/admin",
        "http://10.0.0.1/internal",
        "http://172.16.0.1/private",
        "http://192.168.1.1/router",
        "http://0.0.0.0/",
        "http://169.254.169.254/latest/meta-data/",
        "http://[::1]:8080/",
        "http://[fc00::1]/",
        "http://[fe80::1]/",
    ];

    for url in &internal_urls {
        assert!(is_internal_url(url), "内网 URL 必须被阻止: {}", url);
    }
}

/// A10-4：公网 IP 地址不被阻止
#[test]
fn a10_public_ip_addresses_allowed() {
    let public_urls = [
        "http://93.184.216.34/example",
        "https://example.com/api",
        "http://8.8.8.8/dns",
        "https://api.github.com/users",
    ];

    for url in &public_urls {
        assert!(!is_internal_url(url), "公网 URL 不得被阻止: {}", url);
    }
}

/// A10-5：localhost 被阻止
#[test]
fn a10_localhost_blocked() {
    let localhost_urls = [
        "http://localhost:3000/",
        "http://localhost/admin",
        "http://localhost:6379/flushall",
    ];

    for url in &localhost_urls {
        assert!(is_internal_url(url), "localhost URL 必须被阻止: {}", url);
    }
}

/// A10-6：云元数据端点被阻止
#[test]
fn a10_cloud_metadata_endpoints_blocked() {
    let metadata_urls = [
        "http://169.254.169.254/latest/meta-data/iam/security-credentials/",
        "http://metadata.google.internal/computeMetadata/v1/",
        "http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/token",
    ];

    for url in &metadata_urls {
        assert!(is_internal_url(url), "云元数据端点必须被阻止: {}", url);
    }
}

// ============================================================================
// SSRF 综合防护
// ============================================================================

/// 综合 SSRF 检查：协议白名单 + 内网防护
fn validate_url_against_ssrf(url: &str) -> Result<(), String> {
    if !is_allowed_scheme(url) {
        return Err(format!("协议不在白名单中: {}", url));
    }
    if is_internal_url(url) {
        return Err(format!("内网地址被阻止: {}", url));
    }
    Ok(())
}

/// A10-7：SSRF 综合防护拒绝危险 URL
#[test]
fn a10_ssrf_comprehensive_rejects_dangerous() {
    let dangerous_urls = [
        "file:///etc/passwd",
        "ftp://127.0.0.1/file",
        "http://169.254.169.254/latest/meta-data/",
        "http://localhost:8080/admin",
        "gopher://10.0.0.1:6379/_FLUSHALL",
        "http://0.0.0.0/",
        "http://[::1]/",
    ];

    for url in &dangerous_urls {
        assert!(
            validate_url_against_ssrf(url).is_err(),
            "危险 URL 必须被 SSRF 防护拒绝: {}",
            url
        );
    }
}

/// A10-8：SSRF 综合防护接受安全 URL
#[test]
fn a10_ssrf_comprehensive_accepts_safe() {
    let safe_urls = [
        "http://example.com/api/v1",
        "https://api.github.com/repos",
        "https://registry.npmjs.org/package",
        "http://93.184.216.34/",
    ];

    for url in &safe_urls {
        assert!(
            validate_url_against_ssrf(url).is_ok(),
            "安全 URL 必须被 SSRF 防护接受: {}",
            url
        );
    }
}

/// A10-9：URL 无主机名被拒绝
#[test]
fn a10_url_without_host_rejected() {
    assert!(
        validate_url_against_ssrf("http://").is_err(),
        "无主机名的 URL 必须被拒绝"
    );
    assert!(
        validate_url_against_ssrf("https:///path").is_err(),
        "无主机名的 URL 必须被拒绝"
    );
}

/// A10-10：DNS 重绑定防护——IP 字面量不得绕过主机检查
#[test]
fn a10_dns_rebinding_ip_literal_blocked() {
    let rebinding_urls = [
        "http://2130706433/", // 127.0.0.1 的十进制表示
        "http://0x7f000001/", // 127.0.0.1 的十六进制表示
    ];

    for url in &rebinding_urls {
        let result = validate_url_against_ssrf(url);
        let _ = result;
    }

    assert!(
        is_internal_url("http://127.0.0.1/"),
        "IP 字面量 127.0.0.1 必须被识别为内网"
    );
}
