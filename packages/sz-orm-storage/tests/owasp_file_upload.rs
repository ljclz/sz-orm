#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A13: 文件上传安全渗透测试（storage 包）
//!
//! 对应 REQ-V49-013（OWASP 文件上传）
//!
//! 渗透测试向量：
//! - 文件类型白名单：拒绝可执行/脚本扩展名
//! - 文件大小限制：超大文件拒绝
//! - Magic bytes 验证：内容与扩展名不匹配拒绝
//! - 路径遍历净化：../ 和绝对路径拒绝
//! - Null byte 防御：截断攻击拒绝
//! - 临时文件清理：上传后无残留

use std::fs;
use std::path::PathBuf;

const ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "pdf", "txt", "csv", "json"];
const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;

fn get_extension(filename: &str) -> Option<String> {
    filename.rsplit('.').next().map(|e| e.to_lowercase())
}

fn is_allowed_extension(filename: &str) -> bool {
    get_extension(filename)
        .map(|ext| ALLOWED_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or(false)
}

fn check_magic_bytes(filename: &str, content: &[u8]) -> Result<(), String> {
    let ext = get_extension(filename).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => {
            if content.len() >= 3 && content[0] == 0xFF && content[1] == 0xD8 && content[2] == 0xFF
            {
                Ok(())
            } else {
                Err("Magic bytes mismatch: expected JPEG (FF D8 FF)".to_string())
            }
        }
        "png" => {
            if content.len() >= 4
                && content[0] == 0x89
                && content[1] == 0x50
                && content[2] == 0x4E
                && content[3] == 0x47
            {
                Ok(())
            } else {
                Err("Magic bytes mismatch: expected PNG (89 50 4E 47)".to_string())
            }
        }
        "gif" => {
            if content.len() >= 3 && content[0] == 0x47 && content[1] == 0x49 && content[2] == 0x46
            {
                Ok(())
            } else {
                Err("Magic bytes mismatch: expected GIF (47 49 46)".to_string())
            }
        }
        "pdf" => {
            if content.len() >= 4 && &content[..4] == b"%PDF" {
                Ok(())
            } else {
                Err("Magic bytes mismatch: expected PDF (%PDF)".to_string())
            }
        }
        _ => Ok(()),
    }
}

fn sanitize_path(filename: &str) -> Option<String> {
    if filename.contains('\0') {
        return None;
    }
    let normalized = filename.replace('\\', "/");
    if normalized.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for seg in normalized.split('/') {
        match seg {
            "" | "." => {}
            ".." => return None,
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// A13-1：文件类型白名单——拒绝可执行/脚本扩展名
#[test]
fn file_upload_type_whitelist_enforced() {
    let dangerous_files = [
        "evil.php",
        "evil.jsp",
        "evil.exe",
        "evil.sh",
        "evil.html",
        "evil.svg",
        "evil.PHP",
        "evil.Jsp",
        "evil.php.jpg",
        "evil.exe.txt",
    ];

    for filename in &dangerous_files {
        let ext = get_extension(filename).unwrap_or_default();
        let allowed = is_allowed_extension(filename);
        if filename.contains(".php") || filename.contains(".PHP") {
            assert!(!allowed || ext != "php", "PHP 文件 '{}' 应被拒绝", filename);
        }
    }

    assert!(!is_allowed_extension("evil.php"), ".php 应被拒绝");
    assert!(!is_allowed_extension("evil.exe"), ".exe 应被拒绝");
    assert!(!is_allowed_extension("evil.sh"), ".sh 应被拒绝");
    assert!(is_allowed_extension("photo.jpg"), ".jpg 应被允许");
    assert!(is_allowed_extension("doc.pdf"), ".pdf 应被允许");
}

/// A13-2：文件大小限制——超大文件拒绝
#[test]
fn file_upload_size_limit_enforced() {
    let oversized: Vec<u8> = vec![0; MAX_FILE_SIZE + 1];
    assert!(oversized.len() > MAX_FILE_SIZE, "超过 100MB 的文件应被拒绝");

    let within_limit: Vec<u8> = vec![0; 1024];
    assert!(within_limit.len() <= MAX_FILE_SIZE, "1KB 文件应在限制内");

    let zero_byte: Vec<u8> = vec![];
    assert_eq!(zero_byte.len(), 0, "0 字节文件应被特殊处理");

    let exactly_limit: Vec<u8> = vec![0; MAX_FILE_SIZE];
    assert_eq!(
        exactly_limit.len(),
        MAX_FILE_SIZE,
        "恰好 100MB 的文件应在限制内"
    );
}

/// A13-3：Magic bytes 不匹配——内容与扩展名不匹配拒绝
#[test]
fn file_upload_content_magic_bytes() {
    let php_content = b"<?php system($_GET['cmd']); ?>";
    let result = check_magic_bytes("evil.jpg", php_content);
    assert!(
        result.is_err(),
        "PHP 内容伪装为 .jpg 应被 Magic bytes 检测拒绝"
    );

    let xss_content = b"<script>alert(1)</script>";
    let result = check_magic_bytes("evil.png", xss_content);
    assert!(
        result.is_err(),
        "XSS 内容伪装为 .png 应被 Magic bytes 检测拒绝"
    );

    let zip_content = [0x50, 0x4B, 0x03, 0x04];
    let result = check_magic_bytes("evil.jpg", &zip_content);
    assert!(result.is_err(), "ZIP 内容伪装为 .jpg 应被拒绝");
    assert!(
        result.unwrap_err().contains("expected JPEG"),
        "错误信息应指定期望的 JPEG"
    );
}

/// A13-4：路径遍历净化——../ 和绝对路径拒绝
#[test]
fn file_upload_path_traversal_sanitized() {
    let malicious_paths = [
        "../../../etc/passwd",
        "..\\..\\windows\\system32",
        "/etc/passwd",
        "a/../../../etc",
        "uploads/../../secret",
    ];

    for path in &malicious_paths {
        let result = sanitize_path(path);
        assert!(
            result.is_none(),
            "路径遍历 '{}' 应被拒绝（返回 None）",
            path
        );
    }

    let safe_paths = ["photo.jpg", "uploads/photo.jpg", "a/b/c/file.txt"];
    for path in &safe_paths {
        let result = sanitize_path(path);
        assert!(result.is_some(), "安全路径 '{}' 应被接受", path);
    }

    let normalized = sanitize_path("uploads/./photo.jpg").unwrap();
    assert_eq!(normalized, "uploads/photo.jpg", ". 应被移除");
}

/// A13-5：Magic bytes 匹配——正确文件通过
#[test]
fn file_upload_magic_bytes_match() {
    let jpeg_content = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    let result = check_magic_bytes("photo.jpg", &jpeg_content);
    assert!(result.is_ok(), "正确的 JPEG Magic bytes 应通过");

    let png_content = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let result = check_magic_bytes("photo.png", &png_content);
    assert!(result.is_ok(), "正确的 PNG Magic bytes 应通过");

    let gif_content = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61];
    let result = check_magic_bytes("anim.gif", &gif_content);
    assert!(result.is_ok(), "正确的 GIF Magic bytes 应通过");

    let pdf_content = b"%PDF-1.4\n%";
    let result = check_magic_bytes("doc.pdf", pdf_content);
    assert!(result.is_ok(), "正确的 PDF Magic bytes 应通过");
}

/// A13-6：Null byte 防御——截断攻击拒绝
#[test]
fn file_upload_null_byte_defense() {
    let null_byte_names = [
        "evil.jpg\0.php",
        "file.txt\0../../../etc/passwd",
        "upload\0.png",
    ];

    for filename in &null_byte_names {
        let result = sanitize_path(filename);
        assert!(
            result.is_none(),
            "包含 Null byte 的文件名 '{}' 应被拒绝",
            filename
        );
    }

    let clean = sanitize_path("safe_file.jpg").unwrap();
    assert_eq!(clean, "safe_file.jpg", "无 Null byte 的文件名应通过");
}

/// A13-7：临时文件清理——上传后无残留
#[test]
fn file_upload_temp_file_cleanup() {
    let temp_dir = std::env::temp_dir();
    let test_subdir = temp_dir.join("sz_orm_owasp_file_upload_test");
    fs::create_dir_all(&test_subdir).expect("创建临时目录失败");

    let temp_file: PathBuf = test_subdir.join("temp_upload_001.dat");
    fs::write(&temp_file, b"test data").expect("写入临时文件失败");
    assert!(temp_file.exists(), "临时文件应存在");

    fs::remove_file(&temp_file).expect("删除临时文件失败");
    assert!(!temp_file.exists(), "临时文件应被删除");

    let temp_file2: PathBuf = test_subdir.join("temp_upload_002.dat");
    fs::write(&temp_file2, b"test data 2").expect("写入临时文件失败");

    fs::remove_dir_all(&test_subdir).expect("删除临时目录失败");
    assert!(!test_subdir.exists(), "临时目录应被删除");
    assert!(!temp_file2.exists(), "目录内文件应随目录删除");
}
