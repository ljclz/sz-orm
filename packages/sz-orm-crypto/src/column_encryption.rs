//! 列级加密策略
//!
//! 定义哪些列需要加密、使用什么算法、密钥版本。

use std::collections::HashMap;

use crate::dek_buffer::EncryptionAlgo;

/// 列加密配置
#[derive(Debug, Clone)]
pub struct ColumnCryptoConfig {
    /// 表名
    pub table: String,
    /// 列名
    pub column: String,
    /// 加密算法
    pub algorithm: EncryptionAlgo,
    /// 密钥版本
    pub key_version: u32,
}

impl ColumnCryptoConfig {
    /// 创建列加密配置
    pub fn new(table: impl Into<String>, column: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            column: column.into(),
            algorithm: EncryptionAlgo::default(),
            key_version: 1,
        }
    }

    /// 设置加密算法
    pub fn with_algorithm(mut self, algo: EncryptionAlgo) -> Self {
        self.algorithm = algo;
        self
    }

    /// 设置密钥版本
    pub fn with_key_version(mut self, version: u32) -> Self {
        self.key_version = version;
        self
    }

    /// 键（`table.column`）
    pub fn key(&self) -> String {
        format!("{}.{}", self.table, self.column)
    }
}

/// 列级加密策略
///
/// 存储列级加密配置，键为 `table.column`。
pub struct ColumnEncryptionPolicy {
    configs: HashMap<String, ColumnCryptoConfig>,
}

impl Default for ColumnEncryptionPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl ColumnEncryptionPolicy {
    /// 创建空策略
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// 从配置列表创建
    pub fn from_configs(configs: Vec<ColumnCryptoConfig>) -> Self {
        let mut policy = Self::new();
        for config in configs {
            let _ = policy.add_column(config);
        }
        policy
    }

    /// 添加列策略
    ///
    /// 校验 table 和 column 非空。
    pub fn add_column(&mut self, config: ColumnCryptoConfig) -> Result<(), String> {
        if config.table.is_empty() || config.column.is_empty() {
            return Err("table 和 column 不能为空".to_string());
        }
        let key = config.key();
        self.configs.insert(key, config);
        Ok(())
    }

    /// 查找列策略
    pub fn find(&self, table: &str, column: &str) -> Option<&ColumnCryptoConfig> {
        self.configs.get(&format!("{}.{}", table, column))
    }

    /// 移除列策略
    pub fn remove(&mut self, table: &str, column: &str) -> Option<ColumnCryptoConfig> {
        self.configs.remove(&format!("{}.{}", table, column))
    }

    /// 策略数量
    pub fn len(&self) -> usize {
        self.configs.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// 列是否需要加密
    pub fn is_encrypted(&self, table: &str, column: &str) -> bool {
        self.find(table, column).is_some()
    }

    /// 热更新：替换全部策略
    pub fn reload(&mut self, configs: Vec<ColumnCryptoConfig>) {
        self.configs.clear();
        for config in configs {
            let _ = self.add_column(config);
        }
    }

    /// 获取所有配置
    pub fn all_configs(&self) -> Vec<&ColumnCryptoConfig> {
        self.configs.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_add_and_find() {
        let mut policy = ColumnEncryptionPolicy::new();
        policy
            .add_column(ColumnCryptoConfig::new("users", "ssn").with_key_version(2))
            .unwrap();

        let config = policy.find("users", "ssn").unwrap();
        assert_eq!(config.table, "users");
        assert_eq!(config.column, "ssn");
        assert_eq!(config.key_version, 2);
        assert_eq!(config.algorithm, EncryptionAlgo::Aes256Gcm);
    }

    #[test]
    fn policy_not_found() {
        let policy = ColumnEncryptionPolicy::new();
        assert!(policy.find("users", "ssn").is_none());
        assert!(!policy.is_encrypted("users", "ssn"));
    }

    #[test]
    fn policy_reject_empty() {
        let mut policy = ColumnEncryptionPolicy::new();
        assert!(policy
            .add_column(ColumnCryptoConfig::new("", "ssn"))
            .is_err());
        assert!(policy
            .add_column(ColumnCryptoConfig::new("users", ""))
            .is_err());
    }

    #[test]
    fn policy_is_encrypted() {
        let mut policy = ColumnEncryptionPolicy::new();
        policy
            .add_column(ColumnCryptoConfig::new("users", "ssn"))
            .unwrap();
        assert!(policy.is_encrypted("users", "ssn"));
        assert!(!policy.is_encrypted("users", "name"));
    }

    #[test]
    fn policy_remove() {
        let mut policy = ColumnEncryptionPolicy::new();
        policy
            .add_column(ColumnCryptoConfig::new("users", "ssn"))
            .unwrap();
        assert_eq!(policy.len(), 1);
        policy.remove("users", "ssn");
        assert_eq!(policy.len(), 0);
    }

    #[test]
    fn policy_reload() {
        let mut policy = ColumnEncryptionPolicy::new();
        policy
            .add_column(ColumnCryptoConfig::new("users", "ssn"))
            .unwrap();
        assert_eq!(policy.len(), 1);

        policy.reload(vec![
            ColumnCryptoConfig::new("users", "email"),
            ColumnCryptoConfig::new("orders", "credit_card"),
        ]);
        assert_eq!(policy.len(), 2);
        assert!(policy.is_encrypted("users", "email"));
        assert!(policy.is_encrypted("orders", "credit_card"));
        assert!(!policy.is_encrypted("users", "ssn"));
    }

    #[test]
    fn policy_from_configs() {
        let policy = ColumnEncryptionPolicy::from_configs(vec![
            ColumnCryptoConfig::new("users", "ssn"),
            ColumnCryptoConfig::new("orders", "card"),
        ]);
        assert_eq!(policy.len(), 2);
        assert!(policy.is_encrypted("users", "ssn"));
        assert!(policy.is_encrypted("orders", "card"));
    }

    #[test]
    fn policy_all_configs() {
        let policy = ColumnEncryptionPolicy::from_configs(vec![
            ColumnCryptoConfig::new("users", "ssn"),
            ColumnCryptoConfig::new("orders", "card"),
        ]);
        assert_eq!(policy.all_configs().len(), 2);
    }
}
