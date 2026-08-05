//! ColumnEnum derive 测试（P2-2：自动生成列名枚举）
//!
//! `#[derive(ColumnEnum)]` 从结构体字段自动生成 `<StructName>Column` 枚举：
//! - 每个字段一个变体（snake_case → CamelCase）
//! - 变体通过 `as_str()` 返回数据库列名（支持 `#[column(name = "...")]` 覆盖）
//! - 实现 `ColumnTrait`（as_str / all）与 `Display`

use sz_orm_core::ColumnTrait;
use sz_orm_macros::ColumnEnum;

#[derive(ColumnEnum)]
#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
}

#[test]
fn test_column_enum_variants_as_str() {
    // 每个字段生成对应变体，as_str() 返回列名
    assert_eq!(UserColumn::Id.as_str(), "id");
    assert_eq!(UserColumn::Name.as_str(), "name");
}

#[test]
fn test_column_enum_display() {
    assert_eq!(UserColumn::Id.to_string(), "id");
    assert_eq!(UserColumn::Name.to_string(), "name");
}

#[test]
fn test_column_enum_all() {
    let all = UserColumn::all();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0], UserColumn::Id);
    assert_eq!(all[1], UserColumn::Name);
}

#[test]
fn test_column_enum_trait_object_safe_usage() {
    // as_str 可通过 trait 统一调用（类型安全列名引用）
    fn col_name(c: &dyn ColumnTrait) -> &'static str {
        c.as_str()
    }
    assert_eq!(col_name(&UserColumn::Id), "id");
}

#[derive(ColumnEnum, Debug, PartialEq, Clone, Copy)]
#[allow(dead_code)]
struct Order {
    id: i64,
    #[column(name = "user_id")]
    user_id: i64,
    #[column(name = "total_amount")]
    total: f64,
}

#[test]
fn test_column_enum_rename() {
    // #[column(name)] 覆盖：变体名取自字段名，列名取自覆盖值
    assert_eq!(OrderColumn::UserId.as_str(), "user_id");
    assert_eq!(OrderColumn::Total.as_str(), "total_amount");
    assert_eq!(OrderColumn::Id.as_str(), "id");
}

#[test]
fn test_column_enum_all_rename() {
    let names: Vec<&str> = OrderColumn::all().iter().map(|c| c.as_str()).collect();
    assert_eq!(names, vec!["id", "user_id", "total_amount"]);
}

#[test]
fn test_column_enum_debug_eq() {
    // 生成标准派生：Debug/PartialEq/Clone/Copy 可用
    assert_eq!(UserColumn::Id, UserColumn::Id);
    assert_ne!(UserColumn::Id, UserColumn::Name);
    let _ = format!("{:?}", UserColumn::Id);
}
