#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A02: 加密失败深化渗透测试
//!
//! 对应 REQ-V49-002（OWASP A02 深化）
//!
//! 渗透测试向量：
//! - 弱算法缺席：MD5/DES/RC4/ECB 不在生产代码中使用
//! - 硬编码密钥缺席：源码无硬编码密钥字面量
//! - ECB 模式未使用：AES-GCM 随机 nonce，相同明文密文不同
//! - 不安全随机缺席：thread_rng/DefaultHasher 不在生产代码中使用
//! - 弱密钥长度/弱迭代被拒绝：PBKDF2 迭代 < 100_000 被拒绝

use std::fs;
use std::path::PathBuf;
use sz_orm_crypto::{AesGcmCrypter, Crypter, PasswordHasher, Pbkdf2Hasher};

fn collect_rust_src_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let packages_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    for entry in fs::read_dir(&packages_dir).unwrap() {
        let entry = entry.unwrap();
        let pkg_path = entry.path();
        let src_dir = pkg_path.join("src");
        if src_dir.is_dir() {
            collect_rust_files_recursive(&src_dir, &mut files);
        }
    }
    files
}

fn collect_rust_files_recursive(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files_recursive(&path, files);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            files.push(path);
        }
    }
}

/// 提取生产代码（排除 `#[cfg(test)]` 模块）
fn extract_production_code(content: &str) -> String {
    if let Some(pos) = content.find("#[cfg(test)]") {
        content[..pos].to_string()
    } else {
        content.to_string()
    }
}

/// A02-1：弱算法缺席
///
/// 扫描所有生产源码，断言不使用 MD5/DES/RC4/ECB 弱算法。
/// 排除 TOTP SHA-1（RFC 4226/6238 允许场景）和 `#[cfg(test)]` 模块。
#[test]
fn a02_weak_algorithm_absent() {
    let files = collect_rust_src_files();
    let weak_patterns = ["Md5::new", "Des::new", "Rc4::new", "Ecb::new", "mode::Ecb"];
    let mut violations = Vec::new();

    for file in &files {
        let content = fs::read_to_string(file).unwrap_or_default();
        let prod_code = extract_production_code(&content);
        for pattern in &weak_patterns {
            if prod_code.contains(pattern) {
                violations.push(format!("{}: found `{}`", file.display(), pattern));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "弱算法使用发现:\n{}",
        violations.join("\n")
    );
}

/// A02-2：硬编码密钥缺席
///
/// 扫描生产源码中常见的硬编码密钥模式，断言不存在。
/// 排除 `#[cfg(test)]` 模块中的测试代码。
#[test]
fn a02_hardcoded_secret_absent() {
    let files = collect_rust_src_files();
    let secret_patterns = [
        "\"super-secret\"",
        "\"my-secret-key\"",
        "\"password123\"",
        "\"sk-1234567890\"",
    ];
    let mut violations = Vec::new();

    for file in &files {
        let content = fs::read_to_string(file).unwrap_or_default();
        let prod_code = extract_production_code(&content);
        for pattern in &secret_patterns {
            if prod_code.contains(pattern) {
                violations.push(format!("{}: found hardcoded secret", file.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "硬编码密钥发现:\n{}",
        violations.join("\n")
    );
}

/// A02-3：ECB 模式未使用
///
/// AES-GCM 每次加密生成随机 12 字节 nonce，
/// 相同明文加密两次，密文必须不同（非 ECB 模式）。
#[test]
fn a02_ecb_mode_not_used() {
    let key = [42u8; 32];
    let crypter = AesGcmCrypter::new(&key);

    let plaintext = b"sensitive data for owasp a02 test";

    let ct1 = crypter.encrypt(plaintext).unwrap();
    let ct2 = crypter.encrypt(plaintext).unwrap();

    assert_ne!(
        ct1, ct2,
        "AES-GCM 必须使用随机 nonce，相同明文密文必须不同（ECB 模式则相同）"
    );

    let pt1 = crypter.decrypt(&ct1).unwrap();
    let pt2 = crypter.decrypt(&ct2).unwrap();
    assert_eq!(pt1, plaintext);
    assert_eq!(pt2, plaintext);
}

/// A02-4：不安全随机缺席
///
/// 扫描生产源码，断言不使用 `thread_rng()` / `DefaultHasher::new`
///（安全敏感值应使用 `OsRng`）。排除 `#[cfg(test)]` 模块。
///
/// 已知豁免（非安全敏感场景，登记追踪）：
/// - `sz-orm-batch/src/copy_parallel_shard.rs` — 分片哈希（非密钥）
/// - `sz-orm-core/src/dist_cache.rs` — 缓存击穿守卫（非密钥）
/// - `sz-orm-core/src/l2_cache.rs` — L2 缓存键（非密钥）
/// - `sz-orm-rw/src/lib.rs` — 读写分离路由（非密钥）
#[test]
fn a02_insecure_random_absent() {
    let files = collect_rust_src_files();
    let insecure_patterns = ["thread_rng()", "DefaultHasher::new()"];
    let known_exemptions = [
        "sz-orm-batch\\src\\copy_parallel_shard.rs",
        "sz-orm-core\\src\\dist_cache.rs",
        "sz-orm-core\\src\\l2_cache.rs",
        "sz-orm-rw\\src\\lib.rs",
    ];
    let mut violations = Vec::new();

    for file in &files {
        let content = fs::read_to_string(file).unwrap_or_default();
        let prod_code = extract_production_code(&content);
        for pattern in &insecure_patterns {
            if prod_code.contains(pattern) {
                let is_exempt = known_exemptions
                    .iter()
                    .any(|ex| file.display().to_string().contains(ex));
                if !is_exempt {
                    let relative = file
                        .components()
                        .rev()
                        .take(3)
                        .collect::<Vec<_>>()
                        .iter()
                        .rev()
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join("/");
                    violations.push(format!("{}: found `{}`", relative, pattern));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "不安全随机使用发现（新增，非豁免）:\n{}",
        violations.join("\n")
    );
}

/// A02-5：弱迭代/弱密钥被拒绝
///
/// PBKDF2 迭代次数 < 100_000 被拒绝（M-8 修复）。
/// AES-256 密钥长度由类型系统保证（`&[u8; 32]`）。
#[test]
fn a02_weak_key_length_rejected() {
    let weak_hasher = Pbkdf2Hasher::with_iterations(1000);
    let result = weak_hasher.hash("test_password");
    assert!(
        result.is_err(),
        "PBKDF2 迭代 1000 必须被拒绝（< MIN_ITERATIONS=100_000）"
    );

    let weak_hasher2 = Pbkdf2Hasher::with_iterations(1);
    let result2 = weak_hasher2.hash("test_password");
    assert!(result2.is_err(), "PBKDF2 迭代 1 必须被拒绝");

    let proper_hasher = Pbkdf2Hasher::new();
    let result3 = proper_hasher.hash("test_password");
    assert!(result3.is_ok(), "PBKDF2 默认迭代必须通过");

    let key = [0u8; 32];
    let _crypter = AesGcmCrypter::new(&key);
}
