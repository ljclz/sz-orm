//! v6.2 性能优化：SeaORM/SQLx 对标基准集成测试
//!
//! 验证对标基准可运行（不 panic）。
//! 标注 `#[ignore]` 因需 dev-dependency，用 `--ignored` 触发。

#[cfg(feature = "bench-seaorm")]
mod seaorm_tests {
    /// 验证 SeaORM 查询构建可运行
    #[tokio::test]
    #[ignore]
    async fn seaorm_query_build_runs() {
        use sea_orm::sea_query::{Expr, MysqlQueryBuilder, Query};

        let select = Query::select()
            .from("users")
            .and_where(Expr::col("id").eq(42))
            .to_owned();
        let sql = select.to_string(MysqlQueryBuilder);
        assert!(sql.contains("users"));
    }
}

mod sqlx_tests {
    /// 验证 SQLx 查询构建可运行
    #[tokio::test]
    #[ignore]
    async fn sqlx_query_build_runs() {
        use sqlx::Sqlite;

        let query = sqlx::query::<Sqlite>("SELECT * FROM users WHERE id = ?").bind(42i64);
        let _ = query;
    }

    /// 验证 SQLx SQLite 内存池可创建
    #[tokio::test]
    #[ignore]
    async fn sqlx_pool_create_runs() {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        pool.close().await;
    }
}
