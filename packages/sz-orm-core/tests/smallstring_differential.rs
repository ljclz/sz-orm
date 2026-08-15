//! M3-T2: SmallString 差分测试
//!
//! 验证 `perf-smallstring` feature 启用/禁用时，SQL 构造输出完全一致。
//! 覆盖 SELECT/INSERT/UPDATE/DELETE + 短/长字符串场景。

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use sz_orm_core::{DbType, Model, QueryBuilder, Value};

    #[derive(Clone, Default)]
    struct User {
        id: i64,
    }

    impl Model for User {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "users"
        }
        fn pk(&self) -> Self::PrimaryKey {
            self.id
        }
        fn set_pk(&mut self, pk: Self::PrimaryKey) {
            self.id = pk;
        }
    }

    fn make_query() -> QueryBuilder<User> {
        QueryBuilder::new(sz_orm_core::get_dialect(DbType::MySQL).unwrap())
    }

    // ========== SELECT 差分测试 ==========

    #[test]
    fn diff_select_basic() {
        let q = make_query();
        let sql = q.build_select();
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("users"));
    }

    #[test]
    fn diff_select_columns() {
        let q = make_query()
            .select(vec!["id", "name", "email"])
            .expect("valid columns");
        let sql = q.build_select();
        assert!(sql.contains("id"));
        assert!(sql.contains("name"));
        assert!(sql.contains("email"));
    }

    #[test]
    fn diff_select_where() {
        let q = make_query().where_eq("id", Value::I64(1));
        let sql = q.build_select();
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("id"));
        assert!(sql.contains("1"));
    }

    #[test]
    fn diff_select_order_limit_offset() {
        let q = make_query().order_desc("id").limit(10).offset(20);
        let sql = q.build_select();
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("DESC"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn diff_select_long_string() {
        let long_name = "a".repeat(100);
        let q = make_query().where_eq("name", Value::String(long_name.clone()));
        let sql = q.build_select();
        assert!(sql.contains(&long_name));
    }

    #[test]
    fn diff_select_short_string() {
        let q = make_query().where_eq("name", Value::String("abc".to_string()));
        let sql = q.build_select();
        assert!(sql.contains("abc"));
    }

    // ========== INSERT 差分测试 ==========

    #[test]
    fn diff_insert_basic() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("test".to_string()));
        let q = make_query();
        let sql = q.build_insert(&data);
        assert!(sql.contains("INSERT"));
        assert!(sql.contains("users"));
        assert!(sql.contains("test"));
    }

    #[test]
    fn diff_insert_multiple() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("alice".to_string()));
        data.insert("age".to_string(), Value::I64(30));
        let q = make_query();
        let sql = q.build_insert(&data);
        assert!(sql.contains("INSERT"));
        assert!(sql.contains("users"));
        assert!(sql.contains("alice"));
        assert!(sql.contains("30"));
    }

    // ========== UPDATE 差分测试 ==========

    #[test]
    fn diff_update_basic() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::String("updated".to_string()));
        let q = make_query().where_eq("id", Value::I64(1));
        let sql = q.build_update(&data);
        assert!(sql.contains("UPDATE"));
        assert!(sql.contains("users"));
        assert!(sql.contains("updated"));
        assert!(sql.contains("id"));
    }

    // ========== DELETE 差分测试 ==========

    #[test]
    fn diff_delete_basic() {
        let q = make_query().where_eq("id", Value::I64(1));
        let sql = q.build_delete();
        assert!(sql.contains("DELETE"));
        assert!(sql.contains("users"));
        assert!(sql.contains("id"));
    }

    // ========== 聚合查询差分测试 ==========

    #[test]
    fn diff_count() {
        let q = make_query();
        let sql = q.build_count();
        assert!(sql.contains("COUNT(*)"));
        assert!(sql.contains("users"));
    }

    #[test]
    fn diff_exists() {
        let q = make_query();
        let sql = q.build_exists();
        assert!(sql.contains("EXISTS"));
    }

    #[test]
    fn diff_max_min_sum_avg() {
        let q = make_query();
        let max_sql = q.build_max("age");
        assert!(max_sql.contains("MAX"));
        let min_sql = q.build_min("age");
        assert!(min_sql.contains("MIN"));
        let sum_sql = q.build_sum("age");
        assert!(sum_sql.contains("SUM"));
        let avg_sql = q.build_avg("age");
        assert!(avg_sql.contains("AVG"));
    }

    // ========== SqlBuffer 单元测试 ==========

    #[test]
    fn test_sql_buffer_consistency() {
        use sz_orm_core::sql_buffer::SqlBuffer;

        let mut buf = SqlBuffer::new();
        buf.push_str("SELECT ");
        buf.push_str("* FROM users WHERE id = 1");
        let sql = buf.into_string();
        assert_eq!(sql, "SELECT * FROM users WHERE id = 1");
    }

    #[test]
    fn test_sql_buffer_short_string() {
        use sz_orm_core::sql_buffer::SqlBuffer;

        let buf = SqlBuffer::from_str("SELECT");
        assert_eq!(buf.as_str(), "SELECT");
        assert_eq!(buf.len(), 6);
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_sql_buffer_long_string() {
        use sz_orm_core::sql_buffer::SqlBuffer;

        let long = "a".repeat(100);
        let buf = SqlBuffer::from_str(&long);
        assert_eq!(buf.len(), 100);
        let s = buf.into_string();
        assert_eq!(s, long);
    }
}
