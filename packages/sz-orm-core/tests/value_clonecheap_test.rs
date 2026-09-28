use std::collections::HashMap;

use sz_orm_core::Value;

#[test]
fn test_clonecheap_copy_variant() {
    let v = Value::I64(42);
    assert_eq!(v.clonecheap(), Value::I64(42));
    let b = Value::Bool(true);
    assert_eq!(b.clonecheap(), Value::Bool(true));
    assert_eq!(Value::Null.clonecheap(), Value::Null);
    let f = Value::F64(3.14);
    assert_eq!(f.clonecheap(), Value::F64(3.14));
}

#[test]
fn test_clonecheap_non_copy_variant() {
    let s = Value::String("hello".to_string());
    assert_eq!(s.clonecheap(), Value::String("hello".to_string()));
    let arr = Value::Array(vec![Value::I64(1), Value::I64(2)]);
    assert_eq!(
        arr.clonecheap(),
        Value::Array(vec![Value::I64(1), Value::I64(2)])
    );
    let bytes = Value::Bytes(vec![1, 2, 3]);
    assert_eq!(bytes.clonecheap(), Value::Bytes(vec![1, 2, 3]));
}

#[test]
fn test_clonecheap_object() {
    let map = HashMap::from([("k".to_string(), Value::I64(1))]);
    let obj = Value::Object(map);
    assert_eq!(obj.clonecheap(), obj);
}