//! 数据权限拦截器（Data Permission Interceptor）
//!
//! 对应文档 6.8 节改进项 25（数据权限拦截器）。
//!
//! # 核心概念
//!
//! - **PermissionContext**：权限上下文（当前用户、租户、部门）
//! - **PermissionRule**：数据权限规则 trait（每个规则返回一个 WHERE 子句片段）
//! - **DataPermissionInterceptor**：拦截器，注册多个规则并应用到 SQL
//! - 内置规则：`TenantIsolation`、`OwnerOnly`、`DepartmentScope`、`CustomCondition`
//!
//! # 设计灵感
//!
//! - MyBatis-Plus `DataPermissionInterceptor`
//! - Hibernate `@Filter` / `@FilterDef`
//! - Rails `default_scope`
//! - Spring Security `@PreAuthorize`
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::data_permission::{
//!     DataPermissionInterceptor, PermissionContext, TenantIsolation, OwnerOnly,
//! };
//!
//! // 1. 创建拦截器
//! let mut interceptor = DataPermissionInterceptor::new();
//! interceptor.register(Box::new(TenantIsolation::new("tenant_id")));
//! interceptor.register(Box::new(OwnerOnly::new("user_id")));
//!
//! // 2. 构建权限上下文
//! let ctx = PermissionContext::new()
//!     .with_user_id(100)
//!     .with_tenant_id(5);
//!
//! // 3. 应用到 SQL（SELECT * FROM orders → SELECT * FROM orders WHERE tenant_id = 5 AND user_id = 100）
//! let sql = interceptor.apply_to_select("SELECT * FROM orders", &ctx);
//! ```

use std::collections::HashMap;

// ============================================================================
// PermissionContext — 权限上下文
// ============================================================================

/// 权限上下文 — 当前请求的用户/租户/部门信息
///
/// 由调用方（通常是中间件）在请求开始时构建，并传递给 `DataPermissionInterceptor`。
#[derive(Debug, Clone, Default)]
pub struct PermissionContext {
    /// 当前用户 ID
    pub user_id: Option<i64>,
    /// 当前租户 ID
    pub tenant_id: Option<i64>,
    /// 当前部门 ID
    pub dept_id: Option<i64>,
    /// 用户角色列表（用于角色级别的权限规则）
    pub roles: Vec<String>,
    /// 用户权限列表（细粒度权限码）
    pub permissions: Vec<String>,
    /// 扩展数据（如自定义字段值）
    pub extras: HashMap<String, String>,
}

impl PermissionContext {
    /// 创建空上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置用户 ID
    pub fn with_user_id(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// 设置租户 ID
    pub fn with_tenant_id(mut self, tenant_id: i64) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// 设置部门 ID
    pub fn with_dept_id(mut self, dept_id: i64) -> Self {
        self.dept_id = Some(dept_id);
        self
    }

    /// 设置角色列表
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    /// 设置权限列表
    pub fn with_permissions(mut self, perms: Vec<String>) -> Self {
        self.permissions = perms;
        self
    }

    /// 添加扩展数据
    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extras.insert(key.into(), value.into());
        self
    }

    /// 检查是否拥有指定角色
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// 检查是否拥有指定权限
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm)
    }

    /// 是否为管理员（拥有 "admin" 角色）
    pub fn is_admin(&self) -> bool {
        self.has_role("admin") || self.has_role("super_admin")
    }
}

// ============================================================================
// PermissionRule — 数据权限规则 trait
// ============================================================================

/// 数据权限规则 trait
///
/// 每个规则负责生成一段 WHERE 子句（不含 `WHERE` 关键字本身），
/// 拦截器会将多个规则的子句用 `AND` 连接，自动追加到原 SQL。
///
/// # 规则返回值
/// - `Ok(Some(clause))`：应用规则，返回 WHERE 子句片段（如 `"tenant_id = 5"`）
/// - `Ok(None)`：规则不适用（如管理员跳过、上下文缺失必要字段）
/// - `Err(e)`：规则执行失败（如配置错误）
pub trait PermissionRule: Send + Sync {
    /// 规则名称（用于调试、日志）
    fn name(&self) -> &'static str;

    /// 生成 WHERE 子句片段
    fn apply(&self, ctx: &PermissionContext) -> Result<Option<String>, PermissionError>;
}

// ============================================================================
// PermissionError — 权限错误类型
// ============================================================================

/// 数据权限错误类型
#[derive(Debug)]
pub enum PermissionError {
    /// 上下文缺失必要字段
    MissingContext {
        /// 字段名
        field: &'static str,
    },
    /// 规则配置错误
    ConfigError(String),
    /// 不允许的操作（如非管理员尝试访问其他租户数据）
    Forbidden(String),
}

impl std::fmt::Display for PermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionError::MissingContext { field } => {
                write!(f, "Permission context missing field: `{}`", field)
            }
            PermissionError::ConfigError(msg) => {
                write!(f, "Permission rule config error: {}", msg)
            }
            PermissionError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
        }
    }
}

impl std::error::Error for PermissionError {}

/// 权限结果
pub type PermissionResult<T> = Result<T, PermissionError>;

// ============================================================================
// 内置规则：TenantIsolation — 租户隔离
// ============================================================================

/// 租户隔离规则 — 自动追加 `tenant_id = ?` 条件
///
/// 对应 MyBatis-Plus `TenantLineHandler` / Hibernate `@TenantId`。
///
/// # 示例
///
/// ```
/// use sz_orm_core::data_permission::{TenantIsolation, PermissionRule, PermissionContext};
///
/// let rule = TenantIsolation::new("tenant_id");
/// let ctx = PermissionContext::new().with_tenant_id(5);
/// let clause = rule.apply(&ctx).unwrap().unwrap();
/// assert_eq!(clause, "tenant_id = 5");
/// ```
pub struct TenantIsolation {
    /// 租户字段名（默认 "tenant_id"）
    pub field: &'static str,
}

impl TenantIsolation {
    /// 创建租户隔离规则
    pub fn new(field: &'static str) -> Self {
        Self { field }
    }

    /// 使用默认字段名 "tenant_id"
    pub fn default_field() -> Self {
        Self::new("tenant_id")
    }
}

impl PermissionRule for TenantIsolation {
    fn name(&self) -> &'static str {
        "TenantIsolation"
    }

    fn apply(&self, ctx: &PermissionContext) -> PermissionResult<Option<String>> {
        // 管理员跳过租户隔离（可访问所有租户数据）
        if ctx.is_admin() {
            return Ok(None);
        }
        match ctx.tenant_id {
            Some(tid) => Ok(Some(format!("{} = {}", self.field, tid))),
            None => Err(PermissionError::MissingContext { field: "tenant_id" }),
        }
    }
}

// ============================================================================
// 内置规则：OwnerOnly — 仅所有者可访问
// ============================================================================

/// 仅所有者可访问规则 — 自动追加 `user_id = ?` 条件
///
/// 对应 Rails `current_user` scope / Spring Security `@PostFilter`。
///
/// # 示例
///
/// ```
/// use sz_orm_core::data_permission::{OwnerOnly, PermissionRule, PermissionContext};
///
/// let rule = OwnerOnly::new("user_id");
/// let ctx = PermissionContext::new().with_user_id(100);
/// let clause = rule.apply(&ctx).unwrap().unwrap();
/// assert_eq!(clause, "user_id = 100");
/// ```
pub struct OwnerOnly {
    /// 所有者字段名（默认 "user_id"）
    pub field: &'static str,
}

impl OwnerOnly {
    /// 创建所有者规则
    pub fn new(field: &'static str) -> Self {
        Self { field }
    }

    /// 使用默认字段名 "user_id"
    pub fn default_field() -> Self {
        Self::new("user_id")
    }
}

impl PermissionRule for OwnerOnly {
    fn name(&self) -> &'static str {
        "OwnerOnly"
    }

    fn apply(&self, ctx: &PermissionContext) -> PermissionResult<Option<String>> {
        // 管理员可访问所有数据
        if ctx.is_admin() {
            return Ok(None);
        }
        match ctx.user_id {
            Some(uid) => Ok(Some(format!("{} = {}", self.field, uid))),
            None => Err(PermissionError::MissingContext { field: "user_id" }),
        }
    }
}

// ============================================================================
// 内置规则：DepartmentScope — 部门范围
// ============================================================================

/// 部门范围规则 — 自动追加 `dept_id IN (...)` 或 `dept_id = ?` 条件
///
/// 对应 Spring Security `@DepartmentScope` / MyBatis-Plus `dept_id in (...)`。
///
/// # 示例
///
/// ```
/// use sz_orm_core::data_permission::{DepartmentScope, PermissionRule, PermissionContext};
///
/// let rule = DepartmentScope::new("dept_id");
/// let ctx = PermissionContext::new().with_dept_id(3);
/// let clause = rule.apply(&ctx).unwrap().unwrap();
/// assert_eq!(clause, "dept_id = 3");
/// ```
pub struct DepartmentScope {
    /// 部门字段名（默认 "dept_id"）
    pub field: &'static str,
    /// 子部门 ID 列表（如部门树展开后所有子孙部门）
    pub include_sub_depts: Vec<i64>,
}

impl DepartmentScope {
    /// 创建部门范围规则
    pub fn new(field: &'static str) -> Self {
        Self {
            field,
            include_sub_depts: Vec::new(),
        }
    }

    /// 使用默认字段名 "dept_id"
    pub fn default_field() -> Self {
        Self::new("dept_id")
    }

    /// 包含子部门
    pub fn with_sub_depts(mut self, depts: Vec<i64>) -> Self {
        self.include_sub_depts = depts;
        self
    }
}

impl PermissionRule for DepartmentScope {
    fn name(&self) -> &'static str {
        "DepartmentScope"
    }

    fn apply(&self, ctx: &PermissionContext) -> PermissionResult<Option<String>> {
        if ctx.is_admin() {
            return Ok(None);
        }
        match ctx.dept_id {
            Some(did) => {
                if self.include_sub_depts.is_empty() {
                    Ok(Some(format!("{} = {}", self.field, did)))
                } else {
                    // dept_id IN (did, sub1, sub2, ...)
                    let mut all_depts = vec![did];
                    all_depts.extend(self.include_sub_depts.iter().copied());
                    let list = all_depts
                        .iter()
                        .map(|d| d.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    Ok(Some(format!("{} IN ({})", self.field, list)))
                }
            }
            None => Err(PermissionError::MissingContext { field: "dept_id" }),
        }
    }
}

// ============================================================================
// 内置规则：CustomCondition — 自定义条件
// ============================================================================

/// 条件生成闭包类型
pub type ConditionGenerator = Box<dyn Fn(&PermissionContext) -> Option<String> + Send + Sync>;

/// 自定义条件规则 — 通过闭包动态生成 WHERE 子句
///
/// 适用于业务特定的权限逻辑（如"只能查看自己创建的草稿状态订单"）。
///
/// # 安全警告
///
/// 闭包返回的 String 会**直接拼接**到 SQL 中，调用方必须确保内容来源可信，
/// **严禁将用户输入直接拼接**到返回的字符串中（否则会引入 SQL 注入风险）。
/// 若需要使用用户输入，应使用参数化查询（`?` 占位符 + bind 参数）。
///
/// # 示例
///
/// ```
/// use sz_orm_core::data_permission::{CustomCondition, PermissionRule, PermissionContext};
///
/// let rule = CustomCondition::new("status_filter", |ctx| {
///     if ctx.is_admin() {
///         None
///     } else {
///         Some("status != 'draft'".to_string())
///     }
/// });
/// let ctx = PermissionContext::new().with_user_id(1);
/// let clause = rule.apply(&ctx).unwrap().unwrap();
/// assert_eq!(clause, "status != 'draft'");
/// ```
pub struct CustomCondition {
    /// 规则名称
    pub name_str: &'static str,
    /// 条件生成闭包
    pub generator: ConditionGenerator,
}

impl CustomCondition {
    /// 创建自定义条件规则
    pub fn new(
        name: &'static str,
        generator: impl Fn(&PermissionContext) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name_str: name,
            generator: Box::new(generator),
        }
    }
}

impl PermissionRule for CustomCondition {
    fn name(&self) -> &'static str {
        self.name_str
    }

    fn apply(&self, ctx: &PermissionContext) -> PermissionResult<Option<String>> {
        Ok((self.generator)(ctx))
    }
}

// ============================================================================
// DataPermissionInterceptor — 数据权限拦截器
// ============================================================================

/// 数据权限拦截器 — 注册多个规则并应用到 SQL
///
/// # 工作流程
///
/// 1. 调用方注册多个 `PermissionRule`
/// 2. 在执行 SQL 前，调用 `apply_to_select` / `apply_to_update` / `apply_to_delete`
/// 3. 拦截器按注册顺序依次调用每个 `PermissionRule.apply()`
/// 4. 将所有非 None 的子句用 `AND` 连接，追加到原 SQL 的 WHERE 子句
///
/// # 示例
///
/// ```
/// use sz_orm_core::data_permission::{
///     DataPermissionInterceptor, PermissionContext, TenantIsolation, OwnerOnly,
/// };
///
/// let mut interceptor = DataPermissionInterceptor::new();
/// interceptor.register(Box::new(TenantIsolation::default_field()));
/// interceptor.register(Box::new(OwnerOnly::default_field()));
///
/// let ctx = PermissionContext::new().with_user_id(100).with_tenant_id(5);
/// let sql = interceptor.apply_to_select("SELECT * FROM orders", &ctx).unwrap();
/// assert!(sql.contains("WHERE"));
/// assert!(sql.contains("tenant_id = 5"));
/// assert!(sql.contains("user_id = 100"));
/// ```
pub struct DataPermissionInterceptor {
    rules: Vec<Box<dyn PermissionRule>>,
}

impl DataPermissionInterceptor {
    /// 创建空拦截器
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 注册规则
    pub fn register(&mut self, rule: Box<dyn PermissionRule>) {
        self.rules.push(rule);
    }

    /// 已注册规则数量
    pub fn count(&self) -> usize {
        self.rules.len()
    }

    /// 列出所有规则名称
    pub fn names(&self) -> Vec<&'static str> {
        self.rules.iter().map(|r| r.name()).collect()
    }

    /// 收集所有规则的 WHERE 子句（按注册顺序，跳过 None）
    pub fn collect_clauses(&self, ctx: &PermissionContext) -> PermissionResult<Vec<String>> {
        let mut clauses = Vec::new();
        for rule in &self.rules {
            if let Some(clause) = rule.apply(ctx)? {
                if !clause.trim().is_empty() {
                    clauses.push(clause);
                }
            }
        }
        Ok(clauses)
    }

    /// 将规则应用到 SELECT 语句
    ///
    /// - 原 SQL 无 WHERE 子句：追加 `WHERE clause1 AND clause2 ...`
    /// - 原 SQL 有 WHERE 子句：在 WHERE 后追加 `(原条件) AND (clause1 AND clause2 ...)`
    pub fn apply_to_select(&self, sql: &str, ctx: &PermissionContext) -> PermissionResult<String> {
        let clauses = self.collect_clauses(ctx)?;
        if clauses.is_empty() {
            return Ok(sql.to_string());
        }
        Ok(append_where_clauses(sql, &clauses))
    }

    /// 将规则应用到 UPDATE 语句
    pub fn apply_to_update(&self, sql: &str, ctx: &PermissionContext) -> PermissionResult<String> {
        // 复用 SELECT 的逻辑（WHERE 子句追加）
        self.apply_to_select(sql, ctx)
    }

    /// 将规则应用到 DELETE 语句
    pub fn apply_to_delete(&self, sql: &str, ctx: &PermissionContext) -> PermissionResult<String> {
        self.apply_to_select(sql, ctx)
    }
}

impl Default for DataPermissionInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 辅助函数：append_where_clauses — 追加 WHERE 子句
// ============================================================================

/// 将权限子句追加到 SQL 的 WHERE 部分
///
/// - 若 SQL 无 WHERE：追加 ` WHERE clauses`
/// - 若 SQL 已有 WHERE：在 WHERE 后追加 ` AND (clauses)`
/// - 若 SQL 已有 GROUP BY/ORDER BY/LIMIT：在它们之前追加
pub fn append_where_clauses(sql: &str, clauses: &[String]) -> String {
    if clauses.is_empty() {
        return sql.to_string();
    }

    let combined = clauses.join(" AND ");
    let upper = sql.to_uppercase();

    // 查找 WHERE 关键字位置（独立词）
    let where_pos = find_keyword(&upper, "WHERE");

    // 查找其他可能的关键字位置（用于在 GROUP BY/ORDER BY/LIMIT 前插入）
    let group_by_pos = find_keyword(&upper, "GROUP BY");
    let order_by_pos = find_keyword(&upper, "ORDER BY");
    let limit_pos = find_keyword(&upper, "LIMIT");
    let having_pos = find_keyword(&upper, "HAVING");

    // 找到最早出现的"末尾关键字"
    let end_pos = [group_by_pos, order_by_pos, limit_pos, having_pos]
        .iter()
        .filter_map(|x| *x)
        .min();

    if let Some(wp) = where_pos {
        // 已有 WHERE 子句
        let insert_pos = end_pos.unwrap_or(sql.len());
        let before = &sql[..wp + 5]; // 包含 "WHERE"
        let existing_clause = &sql[wp + 5..insert_pos];
        let after = &sql[insert_pos..];

        // 在 WHERE 后追加 (existing) AND (combined)
        let trimmed_existing = existing_clause.trim();
        if trimmed_existing.is_empty() {
            format!("{} {}{}", before, combined, after)
        } else {
            format!(
                "{} ({} ) AND ({}){}",
                before, trimmed_existing, combined, after
            )
        }
    } else {
        // 无 WHERE 子句
        let insert_pos = end_pos.unwrap_or(sql.len());
        let before = &sql[..insert_pos];
        let after = &sql[insert_pos..];
        let trimmed = before.trim_end();
        let sep = if trimmed.is_empty() { "" } else { " " };
        format!("{}{}WHERE {}{}", trimmed, sep, combined, after)
    }
}

/// 在 SQL 中查找指定关键字的位置（独立词匹配，大小写不敏感）
fn find_keyword(sql: &str, keyword: &str) -> Option<usize> {
    let upper_sql = sql.to_uppercase();
    let kw_upper = keyword.to_uppercase();
    let kw_len = kw_upper.len();
    if kw_len == 0 || upper_sql.len() < kw_len {
        return None;
    }

    let bytes = upper_sql.as_bytes();
    let kw_bytes = kw_upper.as_bytes();

    let mut i = 0;
    while i + kw_len <= bytes.len() {
        if &bytes[i..i + kw_len] == kw_bytes {
            // 检查前一个字符是否为单词边界
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            // 检查后一个字符是否为单词边界
            let next_idx = i + kw_len;
            let next_ok = next_idx >= bytes.len()
                || !bytes[next_idx].is_ascii_alphanumeric() && bytes[next_idx] != b'_';
            if prev_ok && next_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== PermissionContext 测试 =====

    #[test]
    fn test_permission_context_builders() {
        let ctx = PermissionContext::new()
            .with_user_id(100)
            .with_tenant_id(5)
            .with_dept_id(3)
            .with_roles(vec!["user".to_string()])
            .with_permissions(vec!["read".to_string()])
            .with_extra("region", "cn");

        assert_eq!(ctx.user_id, Some(100));
        assert_eq!(ctx.tenant_id, Some(5));
        assert_eq!(ctx.dept_id, Some(3));
        assert!(ctx.has_role("user"));
        assert!(!ctx.has_role("admin"));
        assert!(ctx.has_permission("read"));
        assert_eq!(ctx.extras.get("region"), Some(&"cn".to_string()));
    }

    #[test]
    fn test_permission_context_is_admin() {
        let admin_ctx = PermissionContext::new().with_roles(vec!["admin".to_string()]);
        assert!(admin_ctx.is_admin());

        let super_admin_ctx = PermissionContext::new().with_roles(vec!["super_admin".to_string()]);
        assert!(super_admin_ctx.is_admin());

        let user_ctx = PermissionContext::new().with_roles(vec!["user".to_string()]);
        assert!(!user_ctx.is_admin());
    }

    // ===== TenantIsolation 测试 =====

    #[test]
    fn test_tenant_isolation_applies() {
        let rule = TenantIsolation::default_field();
        let ctx = PermissionContext::new().with_tenant_id(5);
        let clause = rule.apply(&ctx).unwrap().unwrap();
        assert_eq!(clause, "tenant_id = 5");
    }

    #[test]
    fn test_tenant_isolation_skips_admin() {
        let rule = TenantIsolation::default_field();
        let ctx = PermissionContext::new()
            .with_tenant_id(5)
            .with_roles(vec!["admin".to_string()]);
        let clause = rule.apply(&ctx).unwrap();
        assert!(clause.is_none());
    }

    #[test]
    fn test_tenant_isolation_missing_context() {
        let rule = TenantIsolation::default_field();
        let ctx = PermissionContext::new();
        let result = rule.apply(&ctx);
        assert!(matches!(
            result,
            Err(PermissionError::MissingContext { field }) if field == "tenant_id"
        ));
    }

    #[test]
    fn test_tenant_isolation_custom_field() {
        let rule = TenantIsolation::new("org_id");
        let ctx = PermissionContext::new().with_tenant_id(99);
        let clause = rule.apply(&ctx).unwrap().unwrap();
        assert_eq!(clause, "org_id = 99");
    }

    // ===== OwnerOnly 测试 =====

    #[test]
    fn test_owner_only_applies() {
        let rule = OwnerOnly::default_field();
        let ctx = PermissionContext::new().with_user_id(100);
        let clause = rule.apply(&ctx).unwrap().unwrap();
        assert_eq!(clause, "user_id = 100");
    }

    #[test]
    fn test_owner_only_skips_admin() {
        let rule = OwnerOnly::default_field();
        let ctx = PermissionContext::new()
            .with_user_id(100)
            .with_roles(vec!["admin".to_string()]);
        let clause = rule.apply(&ctx).unwrap();
        assert!(clause.is_none());
    }

    #[test]
    fn test_owner_only_missing_context() {
        let rule = OwnerOnly::default_field();
        let ctx = PermissionContext::new();
        let result = rule.apply(&ctx);
        assert!(matches!(
            result,
            Err(PermissionError::MissingContext { field }) if field == "user_id"
        ));
    }

    // ===== DepartmentScope 测试 =====

    #[test]
    fn test_department_scope_simple() {
        let rule = DepartmentScope::default_field();
        let ctx = PermissionContext::new().with_dept_id(3);
        let clause = rule.apply(&ctx).unwrap().unwrap();
        assert_eq!(clause, "dept_id = 3");
    }

    #[test]
    fn test_department_scope_with_sub_depts() {
        let rule = DepartmentScope::default_field().with_sub_depts(vec![10, 11, 12]);
        let ctx = PermissionContext::new().with_dept_id(3);
        let clause = rule.apply(&ctx).unwrap().unwrap();
        assert_eq!(clause, "dept_id IN (3, 10, 11, 12)");
    }

    #[test]
    fn test_department_scope_skips_admin() {
        let rule = DepartmentScope::default_field();
        let ctx = PermissionContext::new()
            .with_dept_id(3)
            .with_roles(vec!["admin".to_string()]);
        let clause = rule.apply(&ctx).unwrap();
        assert!(clause.is_none());
    }

    // ===== CustomCondition 测试 =====

    #[test]
    fn test_custom_condition_returns_clause() {
        let rule = CustomCondition::new("draft_filter", |ctx| {
            if ctx.is_admin() {
                None
            } else {
                Some("status != 'draft'".to_string())
            }
        });
        let ctx = PermissionContext::new().with_user_id(1);
        let clause = rule.apply(&ctx).unwrap().unwrap();
        assert_eq!(clause, "status != 'draft'");
    }

    #[test]
    fn test_custom_condition_skips_admin() {
        let rule = CustomCondition::new("draft_filter", |ctx| {
            if ctx.is_admin() {
                None
            } else {
                Some("status != 'draft'".to_string())
            }
        });
        let ctx = PermissionContext::new().with_roles(vec!["admin".to_string()]);
        let clause = rule.apply(&ctx).unwrap();
        assert!(clause.is_none());
    }

    // ===== DataPermissionInterceptor 测试 =====

    #[test]
    fn test_interceptor_no_rules_returns_original_sql() {
        let interceptor = DataPermissionInterceptor::new();
        let ctx = PermissionContext::new().with_user_id(1);
        let sql = interceptor
            .apply_to_select("SELECT * FROM users", &ctx)
            .unwrap();
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn test_interceptor_single_rule_no_where() {
        let mut interceptor = DataPermissionInterceptor::new();
        interceptor.register(Box::new(TenantIsolation::default_field()));

        let ctx = PermissionContext::new().with_tenant_id(5);
        let sql = interceptor
            .apply_to_select("SELECT * FROM orders", &ctx)
            .unwrap();
        assert!(sql.contains("WHERE tenant_id = 5"));
    }

    #[test]
    fn test_interceptor_multiple_rules_no_where() {
        let mut interceptor = DataPermissionInterceptor::new();
        interceptor.register(Box::new(TenantIsolation::default_field()));
        interceptor.register(Box::new(OwnerOnly::default_field()));

        let ctx = PermissionContext::new().with_tenant_id(5).with_user_id(100);
        let sql = interceptor
            .apply_to_select("SELECT * FROM orders", &ctx)
            .unwrap();
        assert!(sql.contains("tenant_id = 5"));
        assert!(sql.contains("user_id = 100"));
        assert!(sql.contains("AND"));
    }

    #[test]
    fn test_interceptor_appends_to_existing_where() {
        let mut interceptor = DataPermissionInterceptor::new();
        interceptor.register(Box::new(TenantIsolation::default_field()));

        let ctx = PermissionContext::new().with_tenant_id(5);
        let sql = interceptor
            .apply_to_select("SELECT * FROM orders WHERE status = 'active'", &ctx)
            .unwrap();
        // 应保留原 WHERE 条件并追加
        assert!(sql.contains("status = 'active'"));
        assert!(sql.contains("tenant_id = 5"));
        assert!(sql.contains("AND"));
    }

    #[test]
    fn test_interceptor_admin_skips_all_rules() {
        let mut interceptor = DataPermissionInterceptor::new();
        interceptor.register(Box::new(TenantIsolation::default_field()));
        interceptor.register(Box::new(OwnerOnly::default_field()));

        let ctx = PermissionContext::new()
            .with_tenant_id(5)
            .with_user_id(100)
            .with_roles(vec!["admin".to_string()]);

        let sql = interceptor
            .apply_to_select("SELECT * FROM orders", &ctx)
            .unwrap();
        // 管理员跳过所有规则，SQL 不变
        assert_eq!(sql, "SELECT * FROM orders");
    }

    #[test]
    fn test_interceptor_apply_to_update() {
        let mut interceptor = DataPermissionInterceptor::new();
        interceptor.register(Box::new(TenantIsolation::default_field()));

        let ctx = PermissionContext::new().with_tenant_id(5);
        let sql = interceptor
            .apply_to_update("UPDATE orders SET status = 'shipped' WHERE id = 1", &ctx)
            .unwrap();
        assert!(sql.contains("id = 1"));
        assert!(sql.contains("tenant_id = 5"));
    }

    #[test]
    fn test_interceptor_apply_to_delete() {
        let mut interceptor = DataPermissionInterceptor::new();
        interceptor.register(Box::new(OwnerOnly::default_field()));

        let ctx = PermissionContext::new().with_user_id(100);
        let sql = interceptor
            .apply_to_delete("DELETE FROM orders WHERE id = 1", &ctx)
            .unwrap();
        assert!(sql.contains("id = 1"));
        assert!(sql.contains("user_id = 100"));
    }

    #[test]
    fn test_interceptor_count_and_names() {
        let mut interceptor = DataPermissionInterceptor::new();
        assert_eq!(interceptor.count(), 0);
        interceptor.register(Box::new(TenantIsolation::default_field()));
        interceptor.register(Box::new(OwnerOnly::default_field()));
        assert_eq!(interceptor.count(), 2);
        let names = interceptor.names();
        assert!(names.contains(&"TenantIsolation"));
        assert!(names.contains(&"OwnerOnly"));
    }

    // ===== append_where_clauses 测试 =====

    #[test]
    fn test_append_where_no_existing_where_no_clauses() {
        let sql = append_where_clauses("SELECT * FROM users", &[]);
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn test_append_where_no_existing_where_with_clauses() {
        let sql = append_where_clauses("SELECT * FROM users", &["tenant_id = 5".to_string()]);
        assert_eq!(sql, "SELECT * FROM users WHERE tenant_id = 5");
    }

    #[test]
    fn test_append_where_existing_where_with_clauses() {
        let sql = append_where_clauses(
            "SELECT * FROM users WHERE id = 1",
            &["tenant_id = 5".to_string()],
        );
        assert!(sql.contains("id = 1"));
        assert!(sql.contains("tenant_id = 5"));
        assert!(sql.contains("AND"));
    }

    #[test]
    fn test_append_where_inserts_before_group_by() {
        let sql = append_where_clauses(
            "SELECT * FROM users GROUP BY dept_id",
            &["tenant_id = 5".to_string()],
        );
        // WHERE 应在 GROUP BY 之前
        let where_idx = sql.to_uppercase().find("WHERE").unwrap();
        let group_by_idx = sql.to_uppercase().find("GROUP BY").unwrap();
        assert!(where_idx < group_by_idx);
    }

    #[test]
    fn test_append_where_inserts_before_order_by() {
        let sql = append_where_clauses(
            "SELECT * FROM users ORDER BY id",
            &["tenant_id = 5".to_string()],
        );
        let where_idx = sql.to_uppercase().find("WHERE").unwrap();
        let order_by_idx = sql.to_uppercase().find("ORDER BY").unwrap();
        assert!(where_idx < order_by_idx);
    }

    #[test]
    fn test_append_where_inserts_before_limit() {
        let sql = append_where_clauses(
            "SELECT * FROM users LIMIT 10",
            &["tenant_id = 5".to_string()],
        );
        let where_idx = sql.to_uppercase().find("WHERE").unwrap();
        let limit_idx = sql.to_uppercase().find("LIMIT").unwrap();
        assert!(where_idx < limit_idx);
    }

    // ===== PermissionError Display 测试 =====

    #[test]
    fn test_permission_error_display_missing_context() {
        let e = PermissionError::MissingContext { field: "user_id" };
        let s = format!("{}", e);
        assert!(s.contains("user_id"));
        assert!(s.contains("missing"));
    }

    #[test]
    fn test_permission_error_display_config_error() {
        let e = PermissionError::ConfigError("invalid rule".to_string());
        let s = format!("{}", e);
        assert!(s.contains("invalid rule"));
    }

    #[test]
    fn test_permission_error_display_forbidden() {
        let e = PermissionError::Forbidden("cross-tenant access".to_string());
        let s = format!("{}", e);
        assert!(s.contains("Forbidden"));
        assert!(s.contains("cross-tenant access"));
    }

    // ===== find_keyword 测试 =====

    #[test]
    fn test_find_keyword_basic() {
        // "SELECT * FROM users WHERE id = 1" — WHERE 在位置 20
        assert_eq!(
            find_keyword("SELECT * FROM users WHERE id = 1", "WHERE"),
            Some(20)
        );
    }

    #[test]
    fn test_find_keyword_not_found() {
        assert_eq!(find_keyword("SELECT * FROM users", "WHERE"), None);
    }

    #[test]
    fn test_find_keyword_word_boundary() {
        // 不应匹配字段名中的子串
        assert_eq!(find_keyword("SELECT somewhere FROM t", "WHERE"), None);
    }

    #[test]
    fn test_find_keyword_case_insensitive() {
        // "select * from t where id = 1" — where 在位置 16
        assert_eq!(
            find_keyword("select * from t where id = 1", "WHERE"),
            Some(16)
        );
    }

    // ===== 默认 Default 测试 =====

    #[test]
    fn test_interceptor_default_is_empty() {
        let i = DataPermissionInterceptor::default();
        assert_eq!(i.count(), 0);
    }
}
