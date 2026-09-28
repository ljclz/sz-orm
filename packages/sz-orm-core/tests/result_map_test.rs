use sz_orm_core::result_map::*;
use sz_orm_core::Value;
use std::collections::HashMap;

#[test]
fn test_mapping_new() {
    let m = Mapping::new("id", "user_id");
    assert_eq!(m.property, "id");
    assert_eq!(m.column, "user_id");
    assert!(m.type_handler.is_none());
}

#[test]
fn test_mapping_with_handler() {
    let m = Mapping::with_handler("id", "user_id", "my_handler");
    assert_eq!(m.type_handler, Some("my_handler".to_string()));
}

#[test]
fn test_nested_association_new() {
    let a = NestedAssociation::new("dept", "deptMap");
    assert_eq!(a.property, "dept");
    assert_eq!(a.result_map, "deptMap");
    assert!(a.column_prefix.is_none());
    assert!(a.not_null_column.is_none());
}

#[test]
fn test_nested_association_with_prefix() {
    let a = NestedAssociation::new("dept", "deptMap").with_prefix("dept_");
    assert_eq!(a.column_prefix, Some("dept_".to_string()));
}

#[test]
fn test_nested_association_with_not_null_column() {
    let a = NestedAssociation::new("dept", "deptMap").with_not_null_column("dept_id");
    assert_eq!(a.not_null_column, Some("dept_id".to_string()));
}

#[test]
fn test_nested_collection_new() {
    let c = NestedCollection::new("items", "itemMap");
    assert_eq!(c.property, "items");
    assert_eq!(c.result_map, "itemMap");
}

#[test]
fn test_nested_collection_with_prefix() {
    let c = NestedCollection::new("items", "itemMap").with_prefix("item_");
    assert_eq!(c.column_prefix, Some("item_".to_string()));
}

#[test]
fn test_nested_collection_with_not_null_column() {
    let c = NestedCollection::new("items", "itemMap").with_not_null_column("item_id");
    assert_eq!(c.not_null_column, Some("item_id".to_string()));
}

#[test]
fn test_discriminator_new() {
    let d = Discriminator::new("type");
    assert_eq!(d.column, "type");
    assert!(d.cases.is_empty());
}

#[test]
fn test_discriminator_add_case() {
    let mut d = Discriminator::new("type");
    d.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"));
    d.add_case(DiscriminatorCase::new(Value::I64(2), "userMap"));
    assert_eq!(d.cases.len(), 2);
}

#[test]
fn test_discriminator_resolve() {
    let mut d = Discriminator::new("type");
    d.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"));
    d.add_case(DiscriminatorCase::new(Value::I64(2), "userMap"));
    assert_eq!(d.resolve(&Value::I64(1)), Some("adminMap"));
    assert_eq!(d.resolve(&Value::I64(2)), Some("userMap"));
    assert_eq!(d.resolve(&Value::I64(3)), None);
}

#[test]
fn test_result_map_new() {
    let rm = ResultMap::new("userMap", "User");
    assert_eq!(rm.id, "userMap");
    assert_eq!(rm.type_name, "User");
    assert!(rm.id_mappings.is_empty());
    assert!(rm.result_mappings.is_empty());
}

#[test]
fn test_result_map_add_mappings() {
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_id_mapping(Mapping::new("id", "user_id"));
    rm.add_result_mapping(Mapping::new("name", "user_name"));
    assert_eq!(rm.id_mappings.len(), 1);
    assert_eq!(rm.result_mappings.len(), 1);
}

#[test]
fn test_result_map_add_association() {
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_association(NestedAssociation::new("dept", "deptMap"));
    assert_eq!(rm.associations.len(), 1);
}

#[test]
fn test_result_map_add_collection() {
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_collection(NestedCollection::new("items", "itemMap"));
    assert_eq!(rm.collections.len(), 1);
}

#[test]
fn test_result_map_set_discriminator() {
    let mut rm = ResultMap::new("userMap", "User");
    rm.set_discriminator(Discriminator::new("type"));
    assert!(rm.discriminator.is_some());
}

#[test]
fn test_result_map_sub_map_ids() {
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_association(NestedAssociation::new("dept", "deptMap"));
    rm.add_collection(NestedCollection::new("items", "itemMap"));
    let ids = rm.sub_map_ids();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"deptMap".to_string()));
    assert!(ids.contains(&"itemMap".to_string()));
}

#[test]
fn test_result_map_sub_map_ids_with_discriminator() {
    let mut rm = ResultMap::new("userMap", "User");
    let mut disc = Discriminator::new("type");
    disc.add_case(DiscriminatorCase::new(Value::I64(1), "adminMap"));
    rm.set_discriminator(disc);
    let ids = rm.sub_map_ids();
    assert!(ids.contains(&"adminMap".to_string()));
}

#[test]
fn test_registry_new() {
    let reg = ResultMapRegistry::new();
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
}

#[test]
fn test_registry_register_and_get() {
    let reg = ResultMapRegistry::new();
    reg.register(ResultMap::new("userMap", "User"));
    assert!(reg.contains("userMap"));
    assert_eq!(reg.len(), 1);
    let rm = reg.get("userMap").unwrap();
    assert_eq!(rm.type_name, "User");
}

#[test]
fn test_registry_get_nonexistent() {
    let reg = ResultMapRegistry::new();
    assert!(reg.get("nonexistent").is_none());
}

#[test]
fn test_registry_list_ids() {
    let reg = ResultMapRegistry::new();
    reg.register(ResultMap::new("a", "A"));
    reg.register(ResultMap::new("b", "B"));
    let ids = reg.list_ids();
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_registry_clear() {
    let reg = ResultMapRegistry::new();
    reg.register(ResultMap::new("a", "A"));
    reg.clear();
    assert!(reg.is_empty());
}

#[test]
fn test_row_data_new() {
    let mut cols = HashMap::new();
    cols.insert("id".to_string(), Value::I64(1));
    let row = RowData::new(cols);
    assert_eq!(row.get("id"), Some(&Value::I64(1)));
}

#[test]
fn test_row_data_empty() {
    let row = RowData::empty();
    assert!(row.is_empty());
    assert_eq!(row.len(), 0);
}

#[test]
fn test_row_data_set() {
    let mut row = RowData::empty();
    row.set("name", Value::String("Alice".to_string()));
    assert_eq!(row.get("name"), Some(&Value::String("Alice".to_string())));
}

#[test]
fn test_row_data_get_with_prefix() {
    let mut row = RowData::empty();
    row.set("dept_id", Value::I64(10));
    assert_eq!(row.get_with_prefix("dept_", "id"), Some(&Value::I64(10)));
}

#[test]
fn test_row_data_is_not_null() {
    let mut row = RowData::empty();
    row.set("a", Value::I64(1));
    row.set("b", Value::Null);
    assert!(row.is_not_null("a"));
    assert!(!row.is_not_null("b"));
    assert!(!row.is_not_null("c"));
}

#[test]
fn test_row_data_column_names() {
    let mut row = RowData::empty();
    row.set("a", Value::I64(1));
    row.set("b", Value::I64(2));
    let names = row.column_names();
    assert_eq!(names.len(), 2);
}

#[test]
fn test_row_data_sorted_columns() {
    let mut row = RowData::empty();
    row.set("b", Value::I64(2));
    row.set("a", Value::I64(1));
    let sorted = row.sorted_columns();
    assert_eq!(sorted[0].0, "a");
    assert_eq!(sorted[1].0, "b");
}

#[test]
fn test_row_data_iter() {
    let mut row = RowData::empty();
    row.set("a", Value::I64(1));
    row.set("b", Value::I64(2));
    let count = row.iter().count();
    assert_eq!(count, 2);
}

#[test]
fn test_apply_result_map_simple() {
    let reg = ResultMapRegistry::new();
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_id_mapping(Mapping::new("id", "user_id"));
    rm.add_result_mapping(Mapping::new("name", "user_name"));
    reg.register(rm);

    let mut cols = HashMap::new();
    cols.insert("user_id".to_string(), Value::I64(1));
    cols.insert("user_name".to_string(), Value::String("Alice".to_string()));
    let row = RowData::new(cols);

    let result = apply_result_map(&reg, "userMap", &row).unwrap();
    assert_eq!(result.get("id"), Some(&Value::I64(1)));
    assert_eq!(result.get("name"), Some(&Value::String("Alice".to_string())));
}

#[test]
fn test_apply_result_map_not_found() {
    let reg = ResultMapRegistry::new();
    let row = RowData::empty();
    let result = apply_result_map(&reg, "nonexistent", &row);
    assert!(result.is_err());
}

#[test]
fn test_apply_result_map_with_association() {
    let reg = ResultMapRegistry::new();

    let mut dept_map = ResultMap::new("deptMap", "Dept");
    dept_map.add_id_mapping(Mapping::new("id", "dept_id"));
    dept_map.add_result_mapping(Mapping::new("name", "dept_name"));
    reg.register(dept_map);

    let mut user_map = ResultMap::new("userMap", "User");
    user_map.add_id_mapping(Mapping::new("id", "user_id"));
    user_map.add_result_mapping(Mapping::new("name", "user_name"));
    user_map.add_association(NestedAssociation::new("dept", "deptMap"));
    reg.register(user_map);

    let mut cols = HashMap::new();
    cols.insert("user_id".to_string(), Value::I64(1));
    cols.insert("user_name".to_string(), Value::String("Alice".to_string()));
    cols.insert("dept_id".to_string(), Value::I64(10));
    cols.insert("dept_name".to_string(), Value::String("Engineering".to_string()));
    let row = RowData::new(cols);

    let result = apply_result_map(&reg, "userMap", &row).unwrap();
    assert_eq!(result.get("id"), Some(&Value::I64(1)));
}

#[test]
fn test_apply_result_map_null_column() {
    let reg = ResultMapRegistry::new();
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_id_mapping(Mapping::new("id", "user_id"));
    rm.add_result_mapping(Mapping::new("name", "user_name"));
    reg.register(rm);

    let mut cols = HashMap::new();
    cols.insert("user_id".to_string(), Value::I64(1));
    cols.insert("user_name".to_string(), Value::Null);
    let row = RowData::new(cols);

    let result = apply_result_map(&reg, "userMap", &row).unwrap();
    assert_eq!(result.get("id"), Some(&Value::I64(1)));
    assert_eq!(result.get("name"), Some(&Value::Null));
}

#[test]
fn test_apply_result_map_many_empty() {
    let reg = ResultMapRegistry::new();
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_id_mapping(Mapping::new("id", "user_id"));
    reg.register(rm);

    let rows: Vec<RowData> = vec![];
    let result = apply_result_map_many(&reg, "userMap", &rows).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_apply_result_map_many() {
    let reg = ResultMapRegistry::new();
    let mut rm = ResultMap::new("userMap", "User");
    rm.add_id_mapping(Mapping::new("id", "user_id"));
    rm.add_result_mapping(Mapping::new("name", "user_name"));
    reg.register(rm);

    let mut cols1 = HashMap::new();
    cols1.insert("user_id".to_string(), Value::I64(1));
    cols1.insert("user_name".to_string(), Value::String("Alice".to_string()));
    let mut cols2 = HashMap::new();
    cols2.insert("user_id".to_string(), Value::I64(2));
    cols2.insert("user_name".to_string(), Value::String("Bob".to_string()));
    let rows = vec![RowData::new(cols1), RowData::new(cols2)];

    let result = apply_result_map_many(&reg, "userMap", &rows).unwrap();
    assert_eq!(result.len(), 2);
}