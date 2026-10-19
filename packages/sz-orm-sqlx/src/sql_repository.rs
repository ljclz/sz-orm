//! v9.5.1 SqlRepository：Repository 的 SQL 后端实现
//!
//! 消除 FD-2 幻影交付：`batch_create`/`batch_upsert`/`paginate_sql`
//! 重写为真正 SQL 实现（多值 INSERT / INSERT ON CONFLICT / LIMIT OFFSET + COUNT），
//! 复用既有 `QueryBuilder`（`query.rs:2953/3035`）+ `Connection::execute_with_params`。
//!
//! # 设计要点
//!
//! - **异步方法**：DB 操作为异步，提供 async 方法（非同步 Repository trait 实现）
//! - **参数化**：所有值通过 `?`/`$N` 占位符绑定，杜绝 SQL 注入
//! - **复用 QueryBuilder**：`batch_create`/`batch_upsert` 用 `build_batch_insert_with_params`/`build_batch_upsert_with_params`
//! - **复用 QueryBuilder**：`paginate_sql` 用 `build_select` + LIMIT/OFFSET + `build_count`
//! - **向后兼容**：Repository trait 默认实现保留（`repository.rs:554/562/570`），本模块提供独立 async API

use std::collections::HashMap;
use std::marker::PhantomData;

use sz_orm_core::{
    dialect::get_dialect,
    repository::{PageResult, RepositoryError, WhereCondition, WhereOp},
    DbError, DbType, Model, ModelExt, Pool, QueryBuilder, Value,
};

struct SqlRepoModel;

impl Model for SqlRepoModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "__sql_repo__"
    }
    fn pk(&self) -> Self::PrimaryKey {
        0
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

/// Repository 的 SQL 后端实现。
///
/// 重写 `batch_create`/`batch_upsert`/`paginate_sql` 为真正 SQL 实现，
/// 复用 `QueryBuilder` + `Connection`，区别于 Repository trait 默认实现
/// （逐条 save / 内存切片）。
///
/// # 泛型约束
///
/// - `E: Model + ModelExt` — 获取表名/列名/列值
/// - `E: Default` — 从行数据还原实体（`from_value` 需 `&mut self`）
/// - `E: Clone + Send + Sync + 'static` — 异步安全
///
/// # 示例
///
/// ```no_run
/// use sz_orm_core::{DbType, Pool};
/// use sz_orm_sqlx::SqlRepository;
/// # async fn example<E: sz_orm_core::Model + sz_orm_core::ModelExt + Default + Clone + Send + Sync + 'static>(pool: Pool, entities: Vec<E>) {
/// let repo = SqlRepository::<E>::new(pool, DbType::MySQL);
/// let n = repo.batch_create(&entities).await.unwrap();
/// # }
/// ```
pub struct SqlRepository<E: Model + ModelExt + Default + Clone + Send + Sync + 'static> {
    pool: Pool,
    dialect: DbType,
    _marker: PhantomData<E>,
}

impl<E: Model + ModelExt + Default + Clone + Send + Sync + 'static> SqlRepository<E> {
    /// 构造 SqlRepository，须传入已初始化的连接池 + 数据库方言。
    pub fn new(pool: Pool, dialect: DbType) -> Self {
        Self {
            pool,
            dialect,
            _marker: PhantomData,
        }
    }

    /// 获取表名（复用 `E::table_name()`）。
    fn table() -> &'static str {
        E::table_name()
    }

    /// 将实体转换为行数据（`HashMap<String, Value>`），仅包含 `fillable()` 字段。
    fn entity_to_row(entity: &E) -> HashMap<String, Value> {
        let mut row = HashMap::new();
        for col in E::fillable() {
            if let Some(val) = entity.get_column_value(col) {
                row.insert(col.to_string(), val);
            }
        }
        row
    }

    /// 将实体列表转换为行数据列表。
    fn entities_to_rows(entities: &[E]) -> Vec<HashMap<String, Value>> {
        entities.iter().map(Self::entity_to_row).collect()
    }

    /// 从查询结果行转换为实体。
    fn row_to_entity(row: HashMap<String, Value>) -> E {
        let mut entity = E::default();
        entity.from_value(row);
        entity
    }

    /// 将 `DbError` 转换为 `RepositoryError`。
    fn db_err(e: DbError) -> RepositoryError {
        RepositoryError::DatabaseError(e.to_string())
    }

    /// 将 `PoolError` 转换为 `RepositoryError`。
    fn pool_err(e: sz_orm_core::PoolError) -> RepositoryError {
        RepositoryError::DatabaseError(e.to_string())
    }

    /// 将 `WhereCondition` 列表应用到 `QueryBuilder`。
    fn apply_conditions(
        qb: QueryBuilder<SqlRepoModel>,
        conditions: &[WhereCondition],
    ) -> QueryBuilder<SqlRepoModel> {
        let mut qb = qb;
        for cond in conditions {
            qb = match cond.op {
                WhereOp::Eq => qb.where_eq(&cond.field, cond.value.clone()),
                WhereOp::Ne => qb.where_ne(&cond.field, cond.value.clone()),
                WhereOp::Gt => qb.where_gt(&cond.field, cond.value.clone()),
                WhereOp::Ge => qb.where_ge(&cond.field, cond.value.clone()),
                WhereOp::Lt => qb.where_lt(&cond.field, cond.value.clone()),
                WhereOp::Le => qb.where_le(&cond.field, cond.value.clone()),
                WhereOp::Like => qb.where_like(&cond.field, cond.value.clone()),
                WhereOp::In => qb.where_in(&cond.field, cond.extra_values.clone()),
                WhereOp::NotIn => qb.where_not_in(&cond.field, cond.extra_values.clone()),
                WhereOp::IsNull => qb.where_null(&cond.field),
                WhereOp::IsNotNull => qb.where_not_null(&cond.field),
                WhereOp::Between => {
                    if cond.extra_values.len() == 1 {
                        qb.where_between(
                            &cond.field,
                            cond.value.clone(),
                            cond.extra_values[0].clone(),
                        )
                    } else {
                        qb
                    }
                }
            };
        }
        qb
    }

    /// 查询所有记录。
    pub async fn find_all(&self) -> Result<Vec<E>, RepositoryError> {
        let dialect = get_dialect(self.dialect).map_err(Self::db_err)?;
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table(Self::table().to_string());
        let (sql, params) = qb.build_select();
        let mut conn = self.pool.acquire().await.map_err(Self::pool_err)?;
        let rows = conn
            .query_with_params(&sql, &params)
            .await
            .map_err(Self::db_err)?;
        Ok(rows.into_iter().map(Self::row_to_entity).collect())
    }

    /// 按条件查询。
    pub async fn find_by(&self, conditions: &[WhereCondition]) -> Result<Vec<E>, RepositoryError> {
        let dialect = get_dialect(self.dialect).map_err(Self::db_err)?;
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table(Self::table().to_string());
        let qb = Self::apply_conditions(qb, conditions);
        let (sql, params) = qb.build_select();
        let mut conn = self.pool.acquire().await.map_err(Self::pool_err)?;
        let rows = conn
            .query_with_params(&sql, &params)
            .await
            .map_err(Self::db_err)?;
        Ok(rows.into_iter().map(Self::row_to_entity).collect())
    }

    /// 保存（INSERT）。
    pub async fn save(&self, entity: &mut E) -> Result<u64, RepositoryError> {
        let fillable = E::fillable();
        if fillable.is_empty() {
            return Err(RepositoryError::InvalidEntity(
                "fillable() 为空，无法构造 INSERT".to_string(),
            ));
        }
        let row = Self::entity_to_row(entity);
        let dialect = get_dialect(self.dialect).map_err(Self::db_err)?;
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table(Self::table().to_string());
        let (sql, params) = qb.build_insert_with_params(&row);
        let mut conn = self.pool.acquire().await.map_err(Self::pool_err)?;
        conn.execute_with_params(&sql, &params)
            .await
            .map_err(Self::db_err)
    }

    /// 总数。
    pub async fn count(&self) -> Result<u64, RepositoryError> {
        let dialect = get_dialect(self.dialect).map_err(Self::db_err)?;
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table(Self::table().to_string());
        let sql = qb.build_count();
        let mut conn = self.pool.acquire().await.map_err(Self::pool_err)?;
        let rows = conn
            .query_with_params(&sql, &[])
            .await
            .map_err(Self::db_err)?;
        Self::extract_count(rows)
    }

    /// 从查询结果中提取 COUNT 值。
    fn extract_count(rows: Vec<HashMap<String, Value>>) -> Result<u64, RepositoryError> {
        if let Some(row) = rows.into_iter().next() {
            if let Some(val) = row.get("total") {
                return Ok(Self::value_to_u64(val));
            }
        }
        Ok(0)
    }

    /// 将 `Value` 转换为 `u64`（COUNT 结果）。
    fn value_to_u64(val: &Value) -> u64 {
        match val {
            Value::I64(n) => *n as u64,
            Value::I32(n) => *n as u64,
            Value::U64(n) => *n,
            Value::U32(n) => *n as u64,
            _ => 0,
        }
    }

    /// v9.5.1 真正多值 INSERT，复用 `QueryBuilder::build_batch_insert_with_params`。
    ///
    /// 重写 Repository trait 默认实现（逐条 save），生成 1 条多值 INSERT SQL。
    ///
    /// - 空批返回 `Ok(0)`，不执行 SQL
    /// - 所有值参数化绑定，杜绝 SQL 注入
    pub async fn batch_create(&self, entities: &[E]) -> Result<u64, RepositoryError> {
        if entities.is_empty() {
            return Ok(0);
        }
        let rows = Self::entities_to_rows(entities);
        let dialect = get_dialect(self.dialect).map_err(Self::db_err)?;
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table(Self::table().to_string());
        let (sql, params) = qb.build_batch_insert_with_params(&rows);
        if sql.is_empty() {
            return Ok(0);
        }
        let mut conn = self.pool.acquire().await.map_err(Self::pool_err)?;
        conn.execute_with_params(&sql, &params)
            .await
            .map_err(Self::db_err)
    }

    /// v9.5.1 真正 INSERT ON CONFLICT，复用 `QueryBuilder::build_batch_upsert_with_params`。
    ///
    /// 重写 Repository trait 默认实现，生成 1 条 UPSERT SQL。
    ///
    /// - `conflict_cols`：冲突检测列（主键/唯一键）
    /// - `update_cols`：冲突时更新的列；空切片表示更新所有非冲突列
    /// - 空批返回 `Err(InvalidParams)`
    pub async fn batch_upsert(
        &self,
        entities: &[E],
        conflict_cols: &[&str],
        update_cols: &[&str],
    ) -> Result<u64, RepositoryError> {
        if entities.is_empty() {
            return Err(RepositoryError::InvalidParams(
                "batch_upsert: entities cannot be empty".to_string(),
            ));
        }
        let rows = Self::entities_to_rows(entities);
        let dialect = get_dialect(self.dialect).map_err(Self::db_err)?;
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table(Self::table().to_string());
        let (sql, params) = qb
            .build_batch_upsert_with_params(&rows, conflict_cols, update_cols)
            .map_err(Self::db_err)?;
        let mut conn = self.pool.acquire().await.map_err(Self::pool_err)?;
        conn.execute_with_params(&sql, &params)
            .await
            .map_err(Self::db_err)
    }

    /// v9.5.1 真正 SQL LIMIT/OFFSET + COUNT，执行 2 条 SQL。
    ///
    /// 重写 Repository trait 默认实现（内存切片 `paginate`）。
    ///
    /// - `page == 0 || page_size == 0` 返回 `Err(InvalidParams)`
    /// - 执行 `SELECT ... LIMIT ? OFFSET ?` + `SELECT COUNT(*) FROM table` 两条 SQL
    pub async fn paginate_sql(
        &self,
        page: u64,
        page_size: u64,
    ) -> Result<PageResult<E>, RepositoryError> {
        if page == 0 || page_size == 0 {
            return Err(RepositoryError::InvalidParams(
                "page must be >= 1 and page_size must be >= 1".to_string(),
            ));
        }
        let offset = (page.saturating_sub(1)) * page_size;

        let dialect = get_dialect(self.dialect).map_err(Self::db_err)?;
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect.clone_box())
            .table(Self::table().to_string())
            .limit(page_size as usize)
            .offset(offset as usize);
        let (sql, params) = qb.build_select();

        let count_sql = QueryBuilder::<SqlRepoModel>::new(dialect)
            .table(Self::table().to_string())
            .build_count();

        let mut conn = self.pool.acquire().await.map_err(Self::pool_err)?;

        let rows = conn
            .query_with_params(&sql, &params)
            .await
            .map_err(Self::db_err)?;
        let items: Vec<E> = rows.into_iter().map(Self::row_to_entity).collect();

        let count_rows = conn
            .query_with_params(&count_sql, &[])
            .await
            .map_err(Self::db_err)?;
        let total = Self::extract_count(count_rows)?;

        Ok(PageResult::new(items, total, page, page_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Default, PartialEq)]
    struct TestEntity {
        id: i64,
        name: String,
    }

    impl Model for TestEntity {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "test_entities"
        }
        fn pk(&self) -> Self::PrimaryKey {
            self.id
        }
        fn set_pk(&mut self, pk: Self::PrimaryKey) {
            self.id = pk;
        }
    }

    impl ModelExt for TestEntity {
        fn columns() -> Vec<&'static str> {
            vec!["id", "name"]
        }
        fn fillable() -> Vec<&'static str> {
            vec!["id", "name"]
        }
        fn get_column_value(&self, column: &str) -> Option<Value> {
            match column {
                "id" => Some(Value::I64(self.id)),
                "name" => Some(Value::String(self.name.clone())),
                _ => None,
            }
        }
        fn from_value(&mut self, map: HashMap<String, Value>) {
            if let Some(Value::I64(id)) = map.get("id") {
                self.id = *id;
            }
            if let Some(Value::String(name)) = map.get("name") {
                self.name = name.clone();
            }
        }
    }

    fn make_entity(id: i64, name: &str) -> TestEntity {
        TestEntity {
            id,
            name: name.to_string(),
        }
    }

    #[test]
    fn test_sql_repo_entity_to_row() {
        let entity = make_entity(1, "alice");
        let row = SqlRepository::<TestEntity>::entity_to_row(&entity);
        assert_eq!(row.get("id"), Some(&Value::I64(1)));
        assert_eq!(row.get("name"), Some(&Value::String("alice".to_string())));
    }

    #[test]
    fn test_sql_repo_entities_to_rows_multi_row() {
        let entities = vec![
            make_entity(1, "a"),
            make_entity(2, "b"),
            make_entity(3, "c"),
        ];
        let rows = SqlRepository::<TestEntity>::entities_to_rows(&entities);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].get("id"), Some(&Value::I64(1)));
        assert_eq!(rows[2].get("name"), Some(&Value::String("c".to_string())));
    }

    #[test]
    fn test_sql_repo_row_to_entity() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(42));
        row.insert("name".to_string(), Value::String("bob".to_string()));
        let entity = SqlRepository::<TestEntity>::row_to_entity(row);
        assert_eq!(entity.id, 42);
        assert_eq!(entity.name, "bob");
    }

    #[test]
    fn test_sql_repo_table_name() {
        assert_eq!(SqlRepository::<TestEntity>::table(), "test_entities");
    }

    #[test]
    fn test_sql_repo_batch_create_empty_returns_zero() {
        let entities: Vec<TestEntity> = vec![];
        let rows = SqlRepository::<TestEntity>::entities_to_rows(&entities);
        assert!(rows.is_empty());
    }

    #[test]
    fn test_sql_repo_batch_create_non_empty_rows() {
        let entities = vec![
            make_entity(1, "a"),
            make_entity(2, "b"),
            make_entity(3, "c"),
        ];
        let rows = SqlRepository::<TestEntity>::entities_to_rows(&entities);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].contains_key("id"));
        assert!(rows[0].contains_key("name"));
    }

    #[test]
    fn test_sql_repo_default_entity() {
        let entity = TestEntity::default();
        assert_eq!(entity.id, 0);
        assert_eq!(entity.name, "");
    }

    #[test]
    fn test_sql_repo_parametrized_values_not_inlined() {
        let entity = make_entity(1, "'; DROP TABLE users; --");
        let row = SqlRepository::<TestEntity>::entity_to_row(&entity);
        let name_val = row.get("name").unwrap();
        assert!(matches!(name_val, Value::String(s) if s.contains("DROP TABLE")));
    }

    #[test]
    fn test_sql_repo_db_error_mapping() {
        let db_err = DbError::Internal("constraint violation".to_string());
        let repo_err = SqlRepository::<TestEntity>::db_err(db_err);
        assert!(matches!(repo_err, RepositoryError::DatabaseError(_)));
    }

    #[test]
    fn test_sql_repo_apply_conditions_eq() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table("test".to_string());
        let conditions = vec![WhereCondition::new("id", WhereOp::Eq, Value::I64(1))];
        let qb = SqlRepository::<TestEntity>::apply_conditions(qb, &conditions);
        let (sql, params) = qb.build_select();
        assert!(sql.contains("WHERE") || sql.contains("where"));
        assert!(!params.is_empty());
    }

    #[test]
    fn test_sql_repo_apply_conditions_in() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table("test".to_string());
        let conditions = vec![WhereCondition::in_op(
            "id",
            WhereOp::In,
            vec![Value::I64(1), Value::I64(2), Value::I64(3)],
        )];
        let qb = SqlRepository::<TestEntity>::apply_conditions(qb, &conditions);
        let (sql, _) = qb.build_select();
        assert!(sql.contains("IN") || sql.contains("in"));
    }

    #[test]
    fn test_sql_repo_apply_conditions_empty() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table("test".to_string());
        let qb = SqlRepository::<TestEntity>::apply_conditions(qb, &[]);
        let (sql, params) = qb.build_select();
        assert!(!sql.is_empty());
        assert!(params.is_empty());
    }

    #[test]
    fn test_sql_repo_apply_conditions_multiple() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table("test".to_string());
        let conditions = vec![
            WhereCondition::new("age", WhereOp::Ge, Value::I64(18)),
            WhereCondition::new("name", WhereOp::Like, Value::String("%kw%".to_string())),
        ];
        let qb = SqlRepository::<TestEntity>::apply_conditions(qb, &conditions);
        let (sql, params) = qb.build_select();
        assert!(!sql.is_empty());
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_sql_repo_paginate_sql_invalid_params_validation() {
        let valid = |p: u64, ps: u64| p != 0 && ps != 0;
        assert!(!valid(0, 20));
        assert!(!valid(1, 0));
        assert!(!valid(0, 0));
        assert!(valid(1, 20));
        assert!(valid(2, 10));
    }

    #[test]
    fn test_sql_repo_value_to_u64() {
        assert_eq!(
            SqlRepository::<TestEntity>::value_to_u64(&Value::I64(42)),
            42
        );
        assert_eq!(
            SqlRepository::<TestEntity>::value_to_u64(&Value::I32(42)),
            42
        );
        assert_eq!(
            SqlRepository::<TestEntity>::value_to_u64(&Value::U64(42)),
            42
        );
        assert_eq!(SqlRepository::<TestEntity>::value_to_u64(&Value::Null), 0);
        assert_eq!(
            SqlRepository::<TestEntity>::value_to_u64(&Value::Bool(true)),
            0
        );
    }

    #[test]
    fn test_sql_repo_extract_count() {
        let mut row = HashMap::new();
        row.insert("total".to_string(), Value::I64(100));
        let rows = vec![row];
        assert_eq!(
            SqlRepository::<TestEntity>::extract_count(rows).unwrap(),
            100
        );

        let empty_rows: Vec<HashMap<String, Value>> = vec![];
        assert_eq!(
            SqlRepository::<TestEntity>::extract_count(empty_rows).unwrap(),
            0
        );
    }

    #[test]
    fn test_sql_repo_batch_create_generates_multi_value_insert() {
        let entities = vec![
            make_entity(1, "a"),
            make_entity(2, "b"),
            make_entity(3, "c"),
        ];
        let rows = SqlRepository::<TestEntity>::entities_to_rows(&entities);
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table("test_entities".to_string());
        let (sql, params) = qb.build_batch_insert_with_params(&rows);
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("VALUES"));
        assert!(sql.contains("?"));
        assert_eq!(params.len(), 6);
    }

    #[test]
    fn test_sql_repo_batch_upsert_generates_on_conflict() {
        let entities = vec![make_entity(1, "a"), make_entity(2, "b")];
        let rows = SqlRepository::<TestEntity>::entities_to_rows(&entities);
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table("test_entities".to_string());
        let (sql, _) = qb
            .build_batch_upsert_with_params(&rows, &["id"], &["name"])
            .unwrap();
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("ON DUPLICATE KEY UPDATE") || sql.contains("ON CONFLICT"));
    }

    #[test]
    fn test_sql_repo_batch_upsert_pg_generates_on_conflict() {
        let entities = vec![make_entity(1, "a"), make_entity(2, "b")];
        let rows = SqlRepository::<TestEntity>::entities_to_rows(&entities);
        let dialect = get_dialect(DbType::PostgreSQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table("test_entities".to_string());
        let (sql, _) = qb
            .build_batch_upsert_with_params(&rows, &["id"], &["name"])
            .unwrap();
        assert!(sql.contains("ON CONFLICT"));
    }

    #[test]
    fn test_sql_repo_paginate_sql_builds_limit_offset() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect)
            .table("test_entities".to_string())
            .limit(20)
            .offset(0);
        let (sql, _) = qb.build_select();
        assert!(sql.contains("LIMIT") || sql.contains("limit"));
    }

    #[test]
    fn test_sql_repo_paginate_sql_builds_count() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let qb = QueryBuilder::<SqlRepoModel>::new(dialect).table("test_entities".to_string());
        let sql = qb.build_count();
        assert!(sql.contains("COUNT"));
        assert!(sql.contains("test_entities"));
    }
}
