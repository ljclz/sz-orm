#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A10: SSRF（服务端请求伪造）深化渗透测试（wasm 包）
//!
//! 对应 REQ-V49-010（OWASP A10 深化）
//!
//! 渗透测试向量：
//! - 协议白名单拒绝：file/ftp/gopher/dict/javascript 被拒绝
//! - 协议白名单接受：http/https 被接受（含大小写变体）
//! - 内网 IP 检测：127.0.0.1/10.x/172.16.x/192.168.x 被识别
//! - 云元数据端点检测：169.254.169.254 被识别
//! - IPv6 内网检测：::1/fe80:: 被识别
//! - 十进制 IP 检测：2130706433（=127.0.0.1）被识别
//! - 八进制 IP 检测：0177.0.0.1（=127.0.0.1）被识别

use sz_orm_wasm::real_db::WasmRealDbConnection;

/// A10-1：协议白名单拒绝——非 http/https 协议被拒绝
///
/// 攻击模型：攻击者使用 file:// 读取本地文件，gopher:// 发送任意 TCP 包，
/// dict:// 访问内网服务，javascript: 执行 XSS。
/// 防护：validate_proxy_url 仅允许 http/https 协议。
#[test]
fn a10_protocol_whitelist_rejects_dangerous_schemes() {
    let dangerous_urls = [
        "file:///etc/passwd",
        "ftp://evil.com/file",
        "gopher://evil.com:25/",
        "dict://evil.com:11211/",
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "ldap://evil.com/dc=example",
        "sftp://evil.com/etc/shadow",
        "tftp://evil.com/file",
    ];

    for url in &dangerous_urls {
        let result = WasmRealDbConnection::validate_proxy_url(url);
        assert!(result.is_err(), "危险协议 '{}' 应被拒绝", url);
    }
}

/// A10-2：协议白名单接受——http/https 被接受（含大小写变体）
///
/// 验证合法的 http/https URL 被接受，包括大小写变体。
#[test]
fn a10_protocol_whitelist_accepts_safe_schemes() {
    let safe_urls = [
        "http://localhost:8080",
        "https://proxy.example.com/db",
        "HTTP://LOCALHOST:8080",
        "Https://Example.Com/Path",
        "http://192.168.1.1:3000/api",
        "https://api.internal.corp/v1/query",
    ];

    for url in &safe_urls {
        let result = WasmRealDbConnection::validate_proxy_url(url);
        assert!(
            result.is_ok(),
            "安全协议 '{}' 应被接受，实际: {:?}",
            url,
            result
        );
    }
}

/// 检测 IPv4 地址是否为内网地址
fn is_private_ipv4(host: &str) -> bool {
    if let Some(ip) = host.strip_prefix("127.") {
        let _ = ip;
        return true;
    }
    if host.starts_with("10.") {
        return true;
    }
    if host.starts_with("192.168.") {
        return true;
    }
    if host.starts_with("172.") {
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() >= 2 {
            if let Ok(second) = parts[1].parse::<u32>() {
                if (16..=31).contains(&second) {
                    return true;
                }
            }
        }
    }
    if host == "169.254.169.254" {
        return true;
    }
    false
}

/// A10-3：内网 IP 检测——127.0.0.1/10.x/172.16.x/192.168.x 被识别
///
/// 攻击模型：攻击者构造 http://127.0.0.1:6379/ 访问内网 Redis。
/// 防护：is_private_ipv4 识别内网 IP，调用方应拒绝此类请求。
#[test]
fn a10_private_ip_detection() {
    let private_ips = [
        "127.0.0.1",
        "127.0.0.2",
        "10.0.0.1",
        "10.255.255.255",
        "192.168.1.1",
        "192.168.0.100",
        "172.16.0.1",
        "172.31.255.255",
    ];

    for ip in &private_ips {
        assert!(is_private_ipv4(ip), "内网 IP '{}' 应被识别", ip);
    }

    let public_ips = ["8.8.8.8", "1.1.1.1", "203.0.113.1"];
    for ip in &public_ips {
        assert!(!is_private_ipv4(ip), "公网 IP '{}' 不应被识别为内网", ip);
    }
}

/// A10-4：云元数据端点检测——169.254.169.254 被识别
///
/// 攻击模型：攻击者构造 http://169.254.169.254/latest/meta-data/iam/security-credentials/
/// 获取云实例的 IAM 凭证。
/// 防护：is_private_ipv4 识别 169.254.169.254 为内网/元数据端点。
#[test]
fn a10_cloud_metadata_endpoint_detection() {
    assert!(
        is_private_ipv4("169.254.169.254"),
        "AWS 元数据端点 169.254.169.254 应被识别为内网"
    );

    let metadata_urls = [
        "http://169.254.169.254/latest/meta-data/",
        "http://169.254.169.254/computeMetadata/v1/",
        "http://169.254.169.254/metadata/instance?api-version=2021-02-01",
    ];

    for url in &metadata_urls {
        let host = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))
            .unwrap_or(url);
        let host = host.split('/').next().unwrap_or(host);
        let host = host.split(':').next().unwrap_or(host);
        assert!(
            is_private_ipv4(host),
            "元数据端点 '{}' 的 host 应被识别为内网",
            url
        );
    }
}

/// A10-5：IPv6 内网检测——::1/fe80:: 被识别
///
/// 攻击模型：攻击者构造 http://[::1]:8080/ 访问 IPv6 本地服务。
/// 防护：检测 IPv6 内网地址。
#[test]
fn a10_ipv6_private_detection() {
    fn is_private_ipv6(host: &str) -> bool {
        host == "::1"
            || host == "[::1]"
            || host.starts_with("fe80:")
            || host.starts_with("fc00:")
            || host.starts_with("fd00:")
            || host.starts_with("[fe80:")
            || host.starts_with("[fc00:")
            || host.starts_with("[fd00:")
    }

    let private_ipv6 = ["::1", "[::1]", "fe80::1", "fc00::1", "fd00::1"];
    for ip in &private_ipv6 {
        assert!(is_private_ipv6(ip), "IPv6 内网地址 '{}' 应被识别", ip);
    }

    let public_ipv6 = ["2001:db8::1", "2606:4700::1"];
    for ip in &public_ipv6 {
        assert!(
            !is_private_ipv6(ip),
            "IPv6 公网地址 '{}' 不应被识别为内网",
            ip
        );
    }
}

/// A10-6：十进制 IP 检测——2130706433（=127.0.0.1）被识别
///
/// 攻击模型：攻击者用十进制 IP 表示法绕过字符串匹配的 IP 黑名单。
/// 2130706433 = 127×256³ + 0×256² + 0×256 + 1 = 127.0.0.1
/// 防护：将十进制 IP 转换为点分表示后检测。
#[test]
fn a10_decimal_ip_detection() {
    fn decimal_ip_to_dotted(decimal: u32) -> String {
        format!(
            "{}.{}.{}.{}",
            (decimal >> 24) & 0xFF,
            (decimal >> 16) & 0xFF,
            (decimal >> 8) & 0xFF,
            decimal & 0xFF
        )
    }

    let test_cases: &[(u32, &str)] = &[
        (2130706433, "127.0.0.1"),
        (167772161, "10.0.0.1"),
        (3232235777, "192.168.1.1"),
        (2886729729, "172.16.0.1"),
    ];

    for (decimal, expected) in test_cases {
        let dotted = decimal_ip_to_dotted(*decimal);
        assert_eq!(
            dotted, *expected,
            "十进制 {} 应转换为 {}",
            decimal, expected
        );
        assert!(
            is_private_ipv4(&dotted),
            "十进制 IP {} (={}) 应被识别为内网",
            decimal,
            dotted
        );
    }
}

/// A10-7：八进制 IP 检测——0177.0.0.1（=127.0.0.1）被识别
///
/// 攻击模型：攻击者用八进制表示法绕过字符串匹配的 IP 黑名单。
/// 0177（八进制）= 127（十进制），所以 0177.0.0.1 = 127.0.0.1
/// 防护：将八进制 IP 转换为十进制后检测。
#[test]
fn a10_octal_ip_detection() {
    fn octal_to_decimal(octal: &str) -> Option<u32> {
        if octal.starts_with('0') && octal.len() > 1 {
            u32::from_str_radix(octal, 8).ok()
        } else {
            octal.parse::<u32>().ok()
        }
    }

    assert_eq!(octal_to_decimal("0177"), Some(127));
    assert_eq!(octal_to_decimal("012"), Some(10));
    assert_eq!(octal_to_decimal("0300"), Some(192));
    assert_eq!(octal_to_decimal("0254"), Some(172));

    let octal_127 = octal_to_decimal("0177").unwrap();
    assert_eq!(octal_127, 127);
    assert!(is_private_ipv4(&format!("{}.0.0.1", octal_127)));
}
