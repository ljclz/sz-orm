//! v6.5.0 SqlxRowStream — SQLx 三后端流式结果集实现
//!
//! `SqlxRowStream` 是 `BoxedCursorRowStream` 的类型别名，
//! 包装 SQLx `query().fetch()` 真游标流。
//!
//! SQLx 三后端（SQLite/MySQL/PG）的 `query_stream` 已使用
//! `sqlx::query().fetch()` 真游标（any.rs:583/1261/1972），
//! `query_stream_unified` 的默认实现通过 `query_stream_cursor`
//! 间接提供真游标支持，无需为三后端单独实现 `ConnectionExt`。

use sz_orm_core::row_stream::BoxedCursorRowStream;

/// SQLx 流式结果集（包装 `BoxedCursorRowStream`）
///
/// 使用方式：
/// ```ignore
/// let mut stream = conn.query_stream_unified("SELECT * FROM t", 1000)?;
/// while let Some(row) = stream.next_row().await {
///     let row = row?;
///     // 处理行
/// }
/// ```
pub type SqlxRowStream<'a> = BoxedCursorRowStream<'a>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlx_row_stream_type_alias() {
        fn assert_async_row_stream<'a, S: sz_orm_core::row_stream::AsyncRowStream + 'a>() {}
        assert_async_row_stream::<SqlxRowStream<'static>>();
    }
}
