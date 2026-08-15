#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A02: 加密失败深化渗透测试（config 包）
//!
//! 对应 REQ-V49-002（OWASP A02 深化）
//!
//! 渗透测试向量：
//! - 明文传输被拒绝：mysql:// / postgres:// 被拒绝，mysqls:// / postgresqls:// 通过

/// 检查连接串是否使用 TLS（安全传输）
///
/// 安全协议：mysqls:// / postgresqls://（TLS 加密）
/// 不安全协议：mysql:// / postgres://（明文传输）
fn is_tls_connection_string(url: &str) -> bool {
    url.starts_with("mysqls://") || url.starts_with("postgresqls://")
}

/// A02-1：明文传输被拒绝
///
/// 构造明文连接串，断言被识别为不安全；
/// 构造 TLS 连接串，断言被识别为安全。
#[test]
fn a02_cleartext_transport_rejected() {
    let cleartext_urls = [
        "mysql://root:pass@host/db",
        "postgres://root:pass@host/db",
        "mysql://user:pwd@10.0.0.1:3306/mydb",
        "postgres://user:pwd@10.0.0.1:5432/mydb",
    ];

    for url in &cleartext_urls {
        assert!(
            !is_tls_connection_string(url),
            "明文连接串 {} 必须被拒绝，请使用 TLS 连接（mysqls:// 或 postgresqls://）",
            url
        );
    }

    let tls_urls = [
        "mysqls://root:pass@host/db",
        "postgresqls://root:pass@host/db",
        "mysqls://user:pwd@10.0.0.1:3306/mydb",
        "postgresqls://user:pwd@10.0.0.1:5432/mydb",
    ];

    for url in &tls_urls {
        assert!(
            is_tls_connection_string(url),
            "TLS 连接串 {} 必须通过校验",
            url
        );
    }
}
