//! 多租户安全策略：行级安全 + 列级脱敏 + 多租户审计
//!
//! 本模块在 `multi-tenant-enhanced` feature gate 下导出，提供：
//! - [`ParameterizedCondition`] — 参数化过滤条件（禁止 SQL 字符串拼接）
//! - [`Principal`] + [`RowLevelSecurityPolicy`] — 行级安全策略（部门级/角色级细粒度）
//! - [`MaskingFunction`] + [`PermissionPredicate`] + [`ColumnMaskingRule`] — 列级脱敏规则
//! - [`TenantAuditOperation`] + [`AuditResult`] + [`TenantAuditContext`] — 多租户审计

use crate::value::Value;

// ─── M1-T6：行级安全策略 ───────────────────────────────────────────

/// 参数化过滤条件（SQL 片段含占位符 + 参数值列表）
///
/// 禁止 SQL 字符串拼接，所有条件必须通过参数化绑定传递。
#[derive(Debug, Clone)]
pub struct ParameterizedCondition {
    /// SQL 片段，含 `$1` / `$2` 等占位符（如 `"department_id = $1"`）
    pub sql_fragment: String,
    /// 参数值列表，与占位符按位置对应
    pub params: Vec<Value>,
}

impl ParameterizedCondition {
    /// 创建新的参数化条件
    pub fn new(sql_fragment: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql_fragment: sql_fragment.into(),
            params,
        }
    }

    /// 无参数的条件（如 `"is_active = true"`）
    pub fn literal(sql_fragment: impl Into<String>) -> Self {
        Self {
            sql_fragment: sql_fragment.into(),
            params: Vec::new(),
        }
    }
}

/// 权限主体（租户 ID + 角色列表）
#[derive(Debug, Clone)]
pub struct Principal {
    /// 租户 ID
    pub tenant_id: i64,
    /// 角色列表（如 `"admin"` / `"manager"` / `"employee"`）
    pub roles: Vec<String>,
}

impl Principal {
    /// 创建新的权限主体
    pub fn new(tenant_id: i64, roles: Vec<String>) -> Self {
        Self { tenant_id, roles }
    }

    /// 检查是否拥有指定角色
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

/// 行级安全策略（扩展既有 `AccessRule`，提供部门级/角色级细粒度过滤）
///
/// 策略由服务端定义，不可被客户端篡改。
#[derive(Debug, Clone)]
pub struct RowLevelSecurityPolicy {
    /// 表名
    pub table: String,
    /// 参数化过滤条件（如 `"department_id = $1"`，非 SQL 字符串拼接）
    pub filter_condition: ParameterizedCondition,
    /// 权限主体（租户 ID + 角色）
    pub principal: Principal,
}

impl RowLevelSecurityPolicy {
    /// 创建新的行级安全策略
    pub fn new(
        table: impl Into<String>,
        filter_condition: ParameterizedCondition,
        principal: Principal,
    ) -> Self {
        Self {
            table: table.into(),
            filter_condition,
            principal,
        }
    }
}

// ─── M1-T7：列级脱敏规则 ───────────────────────────────────────────

/// 脱敏函数枚举（复用既有 `sz_orm_masking::MaskingRule`）
pub use sz_orm_masking::MaskingRule as MaskingFunction;

/// 权限谓词（描述未授权租户/角色条件）
///
/// 当 `applicable_roles` 为 `None` 时，所有未在 `exempt_roles` 中的角色都适用脱敏。
/// 当 `applicable_roles` 为 `Some(roles)` 时，仅指定角色适用脱敏。
#[derive(Debug, Clone)]
pub struct PermissionPredicate {
    /// 适用脱敏的角色列表（`None` 表示所有角色除 `exempt_roles` 外）
    pub applicable_roles: Option<Vec<String>>,
    /// 豁免脱敏的角色列表（如 `"admin"` 可见原始值）
    pub exempt_roles: Vec<String>,
}

impl PermissionPredicate {
    /// 所有角色都适用脱敏（无豁免）
    pub fn all() -> Self {
        Self {
            applicable_roles: None,
            exempt_roles: Vec::new(),
        }
    }

    /// 仅指定角色适用脱敏
    pub fn for_roles(roles: Vec<String>) -> Self {
        Self {
            applicable_roles: Some(roles),
            exempt_roles: Vec::new(),
        }
    }

    /// 豁免指定角色（如 admin 可见原始值）
    pub fn with_exempt(mut self, roles: Vec<String>) -> Self {
        self.exempt_roles = roles;
        self
    }

    /// 判断给定角色列表是否适用脱敏
    pub fn applies_to(&self, roles: &[String]) -> bool {
        // 先检查豁免
        if roles.iter().any(|r| self.exempt_roles.contains(r)) {
            return false;
        }
        // 再检查适用范围
        match &self.applicable_roles {
            None => true,
            Some(applicable) => roles.iter().any(|r| applicable.contains(r)),
        }
    }
}

impl Default for PermissionPredicate {
    fn default() -> Self {
        Self::all()
    }
}

/// 列级脱敏规则
///
/// ORM 层强制执行，不可绕过。未配置脱敏规则的敏感列默认拒绝读取（安全优先）。
#[derive(Debug, Clone)]
pub struct ColumnMaskingRule {
    /// 表名
    pub table: String,
    /// 列名
    pub column: String,
    /// 脱敏函数
    pub masking_function: MaskingFunction,
    /// 适用权限（未授权租户/角色才脱敏）
    pub applicable_permissions: PermissionPredicate,
}

impl ColumnMaskingRule {
    /// 创建新的列级脱敏规则
    pub fn new(
        table: impl Into<String>,
        column: impl Into<String>,
        masking_function: MaskingFunction,
        applicable_permissions: PermissionPredicate,
    ) -> Self {
        Self {
            table: table.into(),
            column: column.into(),
            masking_function,
            applicable_permissions,
        }
    }

    /// 对给定值执行脱敏
    pub fn mask(&self, value: &str) -> String {
        sz_orm_masking::DataMasker::apply(&self.masking_function, value)
    }

    /// 判断给定角色列表是否需要脱敏
    pub fn applies_to(&self, roles: &[String]) -> bool {
        self.applicable_permissions.applies_to(roles)
    }
}

// ─── M1-T8：多租户审计 ─────────────────────────────────────────────

/// 多租户审计操作类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TenantAuditOperation {
    /// 上下文设置
    ContextSet,
    /// 租户切换
    ContextSwitch,
    /// 跨租户访问拒绝
    CrossTenantDenied,
    /// 行级安全过滤
    RowLevelFiltered,
    /// 列级脱敏执行
    ColumnMasked,
}

impl std::fmt::Display for TenantAuditOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContextSet => write!(f, "context_set"),
            Self::ContextSwitch => write!(f, "context_switch"),
            Self::CrossTenantDenied => write!(f, "cross_tenant_denied"),
            Self::RowLevelFiltered => write!(f, "row_level_filtered"),
            Self::ColumnMasked => write!(f, "column_masked"),
        }
    }
}

/// 审计结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditResult {
    /// 成功
    Success,
    /// 拒绝
    Denied,
}

impl std::fmt::Display for AuditResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Denied => write!(f, "denied"),
        }
    }
}

/// 多租户审计上下文
///
/// 审计日志含租户 ID + 操作 + 时间 + 结果，日志不可篡改（追加写入）。
#[derive(Debug, Clone)]
pub struct TenantAuditContext {
    /// 租户 ID
    pub tenant_id: i64,
    /// 操作类型
    pub operation: TenantAuditOperation,
    /// 时间戳（Unix 秒）
    pub timestamp: i64,
    /// 结果（成功/拒绝）
    pub result: AuditResult,
    /// 详情（如被拒绝的表名、被脱敏的列名等）
    pub detail: String,
}

impl TenantAuditContext {
    /// 创建新的审计上下文
    pub fn new(
        tenant_id: i64,
        operation: TenantAuditOperation,
        result: AuditResult,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            operation,
            timestamp: chrono::Utc::now().timestamp(),
            result,
            detail: detail.into(),
        }
    }

    /// 记入审计日志（调用既有 `SqlAuditor::log`）
    pub fn log_to(&self, auditor: &sz_orm_audit::SqlAuditor) {
        let ctx = sz_orm_audit::SqlAuditContext {
            sql: format!(
                "[tenant={}] {} {} {}",
                self.tenant_id, self.operation, self.result, self.detail
            ),
            user: format!("tenant_{}", self.tenant_id),
            timestamp: self.timestamp,
        };
        auditor.log(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameterized_condition_new() {
        let cond = ParameterizedCondition::new("department_id = $1", vec![Value::I64(10)]);
        assert_eq!(cond.sql_fragment, "department_id = $1");
        assert_eq!(cond.params.len(), 1);
    }

    #[test]
    fn test_parameterized_condition_literal() {
        let cond = ParameterizedCondition::literal("is_active = true");
        assert_eq!(cond.sql_fragment, "is_active = true");
        assert!(cond.params.is_empty());
    }

    #[test]
    fn test_principal_has_role() {
        let principal = Principal::new(42, vec!["admin".to_string(), "manager".to_string()]);
        assert!(principal.has_role("admin"));
        assert!(principal.has_role("manager"));
        assert!(!principal.has_role("employee"));
    }

    #[test]
    fn test_row_level_security_policy() {
        let policy = RowLevelSecurityPolicy::new(
            "orders",
            ParameterizedCondition::new("department_id = $1", vec![Value::I64(10)]),
            Principal::new(42, vec!["manager".to_string()]),
        );
        assert_eq!(policy.table, "orders");
        assert_eq!(policy.filter_condition.sql_fragment, "department_id = $1");
        assert_eq!(policy.principal.tenant_id, 42);
    }

    #[test]
    fn test_permission_predicate_all() {
        let pred = PermissionPredicate::all();
        let roles = vec!["employee".to_string()];
        assert!(pred.applies_to(&roles));
    }

    #[test]
    fn test_permission_predicate_exempt() {
        let pred = PermissionPredicate::all().with_exempt(vec!["admin".to_string()]);
        let admin_roles = vec!["admin".to_string()];
        let employee_roles = vec!["employee".to_string()];
        assert!(!pred.applies_to(&admin_roles));
        assert!(pred.applies_to(&employee_roles));
    }

    #[test]
    fn test_permission_predicate_for_roles() {
        let pred = PermissionPredicate::for_roles(vec!["employee".to_string()]);
        let employee_roles = vec!["employee".to_string()];
        let manager_roles = vec!["manager".to_string()];
        assert!(pred.applies_to(&employee_roles));
        assert!(!pred.applies_to(&manager_roles));
    }

    #[test]
    fn test_column_masking_rule_mask() {
        let rule = ColumnMaskingRule::new(
            "users",
            "phone",
            MaskingFunction::Phone,
            PermissionPredicate::all(),
        );
        let masked = rule.mask("13812345678");
        assert!(masked.starts_with("138"));
        assert!(masked.ends_with("5678"));
        assert!(masked.contains('*'));
    }

    #[test]
    fn test_column_masking_rule_applies_to() {
        let rule = ColumnMaskingRule::new(
            "users",
            "phone",
            MaskingFunction::Phone,
            PermissionPredicate::all().with_exempt(vec!["admin".to_string()]),
        );
        assert!(!rule.applies_to(&["admin".to_string()]));
        assert!(rule.applies_to(&["employee".to_string()]));
    }

    #[test]
    fn test_tenant_audit_operation_display() {
        assert_eq!(TenantAuditOperation::ContextSet.to_string(), "context_set");
        assert_eq!(
            TenantAuditOperation::CrossTenantDenied.to_string(),
            "cross_tenant_denied"
        );
    }

    #[test]
    fn test_audit_result_display() {
        assert_eq!(AuditResult::Success.to_string(), "success");
        assert_eq!(AuditResult::Denied.to_string(), "denied");
    }

    #[test]
    fn test_tenant_audit_context_new() {
        let ctx = TenantAuditContext::new(
            42,
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "tenant context set for tenant 42",
        );
        assert_eq!(ctx.tenant_id, 42);
        assert_eq!(ctx.operation, TenantAuditOperation::ContextSet);
        assert_eq!(ctx.result, AuditResult::Success);
    }

    #[test]
    fn test_tenant_audit_context_log_to() {
        let auditor = sz_orm_audit::SqlAuditor::new();
        let ctx = TenantAuditContext::new(
            42,
            TenantAuditOperation::ContextSet,
            AuditResult::Success,
            "test",
        );
        ctx.log_to(&auditor);
        let logs = auditor.get_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].sql.contains("tenant=42"));
        assert!(logs[0].sql.contains("context_set"));
    }
}
