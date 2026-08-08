//! SQL 敏感字面量脱敏模块
//!
//! 在将 SQL 发送给 LLM 之前，识别并替换敏感字面量（password/token/Base64 token），
//! 防止敏感信息泄露给外部 LLM 服务。
//!
//! # 用法
//!
//! ```
//! use sz_orm_ai::SqlSanitizer;
//!
//! let sql = "SELECT * FROM users WHERE password = 'secret123' AND token = 'abc'";
//! let sanitized = SqlSanitizer::sanitize(sql);
//! assert!(sanitized.contains("'***'"));
//! assert!(!sanitized.contains("secret123"));
//! ```

/// SQL 敏感字面量脱敏器
///
/// 识别以下敏感模式并替换为 `'***'`：
/// - `password = '...'` / `passwd = '...'` / `pwd = '...'`
/// - `token = '...'` / `api_key = '...'` / `secret = '...'`
/// - 超过 40 字符的 Base64 编码字符串字面量
pub struct SqlSanitizer;

impl SqlSanitizer {
    /// 对 SQL 中的敏感字面量进行脱敏
    ///
    /// 返回脱敏后的 SQL 字符串，所有敏感字面量被替换为 `'***'`。
    pub fn sanitize(sql: &str) -> String {
        let mut result = sql.to_string();
        let sensitive_keywords = [
            "password",
            "passwd",
            "pwd",
            "token",
            "api_key",
            "apikey",
            "secret",
            "access_key",
            "private_key",
        ];

        for keyword in &sensitive_keywords {
            result = Self::sanitize_keyword(&result, keyword);
        }

        result = Self::sanitize_base64_literals(&result);

        result
    }

    fn sanitize_keyword(sql: &str, keyword: &str) -> String {
        let lower = sql.to_lowercase();
        let mut result = String::with_capacity(sql.len());
        let mut i = 0;
        let bytes = sql.as_bytes();
        let lower_bytes = lower.as_bytes();

        while i < bytes.len() {
            if i + keyword.len() <= lower_bytes.len()
                && &lower_bytes[i..i + keyword.len()] == keyword.as_bytes()
            {
                let kw_end = i + keyword.len();
                let after_kw = &sql[kw_end..];
                if let Some((lit_start, lit_end, quote)) = Self::find_string_literal_after(after_kw)
                {
                    result.push_str(&sql[i..kw_end]);
                    result.push_str(&sql[kw_end..kw_end + lit_start]);
                    result.push(quote);
                    result.push_str("***");
                    result.push(quote);
                    i = kw_end + lit_end;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }

        result
    }

    fn find_string_literal_after(s: &str) -> Option<(usize, usize, char)> {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n') {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] != b'=' {
            return None;
        }
        i += 1;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n') {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let quote = bytes[i];
        if quote != b'\'' && quote != b'"' {
            return None;
        }
        let literal_start = i;
        i += 1;
        while i < bytes.len() {
            if bytes[i] == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    i += 2;
                    continue;
                }
                let literal_end = i + 1;
                return Some((literal_start, literal_end, quote as char));
            }
            i += 1;
        }
        None
    }

    fn sanitize_base64_literals(sql: &str) -> String {
        let bytes = sql.as_bytes();
        let mut result = String::with_capacity(sql.len());
        let mut i = 0;

        while i < bytes.len() {
            let quote = bytes[i];
            if quote == b'\'' || quote == b'"' {
                let literal_start = i;
                i += 1;
                let content_start = i;
                let mut content_end = i;
                while i < bytes.len() {
                    if bytes[i] == quote {
                        if i + 1 < bytes.len() && bytes[i + 1] == quote {
                            i += 2;
                            continue;
                        }
                        content_end = i;
                        break;
                    }
                    i += 1;
                }
                if i < bytes.len() {
                    let content = &sql[content_start..content_end];
                    if content.len() > 40 && Self::is_base64(content) {
                        result.push(quote as char);
                        result.push_str("***");
                        result.push(quote as char);
                        i += 1;
                        continue;
                    }
                    result.push_str(&sql[literal_start..i + 1]);
                    i += 1;
                    continue;
                }
                result.push_str(&sql[literal_start..]);
                break;
            }
            result.push(quote as char);
            i += 1;
        }

        result
    }

    fn is_base64(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let mut non_base64 = 0;
        for c in s.chars() {
            if !c.is_ascii_alphanumeric() && c != '+' && c != '/' && c != '=' {
                non_base64 += 1;
            }
        }
        let base64_ratio = 1.0 - (non_base64 as f64 / s.len() as f64);
        base64_ratio > 0.95
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_password() {
        let sql = "SELECT * FROM users WHERE password = 'secret123'";
        let result = SqlSanitizer::sanitize(sql);
        assert!(result.contains("'***'"));
        assert!(!result.contains("secret123"));
    }

    #[test]
    fn test_sanitize_token() {
        let sql = "SELECT * FROM api_tokens WHERE token = 'abc123xyz'";
        let result = SqlSanitizer::sanitize(sql);
        assert!(result.contains("'***'"));
        assert!(!result.contains("abc123xyz"));
    }

    #[test]
    fn test_sanitize_api_key() {
        let sql = "SELECT * FROM config WHERE api_key = 'sk-xxxx'";
        let result = SqlSanitizer::sanitize(sql);
        assert!(result.contains("'***'"));
        assert!(!result.contains("sk-xxxx"));
    }

    #[test]
    fn test_sanitize_base64_token() {
        let long_token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpmeJf36POk6yJV_adQssw5c";
        let sql = format!("SELECT * FROM data WHERE value = '{}'", long_token);
        let result = SqlSanitizer::sanitize(&sql);
        assert!(result.contains("'***'"));
        assert!(!result.contains(long_token));
    }

    #[test]
    fn test_no_sanitize_normal_values() {
        let sql = "SELECT * FROM users WHERE name = 'John' AND age = 30";
        let result = SqlSanitizer::sanitize(sql);
        assert!(result.contains("'John'"));
        assert!(!result.contains("'***'"));
    }

    #[test]
    fn test_sanitize_multiple_sensitive() {
        let sql = "SELECT * FROM users WHERE password = 'pw1' AND token = 'tk1' AND secret = 'sc1'";
        let result = SqlSanitizer::sanitize(sql);
        assert!(!result.contains("pw1"));
        assert!(!result.contains("tk1"));
        assert!(!result.contains("sc1"));
        let star_count = result.matches("'***'").count();
        assert_eq!(star_count, 3);
    }

    #[test]
    fn test_sanitize_preserves_structure() {
        let sql = "SELECT id, name FROM users WHERE password = 'secret' LIMIT 10";
        let result = SqlSanitizer::sanitize(sql);
        assert!(result.starts_with("SELECT id, name FROM users WHERE password = "));
        assert!(result.ends_with(" LIMIT 10"));
    }

    #[test]
    fn test_sanitize_double_quotes() {
        let sql = "SELECT * FROM users WHERE password = \"secret123\"";
        let result = SqlSanitizer::sanitize(sql);
        assert!(result.contains("\"***\""));
        assert!(!result.contains("secret123"));
    }

    #[test]
    fn test_sanitize_no_string_literal() {
        let sql = "SELECT * FROM users WHERE password = ?";
        let result = SqlSanitizer::sanitize(sql);
        assert_eq!(result, sql);
    }
}
