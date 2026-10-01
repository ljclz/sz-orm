use sz_orm_core::Model;

struct DefaultModel {
    id: i64,
}
impl Model for DefaultModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "default_table"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

struct CustomModel {
    id: i64,
}
impl Model for CustomModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "custom_table"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
    fn pk_name() -> &'static str {
        "user_id"
    }
    fn soft_delete_field() -> Option<&'static str> {
        Some("deleted_at")
    }
    fn tenant_field() -> Option<&'static str> {
        Some("tenant_id")
    }
}

#[test]
fn test_model_trait_default_impl() {
    assert_eq!(DefaultModel::table_name(), "default_table");
    assert_eq!(DefaultModel::pk_name(), "id");
    assert!(DefaultModel::soft_delete_field().is_none());
    assert!(DefaultModel::timestamp_fields().is_none());
    assert!(DefaultModel::tenant_field().is_none());
    assert!(DefaultModel::fields().is_empty());
    let m = DefaultModel { id: 42 };
    assert_eq!(m.pk(), 42);
    assert_eq!(DefaultModel::foreign_key("Order"), "order_id");
    assert_eq!(DefaultModel::foreign_key("USER"), "user_id");
    let mut m2 = DefaultModel { id: 0 };
    m2.set_pk(99);
    assert_eq!(m2.pk(), 99);
    assert_eq!(m2.pk_as_value(), sz_orm_core::Value::Null);
}

#[test]
fn test_model_trait_custom_overrides() {
    assert_eq!(CustomModel::table_name(), "custom_table");
    assert_eq!(CustomModel::pk_name(), "user_id");
    assert_eq!(CustomModel::soft_delete_field(), Some("deleted_at"));
    assert_eq!(CustomModel::tenant_field(), Some("tenant_id"));
}
