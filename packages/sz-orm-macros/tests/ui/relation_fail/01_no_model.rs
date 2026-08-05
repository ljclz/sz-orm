//! #[derive(Relation)] 单独使用无法编译：ModelExt: Model 约束要求先 derive Entity

use sz_orm_macros::Relation;

#[derive(Relation)]
#[table(name = "users")]
#[relation(kind = "has_many", Post, fk = "user_id")]
struct UserNoModel {
    #[column(primary_key)]
    id: i64,
}

fn main() {
    let _ = UserNoModel::relations();
}
