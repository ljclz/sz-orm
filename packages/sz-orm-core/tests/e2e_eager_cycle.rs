//! P2-7：Eager Loading 循环引用检测 L3 行为测试
//!
//! 验证目标：
//! - `EntityGraph::detect_cycles` 正确检测直接循环（A → B → A）
//! - `EntityGraph::detect_cycles` 正确检测间接循环（A → B → C → A）
//! - `EntityGraph::detect_cycles` 正确检测自环（A → A）
//! - `EntityGraph::detect_cycles` 正确处理无环图（DAG）
//! - `EntityGraph::detect_duplicate_edges` 检测重复边
//! - `EntityGraph::validate` 综合校验
//! - `WithRelation::load_eager` 检测重复关联名
//! - `WithRelation::load_join` 检测重复关联名
//! - 循环路径错误信息包含完整路径

#![cfg(test)]

use sz_orm_core::dialect::get_dialect;
use sz_orm_core::entity_graph::EntityGraph;
use sz_orm_core::find_with_related::WithRelation;
use sz_orm_core::DbType;

// ============================================================================
// EntityGraph 循环引用检测
// ============================================================================

// ===== L3-1：无环图（DAG）不报错 =====

#[test]
fn test_l3_1_dag_no_cycle() {
    // user → posts → comments（线性链，无循环）
    let mut graph = EntityGraph::new();
    graph.add_edge("user", "posts");
    graph.add_edge("posts", "comments");

    assert!(graph.detect_cycles().is_ok(), "线性链不应检测到循环");
}

// ===== L3-2：直接循环 A → B → A =====

#[test]
fn test_l3_2_direct_cycle() {
    // user → posts → user（直接循环，通过子图）
    let mut graph = EntityGraph::new();
    graph.add_edge_with_graph("user", "posts", {
        let mut sub = EntityGraph::new();
        sub.add_edge("posts", "user");
        sub
    });

    let result = graph.detect_cycles();
    assert!(result.is_err(), "直接循环应被检测到");

    let cycle = result.unwrap_err();
    assert!(
        cycle.contains(&"user".to_string()) && cycle.contains(&"posts".to_string()),
        "循环路径应包含 user 和 posts: {:?}",
        cycle
    );
}

// ===== L3-3：间接循环 A → B → C → A =====

#[test]
fn test_l3_3_indirect_cycle() {
    // user → posts → comments → user（间接循环）
    let mut graph = EntityGraph::new();
    graph.add_edge_with_graph("user", "posts", {
        let mut sub = EntityGraph::new();
        sub.add_edge_with_graph("posts", "comments", {
            let mut sub2 = EntityGraph::new();
            sub2.add_edge("comments", "user");
            sub2
        });
        sub
    });

    let result = graph.detect_cycles();
    assert!(result.is_err(), "间接循环应被检测到");

    let cycle = result.unwrap_err();
    assert!(
        cycle.len() >= 3,
        "间接循环路径应至少 3 个节点: {:?}",
        cycle
    );
    // 循环路径的起点取决于 DFS 遍历顺序（字典序），
    // 但循环必须包含 user、posts、comments 三个节点
    assert!(
        cycle.contains(&"user".to_string())
            && cycle.contains(&"posts".to_string())
            && cycle.contains(&"comments".to_string()),
        "循环路径应包含 user、posts、comments: {:?}",
        cycle
    );
    // 循环路径的首尾节点应相同（形成闭环）
    assert_eq!(
        cycle.first(),
        cycle.last(),
        "循环路径首尾应相同: {:?}",
        cycle
    );
}

// ===== L3-4：自环 A → A =====

#[test]
fn test_l3_4_self_loop() {
    // user → user（自环）
    let mut graph = EntityGraph::new();
    graph.add_edge("user", "user");

    let result = graph.detect_cycles();
    assert!(result.is_err(), "自环应被检测到");

    let cycle = result.unwrap_err();
    assert_eq!(
        cycle.first(),
        Some(&"user".to_string()),
        "自环节点应为 user: {:?}",
        cycle
    );
}

// ===== L3-5：多分支无环图不报错 =====

#[test]
fn test_l3_5_multi_branch_dag() {
    //       user
    //      /    \
    //   posts  profile
    //     |
    //  comments
    let mut graph = EntityGraph::new();
    graph.add_edge("user", "posts");
    graph.add_edge("user", "profile");
    graph.add_edge("posts", "comments");

    assert!(graph.detect_cycles().is_ok(), "多分支 DAG 不应检测到循环");
}

// ===== L3-6：循环路径错误信息完整 =====

#[test]
fn test_l3_6_cycle_path_in_error() {
    let mut graph = EntityGraph::new();
    graph.add_edge_with_graph("user", "posts", {
        let mut sub = EntityGraph::new();
        sub.add_edge("posts", "user");
        sub
    });

    let cycle = graph.detect_cycles().unwrap_err();
    // 循环路径的首尾节点应相同（形成闭环）
    assert_eq!(
        cycle.first(),
        cycle.last(),
        "循环路径首尾应相同: {:?}",
        cycle
    );
    // 循环应包含 user 和 posts
    assert!(
        cycle.contains(&"user".to_string()) && cycle.contains(&"posts".to_string()),
        "路径应包含 user 和 posts: {:?}",
        cycle
    );
    // 循环路径应至少 2 个节点（user → posts → user）
    assert!(
        cycle.len() >= 2,
        "循环路径应至少 2 个节点: {:?}",
        cycle
    );
}

// ===== L3-7：重复边检测 =====

#[test]
fn test_l3_7_duplicate_edges() {
    let mut graph = EntityGraph::new();
    graph.add_edge("user", "posts");
    graph.add_edge("user", "posts"); // 重复

    let result = graph.detect_duplicate_edges();
    assert!(result.is_err(), "重复边应被检测到");

    let duplicates = result.unwrap_err();
    assert_eq!(duplicates.len(), 1, "应有 1 个重复边: {:?}", duplicates);
    assert_eq!(
        duplicates[0],
        ("user".to_string(), "posts".to_string()),
        "重复边应为 (user, posts): {:?}",
        duplicates
    );
}

// ===== L3-8：无重复边不报错 =====

#[test]
fn test_l3_8_no_duplicate_edges() {
    let mut graph = EntityGraph::new();
    graph.add_edge("user", "posts");
    graph.add_edge("user", "profile");
    graph.add_edge("posts", "comments");

    assert!(graph.detect_duplicate_edges().is_ok(), "无重复边不应报错");
}

// ===== L3-9：validate 综合校验 — 无环无重复 =====

#[test]
fn test_l3_9_validate_clean_graph() {
    let mut graph = EntityGraph::new();
    graph.add_edge("user", "posts");
    graph.add_edge("posts", "comments");

    assert!(graph.validate().is_ok(), "无环无重复的图应通过校验");
}

// ===== L3-10：validate 综合校验 — 有循环 =====

#[test]
fn test_l3_10_validate_cycle() {
    let mut graph = EntityGraph::new();
    graph.add_edge_with_graph("user", "posts", {
        let mut sub = EntityGraph::new();
        sub.add_edge("posts", "user");
        sub
    });

    let result = graph.validate();
    assert!(result.is_err(), "有循环的图应校验失败");

    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("循环引用"),
        "错误信息应包含'循环引用': {}",
        err_msg
    );
}

// ===== L3-11：validate 综合校验 — 有重复边 =====

#[test]
fn test_l3_11_validate_duplicate() {
    let mut graph = EntityGraph::new();
    graph.add_edge("user", "posts");
    graph.add_edge("user", "posts");

    let result = graph.validate();
    assert!(result.is_err(), "有重复边的图应校验失败");

    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("重复边"),
        "错误信息应包含'重复边': {}",
        err_msg
    );
}

// ===== L3-12：空图不报错 =====

#[test]
fn test_l3_12_empty_graph() {
    let graph = EntityGraph::new();

    assert!(graph.detect_cycles().is_ok(), "空图不应检测到循环");
    assert!(graph.detect_duplicate_edges().is_ok(), "空图不应有重复边");
    assert!(graph.validate().is_ok(), "空图应通过校验");
}

// ===== L3-13：复杂 DAG（多层级）无循环 =====

#[test]
fn test_l3_13_complex_dag() {
    //       user
    //      /    \
    //   posts  profile
    //   /  \
    // comments tags
    let mut graph = EntityGraph::new();
    graph.add_edge("user", "posts");
    graph.add_edge("user", "profile");
    graph.add_edge_with_graph("posts", "comments", {
        let mut sub = EntityGraph::new();
        sub.add_edge("comments", "tags");
        sub
    });

    assert!(graph.detect_cycles().is_ok(), "复杂 DAG 不应检测到循环");
    assert!(graph.validate().is_ok(), "复杂 DAG 应通过校验");
}

// ===== L3-14：深层嵌套循环检测 =====

#[test]
fn test_l3_14_deep_nested_cycle() {
    // a → b → c → d → a（4 层嵌套循环）
    let mut graph = EntityGraph::new();
    graph.add_edge_with_graph("a", "b", {
        let mut sub1 = EntityGraph::new();
        sub1.add_edge_with_graph("b", "c", {
            let mut sub2 = EntityGraph::new();
            sub2.add_edge_with_graph("c", "d", {
                let mut sub3 = EntityGraph::new();
                sub3.add_edge("d", "a");
                sub3
            });
            sub2
        });
        sub1
    });

    let result = graph.detect_cycles();
    assert!(result.is_err(), "4 层嵌套循环应被检测到");

    let cycle = result.unwrap_err();
    assert!(
        cycle.len() >= 4,
        "4 层循环路径应至少 4 个节点: {:?}",
        cycle
    );
}

// ===== L3-15：子图独立循环检测 =====

#[test]
fn test_l3_15_subgraph_cycle() {
    // 主图无环，但子图内部有环
    let mut graph = EntityGraph::new();
    graph.add_edge_with_graph("user", "posts", {
        let mut sub = EntityGraph::new();
        // 子图内部：x → y → x（循环）
        sub.add_edge_with_graph("x", "y", {
            let mut inner = EntityGraph::new();
            inner.add_edge("y", "x");
            inner
        });
        sub
    });

    let result = graph.detect_cycles();
    assert!(result.is_err(), "子图内部循环应被检测到");
}

// ============================================================================
// WithRelation 重复关联检测
// ============================================================================

// ===== L3-16：WithRelation load_eager 检测重复关联名 =====

#[test]
fn test_l3_16_with_relation_duplicate_eager() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let loader = WithRelation::new(&*dialect, "users")
        .with_has_many("orders", "user_id", "id")
        .with_has_many("orders", "user_id", "id"); // 重复关联名

    let result = loader.load_eager(Some("users.id > 0"));
    assert!(result.is_err(), "重复关联应返回 Err");
    // 注意：不能用 unwrap_err()，因为 WithRelation 未实现 Debug
    let err_msg = match result {
        Err(e) => format!("{}", e),
        Ok(_) => panic!("期望 Err，得到 Ok"),
    };
    assert!(
        err_msg.contains("重复关联检测失败"),
        "错误消息应包含'重复关联检测失败': {}",
        err_msg
    );
}

// ===== L3-17：WithRelation load_join 检测重复关联名 =====

#[test]
fn test_l3_17_with_relation_duplicate_join() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let loader = WithRelation::new(&*dialect, "users")
        .with_has_one("profiles", "user_id", "id")
        .with_has_one("profiles", "user_id", "id"); // 重复关联名

    let result = loader.load_join(Some("users.id > 0"));
    assert!(result.is_err(), "重复关联应返回 Err");
    // 注意：不能用 unwrap_err()，因为 WithRelation 未实现 Debug
    let err_msg = match result {
        Err(e) => format!("{}", e),
        Ok(_) => panic!("期望 Err，得到 Ok"),
    };
    assert!(
        err_msg.contains("重复关联检测失败"),
        "错误消息应包含'重复关联检测失败': {}",
        err_msg
    );
}

// ===== L3-18：WithRelation 不同关联名不报错 =====

#[test]
fn test_l3_18_with_relation_different_names_ok() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let loader = WithRelation::new(&*dialect, "users")
        .with_has_many("orders", "user_id", "id")
        .with_has_one("profiles", "user_id", "id");

    // 不同关联名应成功
    let loaded = loader.load_eager(Some("users.id > 0"));
    assert!(loaded.is_ok(), "不同关联名不应报错: {:?}", loaded.err());
    let loaded = loaded.unwrap();
    assert!(!loaded.main_sql().is_empty(), "主表 SQL 不应为空");
}

// ===== L3-19：WithRelation 同名不同类型仍算重复 =====

#[test]
fn test_l3_19_with_relation_same_name_different_type() {
    let dialect = get_dialect(DbType::MySQL).unwrap();
    let loader = WithRelation::new(&*dialect, "users")
        .with_has_many("orders", "user_id", "id")
        .with_belongs_to("orders", "order_id", "id"); // 同名 "orders"，不同类型

    let result = loader.load_eager(None);
    assert!(result.is_err(), "同名不同类型应返回 Err");
    // 注意：不能用 unwrap_err()，因为 WithRelation 未实现 Debug
    let err_msg = match result {
        Err(e) => format!("{}", e),
        Ok(_) => panic!("期望 Err，得到 Ok"),
    };
    assert!(
        err_msg.contains("重复关联检测失败"),
        "错误消息应包含'重复关联检测失败': {}",
        err_msg
    );
}

// ===== L3-20：EntityGraph 循环检测不误报合法自引用 =====

#[test]
fn test_l3_20_legitimate_self_reference_no_cycle() {
    // 树形结构：category → subcategories（自引用但无环）
    // category → children → children（合法的多级展开）
    let mut graph = EntityGraph::new();
    graph.add_edge("category", "children");

    // 自引用关系（category.children 也是 category 类型）是合法的
    // 只要不在图中形成 A → B → A 的循环就不报错
    assert!(
        graph.detect_cycles().is_ok(),
        "合法自引用关系不应被误报为循环"
    );
}
