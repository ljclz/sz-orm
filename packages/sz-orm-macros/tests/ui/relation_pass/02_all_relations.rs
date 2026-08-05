//! #[derive(Relation)] — 全部 6 种关系类型编译通过测试

use sz_orm_core::ModelExt;
use sz_orm_macros::{Entity, Relation};

#[derive(Entity, Relation)]
#[table(name = "users")]
#[relation(has_many = Posts, fk = "user_id")]
#[relation(belongs_to = Tenant, fk = "tenant_id")]
#[relation(has_one = Profile, fk = "user_id")]
#[relation(belongs_to_many = Tags, junction = "user_tags", fk = "user_id", other_key = "tag_id", target = "Tag")]
#[relation(morph_many = Comments, morph_type = "commentable_type", morph_id = "commentable_id", morph_type_value = "User")]
#[relation(morph_to = Image, morph_type = "imageable_type", morph_id = "imageable_id")]
struct User {
    #[column(primary_key)]
    id: i64,
    name: String,
    tenant_id: i64,
}

fn main() {
    let rels = User::relations();
    assert_eq!(rels.len(), 6, "expected 6 relations, got {}", rels.len());
    assert!(rels.contains_key("Posts"));
    assert!(rels.contains_key("Tenant"));
    assert!(rels.contains_key("Profile"));
    assert!(rels.contains_key("Tags"));
    assert!(rels.contains_key("Comments"));
    assert!(rels.contains_key("Image"));
}
