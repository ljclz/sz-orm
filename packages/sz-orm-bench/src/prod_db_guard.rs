//! 生产 DB 守卫（v8.1.0 任务 2.1，feature gate: `bench-real-db`）
//!
//! 检测连接串是否生产 DB，生产 DB 拒绝执行基准，返回 `BENCH_PROD_DB_FORBIDDEN`。
//! 判定规则：`is_production` 标志位为 true → 拒绝；连接串含 `prod`/`production`/`_live`/`:3306/prod` → 拒绝；
//! 连接串含 `sz_orm_test` → 放行（专用测试 DB）。

use crate::{BenchDbConfig, BenchError};

/// 生产 DB 守卫
#[derive(Debug, Clone)]
pub struct BenchProdDbGuard {
    /// 生产 DB URL 黑名单关键字
    prod_keywords: Vec<String>,
    /// 测试 DB URL 白名单关键字
    test_keywords: Vec<String>,
}

impl Default for BenchProdDbGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchProdDbGuard {
    /// 创建默认守卫：黑名单 `["prod", "production", "_live"]`，白名单 `["sz_orm_test"]`
    pub fn new() -> Self {
        Self {
            prod_keywords: vec!["prod".into(), "production".into(), "_live".into()],
            test_keywords: vec!["sz_orm_test".into()],
        }
    }

    /// 自定义黑名单/白名单
    pub fn with_keywords(mut self, prod_keywords: Vec<String>, test_keywords: Vec<String>) -> Self {
        self.prod_keywords = prod_keywords;
        self.test_keywords = test_keywords;
        self
    }

    /// 校验 DB 配置：生产 DB 拒绝，非生产 DB 放行
    pub fn validate(&self, config: &BenchDbConfig) -> Result<(), BenchError> {
        if config.connection.is_empty() {
            return Err(BenchError::ConfigMissing(
                "BenchDbConfig.connection 为空".into(),
            ));
        }
        if config.is_production {
            return Err(BenchError::ProdDbForbidden(format!(
                "is_production=true，连接串={}",
                config.connection
            )));
        }
        let lower = config.connection.to_lowercase();
        if self.test_keywords.iter().any(|k| lower.contains(k)) {
            return Ok(());
        }
        for kw in &self.prod_keywords {
            if lower.contains(kw) {
                return Err(BenchError::ProdDbForbidden(format!(
                    "连接串含生产关键字 `{kw}`：{connection}",
                    connection = config.connection
                )));
            }
        }
        Ok(())
    }

    /// 判定连接串是否生产 DB（不返回错误，仅布尔）
    pub fn is_production_db(&self, config: &BenchDbConfig) -> bool {
        if config.is_production {
            return true;
        }
        let lower = config.connection.to_lowercase();
        if self.test_keywords.iter().any(|k| lower.contains(k)) {
            return false;
        }
        self.prod_keywords.iter().any(|k| lower.contains(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_explicit_production_flag() {
        let guard = BenchProdDbGuard::new();
        let cfg = BenchDbConfig::new("mysql://root:test123@127.0.0.1:3306/sz_orm_test")
            .with_production(true);
        let err = guard.validate(&cfg).unwrap_err();
        assert!(matches!(err, BenchError::ProdDbForbidden(_)));
    }

    #[test]
    fn test_reject_prod_keyword_in_connection() {
        let guard = BenchProdDbGuard::new();
        let cfg = BenchDbConfig::new("mysql://root:test123@10.0.0.1:3306/prod_db");
        let err = guard.validate(&cfg).unwrap_err();
        assert!(matches!(err, BenchError::ProdDbForbidden(_)));
    }

    #[test]
    fn test_reject_production_keyword_in_connection() {
        let guard = BenchProdDbGuard::new();
        let cfg = BenchDbConfig::new("postgres://user:pass@db/production_cluster");
        let err = guard.validate(&cfg).unwrap_err();
        assert!(matches!(err, BenchError::ProdDbForbidden(_)));
    }

    #[test]
    fn test_reject_live_keyword_in_connection() {
        let guard = BenchProdDbGuard::new();
        let cfg = BenchDbConfig::new("mysql://root:pass@host:3306/orders_live");
        let err = guard.validate(&cfg).unwrap_err();
        assert!(matches!(err, BenchError::ProdDbForbidden(_)));
    }

    #[test]
    fn test_allow_test_db_with_sz_orm_test_keyword() {
        let guard = BenchProdDbGuard::new();
        let cfg = BenchDbConfig::new("mysql://root:test123@127.0.0.1:3306/sz_orm_test");
        guard.validate(&cfg).unwrap();
    }

    #[test]
    fn test_allow_sqlite_memory() {
        let guard = BenchProdDbGuard::new();
        let cfg = BenchDbConfig::new("sqlite::memory:");
        guard.validate(&cfg).unwrap();
    }

    #[test]
    fn test_reject_empty_connection() {
        let guard = BenchProdDbGuard::new();
        let cfg = BenchDbConfig::new("");
        let err = guard.validate(&cfg).unwrap_err();
        assert!(matches!(err, BenchError::ConfigMissing(_)));
    }

    #[test]
    fn test_is_production_db_boolean() {
        let guard = BenchProdDbGuard::new();
        let prod_cfg = BenchDbConfig::new("mysql://root:pass@host/prod").with_production(true);
        assert!(guard.is_production_db(&prod_cfg));
        let test_cfg = BenchDbConfig::new("mysql://root:pass@host/sz_orm_test");
        assert!(!guard.is_production_db(&test_cfg));
        let neutral_cfg = BenchDbConfig::new("sqlite::memory:");
        assert!(!guard.is_production_db(&neutral_cfg));
    }

    #[test]
    fn test_custom_keywords() {
        let guard = BenchProdDbGuard::new().with_keywords(
            vec!["staging".into(), "live".into()],
            vec!["my_test_db".into()],
        );
        let reject_cfg = BenchDbConfig::new("mysql://root:pass@host/staging_db");
        assert!(matches!(
            guard.validate(&reject_cfg).unwrap_err(),
            BenchError::ProdDbForbidden(_)
        ));
        let allow_cfg = BenchDbConfig::new("mysql://root:pass@host/my_test_db");
        guard.validate(&allow_cfg).unwrap();
    }
}
