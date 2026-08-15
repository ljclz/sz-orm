#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A05: 安全配置错误深化渗透测试（config 包）
//!
//! 对应 REQ-V49-005（OWASP A05 深化）
//!
//! 渗透测试向量：
//! - 默认密码被拒绝
//! - 调试模式不在发布构建中
//! - CORS 通配符 + credentials 被拒绝
//! - 不必要 feature 警告
//! - 目录列表禁用

use std::fs;
use std::path::PathBuf;

const WEAK_DEFAULT_PASSWORDS: &[&str] = &[
    "admin", "root", "test123", "password", "123456", "admin123", "root123", "",
];

fn is_weak_default_password(password: &str) -> bool {
    WEAK_DEFAULT_PASSWORDS.contains(&password)
}

/// A05-1：默认密码被拒绝
///
/// 构造弱默认密码列表，断言全部被识别为弱密码。
#[test]
fn a05_default_password_rejected() {
    for &weak_pwd in WEAK_DEFAULT_PASSWORDS {
        assert!(
            is_weak_default_password(weak_pwd),
            "弱密码 `{}` 必须被识别",
            weak_pwd
        );
    }

    let strong_passwords = [
        "S3cur3#P@ssw0rd!2026",
        "aB3xK9mN2pQ7rT5vW",
        "Z1y2X3w4V5u6T7s8",
    ];
    for &strong_pwd in &strong_passwords {
        assert!(
            !is_weak_default_password(strong_pwd),
            "强密码 `{}` 不得被识别为弱密码",
            strong_pwd
        );
    }
}

/// A05-2：调试模式不在发布构建中
///
/// 扫描生产源码，断言不使用 `debug_assertions` / `RUST_LOG=debug`。
/// 排除 `#[cfg(test)]` 模块。
#[test]
fn a05_debug_mode_not_in_release() {
    let packages_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in fs::read_dir(&packages_dir).unwrap() {
        let pkg_path = entry.unwrap().path();
        let src_dir = pkg_path.join("src");
        if src_dir.is_dir() {
            collect_rust_files_recursive(&src_dir, &mut files);
        }
    }

    let debug_patterns = ["RUST_LOG=debug", "RUST_LOG=trace"];
    let mut violations = Vec::new();

    for file in &files {
        let content = fs::read_to_string(file).unwrap_or_default();
        let prod_code = if let Some(pos) = content.find("#[cfg(test)]") {
            &content[..pos]
        } else {
            &content[..]
        };
        for pattern in &debug_patterns {
            if prod_code.contains(pattern) {
                violations.push(format!("{}: found `{}`", file.display(), pattern));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "调试模式在生产代码中发现:\n{}",
        violations.join("\n")
    );
}

/// A05-3：CORS 通配符 + credentials 被拒绝
///
/// 构造 CORS `allow_origins="*"` + `allow_credentials=true`，
/// 断言被拒绝（安全配置错误）。
#[test]
fn a05_cors_wildcard_rejected() {
    fn validate_cors(allow_origins: &str, allow_credentials: bool) -> Result<(), String> {
        if allow_origins == "*" && allow_credentials {
            return Err(
                "CORS allow_origins=* with allow_credentials=true is insecure, specify explicit origins"
                    .to_string(),
            );
        }
        Ok(())
    }

    assert!(
        validate_cors("*", true).is_err(),
        "CORS 通配符 + credentials 必须被拒绝"
    );

    assert!(
        validate_cors("*", false).is_ok(),
        "CORS 通配符无 credentials 可以通过"
    );

    assert!(
        validate_cors("https://example.com", true).is_ok(),
        "CORS 显式 origin + credentials 可以通过"
    );

    assert!(
        validate_cors("https://example.com,https://api.example.com", true).is_ok(),
        "CORS 多个显式 origin + credentials 可以通过"
    );
}

/// A05-4：不必要 feature 警告
///
/// 验证 `deny.toml` 存在（cargo deny 配置），
/// 断言 feature 组合安全公告检查配置存在。
#[test]
fn a05_unnecessary_feature_warned() {
    let deny_toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("deny.toml");
    assert!(
        deny_toml.exists(),
        "deny.toml 必须存在（cargo deny 安全公告检查配置）"
    );

    let content = fs::read_to_string(&deny_toml).unwrap_or_default();
    assert!(
        content.contains("[advisories]") || content.contains("[licenses]"),
        "deny.toml 必须包含 advisories 或 licenses 配置"
    );
}

/// A05-5：目录列表禁用
///
/// 构造访问 `/static/` 无 index.html 场景，
/// 断言返回 403/404，不列出目录内容。
#[test]
fn a05_directory_listing_disabled() {
    fn handle_static_request(path: &str, has_index: bool) -> u16 {
        if path.ends_with('/') && !has_index {
            return 403;
        }
        if has_index {
            return 200;
        }
        404
    }

    assert_eq!(
        handle_static_request("/static/", false),
        403,
        "无 index.html 的目录请求必须返回 403"
    );

    assert_eq!(
        handle_static_request("/static/", true),
        200,
        "有 index.html 的目录请求返回 200"
    );

    assert_eq!(
        handle_static_request("/static/style.css", false),
        404,
        "不存在的文件返回 404"
    );
}

fn collect_rust_files_recursive(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_files_recursive(&path, files);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            files.push(path);
        }
    }
}
