//! 权限审计模块
//!
//! 分析 SQL 查询的权限使用，识别过度授权并建议最小权限。
//! 例如应用使用 root 账户、SELECT * 但只用 2 列。

use serde::{Deserialize, Serialize};

/// 数据库账户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbAccount {
    /// 账户名
    pub username: String,
    /// 账户角色列表
    pub roles: Vec<String>,
    /// 是否为超级用户（root/admin/superuser）
    pub is_super_user: bool,
    /// 已授予权限列表
    pub granted_privileges: Vec<String>,
}

impl DbAccount {
    /// 创建一个账户
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            roles: Vec::new(),
            is_super_user: false,
            granted_privileges: Vec::new(),
        }
    }

    /// 设置角色
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.is_super_user = roles.iter().any(|r| {
            let lower = r.to_lowercase();
            lower == "root" || lower == "admin" || lower == "superuser" || lower == "super"
        });
        self.roles = roles;
        self
    }

    /// 设置已授予权限
    pub fn with_privileges(mut self, privileges: Vec<String>) -> Self {
        self.granted_privileges = privileges;
        self
    }
}

/// SQL 查询使用的列信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryUsage {
    /// SQL 文本
    pub sql: String,
    /// 查询涉及的表
    pub tables: Vec<String>,
    /// SELECT 子句中引用的列
    pub referenced_columns: Vec<String>,
    /// 是否使用 SELECT *
    pub uses_select_star: bool,
    /// 是否包含 WHERE 条件
    pub has_where: bool,
    /// 是否包含 JOIN
    pub has_join: bool,
}

impl QueryUsage {
    /// 从 SQL 文本提取查询使用信息
    pub fn from_sql(sql: &str) -> Self {
        let lower = sql.to_lowercase();
        let uses_select_star = lower.contains("select *");
        let has_where = lower.contains(" where ");
        let has_join = lower.contains(" join ");

        let tables = extract_tables_from_sql(&lower);
        let referenced_columns = if uses_select_star {
            Vec::new()
        } else {
            extract_select_columns(&lower)
        };

        Self {
            sql: sql.to_string(),
            tables,
            referenced_columns,
            uses_select_star,
            has_where,
            has_join,
        }
    }
}

fn extract_tables_from_sql(lower_sql: &str) -> Vec<String> {
    let mut tables = Vec::new();
    for keyword in ["from ", "join "] {
        let mut pos = 0;
        while let Some(idx) = lower_sql[pos..].find(keyword) {
            let start = pos + idx + keyword.len();
            let rest = &lower_sql[start..];
            let table_end = rest
                .find(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(')
                .unwrap_or(rest.len());
            let table = rest[..table_end].trim();
            if !table.is_empty()
                && !table.starts_with('(')
                && table != "where"
                && table != "on"
                && table != "as"
            {
                tables.push(table.to_string());
            }
            pos = start + table_end;
            if pos >= lower_sql.len() {
                break;
            }
        }
    }
    tables
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

fn extract_select_columns(lower_sql: &str) -> Vec<String> {
    let select_start = match lower_sql.find("select ") {
        Some(idx) => idx + 7,
        None => return Vec::new(),
    };
    let select_end = lower_sql[select_start..]
        .find(" from ")
        .map(|e| select_start + e)
        .unwrap_or(lower_sql.len());
    let select_clause = &lower_sql[select_start..select_end];

    select_clause
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "*")
        .collect()
}

/// 权限问题级别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionIssueSeverity {
    /// 信息
    Info,
    /// 警告
    Warning,
    /// 严重
    Critical,
}

/// 权限审计发现
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionFinding {
    /// 问题级别
    pub severity: PermissionIssueSeverity,
    /// 问题标题
    pub title: String,
    /// 问题详情
    pub description: String,
    /// 修复建议
    pub recommendation: String,
}

/// 权限审计结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionAuditResult {
    /// 审计的账户
    pub account: String,
    /// 发现的权限问题
    pub findings: Vec<PermissionFinding>,
    /// 建议的最小权限集
    pub minimal_privileges: Vec<String>,
    /// 建议的只读账户名
    pub suggested_readonly_account: Option<String>,
    /// 整体安全评分（0-100，越高越安全）
    pub security_score: f64,
}

/// 权限审计器
///
/// 分析 SQL 查询的权限使用，识别过度授权并建议最小权限。
pub struct PermissionAuditor {
    /// 只读权限集合（用于建议最小权限时参考）
    #[allow(dead_code)]
    readonly_privileges: Vec<String>,
}

impl Default for PermissionAuditor {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionAuditor {
    /// 创建权限审计器
    pub fn new() -> Self {
        Self {
            readonly_privileges: vec![
                "SELECT".to_string(),
                "SHOW".to_string(),
                "DESCRIBE".to_string(),
                "EXPLAIN".to_string(),
            ],
        }
    }

    /// 审计权限使用
    ///
    /// # 参数
    /// - `account`: 数据库账户信息
    /// - `usage`: SQL 查询使用信息
    ///
    /// # 返回
    ///
    /// [`PermissionAuditResult`] 包含发现的问题 + 建议的最小权限。
    pub fn audit_permissions(
        &self,
        account: &DbAccount,
        usage: &QueryUsage,
    ) -> PermissionAuditResult {
        let mut findings = Vec::new();

        self.check_super_user(account, &mut findings);
        self.check_excessive_privileges(account, &mut findings);
        self.check_select_star(usage, &mut findings);
        self.check_missing_where(usage, &mut findings);

        let minimal_privileges = self.compute_minimal_privileges(usage);
        let suggested_readonly_account = if account.is_super_user
            || findings
                .iter()
                .any(|f| f.severity == PermissionIssueSeverity::Critical)
        {
            Some(format!("{}_readonly", account.username))
        } else {
            None
        };
        let security_score = self.compute_security_score(account, &findings);

        PermissionAuditResult {
            account: account.username.clone(),
            findings,
            minimal_privileges,
            suggested_readonly_account,
            security_score,
        }
    }

    fn check_super_user(&self, account: &DbAccount, findings: &mut Vec<PermissionFinding>) {
        if account.is_super_user {
            findings.push(PermissionFinding {
                severity: PermissionIssueSeverity::Critical,
                title: "使用超级用户账户".to_string(),
                description: format!(
                    "应用使用超级用户账户 '{}'（角色: {}），违反最小权限原则",
                    account.username,
                    account.roles.join(", ")
                ),
                recommendation: format!(
                    "创建专用应用账户，仅授予必要的 SELECT 权限，禁止使用 root/admin/superuser"
                ),
            });
        }
    }

    fn check_excessive_privileges(
        &self,
        account: &DbAccount,
        findings: &mut Vec<PermissionFinding>,
    ) {
        let dangerous_privileges = ["DROP", "ALTER", "TRUNCATE", "GRANT", "REVOKE", "SUPER"];
        let excessive: Vec<&str> = account
            .granted_privileges
            .iter()
            .filter(|p| {
                let upper = p.to_uppercase();
                dangerous_privileges.iter().any(|d| upper.contains(d))
            })
            .map(|p| p.as_str())
            .collect();

        if !excessive.is_empty() {
            findings.push(PermissionFinding {
                severity: PermissionIssueSeverity::Warning,
                title: "过度授权".to_string(),
                description: format!(
                    "账户 '{}' 被授予危险权限: {}",
                    account.username,
                    excessive.join(", ")
                ),
                recommendation: "撤销 DROP/ALTER/TRUNCATE/GRANT 等危险权限，仅保留 SELECT"
                    .to_string(),
            });
        }
    }

    fn check_select_star(&self, usage: &QueryUsage, findings: &mut Vec<PermissionFinding>) {
        if usage.uses_select_star {
            findings.push(PermissionFinding {
                severity: PermissionIssueSeverity::Warning,
                title: "SELECT * 查询".to_string(),
                description: "使用 SELECT * 可能暴露不必要的列数据，且无法精确控制列权限"
                    .to_string(),
                recommendation: "指定所需列名，仅查询必要的列".to_string(),
            });
        }
    }

    fn check_missing_where(&self, usage: &QueryUsage, findings: &mut Vec<PermissionFinding>) {
        if !usage.has_where && !usage.tables.is_empty() {
            findings.push(PermissionFinding {
                severity: PermissionIssueSeverity::Info,
                title: "无 WHERE 条件的全表查询".to_string(),
                description: "查询无 WHERE 条件，可能返回全表数据".to_string(),
                recommendation: "添加 WHERE 条件限制查询范围".to_string(),
            });
        }
    }

    fn compute_minimal_privileges(&self, usage: &QueryUsage) -> Vec<String> {
        let mut privs = Vec::new();
        privs.push("SELECT".to_string());

        for table in &usage.tables {
            if usage.uses_select_star {
                privs.push(format!("SELECT({}.*)", table));
            } else {
                for col in &usage.referenced_columns {
                    privs.push(format!("SELECT({}.{})", table, col));
                }
            }
        }
        privs
    }

    fn compute_security_score(&self, account: &DbAccount, findings: &[PermissionFinding]) -> f64 {
        let mut score: f64 = 100.0;
        if account.is_super_user {
            score -= 40.0;
        }
        for finding in findings {
            match finding.severity {
                PermissionIssueSeverity::Critical => score -= 25.0,
                PermissionIssueSeverity::Warning => score -= 10.0,
                PermissionIssueSeverity::Info => score -= 5.0,
            }
        }
        score.max(0.0).min(100.0)
    }
}
