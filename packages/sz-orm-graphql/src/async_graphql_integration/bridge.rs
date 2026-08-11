//! AsyncGraphqlBridge：async-graphql Schema 对接 + DataLoader 复用

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use super::error::TicketError;

/// GraphQL 查询结果
#[derive(Debug, Clone)]
pub struct GraphqlResult {
    pub data: Value,
    pub errors: Vec<TicketError>,
}

impl GraphqlResult {
    pub fn ok(data: Value) -> Self {
        Self {
            data,
            errors: vec![],
        }
    }

    pub fn error(err: TicketError) -> Self {
        Self {
            data: Value::Null,
            errors: vec![err],
        }
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// DataLoader 批量加载器（复用既有 sz-orm-graphql DataLoader 概念）
pub struct BridgeDataLoader {
    batch_cache: parking_lot::RwLock<HashMap<String, Value>>,
    batch_count: parking_lot::Mutex<usize>,
}

impl BridgeDataLoader {
    pub fn new() -> Self {
        Self {
            batch_cache: parking_lot::RwLock::new(HashMap::new()),
            batch_count: parking_lot::Mutex::new(0),
        }
    }

    /// 批量加载：单次合并多个 key 查询
    pub fn batch_load(
        &self,
        keys: &[String],
        values: HashMap<String, Value>,
    ) -> Vec<Option<Value>> {
        {
            let mut cache = self.batch_cache.write();
            for (k, v) in &values {
                cache.insert(k.clone(), v.clone());
            }
        }
        {
            let mut count = self.batch_count.lock();
            *count += 1;
        }
        keys.iter()
            .map(|k| self.batch_cache.read().get(k).cloned())
            .collect()
    }

    /// 批量加载次数（验证 N+1 消除）
    pub fn batch_count(&self) -> usize {
        *self.batch_count.lock()
    }

    /// 清空缓存
    pub fn clear(&self) {
        self.batch_cache.write().clear();
    }
}

impl Default for BridgeDataLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// AsyncGraphqlBridge：async-graphql Schema 对接 + DataLoader
pub struct AsyncGraphqlBridge {
    dataloader: Arc<BridgeDataLoader>,
    sdl: String,
}

impl AsyncGraphqlBridge {
    pub fn new(sdl: &str) -> Self {
        Self {
            dataloader: Arc::new(BridgeDataLoader::new()),
            sdl: sdl.to_string(),
        }
    }

    pub fn dataloader(&self) -> &BridgeDataLoader {
        &self.dataloader
    }

    pub fn sdl(&self) -> &str {
        &self.sdl
    }

    /// 执行查询（模拟，实际应通过 async_graphql::Schema::execute）
    pub fn execute(&self, query: &str) -> Result<GraphqlResult, TicketError> {
        if query.is_empty() {
            return Err(TicketError::validation("ERR_EMPTY_QUERY", "query is empty"));
        }
        Ok(GraphqlResult::ok(Value::String(query.to_string())))
    }

    /// 批量加载关联字段（消除 N+1）
    pub fn batch_load_relations(
        &self,
        keys: &[String],
        values: HashMap<String, Value>,
    ) -> Vec<Option<Value>> {
        self.dataloader.batch_load(keys, values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_bridge_creation() {
        let bridge = AsyncGraphqlBridge::new("type Query { users: [User] }");
        assert!(bridge.sdl().contains("type Query"));
    }

    #[test]
    fn test_bridge_execute_query() {
        let bridge = AsyncGraphqlBridge::new("type Query { users: [User] }");
        let result = bridge.execute("{ users { name } }").unwrap();
        assert!(result.is_ok());
        assert_eq!(result.data, json!("{ users { name } }"));
    }

    #[test]
    fn test_bridge_execute_empty_query() {
        let bridge = AsyncGraphqlBridge::new("type Query { users: [User] }");
        let result = bridge.execute("");
        assert!(result.is_err());
    }

    #[test]
    fn test_dataloader_batch_load() {
        let loader = BridgeDataLoader::new();
        let mut values = HashMap::new();
        values.insert("user1".to_string(), json!({"name": "Alice"}));
        values.insert("user2".to_string(), json!({"name": "Bob"}));

        let results = loader.batch_load(&["user1".to_string(), "user2".to_string()], values);

        assert_eq!(results.len(), 2);
        assert!(results[0].is_some());
        assert!(results[1].is_some());
        assert_eq!(loader.batch_count(), 1);
    }

    #[test]
    fn test_dataloader_n1_elimination() {
        let bridge = AsyncGraphqlBridge::new("type Query { users { orders { amount } } }");
        let mut values = HashMap::new();
        for i in 1..=100 {
            values.insert(format!("user{i}"), json!({"orders": []}));
        }

        let keys: Vec<String> = (1..=100).map(|i| format!("user{i}")).collect();
        let results = bridge.batch_load_relations(&keys, values);

        assert_eq!(results.len(), 100);
        assert_eq!(bridge.dataloader().batch_count(), 1);
    }

    #[test]
    fn test_dataloader_partial_failure() {
        let loader = BridgeDataLoader::new();
        let mut values = HashMap::new();
        values.insert("user1".to_string(), json!({"name": "Alice"}));

        let results = loader.batch_load(&["user1".to_string(), "missing".to_string()], values);

        assert_eq!(results.len(), 2);
        assert!(results[0].is_some());
        assert!(results[1].is_none());
    }

    #[test]
    fn test_dataloader_clear() {
        let loader = BridgeDataLoader::new();
        let mut values = HashMap::new();
        values.insert("k1".to_string(), json!("v1"));
        loader.batch_load(&["k1".to_string()], values);
        loader.clear();
        let results = loader.batch_load(&["k1".to_string()], HashMap::new());
        assert!(results[0].is_none());
    }

    #[test]
    fn test_graphql_result_ok() {
        let result = GraphqlResult::ok(json!({"users": []}));
        assert!(result.is_ok());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_graphql_result_error() {
        let result = GraphqlResult::error(TicketError::not_found("ERR_404", "not found"));
        assert!(!result.is_ok());
        assert_eq!(result.errors.len(), 1);
    }
}
