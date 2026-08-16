//! KeysetPaginator — keyset 分页器
//!
//! 生成 `WHERE key > last_key ORDER BY key LIMIT batch` keyset 分页 SQL，
//! 避免 OFFSET 深翻页性能退化（索引扫描 vs 全表扫描）。

use serde_json::Value;

use crate::config::OrderDirection;

/// keyset 分页器
#[derive(Debug, Clone)]
pub struct KeysetPaginator {
    /// keyset 列名
    pub key_column: String,
    /// 上次最大 key 值
    pub last_key: Option<Value>,
    /// 批次大小
    pub batch_size: usize,
    /// 排序方向
    pub order_direction: OrderDirection,
    /// 是否还有更多数据
    has_more: bool,
}

impl KeysetPaginator {
    pub fn new(key_column: impl Into<String>, batch_size: usize) -> Self {
        Self {
            key_column: key_column.into(),
            last_key: None,
            batch_size: batch_size.max(1),
            order_direction: OrderDirection::Asc,
            has_more: true,
        }
    }

    pub fn with_order_direction(mut self, direction: OrderDirection) -> Self {
        self.order_direction = direction;
        self
    }

    /// 生成下一页 SQL
    ///
    /// - Asc: `SELECT ... WHERE key > last_key ORDER BY key ASC LIMIT batch`
    /// - Desc: `SELECT ... WHERE key < last_key ORDER BY key DESC LIMIT batch`
    /// - 首次（last_key = None）: `SELECT ... ORDER BY key ASC/DESC LIMIT batch`
    pub fn build_next_page_sql(&self, base_sql: &str) -> String {
        let base = base_sql.trim();
        let order = match self.order_direction {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        };
        match &self.last_key {
            None => format!(
                "{base} ORDER BY {} {order} LIMIT {}",
                self.key_column, self.batch_size
            ),
            Some(key) => {
                let key_str = format_key_value(key);
                let cmp = match self.order_direction {
                    OrderDirection::Asc => ">",
                    OrderDirection::Desc => "<",
                };
                format!(
                    "{base} WHERE {} {} {} ORDER BY {} {order} LIMIT {}",
                    self.key_column, cmp, key_str, self.key_column, self.batch_size
                )
            }
        }
    }

    /// 更新 last_key 为本批最后一行的键值
    pub fn update_last_key(&mut self, last_row: &Value) {
        if let Some(obj) = last_row.as_object() {
            self.last_key = obj.get(&self.key_column).cloned();
        }
    }

    /// 标记本批结果数量，判定是否还有更多
    pub fn mark_batch_result(&mut self, batch_len: usize) {
        if batch_len < self.batch_size {
            self.has_more = false;
        }
    }

    /// 是否还有更多数据
    pub fn has_more(&self) -> bool {
        self.has_more
    }
}

fn format_key_value(key: &Value) -> String {
    match key {
        Value::Null => "NULL".into(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Bool(b) => b.to_string(),
        n @ Value::Number(_) => n.to_string(),
        _ => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn first_page_no_where() {
        let paginator = KeysetPaginator::new("id", 1000);
        let sql = paginator.build_next_page_sql("SELECT * FROM t");
        assert!(sql.contains("ORDER BY id ASC"));
        assert!(sql.contains("LIMIT 1000"));
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn second_page_with_where() {
        let mut paginator = KeysetPaginator::new("id", 1000);
        paginator.last_key = Some(json!(1000));
        let sql = paginator.build_next_page_sql("SELECT * FROM t");
        assert!(sql.contains("WHERE id > 1000"));
        assert!(sql.contains("ORDER BY id ASC"));
        assert!(sql.contains("LIMIT 1000"));
    }

    #[test]
    fn desc_order() {
        let mut paginator =
            KeysetPaginator::new("id", 1000).with_order_direction(OrderDirection::Desc);
        paginator.last_key = Some(json!(500));
        let sql = paginator.build_next_page_sql("SELECT * FROM t");
        assert!(sql.contains("WHERE id < 500"));
        assert!(sql.contains("ORDER BY id DESC"));
    }

    #[test]
    fn update_last_key_from_row() {
        let mut paginator = KeysetPaginator::new("id", 1000);
        paginator.update_last_key(&json!({"id": 42, "name": "test"}));
        assert_eq!(paginator.last_key, Some(json!(42)));
    }

    #[test]
    fn has_more_true_initially() {
        let paginator = KeysetPaginator::new("id", 1000);
        assert!(paginator.has_more());
    }

    #[test]
    fn has_more_false_after_small_batch() {
        let mut paginator = KeysetPaginator::new("id", 1000);
        paginator.mark_batch_result(500);
        assert!(!paginator.has_more());
    }

    #[test]
    fn has_more_true_after_full_batch() {
        let mut paginator = KeysetPaginator::new("id", 1000);
        paginator.mark_batch_result(1000);
        assert!(paginator.has_more());
    }

    #[test]
    fn deep_page_keyset() {
        let mut paginator = KeysetPaginator::new("id", 1000);
        paginator.last_key = Some(json!(999999000));
        let sql = paginator.build_next_page_sql("SELECT * FROM big_table");
        assert!(sql.contains("WHERE id > 999999000"));
        assert!(sql.contains("LIMIT 1000"));
    }

    #[test]
    fn first_page_desc() {
        let paginator = KeysetPaginator::new("id", 1000).with_order_direction(OrderDirection::Desc);
        let sql = paginator.build_next_page_sql("SELECT * FROM t");
        assert!(sql.contains("ORDER BY id DESC"));
        assert!(!sql.contains("WHERE"));
    }

    #[test]
    fn string_key_value() {
        let mut paginator = KeysetPaginator::new("uuid", 100);
        paginator.last_key = Some(json!("abc-123"));
        let sql = paginator.build_next_page_sql("SELECT * FROM t");
        assert!(sql.contains("WHERE uuid > 'abc-123'"));
    }

    #[test]
    fn full_pagination_cycle() {
        // 模拟 250 行数据按 100/页 keyset 分页遍历：3 页全部取回
        let mut paginator = KeysetPaginator::new("id", 100);
        let mut fetched = 0usize;
        let mut page = 0;
        loop {
            let sql = paginator.build_next_page_sql("SELECT * FROM t");
            assert!(
                sql.starts_with("SELECT * FROM t"),
                "sql should keep base: {sql}"
            );
            // 模拟返回 100 行（最后一页 50 行）
            let batch = if fetched + 100 <= 250 {
                100
            } else {
                250 - fetched
            };
            fetched += batch;
            page += 1;
            // 用最后一行的 id 更新游标
            paginator.update_last_key(&json!(fetched));
            paginator.mark_batch_result(batch);
            if !paginator.has_more() || fetched >= 250 {
                break;
            }
        }
        assert_eq!(fetched, 250, "all rows should be fetched");
        assert_eq!(page, 3, "250 rows / 100 per page = 3 pages");
    }

    #[test]
    fn paginator_has_more_respects_batch_size() {
        let mut paginator = KeysetPaginator::new("id", 10);
        paginator.update_last_key(&json!(1));
        paginator.mark_batch_result(10);
        assert!(paginator.has_more(), "full batch => more pages");
        paginator.mark_batch_result(5);
        assert!(!paginator.has_more(), "short batch => last page");
    }

    #[test]
    fn build_sql_escapes_string_keys() {
        let mut paginator = KeysetPaginator::new("name", 10);
        paginator.last_key = Some(json!("O'Reilly"));
        let sql = paginator.build_next_page_sql("SELECT * FROM t");
        assert!(
            sql.contains("O''Reilly"),
            "single quote must be escaped: {sql}"
        );
    }

    #[test]
    fn test_desc_order_direction() {
        let paginator = KeysetPaginator::new("id", 500).with_order_direction(OrderDirection::Desc);
        let sql = paginator.build_next_page_sql("SELECT * FROM t");
        assert!(sql.contains("ORDER BY id DESC"));
        assert!(sql.contains("LIMIT 500"));
    }

    #[test]
    fn test_has_more_initial_state() {
        let paginator = KeysetPaginator::new("id", 100);
        assert!(paginator.has_more());
        let mut p2 = KeysetPaginator::new("id", 100);
        p2.mark_batch_result(0);
        assert!(!p2.has_more());
    }
}
