//! WasmDbSqlWhitelist — SQL 白名单
//!
//! 仅允许 SELECT/INSERT/UPDATE/DELETE，禁止 DDL（DROP/ALTER/TRUNCATE/CREATE）。

/// WASM DB SQL 白名单
///
/// 验证 SQL 语句类型是否在允许范围内。
/// 默认允许：SELECT、INSERT、UPDATE、DELETE。
/// 默认禁止：DROP、ALTER、TRUNCATE、CREATE、GRANT、REVOKE。
#[derive(Debug, Clone)]
pub struct WasmDbSqlWhitelist {
    allowed_prefixes: Vec<String>,
    forbidden_keywords: Vec<String>,
}

impl WasmDbSqlWhitelist {
    /// 创建默认白名单（SELECT/INSERT/UPDATE/DELETE）
    pub fn new() -> Self {
        Self {
            allowed_prefixes: vec![
                "SELECT".to_string(),
                "INSERT".to_string(),
                "UPDATE".to_string(),
                "DELETE".to_string(),
            ],
            forbidden_keywords: vec![
                "DROP".to_string(),
                "ALTER".to_string(),
                "TRUNCATE".to_string(),
                "CREATE".to_string(),
                "GRANT".to_string(),
                "REVOKE".to_string(),
            ],
        }
    }

    /// 添加允许的 SQL 前缀
    pub fn allow_prefix(&mut self, prefix: &str) {
        let upper = prefix.to_uppercase();
        if !self.allowed_prefixes.contains(&upper) {
            self.allowed_prefixes.push(upper);
        }
    }

    /// 添加禁止的关键字
    pub fn forbid_keyword(&mut self, keyword: &str) {
        let upper = keyword.to_uppercase();
        if !self.forbidden_keywords.contains(&upper) {
            self.forbidden_keywords.push(upper);
        }
    }

    /// 验证 SQL 是否允许
    ///
    /// 检查逻辑：
    /// 1. SQL 非空
    /// 2. 首关键字在 allowed_prefixes 中
    /// 3. SQL 中不包含任何 forbidden_keywords
    pub fn validate(&self, sql: &str) -> bool {
        let trimmed = sql.trim();
        if trimmed.is_empty() {
            return false;
        }

        let upper = trimmed.to_uppercase();

        let first_word = upper.split_whitespace().next().unwrap_or("");
        if !self.allowed_prefixes.iter().any(|p| first_word == p) {
            return false;
        }

        for keyword in &self.forbidden_keywords {
            if upper.contains(keyword) {
                return false;
            }
        }

        true
    }

    /// 允许的前缀列表
    pub fn allowed_prefixes(&self) -> &[String] {
        &self.allowed_prefixes
    }

    /// 禁止的关键字列表
    pub fn forbidden_keywords(&self) -> &[String] {
        &self.forbidden_keywords
    }
}

impl Default for WasmDbSqlWhitelist {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_select() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(wl.validate("SELECT * FROM users"));
        assert!(wl.validate("select * from users"));
        assert!(wl.validate("SELECT id, name FROM users WHERE id = ?"));
    }

    #[test]
    fn test_allow_insert() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(wl.validate("INSERT INTO users (name) VALUES (?)"));
        assert!(wl.validate("insert into users (name) values (?)"));
    }

    #[test]
    fn test_allow_update() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(wl.validate("UPDATE users SET name = ? WHERE id = ?"));
        assert!(wl.validate("update users set name = ? where id = ?"));
    }

    #[test]
    fn test_allow_delete() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(wl.validate("DELETE FROM users WHERE id = ?"));
        assert!(wl.validate("delete from users where id = ?"));
    }

    #[test]
    fn test_reject_drop() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(!wl.validate("DROP TABLE users"));
    }

    #[test]
    fn test_reject_alter() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(!wl.validate("ALTER TABLE users ADD COLUMN age INT"));
    }

    #[test]
    fn test_reject_truncate() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(!wl.validate("TRUNCATE TABLE users"));
    }

    #[test]
    fn test_reject_create() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(!wl.validate("CREATE TABLE users (id INT)"));
    }

    #[test]
    fn test_reject_grant() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(!wl.validate("GRANT SELECT ON users TO guest"));
    }

    #[test]
    fn test_reject_empty() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(!wl.validate(""));
        assert!(!wl.validate("   "));
    }

    #[test]
    fn test_reject_unknown_prefix() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(!wl.validate("EXPLAIN SELECT * FROM users"));
        assert!(!wl.validate("WITH cte AS (SELECT 1) SELECT * FROM cte"));
    }

    #[test]
    fn test_custom_allow_prefix() {
        let mut wl = WasmDbSqlWhitelist::new();
        wl.allow_prefix("EXPLAIN");
        assert!(wl.validate("EXPLAIN SELECT * FROM users"));
    }

    #[test]
    fn test_custom_forbid_keyword() {
        let mut wl = WasmDbSqlWhitelist::new();
        wl.forbid_keyword("JOIN");
        assert!(!wl.validate("SELECT * FROM a JOIN b ON a.id = b.id"));
        assert!(wl.validate("SELECT * FROM a"));
    }

    #[test]
    fn test_forbidden_keyword_in_select() {
        let wl = WasmDbSqlWhitelist::new();
        assert!(!wl.validate("SELECT * FROM users; DROP TABLE users"));
    }

    #[test]
    fn test_default() {
        let wl = WasmDbSqlWhitelist::default();
        assert!(wl.validate("SELECT 1"));
    }

    #[test]
    fn test_allowed_prefixes_list() {
        let wl = WasmDbSqlWhitelist::new();
        let prefixes = wl.allowed_prefixes();
        assert!(prefixes.contains(&"SELECT".to_string()));
        assert!(prefixes.contains(&"INSERT".to_string()));
        assert!(prefixes.contains(&"UPDATE".to_string()));
        assert!(prefixes.contains(&"DELETE".to_string()));
    }

    #[test]
    fn test_forbidden_keywords_list() {
        let wl = WasmDbSqlWhitelist::new();
        let keywords = wl.forbidden_keywords();
        assert!(keywords.contains(&"DROP".to_string()));
        assert!(keywords.contains(&"ALTER".to_string()));
    }
}
