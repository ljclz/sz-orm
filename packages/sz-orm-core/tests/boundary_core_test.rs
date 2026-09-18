//! v7.4.0 任务 4.1：BorrowedValue/SimdAvailability 边界测试

use sz_orm_core::value_borrowed::BorrowedValue;
use sz_orm_core::simd::SimdAvailability;

#[test]
fn test_borrowed_value_decimal_bytes() {
    let data: &[u8] = b"123.45";
    let v = BorrowedValue::DecimalBytes(data);
    assert_eq!(v.as_bytes(), Some(data));
    let owned = v.to_owned_value();
    assert!(matches!(owned, sz_orm_core::Value::Decimal(_)));
}

#[test]
fn test_borrowed_value_json_bytes() {
    let data: &[u8] = b"{\"key\":1}";
    let v = BorrowedValue::JsonBytes(data);
    assert_eq!(v.as_bytes(), Some(data));
    let owned = v.to_owned_value();
    assert!(matches!(owned, sz_orm_core::Value::Json(_)));
}

#[test]
fn test_borrowed_value_bytes_ref() {
    let data: &[u8] = &[1, 2, 3, 4];
    let v = BorrowedValue::BytesRef(data);
    assert_eq!(v.as_bytes(), Some(data));
    let owned = v.to_owned_value();
    assert!(matches!(owned, sz_orm_core::Value::Bytes(_)));
}

#[test]
fn test_borrowed_value_datetime_int() {
    let v = BorrowedValue::DateTimeInt(1700000000);
    assert_eq!(v.as_bytes(), None);
    let owned = v.to_owned_value();
    assert!(matches!(owned, sz_orm_core::Value::DateTime(_)));
}

#[test]
fn test_borrowed_value_null_default() {
    let v: BorrowedValue<'_> = BorrowedValue::default();
    assert!(matches!(v, BorrowedValue::Null));
}

#[test]
fn test_borrowed_value_display_new_variants() {
    let data: &[u8] = b"123";
    assert_eq!(
        format!("{}", BorrowedValue::DecimalBytes(data)),
        format!("{:?}", data)
    );
    assert_eq!(
        format!("{}", BorrowedValue::JsonBytes(data)),
        format!("{:?}", data)
    );
    assert_eq!(
        format!("{}", BorrowedValue::BytesRef(data)),
        format!("{:?}", data)
    );
    assert_eq!(format!("{}", BorrowedValue::DateTimeInt(42)), "42");
}

#[test]
fn test_simd_availability_none_is_available() {
    let avail = SimdAvailability::None;
    assert!(!avail.is_available());
}

#[test]
fn test_simd_availability_avx2_is_available() {
    let avail = SimdAvailability::Avx2;
    assert!(avail.is_available());
}

#[test]
fn test_simd_availability_sse2_is_available() {
    let avail = SimdAvailability::Sse2;
    assert!(avail.is_available());
}