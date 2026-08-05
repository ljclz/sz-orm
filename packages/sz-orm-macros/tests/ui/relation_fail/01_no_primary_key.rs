//! #[derive(Relation)] — 缺少 #[column(primary_key)] 应编译失败

use sz_orm_macros::{Entity, Relation};

#[derive(Entity, Relation)]
#[table(name = "users")]
#[relation(kind = "has_many", Post, fk = "user_id")]
struct UserNoPk {
    name: String,
}

fn main() {}
