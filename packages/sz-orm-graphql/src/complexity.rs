//! ComplexityCalculator — GraphQL 查询复杂度计算与限制
//!
//! 基于 `GraphQLIR` 计算查询复杂度（深度 + 字段数 + 成本），
//! 超限查询拒绝并返回 `ComplexityError`。
//!
//! # 计算规则
//!
//! - **深度**：最大嵌套层级（叶子字段深度 = 1）
//! - **字段数**：所有选择集字段总数
//! - **成本**：Σ(字段权重 × 子树深度)（递归累加）
//!
//! # 约束
//!
//! 计算开销 ≤ 查询执行总耗时的 5%（spec §4.1 性能）

use std::collections::HashMap;

use crate::query_ir::{GraphQLIR, GraphQLSelection};

/// 复杂度配置
#[derive(Debug, Clone)]
pub struct ComplexityConfig {
    /// 深度上限（默认 10）
    pub max_depth: u32,
    /// 字段数量上限（默认 100）
    pub max_fields: u32,
    /// 计算成本上限（默认 1000）
    pub max_cost: u64,
    /// 字段权重（默认 1，高开销字段可配置更高权重）
    pub field_weights: HashMap<String, u64>,
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            max_fields: 100,
            max_cost: 1000,
            field_weights: HashMap::new(),
        }
    }
}

impl ComplexityConfig {
    /// 创建 Builder
    pub fn builder() -> ComplexityConfigBuilder {
        ComplexityConfigBuilder::default()
    }

    fn field_weight(&self, name: &str) -> u64 {
        self.field_weights.get(name).copied().unwrap_or(1)
    }
}

/// ComplexityConfig Builder
#[derive(Debug, Default)]
pub struct ComplexityConfigBuilder {
    max_depth: Option<u32>,
    max_fields: Option<u32>,
    max_cost: Option<u64>,
    field_weights: HashMap<String, u64>,
}

impl ComplexityConfigBuilder {
    pub fn max_depth(mut self, d: u32) -> Self {
        self.max_depth = Some(d);
        self
    }
    pub fn max_fields(mut self, n: u32) -> Self {
        self.max_fields = Some(n);
        self
    }
    pub fn max_cost(mut self, c: u64) -> Self {
        self.max_cost = Some(c);
        self
    }
    pub fn field_weight(mut self, name: &str, w: u64) -> Self {
        self.field_weights.insert(name.to_string(), w);
        self
    }
    pub fn build(self) -> ComplexityConfig {
        ComplexityConfig {
            max_depth: self.max_depth.unwrap_or(10),
            max_fields: self.max_fields.unwrap_or(100),
            max_cost: self.max_cost.unwrap_or(1000),
            field_weights: self.field_weights,
        }
    }
}

/// 复杂度超限错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComplexityError {
    DepthExceeded { actual: u32, max: u32 },
    FieldsExceeded { actual: u32, max: u32 },
    CostExceeded { actual: u64, max: u64 },
}

impl std::fmt::Display for ComplexityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DepthExceeded { actual, max } => {
                write!(f, "depth {actual} exceeds max {max}")
            }
            Self::FieldsExceeded { actual, max } => {
                write!(f, "field count {actual} exceeds max {max}")
            }
            Self::CostExceeded { actual, max } => {
                write!(f, "cost {actual} exceeds max {max}")
            }
        }
    }
}

impl std::error::Error for ComplexityError {}

/// 复杂度计算结果
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexityResult {
    pub depth: u32,
    pub field_count: u32,
    pub cost: u64,
    pub exceeded: Option<ComplexityError>,
}

/// 复杂度计算器
pub struct ComplexityCalculator {
    config: ComplexityConfig,
}

impl ComplexityCalculator {
    /// 创建计算器
    pub fn new(config: ComplexityConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建计算器
    pub fn with_defaults() -> Self {
        Self::new(ComplexityConfig::default())
    }

    /// 计算查询复杂度
    ///
    /// 深度 = 最大嵌套层级；字段数 = 所有选择集字段总数；
    /// 成本 = Σ(字段权重 × 子树深度)（递归累加）
    pub fn calculate(&self, ir: &GraphQLIR) -> ComplexityResult {
        let depth = calculate_depth(&ir.selection_set);
        let field_count = count_fields(&ir.selection_set);
        let cost = calculate_cost(&ir.selection_set, &self.config);

        let exceeded = if depth > self.config.max_depth {
            Some(ComplexityError::DepthExceeded {
                actual: depth,
                max: self.config.max_depth,
            })
        } else if field_count > self.config.max_fields {
            Some(ComplexityError::FieldsExceeded {
                actual: field_count,
                max: self.config.max_fields,
            })
        } else if cost > self.config.max_cost {
            Some(ComplexityError::CostExceeded {
                actual: cost,
                max: self.config.max_cost,
            })
        } else {
            None
        };

        ComplexityResult {
            depth,
            field_count,
            cost,
            exceeded,
        }
    }

    /// 校验查询，超限返回错误
    pub fn validate(&self, ir: &GraphQLIR) -> Result<(), ComplexityError> {
        let result = self.calculate(ir);
        match result.exceeded {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

fn calculate_depth(selections: &[GraphQLSelection]) -> u32 {
    let mut max_depth = 0;
    for sel in selections {
        if !sel.selection_set.is_empty() {
            let d = 1 + calculate_depth(&sel.selection_set);
            max_depth = max_depth.max(d);
        } else {
            max_depth = max_depth.max(1);
        }
    }
    max_depth
}

fn count_fields(selections: &[GraphQLSelection]) -> u32 {
    let mut count = 0u32;
    for sel in selections {
        count += 1;
        count += count_fields(&sel.selection_set);
    }
    count
}

fn field_subtree_depth(sel: &GraphQLSelection) -> u64 {
    if sel.selection_set.is_empty() {
        1
    } else {
        1 + sel
            .selection_set
            .iter()
            .map(field_subtree_depth)
            .max()
            .unwrap_or(0)
    }
}

fn calculate_cost(selections: &[GraphQLSelection], config: &ComplexityConfig) -> u64 {
    let mut cost = 0u64;
    for sel in selections {
        let weight = config.field_weight(&sel.name);
        let sub_depth = field_subtree_depth(sel);
        cost += weight * sub_depth;
        if !sel.selection_set.is_empty() {
            cost += calculate_cost(&sel.selection_set, config);
        }
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::{GraphQLIR, GraphQLOperation, GraphQLSelection};

    fn make_ir(selections: Vec<GraphQLSelection>) -> GraphQLIR {
        GraphQLIR {
            operation: GraphQLOperation::Query,
            selection_set: selections,
        }
    }

    fn sel(name: &str) -> GraphQLSelection {
        GraphQLSelection {
            name: name.into(),
            alias: None,
            arguments: Default::default(),
            directives: vec![],
            selection_set: vec![],
        }
    }

    fn sel_nested(name: &str, children: Vec<GraphQLSelection>) -> GraphQLSelection {
        GraphQLSelection {
            name: name.into(),
            alias: None,
            arguments: Default::default(),
            directives: vec![],
            selection_set: children,
        }
    }

    #[test]
    fn test_simple_query() {
        let ir = make_ir(vec![sel("id"), sel("name")]);
        let calc = ComplexityCalculator::with_defaults();
        let result = calc.calculate(&ir);
        assert_eq!(result.depth, 1);
        assert_eq!(result.field_count, 2);
        assert_eq!(result.cost, 2);
        assert!(result.exceeded.is_none());
    }

    #[test]
    fn test_nested_depth() {
        let ir = make_ir(vec![sel_nested(
            "user",
            vec![
                sel("id"),
                sel_nested("orders", vec![sel("id"), sel("total")]),
            ],
        )]);
        let calc = ComplexityCalculator::with_defaults();
        let result = calc.calculate(&ir);
        assert_eq!(result.depth, 3);
        assert_eq!(result.field_count, 5);
        assert!(result.exceeded.is_none());
    }

    #[test]
    fn test_depth_exceeded() {
        let ir = make_ir(vec![sel_nested(
            "a",
            vec![sel_nested(
                "b",
                vec![sel_nested(
                    "c",
                    vec![sel_nested(
                        "d",
                        vec![sel_nested("e", vec![sel_nested("f", vec![sel("x")])])],
                    )],
                )],
            )],
        )]);
        let config = ComplexityConfig::builder().max_depth(5).build();
        let calc = ComplexityCalculator::new(config);
        let result = calc.calculate(&ir);
        assert_eq!(result.depth, 7);
        assert!(matches!(
            result.exceeded,
            Some(ComplexityError::DepthExceeded { actual: 7, max: 5 })
        ));
    }

    #[test]
    fn test_fields_exceeded() {
        let selections: Vec<GraphQLSelection> = (0..101).map(|i| sel(&format!("f{i}"))).collect();
        let ir = make_ir(selections);
        let config = ComplexityConfig::builder().max_fields(100).build();
        let calc = ComplexityCalculator::new(config);
        let result = calc.calculate(&ir);
        assert_eq!(result.field_count, 101);
        assert!(matches!(
            result.exceeded,
            Some(ComplexityError::FieldsExceeded {
                actual: 101,
                max: 100
            })
        ));
    }

    #[test]
    fn test_cost_exceeded() {
        let ir = make_ir(vec![sel_nested(
            "expensive",
            vec![sel("a"), sel("b"), sel("c"), sel("d"), sel("e")],
        )]);
        let config = ComplexityConfig::builder()
            .field_weight("expensive", 100)
            .max_cost(50)
            .build();
        let calc = ComplexityCalculator::new(config);
        let result = calc.calculate(&ir);
        assert!(result.cost > 50);
        assert!(matches!(
            result.exceeded,
            Some(ComplexityError::CostExceeded { .. })
        ));
    }

    #[test]
    fn test_validate_ok() {
        let ir = make_ir(vec![sel("id"), sel("name")]);
        let calc = ComplexityCalculator::with_defaults();
        assert!(calc.validate(&ir).is_ok());
    }

    #[test]
    fn test_validate_reject() {
        let ir = make_ir(vec![sel_nested(
            "a",
            vec![sel_nested(
                "b",
                vec![sel_nested("c", vec![sel_nested("d", vec![sel("x")])])],
            )],
        )]);
        let config = ComplexityConfig::builder().max_depth(3).build();
        let calc = ComplexityCalculator::new(config);
        assert!(calc.validate(&ir).is_err());
    }

    #[test]
    fn test_field_weights_default() {
        let ir = make_ir(vec![sel("a"), sel("b"), sel("c")]);
        let calc = ComplexityCalculator::with_defaults();
        let result = calc.calculate(&ir);
        assert_eq!(result.cost, 3);
    }

    #[test]
    fn test_field_weights_custom() {
        let ir = make_ir(vec![sel("cheap"), sel("expensive")]);
        let config = ComplexityConfig::builder()
            .field_weight("expensive", 10)
            .build();
        let calc = ComplexityCalculator::new(config);
        let result = calc.calculate(&ir);
        assert_eq!(result.cost, 11);
    }

    #[test]
    fn test_empty_selection_set() {
        let ir = make_ir(vec![]);
        let calc = ComplexityCalculator::with_defaults();
        let result = calc.calculate(&ir);
        assert_eq!(result.depth, 0);
        assert_eq!(result.field_count, 0);
        assert_eq!(result.cost, 0);
        assert!(result.exceeded.is_none());
    }

    #[test]
    fn test_config_defaults() {
        let config = ComplexityConfig::default();
        assert_eq!(config.max_depth, 10);
        assert_eq!(config.max_fields, 100);
        assert_eq!(config.max_cost, 1000);
    }
}
