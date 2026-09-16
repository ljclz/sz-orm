//! ABAC 策略引擎
//!
//! 基于属性的条件评估，支持主体/资源/环境属性。

use std::collections::HashMap;

/// 策略效果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// 允许
    Allow,
    /// 拒绝
    Deny,
}

/// 属性值
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    /// 字符串
    String(String),
    /// 整数
    Int(i64),
    /// 布尔
    Bool(bool),
}

impl AttributeValue {
    /// 从字符串创建
    pub fn str_val(s: &str) -> Self {
        Self::String(s.to_string())
    }

    /// 字符串值
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// 整数值
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// 布尔值
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// 属性集合类型
pub type Attributes = HashMap<String, AttributeValue>;

/// 访问请求
#[derive(Debug, Clone)]
pub struct AccessRequest {
    /// 主体属性（用户属性）
    pub subject: Attributes,
    /// 资源属性
    pub resource: Attributes,
    /// 环境属性
    pub environment: Attributes,
    /// 请求的操作
    pub action: String,
}

impl AccessRequest {
    /// 创建访问请求
    pub fn new(action: &str) -> Self {
        Self {
            subject: HashMap::new(),
            resource: HashMap::new(),
            environment: HashMap::new(),
            action: action.to_string(),
        }
    }

    /// 添加主体属性
    pub fn with_subject_attr(mut self, key: &str, value: AttributeValue) -> Self {
        self.subject.insert(key.to_string(), value);
        self
    }

    /// 添加资源属性
    pub fn with_resource_attr(mut self, key: &str, value: AttributeValue) -> Self {
        self.resource.insert(key.to_string(), value);
        self
    }

    /// 添加环境属性
    pub fn with_env_attr(mut self, key: &str, value: AttributeValue) -> Self {
        self.environment.insert(key.to_string(), value);
        self
    }
}

/// 条件表达式
#[derive(Debug, Clone)]
pub enum Condition {
    /// 属性等于值
    Eq {
        scope: AttributeScope,
        key: String,
        value: AttributeValue,
    },
    /// 属性不等于值
    NotEq {
        scope: AttributeScope,
        key: String,
        value: AttributeValue,
    },
    /// 属性存在于列表
    In {
        scope: AttributeScope,
        key: String,
        values: Vec<AttributeValue>,
    },
    /// 属性大于值
    Gt {
        scope: AttributeScope,
        key: String,
        value: i64,
    },
    /// 属性小于值
    Lt {
        scope: AttributeScope,
        key: String,
        value: i64,
    },
    /// AND 组合
    And(Vec<Condition>),
    /// OR 组合
    Or(Vec<Condition>),
}

/// 属性作用域
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeScope {
    Subject,
    Resource,
    Environment,
}

impl Condition {
    /// 评估条件
    pub fn evaluate(&self, request: &AccessRequest) -> bool {
        match self {
            Self::Eq { scope, key, value } => get_attr(request, scope, key) == Some(value),
            Self::NotEq { scope, key, value } => get_attr(request, scope, key) != Some(value),
            Self::In { scope, key, values } => {
                get_attr(request, scope, key).is_some_and(|v| values.contains(v))
            }
            Self::Gt { scope, key, value } => get_attr(request, scope, key)
                .is_some_and(|v| v.as_int().is_some_and(|n| n > *value)),
            Self::Lt { scope, key, value } => get_attr(request, scope, key)
                .is_some_and(|v| v.as_int().is_some_and(|n| n < *value)),
            Self::And(conds) => conds.iter().all(|c| c.evaluate(request)),
            Self::Or(conds) => conds.iter().any(|c| c.evaluate(request)),
        }
    }
}

fn get_attr<'a>(
    request: &'a AccessRequest,
    scope: &AttributeScope,
    key: &str,
) -> Option<&'a AttributeValue> {
    match scope {
        AttributeScope::Subject => request.subject.get(key),
        AttributeScope::Resource => request.resource.get(key),
        AttributeScope::Environment => request.environment.get(key),
    }
}

/// ABAC 策略
#[derive(Debug, Clone)]
pub struct AbacPolicy {
    /// 策略 ID
    pub id: String,
    /// 策略描述
    pub description: String,
    /// 目标操作
    pub action: String,
    /// 条件
    pub condition: Condition,
    /// 效果
    pub effect: Effect,
}

impl AbacPolicy {
    /// 创建策略
    pub fn new(id: &str, action: &str, condition: Condition, effect: Effect) -> Self {
        Self {
            id: id.to_string(),
            description: String::new(),
            action: action.to_string(),
            condition,
            effect,
        }
    }

    /// 添加描述
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// 检查策略是否适用于请求
    pub fn matches(&self, request: &AccessRequest) -> bool {
        self.action == request.action && self.condition.evaluate(request)
    }
}

/// ABAC 策略引擎
pub struct AbacPolicyEngine {
    policies: Vec<AbacPolicy>,
    default_effect: Effect,
}

impl AbacPolicyEngine {
    /// 创建策略引擎（默认拒绝）
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
            default_effect: Effect::Deny,
        }
    }

    /// 设置默认效果
    pub fn with_default_effect(mut self, effect: Effect) -> Self {
        self.default_effect = effect;
        self
    }

    /// 添加策略
    pub fn add_policy(&mut self, policy: AbacPolicy) {
        self.policies.push(policy);
    }

    /// 评估访问请求
    pub fn evaluate(&self, request: &AccessRequest) -> Effect {
        let mut matched = false;
        for policy in &self.policies {
            if policy.matches(request) {
                matched = true;
                if policy.effect == Effect::Deny {
                    return Effect::Deny;
                }
            }
        }
        if matched {
            Effect::Allow
        } else {
            self.default_effect.clone()
        }
    }

    /// 策略数
    pub fn policy_count(&self) -> usize {
        self.policies.len()
    }
}

impl Default for AbacPolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request() -> AccessRequest {
        AccessRequest::new("read")
            .with_subject_attr("role", AttributeValue::str_val("admin"))
            .with_subject_attr("department", AttributeValue::str_val("engineering"))
            .with_resource_attr("owner", AttributeValue::str_val("alice"))
            .with_resource_attr("classification", AttributeValue::str_val("public"))
            .with_env_attr("time_hour", AttributeValue::Int(14))
    }

    #[test]
    fn test_policy_allow_on_match() {
        let policy = AbacPolicy::new(
            "p1",
            "read",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
            Effect::Allow,
        );
        assert!(policy.matches(&make_request()));
    }

    #[test]
    fn test_policy_deny_on_no_match() {
        let policy = AbacPolicy::new(
            "p1",
            "write",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
            Effect::Allow,
        );
        assert!(!policy.matches(&make_request()));
    }

    #[test]
    fn test_engine_default_deny() {
        let engine = AbacPolicyEngine::new();
        assert_eq!(engine.evaluate(&make_request()), Effect::Deny);
    }

    #[test]
    fn test_engine_allow_policy() {
        let mut engine = AbacPolicyEngine::new();
        engine.add_policy(AbacPolicy::new(
            "p1",
            "read",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
            Effect::Allow,
        ));
        assert_eq!(engine.evaluate(&make_request()), Effect::Allow);
    }

    #[test]
    fn test_engine_deny_overrides_allow() {
        let mut engine = AbacPolicyEngine::new();
        engine.add_policy(AbacPolicy::new(
            "p1",
            "read",
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
            Effect::Allow,
        ));
        engine.add_policy(AbacPolicy::new(
            "p2",
            "read",
            Condition::Eq {
                scope: AttributeScope::Resource,
                key: "classification".into(),
                value: AttributeValue::str_val("secret"),
            },
            Effect::Deny,
        ));
        let req =
            make_request().with_resource_attr("classification", AttributeValue::str_val("secret"));
        assert_eq!(engine.evaluate(&req), Effect::Deny);
    }

    #[test]
    fn test_condition_and() {
        let cond = Condition::And(vec![
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "department".into(),
                value: AttributeValue::str_val("engineering"),
            },
        ]);
        assert!(cond.evaluate(&make_request()));
    }

    #[test]
    fn test_condition_or() {
        let cond = Condition::Or(vec![
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("superadmin"),
            },
            Condition::Eq {
                scope: AttributeScope::Subject,
                key: "role".into(),
                value: AttributeValue::str_val("admin"),
            },
        ]);
        assert!(cond.evaluate(&make_request()));
    }

    #[test]
    fn test_condition_gt_lt() {
        let cond = Condition::And(vec![
            Condition::Gt {
                scope: AttributeScope::Environment,
                key: "time_hour".into(),
                value: 9,
            },
            Condition::Lt {
                scope: AttributeScope::Environment,
                key: "time_hour".into(),
                value: 18,
            },
        ]);
        assert!(cond.evaluate(&make_request()));
    }

    #[test]
    fn test_condition_in() {
        let cond = Condition::In {
            scope: AttributeScope::Subject,
            key: "role".into(),
            values: vec![
                AttributeValue::str_val("admin"),
                AttributeValue::str_val("superadmin"),
            ],
        };
        assert!(cond.evaluate(&make_request()));
    }

    #[test]
    fn test_condition_not_eq() {
        let cond = Condition::NotEq {
            scope: AttributeScope::Resource,
            key: "classification".into(),
            value: AttributeValue::str_val("secret"),
        };
        assert!(cond.evaluate(&make_request()));
    }
}
