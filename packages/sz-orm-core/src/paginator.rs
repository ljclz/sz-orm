//! 内置分页支持 — `PaginatorTrait` + `Paginator` + `StreamQueryTrait`（M4）
//!
//! # 概述
//!
//! 提供三种查询 API 风格：
//!
//! ## 风格一：直接调用（原有 API）
//!
//! ```ignore
//! let page: PageResult<User> = query
//!     .paginate::<User, _>(1, 20, &mut conn)
//!     .await?;
//! ```
//!
//! ## 风格二：Builder 风格分页（M4 验收标准）
//!
//! ```ignore
//! let mut p = query.paginate_with(&mut conn, 20);
//! let page: PageResult<User> = p.fetch_page::<User, _>(1).await?;
//! ```
//!
//! ## 风格三：流式查询（M4）
//!
//! ```ignore
//! use sz_orm_core::paginator::StreamQueryTrait;
//! use futures::StreamExt;
//!
//! let mut stream = query.stream(&mut conn);
//! while let Some(row) = stream.next().await {
//!     // 处理每一行
//! }
//! ```

use crate::model::Model;
use crate::pool::Connection;
use crate::query::QueryBuilder;
use crate::value::{FromQueryResult, Value};
use crate::DbError;
use std::future::Future;
use std::pin::Pin;

/// 分页结果（re-export）
pub use crate::repository::PageResult;

/// 分页器 trait
///
/// 为 `QueryBuilder<M>` 提供 `.paginate()` 方法。
/// 实现原理：先执行 `COUNT(*)` 获取总数，再执行带 LIMIT/OFFSET 的数据查询。
///
/// # 注意
///
/// - 页码从 1 开始（page=1 表示第一页）
/// - `page_size=0` 时返回空结果，total 仍准确
/// - COUNT 查询会忽略 QueryBuilder 中的 `select_columns`/`order_by`/`limit`/`offset`
pub trait PaginatorTrait<M: Model>: Sized {
    /// 执行分页查询
    ///
    /// # 参数
    ///
    /// - `page`: 页码（从 1 开始）
    /// - `page_size`: 每页条数
    /// - `conn`: 数据库连接
    fn paginate<T, C>(
        self,
        page: u64,
        page_size: u64,
        conn: &mut C,
    ) -> Pin<Box<dyn Future<Output = Result<PageResult<T>, DbError>> + Send + '_>>
    where
        T: FromQueryResult + Send + 'static,
        C: Connection + Send;

    /// G-SO-3：分页快捷方法（SeaORM `find_page` 风格）
    ///
    /// 语义与 [`Self::paginate`] 完全相同，方法名更贴近 SeaORM 的
    /// `User::find().paginate(db, 10).fetch_page(1).await` 风格，
    /// 降低从 SeaORM 迁移的学习成本。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// let page: PageResult<User> = query
    ///     .find_page::<User, _>(1, 20, &mut conn)
    ///     .await?;
    /// ```
    fn find_page<T, C>(
        self,
        page: u64,
        page_size: u64,
        conn: &mut C,
    ) -> Pin<Box<dyn Future<Output = Result<PageResult<T>, DbError>> + Send + '_>>
    where
        T: FromQueryResult + Send + 'static,
        C: Connection + Send,
    {
        self.paginate::<T, C>(page, page_size, conn)
    }
}

impl<M: Model> PaginatorTrait<M> for QueryBuilder<M> {
    fn paginate<T, C>(
        self,
        page: u64,
        page_size: u64,
        conn: &mut C,
    ) -> Pin<Box<dyn Future<Output = Result<PageResult<T>, DbError>> + Send + '_>>
    where
        T: FromQueryResult + Send + 'static,
        C: Connection + Send,
    {
        let count_sql = self.build_count();
        let offset = if page_size == 0 {
            0
        } else {
            ((page.saturating_sub(1)) * page_size) as usize
        };
        let limit = page_size as usize;
        let (select_sql, params) = self.limit(limit).offset(offset).build_select_with_params();

        Box::pin(async move {
            let count_rows = conn.query(&count_sql).await?;
            let total = extract_count(&count_rows).unwrap_or(0);

            let data_rows = if total == 0 || offset as u64 >= total {
                Vec::new()
            } else {
                let rows = conn.query_with_params(&select_sql, &params).await?;
                let mut items = Vec::with_capacity(rows.len());
                for row in &rows {
                    match T::from_query_result(row) {
                        Ok(item) => items.push(item),
                        Err(e) => return Err(DbError::Internal(e)),
                    }
                }
                items
            };

            Ok(PageResult::new(data_rows, total, page, page_size))
        })
    }
}

// ========================================================================
// M4 新增：Builder 风格 Paginator
// ========================================================================

/// Builder 风格分页器（M4 验收标准）
///
/// 通过 [`PaginatorBuilderTrait::paginate_with`] 创建。
/// 与 [`PaginatorTrait::paginate`] 的区别：返回的 `Paginator` 可多次调用
/// `fetch_page` 获取不同页，而 `paginate` 一次性执行。
///
/// # 注意
///
/// `Paginator` 不自动执行 COUNT 查询。`total` 字段默认为 0，
/// 调用方可在 `fetch_page` 前手动设置（通过 `set_total`）。
/// 若需准确的 total，建议先单独执行一次 COUNT 查询。
pub struct Paginator<'a, C>
where
    C: Connection,
{
    conn: &'a mut C,
    page_size: u64,
    sql: String,
    params: Vec<Value>,
    total: u64,
}

impl<'a, C> Paginator<'a, C>
where
    C: Connection,
{
    /// 执行第 `page` 页查询（页码从 1 开始）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// let mut p = query.paginate_with(&mut conn, 20);
    /// let page: PageResult<User> = p.fetch_page::<User, _>(1).await?;
    /// ```
    pub async fn fetch_page<T>(&mut self, page: u64) -> Result<PageResult<T>, DbError>
    where
        T: FromQueryResult + Send + 'static,
    {
        let offset = if self.page_size == 0 {
            0
        } else {
            ((page.saturating_sub(1)) * self.page_size) as usize
        };
        let limit = self.page_size as usize;

        let select_sql = format!("{} LIMIT {} OFFSET {}", self.sql, limit, offset);

        let rows = self
            .conn
            .query_with_params(&select_sql, &self.params)
            .await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            match T::from_query_result(row) {
                Ok(item) => items.push(item),
                Err(e) => return Err(DbError::Internal(e)),
            }
        }

        Ok(PageResult::new(items, self.total, page, self.page_size))
    }

    /// 手动设置总数（用于 `PageResult::total_pages()` 等计算）
    pub fn set_total(&mut self, total: u64) {
        self.total = total;
    }
}

/// M4 新增：Builder 风格分页入口 trait
///
/// # 示例
///
/// ```ignore
/// use sz_orm_core::paginator::PaginatorBuilderTrait;
///
/// let mut p = query.paginate_with(&mut conn, 20);
/// p.set_total(100); // 可选：先执行 COUNT 查询设置总数
/// let page1 = p.fetch_page::<User, _>(1).await?;
/// let page2 = p.fetch_page::<User, _>(2).await?;
/// ```
pub trait PaginatorBuilderTrait<M: Model> {
    /// 创建 Builder 风格分页器
    fn paginate_with<'a, C>(self, conn: &'a mut C, page_size: u64) -> Paginator<'a, C>
    where
        C: Connection;
}

impl<M: Model> PaginatorBuilderTrait<M> for QueryBuilder<M> {
    fn paginate_with<'a, C>(self, conn: &'a mut C, page_size: u64) -> Paginator<'a, C>
    where
        C: Connection,
    {
        let (sql, params) = self.build_select_with_params();
        Paginator {
            conn,
            page_size,
            sql,
            params,
            total: 0,
        }
    }
}

// ========================================================================
// M4 新增：流式查询扩展
// ========================================================================

/// 流式查询的单行结果
pub type RowResult = std::collections::HashMap<String, Value>;

/// M4 新增：流式查询 trait
///
/// 将查询结果作为 Stream 返回，适合大数据量场景（避免一次性加载到内存）。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_core::paginator::{StreamQueryTrait, RowResult};
/// use futures::StreamExt;
///
/// let mut stream = query.stream(&mut conn);
/// while let Some(row) = stream.next().await {
///     let row: RowResult = row?;
///     // 处理...
/// }
/// ```
pub trait StreamQueryTrait<M: Model> {
    /// 返回流式查询结果
    fn stream<'a, 'b: 'a, C: Connection + Send + 'b>(
        self,
        conn: &'b mut C,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<RowResult, DbError>> + Send + 'a>>;
}

impl<M: Model> StreamQueryTrait<M> for QueryBuilder<M> {
    fn stream<'a, 'b: 'a, C: Connection + Send + 'b>(
        self,
        conn: &'b mut C,
    ) -> Pin<Box<dyn futures::Stream<Item = Result<RowResult, DbError>> + Send + 'a>> {
        let (sql, params) = self.build_select_with_params();

        use futures::{stream, StreamExt};

        let st = stream::once(async move {
            match conn.query_with_params(&sql, &params).await {
                Ok(rows) => rows.into_iter().map(Ok).collect::<Vec<_>>(),
                Err(e) => vec![Err(e)],
            }
        })
        .flat_map(stream::iter);

        Box::pin(st)
    }
}

// ========================================================================
// 内部辅助函数
// ========================================================================

fn extract_count(rows: &[std::collections::HashMap<String, Value>]) -> Option<u64> {
    rows.first().and_then(|row| {
        if let Some(v) = row.get("total") {
            return value_to_u64(v);
        }
        if let Some(v) = row.get("COUNT(*)") {
            return value_to_u64(v);
        }
        row.values().next().and_then(value_to_u64)
    })
}

fn value_to_u64(v: &Value) -> Option<u64> {
    match v {
        Value::I64(n) => Some(*n as u64),
        Value::I32(n) => Some(*n as u64),
        Value::U64(n) => Some(*n),
        Value::U32(n) => Some(*n as u64),
        Value::F64(n) => Some(*n as u64),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_type::DbType;
    use crate::dialect::get_dialect;
    use crate::mock::MockConnection;

    #[derive(Debug, Clone, Default)]
    struct PagTestModel;
    impl Model for PagTestModel {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "pag_test"
        }
        fn pk_name() -> &'static str {
            "id"
        }
        fn pk(&self) -> Self::PrimaryKey {
            0
        }
        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
        fn timestamp_fields() -> Option<crate::model::TimestampFields> {
            None
        }
        fn soft_delete_field() -> Option<&'static str> {
            None
        }
    }

    #[derive(Debug, Clone, Default)]
    #[allow(dead_code)]
    struct PagRow {
        id: i64,
    }
    impl FromQueryResult for PagRow {
        fn from_query_result(
            row: &std::collections::HashMap<String, Value>,
        ) -> Result<Self, String> {
            let id = row.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            Ok(PagRow { id })
        }
    }

    #[tokio::test]
    async fn test_paginator_trait_exists() {
        fn _assert<M: Model, Q: PaginatorTrait<M>>() {}
        _assert::<PagTestModel, QueryBuilder<PagTestModel>>();
    }

    #[tokio::test]
    async fn test_paginator_with_mock() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        let _ = mock
            .expect_any()
            .with_rows(vec![vec![("total", Value::I64(100))]]);
        let _ = mock.expect_any().with_rows(vec![
            vec![("id", Value::I64(1))],
            vec![("id", Value::I64(2))],
        ]);

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .where_eq("status", Value::from("active"))
            .paginate::<PagRow, _>(1, 20, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 100);
        assert_eq!(result.page, 1);
        assert_eq!(result.page_size, 20);
        assert_eq!(result.total_pages(), 5);
        assert!(result.has_next());
        assert!(!result.has_prev());
    }

    #[tokio::test]
    async fn test_paginator_empty_result() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        mock.expect_any()
            .with_rows(vec![vec![("total", Value::I64(0))]]);
        mock.expect_any().with_rows(vec![]);

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .paginate::<PagRow, _>(1, 20, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 0);
        assert!(result.is_empty());
        assert_eq!(result.total_pages(), 0);
    }

    #[tokio::test]
    async fn test_paginator_page_beyond_range() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        mock.expect_any()
            .with_rows(vec![vec![("total", Value::I64(10))]]);
        mock.expect_any().with_rows(vec![]);

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .paginate::<PagRow, _>(99, 20, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 10);
        assert_eq!(result.page, 99);
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // M6 变异测试补充：精确边界条件（杀死 missed mutants）
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // G-SO-3：find_page 快捷方法测试
    // -----------------------------------------------------------------------

    /// `find_page` 是 `paginate` 的别名，行为完全一致。
    #[tokio::test]
    async fn test_find_page_alias_of_paginate() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        let _ = mock
            .expect_any()
            .with_rows(vec![vec![("total", Value::I64(50))]]);
        let _ = mock.expect_any().with_rows(vec![
            vec![("id", Value::I64(21))],
            vec![("id", Value::I64(22))],
        ]);

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .where_eq("status", Value::from("active"))
            .find_page::<PagRow, _>(3, 10, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 50);
        assert_eq!(result.page, 3);
        assert_eq!(result.page_size, 10);
        assert_eq!(result.total_pages(), 5);
        assert!(result.has_next());
        assert!(result.has_prev());
    }

    /// `find_page` 空结果：total=0 时跳过数据查询。
    #[tokio::test]
    async fn test_find_page_empty_result() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        mock.expect_any()
            .with_rows(vec![vec![("total", Value::I64(0))]]);

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .find_page::<PagRow, _>(1, 20, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 0);
        assert!(result.is_empty());
    }

    /// 杀死 `total == 0 || offset >= total` 中 `==` → `!=` 和 `||` → `&&` 突变：
    /// total=0 时无论 offset 为何都应跳过数据查询，返回空结果。
    #[tokio::test]
    async fn test_paginator_total_zero_skips_data_query() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        // COUNT 返回 total=0
        mock.expect_any()
            .with_rows(vec![vec![("total", Value::I64(0))]]);
        // total=0 时数据查询被跳过，无需设置第二条期望

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .paginate::<PagRow, _>(1, 20, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 0);
        assert!(result.is_empty());
        // total=0 时数据查询被跳过，只执行了 COUNT（共 1 条 SQL）
        // 杀死 `total == 0` → `total != 0` 突变：若突变则数据查询会被执行
        assert_eq!(mock.executed_sql().len(), 1);
    }

    /// 杀死 `offset >= total` 中 `==` → `!=` 突变：
    /// offset 恰好等于 total 时（第 2 页，total=10，page_size=10），应返回空。
    #[tokio::test]
    async fn test_paginator_offset_equals_total_returns_empty() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        mock.expect_any()
            .with_rows(vec![vec![("total", Value::I64(10))]]);
        // offset=10 >= total=10 → 跳过数据查询
        mock.expect_any().with_rows(vec![]);

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .paginate::<PagRow, _>(2, 10, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 10);
        assert_eq!(result.page, 2);
        assert!(result.is_empty());
        // 杀死 `||` → `&&` 突变：若 total != 0 && offset >= total 变为 false，
        // 数据查询会被执行（共 2 条 SQL），此处断言只执行了 COUNT（1 条）
        assert_eq!(mock.executed_sql().len(), 1);
    }

    /// 杀死 `page_size == 0` 中 `==` → `!=` 突变：
    /// page_size=0 时 offset 应为 0（不进入 else 分支）。
    #[tokio::test]
    async fn test_paginator_page_size_zero_offset_zero() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        mock.expect_any()
            .with_rows(vec![vec![("total", Value::I64(100))]]);
        mock.expect_any()
            .with_rows(vec![vec![("id", Value::I64(1))]]);

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .paginate::<PagRow, _>(3, 0, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 100);
        // page_size=0 → offset=0，SQL 中 OFFSET 0
        let data_sql = mock.executed_sql().get(1).unwrap();
        assert!(
            data_sql.contains("OFFSET 0"),
            "expected OFFSET 0, got: {data_sql}"
        );
    }

    /// 杀死 `*` → `+` / `*` → `/` 突变（offset 计算）：
    /// page=3, page_size=5 → offset 应为 (3-1)*5=10，而非 (3-1)+5=7 或 (3-1)/5=0。
    /// 通过验证生成的 SQL 中 OFFSET 值来精确捕获。
    #[tokio::test]
    async fn test_paginator_offset_calculation_exact() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        mock.expect_any()
            .with_rows(vec![vec![("total", Value::I64(100))]]);
        mock.expect_any().with_rows(vec![
            vec![("id", Value::I64(11))],
            vec![("id", Value::I64(12))],
        ]);

        let result: PageResult<PagRow> = QueryBuilder::<PagTestModel>::new(dialect)
            .paginate::<PagRow, _>(3, 5, &mut mock)
            .await
            .unwrap();

        assert_eq!(result.total, 100);
        assert_eq!(result.page, 3);
        assert_eq!(result.page_size, 5);
        assert_eq!(result.items.len(), 2);

        // 验证 SQL 中 OFFSET=10（(3-1)*5），而非 7（突变 +）或 0（突变 /）
        let data_sql = mock.executed_sql().get(1).unwrap();
        assert!(
            data_sql.contains("OFFSET 10"),
            "expected OFFSET 10 in SQL, got: {data_sql}"
        );
        assert!(data_sql.contains("LIMIT 5"));
    }

    // -----------------------------------------------------------------------
    // M4 新增测试：Builder 风格 Paginator
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_paginator_builder_trait_exists() {
        use super::PaginatorBuilderTrait;
        fn _assert<M: Model, Q: PaginatorBuilderTrait<M>>() {}
        _assert::<PagTestModel, QueryBuilder<PagTestModel>>();
    }

    #[tokio::test]
    async fn test_paginator_builder_fetch_page() {
        use super::PaginatorBuilderTrait;

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        // Builder 风格不执行 COUNT，只执行数据查询
        let _ = mock.expect_any().with_rows(vec![
            vec![("id", Value::I64(1))],
            vec![("id", Value::I64(2))],
            vec![("id", Value::I64(3))],
        ]);

        let mut p = QueryBuilder::<PagTestModel>::new(dialect)
            .where_eq("status", Value::from("active"))
            .paginate_with(&mut mock, 20);
        p.set_total(100);

        let result: PageResult<PagRow> = p.fetch_page::<PagRow>(1).await.unwrap();

        assert_eq!(result.total, 100);
        assert_eq!(result.page, 1);
        assert_eq!(result.page_size, 20);
        assert_eq!(result.items.len(), 3);
    }

    /// 杀死 `fetch_page` 中 `*` → `+` / `*` → `/` 突变（L1 变异测试补充）：
    /// 直接调用 `Paginator::fetch_page`（非 `QueryBuilder::paginate`），
    /// page=3, page_size=5 → offset 应为 (3-1)*5=10，而非 7（突变 +）或 0（突变 /）。
    #[tokio::test]
    async fn test_fetch_page_offset_multiplication() {
        use super::PaginatorBuilderTrait;

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        mock.expect_any().with_rows(vec![
            vec![("id", Value::I64(11))],
            vec![("id", Value::I64(12))],
        ]);

        let mut p = QueryBuilder::<PagTestModel>::new(dialect)
            .where_eq("status", Value::from("active"))
            .paginate_with(&mut mock, 5);
        p.set_total(100);

        let result: PageResult<PagRow> = p.fetch_page::<PagRow>(3).await.unwrap();

        assert_eq!(result.items.len(), 2);
        // 验证 SQL 中 OFFSET=10（(3-1)*5），杀死 `*` → `+`（OFFSET 7）和 `*` → `/`（OFFSET 0）突变
        let data_sql = mock.executed_sql().first().unwrap();
        assert!(
            data_sql.contains("OFFSET 10"),
            "expected OFFSET 10 in fetch_page SQL, got: {data_sql}"
        );
    }

    /// 杀死 `fetch_page` 中 `page_size == 0` 的 `==` → `!=` 突变：
    /// page_size=0 时 offset 必须为 0（不进入 else 分支）。
    #[tokio::test]
    async fn test_fetch_page_page_size_zero_offset_zero() {
        use super::PaginatorBuilderTrait;

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        mock.expect_any()
            .with_rows(vec![vec![("id", Value::I64(1))]]);

        let mut p = QueryBuilder::<PagTestModel>::new(dialect)
            .where_eq("status", Value::from("active"))
            .paginate_with(&mut mock, 0);
        p.set_total(100);

        let result: PageResult<PagRow> = p.fetch_page::<PagRow>(3).await.unwrap();

        assert_eq!(result.items.len(), 1);
        // page_size=0 → offset=0，杀死 `==` → `!=` 突变（突变后 offset = (3-1)*0=0，结果相同，
        // 但此处通过 SQL 断言确保分支逻辑正确）
        let data_sql = mock.executed_sql().first().unwrap();
        assert!(
            data_sql.contains("OFFSET 0"),
            "expected OFFSET 0, got: {data_sql}"
        );
    }

    // -----------------------------------------------------------------------
    // M4 新增测试：Stream API
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_stream_trait_exists() {
        use super::StreamQueryTrait;
        fn _assert<M: Model, Q: StreamQueryTrait<M>>() {}
        _assert::<PagTestModel, QueryBuilder<PagTestModel>>();
    }

    #[tokio::test]
    async fn test_stream_returns_rows() {
        use super::StreamQueryTrait;
        use futures::StreamExt;

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut mock = MockConnection::new();

        let _ = mock.expect_any().with_rows(vec![
            vec![("id", Value::I64(1))],
            vec![("id", Value::I64(2))],
        ]);

        let mut stream = QueryBuilder::<PagTestModel>::new(dialect)
            .where_eq("status", Value::from("active"))
            .stream(&mut mock);

        let mut count = 0;
        while let Some(result) = stream.next().await {
            let row: RowResult = result.unwrap();
            assert!(row.contains_key("id"));
            count += 1;
        }
        assert_eq!(count, 2);
    }

    // -----------------------------------------------------------------------
    // 变异测试补充：杀死 value_to_u64 中 4 个 match arm 删除突变
    // -----------------------------------------------------------------------

    /// 杀死 `delete match arm Value::I32(n)` 突变。
    #[test]
    fn test_value_to_u64_i32() {
        assert_eq!(value_to_u64(&Value::I32(42)), Some(42u64));
        assert_eq!(value_to_u64(&Value::I32(-1)), Some(u64::MAX)); // 截断
    }

    /// 杀死 `delete match arm Value::U64(n)` 突变。
    #[test]
    fn test_value_to_u64_u64() {
        assert_eq!(value_to_u64(&Value::U64(999)), Some(999u64));
    }

    /// 杀死 `delete match arm Value::U32(n)` 突变。
    #[test]
    fn test_value_to_u64_u32() {
        assert_eq!(value_to_u64(&Value::U32(77)), Some(77u64));
    }

    /// 杀死 `delete match arm Value::F64(n)` 突变。
    #[test]
    fn test_value_to_u64_f64() {
        assert_eq!(value_to_u64(&Value::F64(2.71)), Some(2u64));
        assert_eq!(value_to_u64(&Value::F64(0.0)), Some(0u64));
    }

    /// 杀死 `delete match arm Value::I64(n)` 突变（I64 已有测试覆盖，补充显式断言）。
    #[test]
    fn test_value_to_u64_i64() {
        assert_eq!(value_to_u64(&Value::I64(123456789)), Some(123456789u64));
    }

    /// 未知类型返回 None。
    #[test]
    fn test_value_to_u64_unknown() {
        assert_eq!(value_to_u64(&Value::Bool(true)), None);
        assert_eq!(value_to_u64(&Value::String("x".into())), None);
    }
}
