//! 黑帽审计 PoC（攻击者视角）——2026-08-14
//!
//! 对应白帽报告：L-6（TopicFilter From 静默降级）、L-7（$ 前缀主题通配）。

use sz_orm_mqtt::topics::TopicFilter;

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（L-6 修复验证）：非法 TopicFilter 不再静默降级
//
// 修复前（黑帽实证）：`TopicFilter::from("a/#/b")` 静默保留非法 pattern，
// 且 matches 遇到 '#' 即返回 true → 匹配整个 a/ 命名空间（越权订阅）。
// 修复后：From 对非法过滤器 fail-fast panic。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_invalid_topic_filter_no_longer_silent() {
    // new() 仍正确拒绝
    assert!(TopicFilter::new("a/#/b").is_err());

    // From 不再静默降级——非法过滤器直接 panic（fail-fast）
    let result = std::panic::catch_unwind(|| -> TopicFilter { "a/#/b".into() });
    assert!(
        result.is_err(),
        "非法过滤器经 From 必须 panic（L-6 修复失效）"
    );
    println!("[regress-L-6] ✅ 非法过滤器 From 转换 fail-fast（静默降级已消除）");

    // 合法过滤器仍正常
    let valid: TopicFilter = "a/+/b".into();
    assert!(valid.matches("a/x/b"));
    assert!(!valid.matches("a/x/y"));
    println!("[regress-L-6] ✅ 合法过滤器不受影响");
}

// ═══════════════════════════════════════════════════════════════════════════
// 回归测试（L-7 修复验证）：$ 前缀系统主题不再被通配符匹配
//
// 修复前（黑帽实证）：`#` 与 `+/broker/#` 可匹配 $SYS 系统主题（违反
// MQTT 3.1.1 §4.7.2，普通订阅者可读取系统级主题）。
// 修复后：首级通配符不得匹配 $ 前缀主题；字面 $SYS 过滤器仍可用。
// ═══════════════════════════════════════════════════════════════════════════
#[test]
fn regress_wildcard_no_longer_matches_dollar_topics() {
    let all: TopicFilter = "#".into();
    let plus: TopicFilter = "+/broker/#".into();

    // 通配符不得匹配 $SYS
    assert!(
        !all.matches("$SYS/broker/uptime"),
        "`#` 不得匹配 $SYS 主题（L-7 修复失效）"
    );
    assert!(
        !plus.matches("$SYS/broker/uptime"),
        "`+` 不得匹配 $SYS 主题（L-7 修复失效）"
    );

    // 字面 $ 前缀过滤器仍可订阅系统主题（管理端合法用途）
    let sys: TopicFilter = "$SYS/broker/#".into();
    assert!(
        sys.matches("$SYS/broker/uptime"),
        "字面 $SYS 过滤器必须仍能匹配"
    );

    // 普通主题不受影响
    assert!(all.matches("home/temp"));
    println!("[regress-L-7] ✅ $ 前缀系统主题已隔离（字面 $SYS 订阅保留）");
}
