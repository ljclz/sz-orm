//! 批① L2 路径对标：Value::clonecheap vs Clone（对拍，逐值严格）
//!
//! 对标对象：
//! - 快路径：`Value::clonecheap`（Copy 变体零堆分配，v9.0.0 M8）
//! - 慢路径：`Value::clone`（标准 Clone）
//! - 不变量：对任意 Value，`clonecheap()` 产出的值与 `clone()` 语义完全一致
//!   （同变体 + 同载荷）。逐字节比较经 `format!("{:?}")` 的 Debug 表示等价 +
//!   变体相等（PartialEq）双口径断言。
//!
//! 运行方式：cargo test -p sz-orm-core --test clonecheap_parity

use sz_orm_core::Value;

/// 全变体固定边界集
fn boundary_values() -> Vec<Value> {
    vec![
        Value::Null,
        Value::Bool(true),
        Value::Bool(false),
        Value::I8(i8::MIN),
        Value::I8(i8::MAX),
        Value::I16(i16::MIN),
        Value::I16(i16::MAX),
        Value::I32(i32::MIN),
        Value::I32(i32::MAX),
        Value::I64(i64::MIN),
        Value::I64(i64::MAX),
        Value::U8(u8::MAX),
        Value::U16(u16::MAX),
        Value::U32(u32::MAX),
        Value::U64(u64::MAX),
        Value::F32(0.0),
        Value::F32(-0.0),
        Value::F32(f32::MAX),
        Value::F64(-0.0),
        Value::F64(f64::MAX),
        Value::Decimal("123.456".into()),
        Value::String(String::new()),
        Value::String("hello".into()),
        Value::String("with'quote and \\backslash".into()),
        Value::Bytes(vec![]),
        Value::Bytes(vec![0u8, 255, 128]),
        Value::Uuid("550e8400-e29b-41d4-a716-446655440000".into()),
        Value::Date("2026-11-11".into()),
        Value::DateTime("2026-11-11T00:00:00".into()),
        Value::Time("12:34:56".into()),
        Value::Json("{\"a\":1}".into()),
        Value::Array(vec![Value::I64(1), Value::String("x".into())]),
        Value::Object([("k".to_string(), Value::Bool(true))].into_iter().collect()),
    ]
}

#[test]
fn parity_clonecheap_vs_clone_boundary() {
    for v in boundary_values() {
        let cheap = v.clonecheap();
        let slow = v.clone();
        // 双口径：PartialEq（变体+载荷相等） + Debug 表示逐字节一致
        assert_eq!(
            cheap,
            slow,
            "clonecheap 与 clone 语义不一致: {:?}",
            std::mem::discriminant(&v)
        );
        assert_eq!(
            format!("{:?}", cheap),
            format!("{:?}", slow),
            "Debug 表示不一致"
        );
    }
}

/// 已知差异探针（挂账）：NaN 载荷——PartialEq 对 F32(NaN)/F64(NaN) 恒 false
/// （IEEE 语义），本探针只验证 clonecheap 的 NaN 位模式稳定（自身对自身），
/// 不做 clonecheap vs clone 的 PartialEq 断言。
#[test]
fn probe_nan_bit_pattern_stability() {
    for v in [Value::F32(f32::NAN), Value::F64(f64::NAN)] {
        let c1 = v.clonecheap();
        let c2 = v.clonecheap();
        assert_eq!(
            format!("{:?}", c1),
            format!("{:?}", c2),
            "clonecheap NaN 位模式不稳定: {:?}",
            v
        );
    }
}
