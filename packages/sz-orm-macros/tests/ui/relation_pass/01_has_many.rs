//! #[derive(Relation)] — has_many 关系编译通过测试

use sz_orm_core::ModelExt;
use sz_orm_macros::{Entity, Relation};

#[derive(Entity, Relation)]
#[table(name = "users")]
struct User {
    #[column(primary_key)]
    id: i64,
    name: String,
}

#[derive(Entity, Relation)]
#[table(name = "posts")]
#[relation(has_many = Post, fk = "user_id")]
struct UserWithPosts {
    #[column(primary_key)]
    id: i64,
    name: String,
}

fn main() {
    // 验证 columns()/fillable() 由 Relation 宏生成
    let _cols = UserWithPosts::columns();
    let _fill = UserWithPosts::fillable();
    // 验证 relations() 返回含 Post 键的 HashMap
    let rels = UserWithPosts::relations();
    assert!(rels.contains_key("Post"), "expected 'Post' in relations, got: {:?}", rels.keys().collect::<Vec<_>>());
}
