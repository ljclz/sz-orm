//! SQL 构造缓冲区抽象
//!
//! 当 `perf-smallstring` feature 启用时，使用 `CompactString` 作为内部缓冲区，
//! 短字符串（≤ 23 字节）内联存储，减少堆分配。
//! 当 feature 关闭时，退化为 `String`，零额外开销。

#[cfg(feature = "perf-smallstring")]
mod inner {
    use compact_str::CompactString;

    /// SQL 构造缓冲区
    pub struct SqlBuffer {
        buf: CompactString,
    }

    impl SqlBuffer {
        /// 创建空缓冲区
        pub fn new() -> Self {
            Self {
                buf: CompactString::with_capacity(0),
            }
        }

        /// 从字符串切片创建
        #[allow(clippy::should_implement_trait)]
        pub fn from_str(s: &str) -> Self {
            Self {
                buf: CompactString::from(s),
            }
        }

        /// 追加字符串切片
        pub fn push_str(&mut self, s: &str) {
            self.buf.push_str(s);
        }

        /// 追加单个字符
        pub fn push(&mut self, c: char) {
            self.buf.push(c);
        }

        /// 返回字符串切片
        pub fn as_str(&self) -> &str {
            &self.buf
        }

        /// 判断是否为空
        pub fn is_empty(&self) -> bool {
            self.buf.is_empty()
        }

        /// 消耗缓冲区，返回 `String`
        pub fn into_string(self) -> String {
            self.buf.to_string()
        }

        /// 返回已存储的字节数
        pub fn len(&self) -> usize {
            self.buf.len()
        }
    }

    impl Default for SqlBuffer {
        fn default() -> Self {
            Self::new()
        }
    }

    impl std::fmt::Write for SqlBuffer {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.push_str(s);
            Ok(())
        }
    }
}

#[cfg(not(feature = "perf-smallstring"))]
mod inner {
    /// SQL 构造缓冲区（退化为 String）
    pub struct SqlBuffer {
        buf: String,
    }

    impl SqlBuffer {
        /// 创建空缓冲区
        pub fn new() -> Self {
            Self { buf: String::new() }
        }

        /// 从字符串切片创建
        #[allow(clippy::should_implement_trait)]
        pub fn from_str(s: &str) -> Self {
            Self { buf: s.to_string() }
        }

        /// 追加字符串切片
        pub fn push_str(&mut self, s: &str) {
            self.buf.push_str(s);
        }

        /// 追加单个字符
        pub fn push(&mut self, c: char) {
            self.buf.push(c);
        }

        /// 返回字符串切片
        pub fn as_str(&self) -> &str {
            &self.buf
        }

        /// 判断是否为空
        pub fn is_empty(&self) -> bool {
            self.buf.is_empty()
        }

        /// 消耗缓冲区，返回 `String`
        pub fn into_string(self) -> String {
            self.buf
        }

        /// 返回已存储的字节数
        pub fn len(&self) -> usize {
            self.buf.len()
        }
    }

    impl Default for SqlBuffer {
        fn default() -> Self {
            Self::new()
        }
    }

    impl std::fmt::Write for SqlBuffer {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.push_str(s);
            Ok(())
        }
    }
}

pub use inner::SqlBuffer;

#[cfg(test)]
mod tests {
    use super::SqlBuffer;

    #[test]
    fn test_sql_buffer_basic() {
        let mut buf = SqlBuffer::new();
        buf.push_str("SELECT ");
        buf.push_str("* FROM users");
        assert_eq!(buf.as_str(), "SELECT * FROM users");
        assert!(!buf.is_empty());
        assert_eq!(buf.len(), 19);
    }

    #[test]
    fn test_sql_buffer_from_str() {
        let buf = SqlBuffer::from_str("SELECT * FROM users");
        assert_eq!(buf.as_str(), "SELECT * FROM users");
        let s = buf.into_string();
        assert_eq!(s, "SELECT * FROM users");
    }

    #[test]
    fn test_sql_buffer_push_char() {
        let mut buf = SqlBuffer::new();
        buf.push('A');
        buf.push('B');
        buf.push('C');
        assert_eq!(buf.as_str(), "ABC");
    }

    #[test]
    fn test_sql_buffer_empty() {
        let buf = SqlBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_sql_buffer_into_string() {
        let mut buf = SqlBuffer::new();
        buf.push_str("INSERT INTO users (name) VALUES ('test')");
        let sql = buf.into_string();
        assert_eq!(sql, "INSERT INTO users (name) VALUES ('test')");
    }

    #[test]
    fn test_sql_buffer_write_fmt() {
        use std::fmt::Write;
        let mut buf = SqlBuffer::new();
        let cols = "id, name";
        let table = "users";
        write!(buf, "SELECT {cols} FROM {table}").unwrap();
        assert_eq!(buf.as_str(), "SELECT id, name FROM users");
    }
}
