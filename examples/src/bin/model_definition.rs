//! Model 定义 — 如何实现 Model + ModelExt trait
//!
//! 演示完整的 Model 定义，包含主键、时间戳、软删除、列定义、批量赋值、JSON 序列化。
//!
//! 运行：`cargo run -p sz-orm-examples --bin model_definition`

use std::collections::HashMap;

use sz_orm_core::{BelongsTo, HasMany, Model, ModelExt, Relation, TimestampFields, Value};

#[derive(Debug, Clone, Default)]
struct Article {
    id: i64,
    title: String,
    content: String,
    author_id: i64,
    views: i64,
    deleted_at: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

impl Model for Article {
    type PrimaryKey = i64;

    fn table_name() -> &'static str {
        "articles"
    }

    fn pk_name() -> &'static str {
        "id"
    }

    fn pk(&self) -> Self::PrimaryKey {
        self.id
    }

    fn set_pk(&mut self, pk: Self::PrimaryKey) {
        self.id = pk;
    }

    fn foreign_key(relation: &str) -> String {
        match relation {
            "author" => "author_id".to_string(),
            _ => format!("{}_id", relation.to_lowercase()),
        }
    }

    fn timestamp_fields() -> Option<TimestampFields> {
        Some(TimestampFields::with_both("created_at", "updated_at"))
    }

    fn soft_delete_field() -> Option<&'static str> {
        Some("deleted_at")
    }
}

impl ModelExt for Article {
    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "title",
            "content",
            "author_id",
            "views",
            "deleted_at",
            "created_at",
            "updated_at",
        ]
    }

    fn fillable() -> Vec<&'static str> {
        // 允许批量赋值的字段（不含主键、时间戳、软删除）
        vec!["title", "content", "author_id", "views"]
    }

    fn guarded() -> Vec<&'static str> {
        vec!["id"]
    }

    fn hidden() -> Vec<&'static str> {
        // 序列化时隐藏
        vec!["deleted_at"]
    }

    fn relations() -> HashMap<&'static str, Relation> {
        let mut map = HashMap::new();
        map.insert(
            "author",
            Relation::BelongsTo(BelongsTo {
                foreign_key: "author_id".to_string(),
                parent_model: "users".to_string(),
                parent_pk: "id".to_string(),
            }),
        );
        map.insert(
            "comments",
            Relation::HasMany(HasMany {
                foreign_key: "article_id".to_string(),
                child_model: "comments".to_string(),
                child_pk: "id".to_string(),
            }),
        );
        map
    }

    fn get_column_value(&self, column: &str) -> Option<Value> {
        match column {
            "id" => Some(Value::I64(self.id)),
            "title" => Some(Value::String(self.title.clone())),
            "content" => Some(Value::String(self.content.clone())),
            "author_id" => Some(Value::I64(self.author_id)),
            "views" => Some(Value::I64(self.views)),
            "deleted_at" => self.deleted_at.clone().map(Value::String),
            "created_at" => self.created_at.clone().map(Value::String),
            "updated_at" => self.updated_at.clone().map(Value::String),
            _ => None,
        }
    }

    fn from_value(&mut self, map: HashMap<String, Value>) {
        for (k, v) in map {
            match k.as_str() {
                "id" => {
                    if let Some(i) = v.as_i64() {
                        self.id = i;
                    }
                }
                "title" => {
                    if let Some(s) = v.as_str() {
                        self.title = s.to_string();
                    }
                }
                "content" => {
                    if let Some(s) = v.as_str() {
                        self.content = s.to_string();
                    }
                }
                "author_id" => {
                    if let Some(i) = v.as_i64() {
                        self.author_id = i;
                    }
                }
                "views" => {
                    if let Some(i) = v.as_i64() {
                        self.views = i;
                    }
                }
                _ => {}
            }
        }
    }
}

fn main() {
    let mut article = Article {
        id: 42,
        title: "SZ-ORM 入门".to_string(),
        content: "这是一篇关于 SZ-ORM 的文章...".to_string(),
        author_id: 1,
        views: 100,
        deleted_at: None,
        created_at: Some("2026-07-19 10:00:00".to_string()),
        updated_at: Some("2026-07-19 10:00:00".to_string()),
    };

    println!("=== 模型基础信息 ===");
    println!("表名:     {}", Article::table_name());
    println!("主键列:   {}", Article::pk_name());
    println!("主键值:   {}", article.pk());
    println!("时间戳:   {:?}", Article::timestamp_fields());
    println!("软删除:   {:?}", Article::soft_delete_field());
    println!("列:       {:?}", Article::columns());
    println!("可填充:   {:?}", Article::fillable());
    println!("保护列:   {:?}", Article::guarded());
    println!("隐藏列:   {:?}", Article::hidden());
    println!(
        "关联:     {:?}",
        Article::relations().keys().collect::<Vec<_>>()
    );

    println!("\n=== 序列化为 JSON（to_json 自动隐藏 hidden 列）===");
    let json = article.to_json();
    println!("{}", serde_json::to_string_pretty(&json).unwrap());

    println!("\n=== 批量赋值（fill 会过滤 guarded，仅保留 fillable）===");
    let mut fill_data = HashMap::new();
    fill_data.insert("title".to_string(), Value::String("新标题".to_string()));
    fill_data.insert("views".to_string(), Value::I64(200));
    // 以下字段应被过滤掉
    fill_data.insert("id".to_string(), Value::I64(999));
    fill_data.insert("deleted_at".to_string(), Value::String("hack".to_string()));

    article.fill(fill_data);
    println!("id (应保持 42):       {}", article.id);
    println!("title (应为新标题):   {}", article.title);
    println!("views (应为 200):     {}", article.views);
    println!("deleted_at (应不变):  {:?}", article.deleted_at);

    println!("\n=== 设置主键 ===");
    article.set_pk(100);
    println!("新主键: {}", article.pk());
}
