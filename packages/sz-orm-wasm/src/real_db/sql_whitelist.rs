//! WasmDbSqlWhitelist — SQL 白名单
//!
//! 仅允许 SELECT/INSERT/UPDATE/DELETE，禁止 DDL（DROP/ALTER/TRUNCATE/CREATE）、
//! 禁止文件读写原语（INTO OUTFILE/LOAD_FILE 等）与多语句注入。

/// WASM DB SQL 白名单
///
/// 验证 SQL 语句类型是否在允许范围内。
/// 默认允许：SELECT、INSERT、UPDATE、DELETE。
/// 默认禁止：DROP、ALTER、TRUNCATE、CREATE、GRANT、REVOKE、
/// 文件读写原语（OUTFILE/DUMPFILE/LOAD_FILE）、命令执行原语（PROCEDURE/EXEC/CALL）。
///
/// # 安全说明（v4.8.0 修复 H-2）
///
/// - **多语句检测**：语句分隔符 `;` 后存在非空白内容即拒绝（黑帽实证：
///   `SELECT 1; DELETE FROM users` 曾通过检查；SQL 字符串内的分号经引号
///   状态机识别，不误伤）；
/// - **文件读写原语**：`INTO OUTFILE`/`INTO DUMPFILE`/`LOAD_FILE()` 加入
///   forbidden 列表（MySQL FILE 权限下曾可读写任意文件）；
/// - 注释拆分（`DR/**/OP`）可重组 forbidden 关键字——本实现按**完整词**
///   匹配 forbidden 列表，且禁止注释符内嵌关键字仍需解析器，此残余风险
///   已在部署文档标注（配合连接层禁用多语句 + 最小权限账号使用）。
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
                // v4.8.0 修复 H-2：MySQL 文件读写原语（FILE 权限下可读写任意文件）
                "OUTFILE".to_string(),
                "DUMPFILE".to_string(),
                "LOAD_FILE".to_string(),
                // 命令执行/存储过程调用原语（可被利用做提权或命令执行）
                "PROCEDURE".to_string(),
                "EXEC".to_string(),
                "EXECUTE".to_string(),
                "CALL".to_string(),
                // PostgreSQL COPY ... TO PROGRAM（superuser 命令执行）
                "COPY".to_string(),
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

    /// 检测多语句注入：语句分隔符 `;` 后存在非空白内容
    ///
    /// 用引号状态机区分 SQL 字符串内的分号（`WHERE name='a;b'`）与
    /// 真实语句分隔符；支持反斜杠转义（MySQL）与 `''` 双写转义（标准 SQL）。
    /// 尾部单分号（`SELECT 1;`）视为合法。
    fn has_statement_semicolon(sql: &str) -> bool {
        let bytes = sql.as_bytes();
        let mut in_string = false;
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if in_string {
                if b == b'\\' {
                    i += 2; // 跳过转义字符（MySQL）
                    continue;
                }
                if b == b'\'' {
                    // 双写单引号（''）是标准 SQL 转义，不结束字符串
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    in_string = false;
                }
            } else if b == b'\'' {
                in_string = true;
            } else if b == b';' {
                // 语句分隔符：分号后存在非空白内容 → 多语句
                if !sql[i + 1..].trim().is_empty() {
                    return true;
                }
            }
            i += 1;
        }
        false
    }

    /// 验证 SQL 是否允许
    ///
    /// 检查逻辑：
    /// 1. SQL 非空
    /// 2. 首关键字在 allowed_prefixes 中
    /// 3. SQL 中不包含任何 forbidden_keywords
    /// 4. 不包含多语句分隔符（v4.8.0 修复 H-2）
    pub fn validate(&self, sql: &str) -> bool {
        let trimmed = sql.trim();
        if trimmed.is_empty() {
            return false;
        }

        if Self::has_statement_semicolon(trimmed) {
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
