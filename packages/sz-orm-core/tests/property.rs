//! Property-Based Testing — 对应 sz-orm 项目成熟度评估报告 §3.7 验证体系
//!
//! 使用 proptest 对核心数据类型（Value）进行属性测试，
//! 自动生成大量随机输入验证不变量（invariant），弥补单元测试的边界盲区。
//!
//! # 测试的属性
//!
//! - **整数往返**：`Value::from(v).as_i64()` 保持原值
//! - **浮点往返**：`Value::from(v).as_f64()` 保持原值
//! - **布尔往返**：`Value::Bool(b).as_bool()` 保持原值
//! - **字符串往返**：`Value::String(s).as_str()` 保持原值
//! - **SQL 注入防御**：`to_param()` 结果中 `'` 成对出现（无法突破字面量）
//! - **U64 边界**：超过 `i64::MAX` 的 U64 转换为 i64 时返回 None（不静默截断）
//!
//! # 与单元测试的区别
//!
//! 单元测试是"已知输入 → 期望输出"，属性测试是"任意满足前置条件的输入 → 不变量必成立"。
//! proptest 在属性不成立时会自动 shrinking，找到最小反例。

use proptest::prelude::*;
use sz_orm_core::Value;

proptest! {
    /// 属性：I8 → as_i64 往返保持原值
    #[test]
    fn prop_i8_to_i64_roundtrip(v: i8) {
        prop_assert_eq!(Value::I8(v).as_i64(), Some(v as i64));
    }

    /// 属性：I16 → as_i64 往返保持原值
    #[test]
    fn prop_i16_to_i64_roundtrip(v: i16) {
        prop_assert_eq!(Value::I16(v).as_i64(), Some(v as i64));
    }

    /// 属性：I32 → as_i64 往返保持原值
    #[test]
    fn prop_i32_to_i64_roundtrip(v: i32) {
        prop_assert_eq!(Value::I32(v).as_i64(), Some(v as i64));
    }

    /// 属性：I64 → as_i64 往返保持原值（完整 i64 范围）
    #[test]
    fn prop_i64_to_i64_roundtrip(v: i64) {
        prop_assert_eq!(Value::I64(v).as_i64(), Some(v));
    }

    /// 属性：U8 → as_i64 往返保持原值
    #[test]
    fn prop_u8_to_i64_roundtrip(v: u8) {
        prop_assert_eq!(Value::U8(v).as_i64(), Some(v as i64));
    }

    /// 属性：U16 → as_i64 往返保持原值
    #[test]
    fn prop_u16_to_i64_roundtrip(v: u16) {
        prop_assert_eq!(Value::U16(v).as_i64(), Some(v as i64));
    }

    /// 属性：U32 → as_i64 往返保持原值
    #[test]
    fn prop_u32_to_i64_roundtrip(v: u32) {
        prop_assert_eq!(Value::U32(v).as_i64(), Some(v as i64));
    }

    /// 属性：U64 → as_i64 边界正确
    /// - v <= i64::MAX 时，as_i64() == Some(v as i64)
    /// - v > i64::MAX 时，as_i64() == None（不静默截断为负数）
    #[test]
    fn prop_u64_to_i64_boundary(v: u64) {
        let expected = i64::try_from(v).ok();
        prop_assert_eq!(Value::U64(v).as_i64(), expected);
        if v > i64::MAX as u64 {
            prop_assert_eq!(Value::U64(v).as_i64(), None,
                "U64 超过 i64::MAX 时必须返回 None，不静默截断");
        }
    }

    /// 属性：F32 → as_f64 往返保持原值（f32 → f64 是无损扩展）
    #[test]
    fn prop_f32_to_f64_roundtrip(v: f32) {
        // 排除 NaN/Inf，因为 to_param/to_string 不支持
        prop_assume!(v.is_finite());
        prop_assert_eq!(Value::F32(v).as_f64(), Some(v as f64));
    }

    /// 属性：F64 → as_f64 往返保持原值
    #[test]
    fn prop_f64_to_f64_roundtrip(v: f64) {
        prop_assume!(v.is_finite());
        prop_assert_eq!(Value::F64(v).as_f64(), Some(v));
    }

    /// 属性：Bool → as_bool 往返保持原值
    #[test]
    fn prop_bool_roundtrip(b: bool) {
        prop_assert_eq!(Value::Bool(b).as_bool(), Some(b));
    }

    /// 属性：String → as_str 往返保持原值
    #[test]
    fn prop_string_as_str_roundtrip(s: String) {
        let v = Value::String(s.clone());
        prop_assert_eq!(v.as_str(), Some(s.as_str()));
    }

    /// 属性：String → to_param 的 SQL 注入防御
    ///
    /// `to_param()` 返回 `'...'` 格式的 SQL 字符串字面量，
    /// 其中所有 `'` 都被转义为 `''`。因此：
    ///
    /// 1. 结果必须以 `'` 开头
    /// 2. 结果必须以 `'` 结尾
    /// 3. 去掉首尾 `'` 后，剩余部分中 `'` 的数量必须是偶数
    ///    （每个原始 `'` 变成 `''`，所以数量翻倍）
    ///
    /// 这保证了攻击者无法通过包含 `'` 的输入突破字符串字面量。
    #[test]
    fn prop_to_param_sql_injection_defense(s: String) {
        // to_param() 返回 Cow<str>，用 into_owned() 避免 temporary value 生命周期问题
        let param = Value::String(s).to_param().into_owned();
        // 属性 1：以 ' 开头
        prop_assert!(
            param.starts_with('\''),
            "to_param 必须以 ' 开头，实际: {:?}",
            param
        );
        // 属性 2：以 ' 结尾
        prop_assert!(
            param.ends_with('\''),
            "to_param 必须以 ' 结尾，实际: {:?}",
            param
        );
        // 属性 3：去掉首尾 ' 后，' 数量为偶数
        let inner = &param[1..param.len() - 1];
        let quote_count = inner.chars().filter(|&c| c == '\'').count();
        prop_assert_eq!(
            quote_count % 2,
            0,
            "to_param 内部 ' 数量必须为偶数（每个 ' 被转义为 ''），实际 {} 个",
            quote_count
        );
    }

    /// 属性：整数 → as_f64 往返保持原值（i64 → f64 在 2^53 内无损）
    ///
    /// 限制 v 在 |v| < 2^53 范围内（f64 尾数 52 位 + 符号位），
    /// 此范围内 i64 → f64 转换无损。超出此范围会有精度损失（非 bug，是 IEEE 754 限制）。
    #[test]
    fn prop_i64_to_f64_roundtrip_within_safe_range(
        v in -(1i64 << 53)..(1i64 << 53)
    ) {
        prop_assert_eq!(Value::I64(v).as_f64(), Some(v as f64));
    }

    /// 属性：Value::from 往返 — 任意 i64 通过 from 创建后 as_i64 还原
    #[test]
    fn prop_value_from_i64_roundtrip(v: i64) {
        let value: Value = v.into();
        prop_assert_eq!(value.as_i64(), Some(v));
    }

    /// 属性：Value::from(String) 往返
    #[test]
    fn prop_value_from_string_roundtrip(s: String) {
        let value: Value = s.clone().into();
        prop_assert_eq!(value.as_str(), Some(s.as_str()));
    }
}
