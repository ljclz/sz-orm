use sz_orm_core::Model;

struct TestModel {
    id: i64,
}
impl Model for TestModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "test"
    }
    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }
    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }
}

#[test]
fn test_model_validate_foreign_key_lowercase() {
    assert_eq!(TestModel::foreign_key("Order"), "order_id");
    assert_eq!(TestModel::foreign_key("USER"), "user_id");
    assert_eq!(TestModel::foreign_key("Team"), "team_id");
}

#[test]
fn test_model_validate_foreign_key_preserved() {
    assert_eq!(TestModel::foreign_key("order"), "order_id");
    assert_eq!(TestModel::foreign_key("user"), "user_id");
    assert_eq!(TestModel::foreign_key("a_b_c"), "a_b_c_id");
}