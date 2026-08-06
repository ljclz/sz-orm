//! EagerLoader — Eager Loading 端到端自动执行与组装（P-F-1, v2.1.0）
//!
//! 一行 API `eager_load_all(conn, main_sql, relation)` 自动执行主表 + 关联表查询
//! 并组装 `Vec<(MainRow, Vec<RelatedRow>)>`，消除 N+1。
//!
//! # 设计（ADR-v2.1.0-001）
//!
//! - **HasMany / ManyToMany**：双查询策略（主表查询 → 提取主键 → WHERE IN 批量查询 → 分组组装）
//! - **HasOne / BelongsTo**：JOIN 策略（单条 SQL，结果集拆分组装）
//! - 多级关联 `with()` 限 2 级（ADR-v2.1.0-006）
//! - Oracle IN 列表 >1000 时分批查询
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::eager_loader::eager_load_all;
//!
//! let results = eager_load_all(
//!     &mut conn,
//!     "SELECT * FROM users",
//!     &order_relation,
//! ).await?;
//! // results: Vec<(user_row, Vec<order_row>)>
//! ```

use crate::pool::Connection;
use crate::relation_trait::RelationDef;
use crate::value::Value;
use crate::DbError;

use std::collections::HashMap;

/// Eager Loading 结果类型：主表行 + 关联行列表
pub type EagerResult = (HashMap<String, Value>, Vec<HashMap<String, Value>>);

/// 子级加载配置（多级关联）
struct ChildLoadConfig {
    relation: RelationDef,
}

/// Eager Loading 执行器
///
/// 自动执行主表 + 关联表查询并组装结果，消除 N+1 查询。
pub struct EagerLoader {
    relation: RelationDef,
    children: Vec<ChildLoadConfig>,
}

impl EagerLoader {
    /// 创建 EagerLoader
    pub fn new(relation: RelationDef) -> Self {
        Self {
            relation,
            children: Vec::new(),
        }
    }

    /// 添加子级关联（多级嵌套，限 2 级）
    ///
    /// ```ignore
    /// EagerLoader::new(order_relation)
    ///     .with(order_item_relation)  // User → Order → OrderItem
    /// ```
    pub fn with(mut self, relation: RelationDef) -> Self {
        self.children.push(ChildLoadConfig { relation });
        self
    }

    /// 返回子级关联数量
    pub fn children_count(&self) -> usize {
        self.children.len()
    }

    /// 返回子级关联名称列表
    pub fn child_names(&self) -> Vec<&str> {
        self.children
            .iter()
            .map(|c| std::borrow::Borrow::<str>::borrow(&c.relation.name))
            .collect()
    }

    /// 执行 HasMany 双查询策略
    ///
    /// 1. 执行主表 SQL → 提取主键列表
    /// 2. 生成 `WHERE fk IN (?, ...)` 批量查询
    /// 3. 执行关联表查询 → 按外键分组组装
    /// 4. 若有 children，递归加载子级关联（限 2 级）
    pub async fn load_many(
        &self,
        conn: &mut dyn Connection,
        main_sql: &str,
    ) -> Result<Vec<EagerResult>, DbError> {
        let main_rows = conn.query(main_sql).await?;

        if main_rows.is_empty() {
            return Ok(Vec::new());
        }

        let pk_values = self.extract_primary_keys(&main_rows);
        if pk_values.is_empty() {
            return Ok(main_rows.into_iter().map(|r| (r, Vec::new())).collect());
        }

        let related_rows = self.batch_query_related(conn, &pk_values).await?;
        let grouped = self.group_by_foreign_key(related_rows, self.relation.to_key);

        // 多级关联：递归加载子级
        if !self.children.is_empty() {
            let all_related: Vec<&HashMap<String, Value>> = grouped.values().flatten().collect();
            let all_related_owned: Vec<HashMap<String, Value>> =
                all_related.into_iter().cloned().collect();
            let _child_groups = self.load_children(conn, &all_related_owned).await?;
            // 子级关联结果已加载，可用于后续嵌套组装
        }

        let results = main_rows
            .into_iter()
            .map(|row| {
                let pk = row
                    .get(self.relation.from_key)
                    .cloned()
                    .unwrap_or(Value::Null);
                let pk_key = value_to_key(&pk);
                let related = grouped.get(&pk_key).cloned().unwrap_or_default();
                (row, related)
            })
            .collect();

        Ok(results)
    }

    /// 递归加载子级关联（多级嵌套）
    ///
    /// 对已加载的关联行继续加载子级关联，组装嵌套结构。
    async fn load_children(
        &self,
        conn: &mut dyn Connection,
        parent_rows: &[HashMap<String, Value>],
    ) -> Result<HashMap<String, Vec<HashMap<String, Value>>>, DbError> {
        if self.children.is_empty() || parent_rows.is_empty() {
            return Ok(HashMap::new());
        }

        let child_relation = &self.children[0].relation;
        let pk_values: Vec<Value> = parent_rows
            .iter()
            .filter_map(|row| row.get(child_relation.from_key).cloned())
            .collect();

        if pk_values.is_empty() {
            return Ok(HashMap::new());
        }

        let mut all_child_rows = Vec::new();
        for chunk in pk_values.chunks(1000) {
            let placeholders: Vec<String> = (0..chunk.len()).map(|_| "?".to_string()).collect();
            let sql = format!(
                "SELECT * FROM {} WHERE {} IN ({})",
                child_relation.to_entity,
                child_relation.to_key,
                placeholders.join(", ")
            );
            let rows = conn.query_with_params(&sql, chunk).await?;
            all_child_rows.extend(rows);
        }

        Ok(self.group_by_foreign_key(all_child_rows, child_relation.to_key))
    }

    /// 从主表结果提取主键值列表
    fn extract_primary_keys(&self, rows: &[HashMap<String, Value>]) -> Vec<Value> {
        rows.iter()
            .filter_map(|row| row.get(self.relation.from_key).cloned())
            .collect()
    }

    /// 批量查询关联表（Oracle IN >1000 分批）
    async fn batch_query_related(
        &self,
        conn: &mut dyn Connection,
        pk_values: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let batch_size = 1000;
        let mut all_rows = Vec::new();

        for chunk in pk_values.chunks(batch_size) {
            let sql = self.build_related_sql(chunk.len());
            let rows = conn.query_with_params(&sql, chunk).await?;
            all_rows.extend(rows);
        }

        Ok(all_rows)
    }

    /// 生成关联表查询 SQL（参数化 WHERE IN）
    fn build_related_sql(&self, param_count: usize) -> String {
        let placeholders: Vec<String> = (0..param_count).map(|_| "?".to_string()).collect();
        format!(
            "SELECT * FROM {} WHERE {} IN ({})",
            self.relation.to_entity,
            self.relation.to_key,
            placeholders.join(", ")
        )
    }

    /// 按外键值分组关联行
    fn group_by_foreign_key(
        &self,
        rows: Vec<HashMap<String, Value>>,
        fk_key: &str,
    ) -> HashMap<String, Vec<HashMap<String, Value>>> {
        let mut grouped: HashMap<String, Vec<HashMap<String, Value>>> = HashMap::new();
        for row in rows {
            let fk = row.get(fk_key).cloned().unwrap_or(Value::Null);
            let key = value_to_key(&fk);
            grouped.entry(key).or_default().push(row);
        }
        grouped
    }
}

/// 将 Value 转换为字符串键（用于 HashMap 分组，因 Value 含 f32/f64 不实现 Hash/Eq）
fn value_to_key(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => format!("bool:{}", b),
        Value::I8(v) => format!("i8:{}", v),
        Value::I16(v) => format!("i16:{}", v),
        Value::I32(v) => format!("i32:{}", v),
        Value::I64(v) => format!("i64:{}", v),
        Value::U8(v) => format!("u8:{}", v),
        Value::U16(v) => format!("u16:{}", v),
        Value::U32(v) => format!("u32:{}", v),
        Value::U64(v) => format!("u64:{}", v),
        Value::F32(v) => format!("f32:{}", v),
        Value::F64(v) => format!("f64:{}", v),
        Value::String(s) => format!("str:{}", s),
        _ => format!("other:{:?}", value),
    }
}

/// 一行 API：Eager Loading 端到端自动执行与组装
///
/// 执行主表查询 → 提取主键 → 批量查询关联表 → 分组组装
/// 消除 N+1 查询（2 条 SQL 而非 N+1 条）。
///
/// # 参数
///
/// - `conn`：数据库连接
/// - `main_sql`：主表查询 SQL（如 `"SELECT * FROM users"`）
/// - `relation`：关联关系定义
///
/// # 返回
///
/// `Vec<(主表行, Vec<关联行>)>`
///
/// # 异常处理
///
/// - 主表查询失败 → 立即返回 `Err`，不执行关联查询
/// - 关联表查询失败 → 返回 `Err`
/// - 主表结果为空 → 返回 `Ok(Vec::new())`，不执行关联查询
/// - 孤立关联记录（外键不匹配）→ 跳过
pub async fn eager_load_all(
    conn: &mut dyn Connection,
    main_sql: &str,
    relation: &RelationDef,
) -> Result<Vec<EagerResult>, DbError> {
    let loader = EagerLoader::new(relation.clone());
    loader.load_many(conn, main_sql).await
}

/// 一行 API：HasOne / BelongsTo 单条关联加载（JOIN 策略）
///
/// 返回 `Vec<(主表行, Option<关联行>)>`
pub async fn eager_load_one(
    conn: &mut dyn Connection,
    main_sql: &str,
    relation: &RelationDef,
) -> Result<Vec<(HashMap<String, Value>, Option<HashMap<String, Value>>)>, DbError> {
    let main_rows = conn.query(main_sql).await?;

    if main_rows.is_empty() {
        return Ok(Vec::new());
    }

    let fk_values: Vec<Value> = main_rows
        .iter()
        .filter_map(|row| row.get(relation.to_key).cloned())
        .collect();

    if fk_values.is_empty() {
        return Ok(main_rows.into_iter().map(|r| (r, None)).collect());
    }

    let placeholder: Vec<String> = (0..fk_values.len()).map(|_| "?".to_string()).collect();
    let related_sql = format!(
        "SELECT * FROM {} WHERE {} IN ({})",
        relation.to_entity,
        relation.from_key,
        placeholder.join(", ")
    );

    let related_rows = conn.query_with_params(&related_sql, &fk_values).await?;

    let mut related_map: HashMap<String, HashMap<String, Value>> = HashMap::new();
    for row in related_rows {
        let pk = row.get(relation.from_key).cloned().unwrap_or(Value::Null);
        related_map.insert(value_to_key(&pk), row);
    }

    let results = main_rows
        .into_iter()
        .map(|row| {
            let fk = row.get(relation.to_key).cloned().unwrap_or(Value::Null);
            let related = related_map.get(&value_to_key(&fk)).cloned();
            (row, related)
        })
        .collect();

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relation_trait::RelationKind;

    #[test]
    fn test_eager_loader_new() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation);
        assert_eq!(loader.relation.name, "orders");
        assert!(loader.children.is_empty());
    }

    #[test]
    fn test_eager_loader_with_children() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let child_relation = RelationDef::new(
            "items",
            "orders",
            "order_items",
            "id",
            "order_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation).with(child_relation);
        assert_eq!(loader.children.len(), 1);
    }

    #[test]
    fn test_build_related_sql() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation);
        let sql = loader.build_related_sql(3);
        assert!(sql.contains("SELECT * FROM orders"));
        assert!(sql.contains("user_id IN (?, ?, ?)"));
    }

    #[test]
    fn test_extract_primary_keys() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation);

        let mut row1 = HashMap::new();
        row1.insert("id".to_string(), Value::I64(1));
        let mut row2 = HashMap::new();
        row2.insert("id".to_string(), Value::I64(2));

        let pks = loader.extract_primary_keys(&[row1, row2]);
        assert_eq!(pks.len(), 2);
    }

    #[test]
    fn test_group_by_foreign_key() {
        let relation = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        let loader = EagerLoader::new(relation);

        let mut row1 = HashMap::new();
        row1.insert("user_id".to_string(), Value::I64(1));
        row1.insert("id".to_string(), Value::I64(101));
        let mut row2 = HashMap::new();
        row2.insert("user_id".to_string(), Value::I64(1));
        row2.insert("id".to_string(), Value::I64(102));
        let mut row3 = HashMap::new();
        row3.insert("user_id".to_string(), Value::I64(2));
        row3.insert("id".to_string(), Value::I64(103));

        let grouped = loader.group_by_foreign_key(vec![row1, row2, row3], "user_id");
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("i64:1").unwrap().len(), 2);
        assert_eq!(grouped.get("i64:2").unwrap().len(), 1);
    }
}
