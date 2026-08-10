//! # 五方言连接安全验证（`prod-dialect-security` feature）
//!
//! 对 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五种方言验证 TLS/认证/连接串脱敏/连接池参数。
//! SQLite TLS 标记 N/A；不可用方言标记 Skipped。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 数据库方言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dialect {
    /// MySQL 方言
    MySql,
    /// PostgreSQL 方言
    PostgreSql,
    /// SQLite 方言
    Sqlite,
    /// Oracle 方言
    Oracle,
    /// MSSQL 方言
    Mssql,
}

impl Dialect {
    /// 返回方言字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            Dialect::MySql => "mysql",
            Dialect::PostgreSql => "postgresql",
            Dialect::Sqlite => "sqlite",
            Dialect::Oracle => "oracle",
            Dialect::Mssql => "mssql",
        }
    }

    /// 是否支持 TLS
    pub fn supports_tls(&self) -> bool {
        !matches!(self, Dialect::Sqlite)
    }
}

/// 检查状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    /// 通过
    Pass,
    /// 失败
    Fail,
    /// 跳过（方言不可用）
    Skipped,
    /// 不适用（如 SQLite 的 TLS）
    NotApplicable,
}

/// 方言安全配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialectSecurityConfig {
    /// 数据库方言
    pub dialect: Dialect,
    /// 是否启用 TLS
    pub tls_enabled: bool,
    /// 是否配置认证
    pub auth_configured: bool,
    /// 连接串是否已脱敏
    pub conn_str_masked: bool,
    /// 连接池参数是否有效
    pub pool_params_valid: bool,
    /// 方言是否可用
    pub available: bool,
    /// 跳过原因
    pub skip_reason: Option<String>,
}

/// 单方言验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialectSecurityResult {
    /// 数据库方言
    pub dialect: Dialect,
    /// TLS 检查状态
    pub tls: CheckStatus,
    /// 认证检查状态
    pub auth: CheckStatus,
    /// 连接串脱敏检查状态
    pub conn_str_masking: CheckStatus,
    /// 连接池参数检查状态
    pub pool_params: CheckStatus,
    /// 证据列表
    pub evidence: Vec<String>,
}

/// 验证报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialectSecurityReport {
    /// 各方言验证结果
    pub results: Vec<DialectSecurityResult>,
}

impl DialectSecurityReport {
    /// 所有方言是否全部通过（Skipped/NotApplicable 视为非失败）
    pub fn all_pass(&self) -> bool {
        self.results.iter().all(|r| {
            r.tls != CheckStatus::Fail
                && r.auth != CheckStatus::Fail
                && r.conn_str_masking != CheckStatus::Fail
                && r.pool_params != CheckStatus::Fail
        })
    }
}

/// 方言安全验证器
pub struct DialectSecurityVerifier {
    configs: HashMap<Dialect, DialectSecurityConfig>,
}

impl DialectSecurityVerifier {
    /// 创建验证器
    pub fn new(configs: Vec<DialectSecurityConfig>) -> Self {
        let map = configs.into_iter().map(|c| (c.dialect, c)).collect();
        Self { configs: map }
    }

    /// 验证所有方言
    pub fn verify(&self) -> DialectSecurityReport {
        let mut results = Vec::new();
        for dialect in [
            Dialect::MySql,
            Dialect::PostgreSql,
            Dialect::Sqlite,
            Dialect::Oracle,
            Dialect::Mssql,
        ] {
            results.push(self.verify_one(dialect));
        }
        DialectSecurityReport { results }
    }

    fn verify_one(&self, dialect: Dialect) -> DialectSecurityResult {
        let config = self.configs.get(&dialect);
        let mut evidence = Vec::new();

        if let Some(cfg) = config {
            if !cfg.available {
                return DialectSecurityResult {
                    dialect,
                    tls: CheckStatus::Skipped,
                    auth: CheckStatus::Skipped,
                    conn_str_masking: CheckStatus::Skipped,
                    pool_params: CheckStatus::Skipped,
                    evidence: vec![cfg
                        .skip_reason
                        .clone()
                        .unwrap_or_else(|| "not available".into())],
                };
            }

            let tls = if dialect.supports_tls() {
                if cfg.tls_enabled {
                    evidence.push(format!("{} TLS enabled", dialect.as_str()));
                    CheckStatus::Pass
                } else {
                    evidence.push(format!("{} TLS not enabled", dialect.as_str()));
                    CheckStatus::Fail
                }
            } else {
                evidence.push(format!("{} TLS N/A (file-based)", dialect.as_str()));
                CheckStatus::NotApplicable
            };

            let auth = if cfg.auth_configured {
                evidence.push(format!("{} auth configured", dialect.as_str()));
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            };

            let conn_str_masking = if cfg.conn_str_masked {
                evidence.push(format!("{} conn_str masked", dialect.as_str()));
                CheckStatus::Pass
            } else {
                evidence.push(format!(
                    "{} conn_str has plaintext password",
                    dialect.as_str()
                ));
                CheckStatus::Fail
            };

            let pool_params = if cfg.pool_params_valid {
                evidence.push(format!("{} pool params valid", dialect.as_str()));
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            };

            DialectSecurityResult {
                dialect,
                tls,
                auth,
                conn_str_masking,
                pool_params,
                evidence,
            }
        } else {
            DialectSecurityResult {
                dialect,
                tls: CheckStatus::Skipped,
                auth: CheckStatus::Skipped,
                conn_str_masking: CheckStatus::Skipped,
                pool_params: CheckStatus::Skipped,
                evidence: vec!["no config provided".into()],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialect_supports_tls() {
        assert!(Dialect::MySql.supports_tls());
        assert!(Dialect::PostgreSql.supports_tls());
        assert!(!Dialect::Sqlite.supports_tls());
        assert!(Dialect::Oracle.supports_tls());
        assert!(Dialect::Mssql.supports_tls());
    }

    #[test]
    fn test_verifier_all_pass() {
        let configs = vec![
            DialectSecurityConfig {
                dialect: Dialect::MySql,
                tls_enabled: true,
                auth_configured: true,
                conn_str_masked: true,
                pool_params_valid: true,
                available: true,
                skip_reason: None,
            },
            DialectSecurityConfig {
                dialect: Dialect::PostgreSql,
                tls_enabled: true,
                auth_configured: true,
                conn_str_masked: true,
                pool_params_valid: true,
                available: true,
                skip_reason: None,
            },
            DialectSecurityConfig {
                dialect: Dialect::Sqlite,
                tls_enabled: false,
                auth_configured: true,
                conn_str_masked: true,
                pool_params_valid: true,
                available: true,
                skip_reason: None,
            },
        ];
        let verifier = DialectSecurityVerifier::new(configs);
        let report = verifier.verify();
        assert!(report.all_pass());
        let sqlite = report
            .results
            .iter()
            .find(|r| r.dialect == Dialect::Sqlite)
            .unwrap();
        assert_eq!(sqlite.tls, CheckStatus::NotApplicable);
    }

    #[test]
    fn test_verifier_skipped_for_unavailable() {
        let configs = vec![DialectSecurityConfig {
            dialect: Dialect::Mssql,
            tls_enabled: false,
            auth_configured: false,
            conn_str_masked: false,
            pool_params_valid: false,
            available: false,
            skip_reason: Some("MSSQL not installed".into()),
        }];
        let verifier = DialectSecurityVerifier::new(configs);
        let report = verifier.verify();
        let mssql = report
            .results
            .iter()
            .find(|r| r.dialect == Dialect::Mssql)
            .unwrap();
        assert_eq!(mssql.tls, CheckStatus::Skipped);
        assert!(mssql.evidence[0].contains("MSSQL not installed"));
    }

    #[test]
    fn test_verifier_fail_for_plaintext_password() {
        let configs = vec![DialectSecurityConfig {
            dialect: Dialect::MySql,
            tls_enabled: true,
            auth_configured: true,
            conn_str_masked: false,
            pool_params_valid: true,
            available: true,
            skip_reason: None,
        }];
        let verifier = DialectSecurityVerifier::new(configs);
        let report = verifier.verify();
        assert!(!report.all_pass());
        let mysql = report
            .results
            .iter()
            .find(|r| r.dialect == Dialect::MySql)
            .unwrap();
        assert_eq!(mysql.conn_str_masking, CheckStatus::Fail);
    }

    #[test]
    fn test_verifier_no_config_skipped() {
        let verifier = DialectSecurityVerifier::new(vec![]);
        let report = verifier.verify();
        assert_eq!(report.results.len(), 5);
        for r in &report.results {
            assert_eq!(r.tls, CheckStatus::Skipped);
        }
    }
}
