//! 黑帽审计 PoC（攻击者视角）——2026-08-14
//!
//! 对应白帽报告：H-1（HmacSigner 参数走私）、M-8（PBKDF2 弱迭代接受）。

use std::collections::HashMap;
use sz_orm_crypto::{ApiSigner, HmacSigner, PasswordHasher, Pbkdf2Hasher};

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（H-1 修复验证）：HmacSigner 参数走私被切断
//
// 修复前（v4.8.0 之前行为）：`{a:"1", b:"2"}` 与 `{a:"1&b=2"}` 产生完全
// 相同签名（黑帽实证）。修复后：key/value 做 RFC 3986 percent-encoding，
// 两个参数集的规范串不同 → 签名不同 → 走私请求被拒绝。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_hmac_signer_parameter_smuggling_blocked() {
    let signer = HmacSigner::new();
    let secret = "shared-api-secret";

    // 合法请求：两个独立参数
    let mut legit = HashMap::new();
    legit.insert("a".to_string(), "1".to_string());
    legit.insert("b".to_string(), "2".to_string());
    let legit_sig = signer.sign(&legit, secret);

    // 攻击载荷：单参数，值内含 "&b=2"（修复前与合法请求产生相同规范串）
    let mut smuggled = HashMap::new();
    smuggled.insert("a".to_string(), "1&b=2".to_string());
    let smuggled_sig = signer.sign(&smuggled, secret);

    println!("[regress-H-1] legit_sig    = {legit_sig}");
    println!("[regress-H-1] smuggled_sig = {smuggled_sig}");
    assert_ne!(
        legit_sig, smuggled_sig,
        "两个语义不同的参数集必须产生不同签名（参数走私修复失效）"
    );
    assert!(
        !signer.verify(&smuggled, secret, &legit_sig),
        "走私载荷必须验证失败"
    );
    println!("[regress-H-1] ✅ 修复验证通过：参数走私被切断（percent-encoding 生效）");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（H-1 补充）：URL 编码不破坏正常签名（含特殊字符参数可正常验证）
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_hmac_signer_special_chars_roundtrip() {
    let signer = HmacSigner::new();
    let secret = "secret";
    let mut params = HashMap::new();
    params.insert("q".to_string(), "a b&c=d%".to_string());
    params.insert("user".to_string(), "张三".to_string());
    let sig = signer.sign(&params, secret);
    assert!(
        signer.verify(&params, secret, &sig),
        "合法含特殊字符参数必须签名-验证一致"
    );
    println!("[regress-H-1] ✅ 特殊字符/中文参数签名往返一致");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（M-8 修复验证）：PBKDF2 低/高迭代哈希均被拒绝
//
// 修复前（黑帽实证）：c=1 弱哈希被 verify 接受；`$4294967295$...` 可使
// 单次校验卡死数分钟（CPU DoS）。修复后：迭代次数强制 100_000 ~ 10_000_000。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_pbkdf2_iteration_bounds_enforced() {
    let hasher = Pbkdf2Hasher::new();

    // Python hashlib 官方向量 c=1：P="password", S="salt"
    let c1_hash = "$1$73616c74$120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b";
    // 低迭代必须被拒绝（而非接受）
    assert!(
        hasher.verify("password", c1_hash).is_err(),
        "c=1 弱哈希必须被拒绝（M-8 修复失效）"
    );

    // 高迭代 DoS 载荷必须被拒绝（且不执行计算）
    let dos_hash =
        "$4294967295$73616c74$120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b";
    assert!(
        hasher.verify("password", dos_hash).is_err(),
        "u32::MAX 迭代载荷必须被拒绝（CPU DoS 修复失效）"
    );

    // 正常路径：默认 100_000 次迭代 hash→verify 往返
    let hashed = hasher.hash("password").unwrap();
    assert!(hasher.verify("password", &hashed).unwrap());
    println!("[regress-M-8] ✅ 迭代上下限生效（c=1 / u32::MAX 均拒绝，正常往返通过）");
}
