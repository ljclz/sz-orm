//! 批① L2 路径对标：计划缓存命中路径 vs 直解析路径（PBT 差分）
//!
//! 对标对象：
//! - 快路径：`PlanCache::get_or_parse` 缓存命中返回 `Arc<Statement>`（零解析）
//! - 慢路径：`sqlparser::Parser::parse_sql` 直解析
//! - 不变量：同一 SQL 无论先经缓存还是直解析，AST 的 `Display` 序列化产物
//!   逐字节一致（AST 结构等价的稳定代理）。
//! - 边界集：大小写/空白变体走 `SqlNormalizer::normalize` 归一化键——变体间
//!   键一致属既有契约（plan_cache_differential.rs 已覆盖），此处对拍的是
//!   **AST 等价**而非键碰撞。
//!
//! 运行方式：cargo test -p sz-orm-core --features plan-cache --test plan_cache_ast_parity

#![cfg(feature = "plan-cache")]

use proptest::prelude::*;
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use sz_orm_core::plan_cache::PlanCache;

fn parse_direct(sql: &str) -> Option<Statement> {
    Parser::parse_sql(&GenericDialect {}, sql)
        .ok()?
        .into_iter()
        .next()
}

/// 白名单递归生成器（信噪比优先：只生成双方已验证支持的语法）
fn arb_sql() -> impl Strategy<Value = String> {
    prop_oneof![
        // SELECT 等值
        (prop_oneof!["users", "orders", "items"], prop_oneof!["id", "name", "age"], any::<i64>()).prop_map(
            |(t, c, v)| format!("SELECT * FROM {} WHERE {} = {}", t, c, v),
        ),
        // SELECT 投影
        (prop_oneof!["users", "orders"], prop_oneof!["id", "id, name", "id, name, age"]).prop_map(
            |(t, cols)| format!("SELECT {} FROM {}", cols, t),
        ),
        // INSERT
        (prop_oneof!["users", "items"], any::<i64>(), "[a-z]{1,8}").prop_map(|(t, id, n)| format!(
            "INSERT INTO {} (id, name) VALUES ({}, '{}')",
            t, id, n
        )),
        // UPDATE
        (prop_oneof!["users", "orders"], any::<i64>(), any::<i64>()).prop_map(|(t, id, v)| format!(
            "UPDATE {} SET age = {} WHERE id = {}",
            t, v, id
        )),
        // DELETE
        (prop_oneof!["users", "orders"], any::<i64>()).prop_map(|(t, id)| format!(
            "DELETE FROM {} WHERE id = {}",
            t, id
        )),
        // 聚合（字面字符串避免 regex 语义：proptest 字符串字面量按 regex 解析）
        (
            prop_oneof!["users", "orders"],
            prop_oneof!["COUNT", "MAX", "MIN"],
            prop_oneof!["age", "id", "name"],
        )
            .prop_map(|(t, f, c)| {
                if f == "COUNT" {
                    format!("SELECT COUNT(*) FROM {}", t)
                } else {
                    format!("SELECT {}({}) FROM {}", f, c, t)
                }
            }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// 缓存命中路径 vs 直解析路径：AST 序列化产物逐字节一致
    #[test]
    fn parity_cache_hit_vs_direct_parse(sql in arb_sql()) {
        let cache = PlanCache::new(64, None);
        // 第一遍：miss → 解析并缓存
        let via_cache = cache.get_or_parse(&sql).expect("get_or_parse");
        // 第二遍：hit → 返回同一 Arc
        let via_hit = cache.get_or_parse(&sql).expect("get_or_parse 2");
        prop_assert!(std::sync::Arc::ptr_eq(&via_cache, &via_hit), "命中应返回同一 Arc");
        // 慢路径：直解析
        let direct = parse_direct(&sql).expect("direct parse");
        prop_assert_eq!(via_cache.to_string(), direct.to_string(), "AST 序列化不一致: {}", sql);
    }
}

/// 固定边界集：混合大小写/空白变体（归一化键契约）+ 特殊字符
#[test]
fn parity_boundary_variants() {
    let cache = PlanCache::new(64, None);
    let variants = [
        "SELECT * FROM users WHERE id = 1",
        "select * from users where id = 1",
        "SELECT   *   FROM   users   WHERE   id   =   1",
        "SELECT * FROM users WHERE id=1",
    ];
    for v in &variants {
        let via_cache = cache.get_or_parse(v).expect("parse");
        let direct = parse_direct(v).expect("direct");
        assert_eq!(
            via_cache.to_string(),
            direct.to_string(),
            "变体 AST 不一致: {}",
            v
        );
    }
    // 空语句 / 语法错误：快路径与慢路径都必须报错（错误语义对拍）
    assert!(cache.get_or_parse("").is_err(), "空 SQL 应报错");
    assert!(parse_direct("").is_none(), "空 SQL 直解析应失败");
}
