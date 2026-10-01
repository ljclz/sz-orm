//! v9.2.0 M18: result_map.rs 未覆盖纯逻辑方法（从内联测试迁出）

use sz_orm_core::result_map::{
    NestedCollection, ResultMap, ResultMapRegistry, ResultSetMappingRegistry, RowData,
};
use sz_orm_core::Value;

#[test]
fn test_nested_collection_with_not_null_column() {
    let nc = NestedCollection::new("roles", "role_map").with_not_null_column("role_id");
    assert_eq!(nc.not_null_column.as_deref(), Some("role_id"));
}

#[test]
fn test_result_map_registry_is_empty() {
    let registry = ResultMapRegistry::new();
    assert!(registry.is_empty());
    registry.register(ResultMap::new("test", "TestType"));
    assert!(!registry.is_empty());
}

#[test]
fn test_row_data_is_empty() {
    let row = RowData::empty();
    assert!(row.is_empty());
    let mut row2 = RowData::empty();
    row2.set("id", Value::I64(1));
    assert!(!row2.is_empty());
}

#[test]
fn test_row_data_sorted_columns() {
    let mut row = RowData::empty();
    row.set("c", Value::I64(3));
    row.set("a", Value::I64(1));
    row.set("b", Value::I64(2));
    let sorted = row.sorted_columns();
    assert_eq!(sorted.len(), 3);
    assert_eq!(sorted[0].0, "a");
    assert_eq!(sorted[1].0, "b");
    assert_eq!(sorted[2].0, "c");
}

#[test]
fn test_row_data_iter() {
    let mut row = RowData::empty();
    row.set("x", Value::I64(10));
    row.set("y", Value::I64(20));
    let entries: Vec<(&String, &Value)> = row.iter().collect();
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_result_set_mapping_registry_is_empty() {
    let registry = ResultSetMappingRegistry::new();
    assert!(registry.is_empty());
}