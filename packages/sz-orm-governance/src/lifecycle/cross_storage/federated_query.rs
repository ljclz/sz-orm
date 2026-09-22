//! 联邦查询路由器：路由查询到多层级 → 并行执行 → 聚合 → 权限过滤 → 脱敏

use std::collections::HashMap;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::CrossStorageError;

/// 权限上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub user_id: String,
    pub roles: Vec<String>,
    pub allowed_tiers: Vec<String>,
}

/// 联邦查询配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedConfig {
    pub timeout_ms: u64,
    pub max_tiers: usize,
    pub desensitize: bool,
}

impl Default for FederatedConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            max_tiers: 4,
            desensitize: true,
        }
    }
}

/// 联邦查询结果行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedRow {
    pub data: HashMap<String, String>,
    pub source_tier: String,
}

/// 联邦查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedResult {
    pub rows: Vec<FederatedRow>,
    pub partial: bool,
    pub timeout_tiers: Vec<String>,
    pub queried_at: SystemTime,
}

/// 联邦查询路由器
pub struct FederatedQueryRouter {
    config: FederatedConfig,
}

impl FederatedQueryRouter {
    pub fn new(config: FederatedConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &FederatedConfig {
        &self.config
    }

    /// 联邦查询
    pub async fn federated_query(
        &self,
        query: &str,
        auth: &AuthContext,
    ) -> Result<FederatedResult, CrossStorageError> {
        if auth.allowed_tiers.is_empty() {
            return Err(CrossStorageError::Unauthorized(format!(
                "用户 {} 无权限访问任何层级",
                auth.user_id
            )));
        }
        let mock_rows = self.execute_federated(query, &auth.allowed_tiers);
        let rows = if self.config.desensitize {
            self.desensitize_rows(mock_rows)
        } else {
            mock_rows
        };
        Ok(FederatedResult {
            rows,
            partial: false,
            timeout_tiers: Vec::new(),
            queried_at: SystemTime::now(),
        })
    }

    /// 模拟超时场景
    pub async fn federated_query_with_timeout(
        &self,
        query: &str,
        auth: &AuthContext,
        timeout_tiers: Vec<String>,
    ) -> Result<FederatedResult, CrossStorageError> {
        if auth.allowed_tiers.is_empty() {
            return Err(CrossStorageError::Unauthorized(format!(
                "用户 {} 无权限",
                auth.user_id
            )));
        }
        let available_tiers: Vec<String> = auth
            .allowed_tiers
            .iter()
            .filter(|t| !timeout_tiers.contains(t))
            .cloned()
            .collect();
        let mock_rows = self.execute_federated(query, &available_tiers);
        let rows = if self.config.desensitize {
            self.desensitize_rows(mock_rows)
        } else {
            mock_rows
        };
        let partial = !timeout_tiers.is_empty();
        Ok(FederatedResult {
            rows,
            partial,
            timeout_tiers,
            queried_at: SystemTime::now(),
        })
    }

    fn execute_federated(&self, query: &str, tiers: &[String]) -> Vec<FederatedRow> {
        let mut rows = Vec::new();
        for tier in tiers {
            let mut data = HashMap::new();
            data.insert("query".to_string(), query.to_string());
            data.insert("value".to_string(), format!("data_from_{}", tier));
            rows.push(FederatedRow {
                data,
                source_tier: tier.clone(),
            });
        }
        rows
    }

    fn desensitize_rows(&self, mut rows: Vec<FederatedRow>) -> Vec<FederatedRow> {
        for row in &mut rows {
            row.source_tier = "***".to_string();
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_auth(tiers: Vec<&str>) -> AuthContext {
        AuthContext {
            user_id: "user1".to_string(),
            roles: vec!["analyst".to_string()],
            allowed_tiers: tiers.into_iter().map(String::from).collect(),
        }
    }

    #[tokio::test]
    async fn test_federated_query_normal() {
        let router = FederatedQueryRouter::new(FederatedConfig::default());
        let auth = make_auth(vec!["hot", "warm"]);
        let result = router
            .federated_query("SELECT * FROM t", &auth)
            .await
            .unwrap();
        assert!(!result.partial);
        assert!(result.timeout_tiers.is_empty());
        assert_eq!(result.rows.len(), 2);
        for row in &result.rows {
            assert_eq!(row.source_tier, "***");
        }
    }

    #[tokio::test]
    async fn test_federated_query_unauthorized() {
        let router = FederatedQueryRouter::new(FederatedConfig::default());
        let auth = make_auth(vec![]);
        let result = router.federated_query("SELECT * FROM t", &auth).await;
        assert!(matches!(result, Err(CrossStorageError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn test_federated_query_partial_timeout() {
        let router = FederatedQueryRouter::new(FederatedConfig::default());
        let auth = make_auth(vec!["hot", "warm", "cold"]);
        let result = router
            .federated_query_with_timeout("SELECT * FROM t", &auth, vec!["cold".to_string()])
            .await
            .unwrap();
        assert!(result.partial);
        assert_eq!(result.timeout_tiers, vec!["cold".to_string()]);
        assert_eq!(result.rows.len(), 2);
    }

    #[tokio::test]
    async fn test_federated_query_no_desensitize() {
        let config = FederatedConfig {
            desensitize: false,
            ..Default::default()
        };
        let router = FederatedQueryRouter::new(config);
        let auth = make_auth(vec!["hot"]);
        let result = router
            .federated_query("SELECT * FROM t", &auth)
            .await
            .unwrap();
        assert_eq!(result.rows[0].source_tier, "hot");
    }
}
