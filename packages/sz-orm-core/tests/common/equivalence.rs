//! 等价性断言工具（v2.4.0 任务 1.1~1.3）
//!
//! 提供 SmartEagerLoader 与手动 EagerLoader 结果集等价性验证的断言工具：
//! - `assert_eager_equivalent`：逐行逐字段比对结果集
//! - `assert_strategy_selected`：断言策略选择
//! - `assert_nested_depth_equal`：递归比对嵌套树

use std::collections::HashMap;

use sz_orm_core::eager_loader::NestedEagerResult;
use sz_orm_core::relation_trait::RelationKind;
use sz_orm_core::smart_eager_loader::{LoadStrategy, StrategyDecision};
use sz_orm_core::Value;

/// 比较两个 Value 是否语义相等（整型跨宽度相等，如 I32(1) == I64(1)）
fn values_equivalent(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    let a_i = value_to_i64(a);
    let b_i = value_to_i64(b);
    if let (Some(av), Some(bv)) = (a_i, b_i) {
        return av == bv;
    }
    let a_s = value_to_str(a);
    let b_s = value_to_str(b);
    if let (Some(av), Some(bv)) = (a_s, b_s) {
        return av == bv;
    }
    false
}

fn value_to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::I8(x) => Some(*x as i64),
        Value::I16(x) => Some(*x as i64),
        Value::I32(x) => Some(*x as i64),
        Value::I64(x) => Some(*x),
        Value::U8(x) => Some(*x as i64),
        Value::U16(x) => Some(*x as i64),
        Value::U32(x) => Some(*x as i64),
        Value::U64(x) => Some(*x as i64),
        _ => None,
    }
}

fn value_to_str(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s),
        Value::Decimal(s) => Some(s),
        Value::Uuid(s) => Some(s),
        _ => None,
    }
}

/// 比较两行（HashMap<String, Value>）是否逐字段等价
fn rows_equivalent(a: &HashMap<String, Value>, b: &HashMap<String, Value>) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (k, av) in a {
        match b.get(k) {
            Some(bv) => {
                if !values_equivalent(av, bv) {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

/// 对结果集按外键值排序，消除方言默认排序差异
fn sort_results_by_fk(
    results: &[HashMap<String, Value>],
    fk_field: &str,
) -> Vec<HashMap<String, Value>> {
    let mut sorted = results.to_vec();
    sorted.sort_by(|a, b| {
        let av = value_to_i64(a.get(fk_field).unwrap_or(&Value::Null)).unwrap_or(0);
        let bv = value_to_i64(b.get(fk_field).unwrap_or(&Value::Null)).unwrap_or(0);
        av.cmp(&bv)
    });
    sorted
}

/// 断言 SmartEagerLoader 与手动 EagerLoader 结果集等价
///
/// 逐行逐字段比对智能与手动加载结果集（行数、字段名、字段值）。
/// HasMany/ManyToMany 子集合按外键值无序排序后比对，避免方言默认排序差异。
///
/// # 参数
/// - `smart_results`：SmartEagerLoader 加载的结果集
/// - `manual_results`：手动 EagerLoader 加载的结果集
/// - `relation_kind`：关联类型（决定是否排序子集合）
/// - `fk_field`：外键字段名（HasMany/ManyToMany 排序用，HasOne 传空字符串）
///
/// # Panics
/// 断言失败时 panic 输出差异明细（差异行号、字段名、期望值 vs 实际值）
pub fn assert_eager_equivalent(
    smart_results: &[HashMap<String, Value>],
    manual_results: &[HashMap<String, Value>],
    relation_kind: RelationKind,
    fk_field: &str,
) {
    assert_eq!(
        smart_results.len(),
        manual_results.len(),
        "结果集行数不匹配: smart={} manual={}",
        smart_results.len(),
        manual_results.len()
    );

    let (smart_sorted, manual_sorted) = match relation_kind {
        RelationKind::HasOne | RelationKind::BelongsTo => {
            (smart_results.to_vec(), manual_results.to_vec())
        }
        RelationKind::HasMany | RelationKind::ManyToMany => {
            if fk_field.is_empty() {
                (smart_results.to_vec(), manual_results.to_vec())
            } else {
                (
                    sort_results_by_fk(smart_results, fk_field),
                    sort_results_by_fk(manual_results, fk_field),
                )
            }
        }
    };

    for (i, (smart_row, manual_row)) in smart_sorted.iter().zip(manual_sorted.iter()).enumerate() {
        if !rows_equivalent(smart_row, manual_row) {
            let mut diffs = Vec::new();
            let all_keys: std::collections::BTreeSet<&String> =
                smart_row.keys().chain(manual_row.keys()).collect();
            for k in &all_keys {
                let sv = smart_row.get(*k);
                let mv = manual_row.get(*k);
                if sv != mv
                    && !match (sv, mv) {
                        (Some(a), Some(b)) => values_equivalent(a, b),
                        _ => false,
                    }
                {
                    diffs.push(format!("  字段 '{k}': smart={sv:?} manual={mv:?}"));
                }
            }
            panic!(
                "结果集第 {i} 行不匹配:\n{}\nsmart_row={smart_row:?}\nmanual_row={manual_row:?}",
                diffs.join("\n")
            );
        }
    }
}

/// 断言策略选择正确
///
/// # 参数
/// - `decision`：StrategyResolver 的决策结果
/// - `expected`：期望的加载策略
///
/// # Panics
/// 不符时 panic 输出 relation_name / actual / expected / reason
pub fn assert_strategy_selected(decision: &StrategyDecision, expected: LoadStrategy) {
    if decision.strategy != expected {
        panic!(
            "策略选择不符: relation='{}' actual={:?} expected={:?} reason='{}'",
            decision.relation_name, decision.strategy, expected, decision.reason
        );
    }
}

/// 递归计算 NestedEagerResult 树的最大深度
fn nested_depth(tree: &NestedEagerResult, max_recursion: usize) -> usize {
    if max_recursion == 0 {
        return 1;
    }
    match tree {
        NestedEagerResult::Leaf(_) => 1,
        NestedEagerResult::Node { children, .. } => {
            1 + children
                .iter()
                .map(|c| nested_depth(c, max_recursion - 1))
                .max()
                .unwrap_or(0)
        }
    }
}

/// 断言嵌套树深度与结构等价
///
/// 递归比对 NestedEagerResult 树（节点类型 Leaf/Node 一致、逐层 children 数量一致、
/// 逐字段 row 比对、max_depth 一致）。递归深度限 3 级避免栈溢出。
///
/// # 参数
/// - `smart_tree`：SmartEagerLoader 加载的嵌套树
/// - `manual_tree`：手动 EagerLoader 加载的嵌套树
///
/// # Panics
/// 对相同嵌套树断言通过、对深度/节点数差异 panic 输出差异层
pub fn assert_nested_depth_equal(smart_tree: &NestedEagerResult, manual_tree: &NestedEagerResult) {
    let smart_depth = nested_depth(smart_tree, 3);
    let manual_depth = nested_depth(manual_tree, 3);
    assert_eq!(
        smart_depth, manual_depth,
        "嵌套深度不一致: smart={smart_depth} manual={manual_depth}"
    );
    assert_nested_node_equal(smart_tree, manual_tree, 0, 3);
}

fn assert_nested_node_equal(
    smart: &NestedEagerResult,
    manual: &NestedEagerResult,
    level: usize,
    max_recursion: usize,
) {
    if level >= max_recursion {
        return;
    }
    match (smart, manual) {
        (NestedEagerResult::Leaf(smart_row), NestedEagerResult::Leaf(manual_row)) => {
            if !rows_equivalent(smart_row, manual_row) {
                panic!(
                    "第 {level} 层 Leaf 节点 row 不匹配:\nsmart={smart_row:?}\nmanual={manual_row:?}"
                );
            }
        }
        (
            NestedEagerResult::Node {
                row: smart_row,
                children: smart_children,
            },
            NestedEagerResult::Node {
                row: manual_row,
                children: manual_children,
            },
        ) => {
            if !rows_equivalent(smart_row, manual_row) {
                panic!(
                    "第 {level} 层 Node 节点 row 不匹配:\nsmart={smart_row:?}\nmanual={manual_row:?}"
                );
            }
            assert_eq!(
                smart_children.len(),
                manual_children.len(),
                "第 {level} 层 children 数量不一致: smart={} manual={}",
                smart_children.len(),
                manual_children.len()
            );
            for (sc, mc) in smart_children.iter().zip(manual_children.iter()) {
                assert_nested_node_equal(sc, mc, level + 1, max_recursion);
            }
        }
        (smart, manual) => {
            panic!("第 {level} 层节点类型不一致: smart={smart:?} manual={manual:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, name: &str) -> HashMap<String, Value> {
        let mut r = HashMap::new();
        r.insert("id".to_string(), Value::I64(id));
        r.insert("name".to_string(), Value::String(name.to_string()));
        r
    }

    #[test]
    fn test_assert_eager_equivalent_equal() {
        let smart = vec![row(1, "a"), row(2, "b")];
        let manual = vec![row(1, "a"), row(2, "b")];
        assert_eager_equivalent(&smart, &manual, RelationKind::HasOne, "");
    }

    #[test]
    fn test_assert_eager_equivalent_hasmany_unordered() {
        let smart = vec![row(2, "b"), row(1, "a")];
        let manual = vec![row(1, "a"), row(2, "b")];
        assert_eager_equivalent(&smart, &manual, RelationKind::HasMany, "id");
    }

    #[test]
    #[should_panic(expected = "结果集行数不匹配")]
    fn test_assert_eager_equivalent_row_count_mismatch() {
        let smart = vec![row(1, "a")];
        let manual = vec![row(1, "a"), row(2, "b")];
        assert_eager_equivalent(&smart, &manual, RelationKind::HasOne, "");
    }

    #[test]
    #[should_panic(expected = "结果集第 0 行不匹配")]
    fn test_assert_eager_equivalent_field_mismatch() {
        let smart = vec![row(1, "a")];
        let manual = vec![row(1, "b")];
        assert_eager_equivalent(&smart, &manual, RelationKind::HasOne, "");
    }

    #[test]
    fn test_assert_strategy_selected_correct() {
        let decision = StrategyDecision {
            relation_name: "profile".to_string(),
            relation_kind: RelationKind::HasOne,
            strategy: LoadStrategy::Join,
            reason: "HasOne → Join".to_string(),
            estimated_query_count: 1,
        };
        assert_strategy_selected(&decision, LoadStrategy::Join);
    }

    #[test]
    #[should_panic(expected = "策略选择不符")]
    fn test_assert_strategy_selected_incorrect() {
        let decision = StrategyDecision {
            relation_name: "orders".to_string(),
            relation_kind: RelationKind::HasMany,
            strategy: LoadStrategy::Join,
            reason: "HasMany → DataLoader".to_string(),
            estimated_query_count: 2,
        };
        assert_strategy_selected(&decision, LoadStrategy::DataLoader);
    }

    #[test]
    fn test_assert_nested_depth_equal_leaf() {
        let smart = NestedEagerResult::Leaf(row(1, "a"));
        let manual = NestedEagerResult::Leaf(row(1, "a"));
        assert_nested_depth_equal(&smart, &manual);
    }

    #[test]
    fn test_assert_nested_depth_equal_node() {
        let smart = NestedEagerResult::Node {
            row: row(1, "user"),
            children: vec![NestedEagerResult::Leaf(row(1, "order"))],
        };
        let manual = NestedEagerResult::Node {
            row: row(1, "user"),
            children: vec![NestedEagerResult::Leaf(row(1, "order"))],
        };
        assert_nested_depth_equal(&smart, &manual);
    }

    #[test]
    #[should_panic(expected = "嵌套深度不一致")]
    fn test_assert_nested_depth_equal_mismatch() {
        let smart = NestedEagerResult::Leaf(row(1, "a"));
        let manual = NestedEagerResult::Node {
            row: row(1, "a"),
            children: vec![NestedEagerResult::Leaf(row(1, "b"))],
        };
        assert_nested_depth_equal(&smart, &manual);
    }

    #[test]
    fn test_values_equivalent_cross_int_width() {
        assert!(values_equivalent(&Value::I32(1), &Value::I64(1)));
        assert!(values_equivalent(&Value::U8(1), &Value::I64(1)));
        assert!(!values_equivalent(&Value::I32(1), &Value::I64(2)));
    }
}
