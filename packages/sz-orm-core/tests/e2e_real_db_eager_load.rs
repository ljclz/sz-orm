//! M1-T4: 真实数据库预加载（Eager Loading）端到端测试
//!
//! 连真实 PostgreSQL/MySQL/SQLite 验证 BelongsTo / HasMany / HasOne
//! 关联查询的 JOIN 和批量加载策略，以及 N+1 查询检测。
//!
//! 测试策略：用 raw sqlx 执行 ORM 会生成的 SQL 模式（JOIN / IN 批量），
//! 验证结果集正确性和关联关系完整性。

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

mod common;

use common::cleanup::unique_table_name;

/// 获取 PostgreSQL 连接池
async fn pg_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("POSTGRES_URL").ok()?;
    sqlx::PgPool::connect(&url).await.ok()
}

/// 获取 MySQL 连接池
async fn mysql_pool() -> Option<sqlx::MySqlPool> {
    let url = std::env::var("MYSQL_URL").ok()?;
    sqlx::MySqlPool::connect(&url).await.ok()
}

/// 获取 SQLite 连接池
async fn sqlite_pool() -> Option<sqlx::SqlitePool> {
    sqlx::SqlitePool::connect("sqlite::memory:").await.ok()
}

// ==================== PostgreSQL HasMany + BelongsTo JOIN ====================

/// 测试 HasMany 关联：User has many Posts，通过 LEFT JOIN 一次性加载。
#[tokio::test]
async fn test_pg_eager_has_many_join() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => {
            eprintln!("PostgreSQL 未配置，跳过");
            return;
        }
    };
    let users_tbl = unique_table_name("e2e_el_users");
    let posts_tbl = unique_table_name("e2e_el_posts");

    // 建表
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            users_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, user_id BIGINT REFERENCES \"{}\"(id), title TEXT)",
            posts_tbl, users_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    // 插入 2 个用户，各 2 篇帖子
    for name in ["Alice", "Bob"] {
        let sql = format!("INSERT INTO \"{}\" (name) VALUES ($1)", users_tbl);
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
    }
    let user_ids: Vec<(i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT id, name FROM \"{}\" ORDER BY id", users_tbl).as_str(),
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(user_ids.len(), 2);

    for (uid, uname) in &user_ids {
        for i in 1..=2 {
            let sql = format!(
                "INSERT INTO \"{}\" (user_id, title) VALUES ($1, $2)",
                posts_tbl
            );
            let title = format!("{}_post_{}", uname, i);
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(uid)
                .bind(&title)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    // Eager load：LEFT JOIN 一次查出所有 user + post
    let join_sql = format!(
        "SELECT u.id AS user_id, u.name AS user_name, p.id AS post_id, p.title AS post_title \
         FROM \"{}\" u LEFT JOIN \"{}\" p ON p.user_id = u.id \
         ORDER BY u.id, p.id",
        users_tbl, posts_tbl
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(join_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);

    let alice_count = rows
        .iter()
        .filter(|r| r.try_get::<String, _>("user_name").unwrap() == "Alice")
        .count();
    let bob_count = rows
        .iter()
        .filter(|r| r.try_get::<String, _>("user_name").unwrap() == "Bob")
        .count();
    assert_eq!(alice_count, 2);
    assert_eq!(bob_count, 2);

    // 清理
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", posts_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", users_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

/// 测试 HasOne 关联：User has one Profile，通过 LEFT JOIN 加载。
#[tokio::test]
async fn test_pg_eager_has_one_join() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let users_tbl = unique_table_name("e2e_el_users");
    let profiles_tbl = unique_table_name("e2e_el_profiles");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            users_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, user_id BIGINT UNIQUE REFERENCES \"{}\"(id), bio TEXT)",
            profiles_tbl, users_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    // 插入用户和 profile
    let sql = format!("INSERT INTO \"{}\" (name) VALUES ($1), ($2)", users_tbl);
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind("Alice")
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();

    let user_ids: Vec<(i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT id, name FROM \"{}\" ORDER BY id", users_tbl).as_str(),
    ))
    .fetch_all(&pool)
    .await
    .unwrap();

    // Alice 有 profile，Bob 没有
    let sql = format!(
        "INSERT INTO \"{}\" (user_id, bio) VALUES ($1, $2)",
        profiles_tbl
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(user_ids[0].0)
        .bind("Alice's bio")
        .execute(&pool)
        .await
        .unwrap();

    // Eager load：LEFT JOIN
    let join_sql = format!(
        "SELECT u.id, u.name, p.bio \
         FROM \"{}\" u LEFT JOIN \"{}\" p ON p.user_id = u.id \
         ORDER BY u.id",
        users_tbl, profiles_tbl
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(join_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    let alice_bio: Option<String> = rows[0].try_get("bio").unwrap();
    let bob_bio: Option<String> = rows[1].try_get("bio").unwrap();
    assert_eq!(alice_bio, Some("Alice's bio".to_string()));
    assert_eq!(bob_bio, None);

    // 清理
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", profiles_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", users_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

/// 测试 BelongsTo 关联：Post belongs to User，通过 INNER JOIN 加载。
#[tokio::test]
async fn test_pg_eager_belongs_to_join() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let users_tbl = unique_table_name("e2e_el_users");
    let posts_tbl = unique_table_name("e2e_el_posts");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            users_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, user_id BIGINT REFERENCES \"{}\"(id), title TEXT)",
            posts_tbl, users_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let sql = format!("INSERT INTO \"{}\" (name) VALUES ($1)", users_tbl);
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();
    let user_id: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT id FROM \"{}\"", users_tbl).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();

    let sql = format!(
        "INSERT INTO \"{}\" (user_id, title) VALUES ($1, $2), ($1, $3)",
        posts_tbl
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(user_id.0)
        .bind("Post1")
        .bind("Post2")
        .execute(&pool)
        .await
        .unwrap();

    // BelongsTo：从 Post JOIN User
    let join_sql = format!(
        "SELECT p.id, p.title, u.name AS author \
         FROM \"{}\" p INNER JOIN \"{}\" u ON p.user_id = u.id \
         ORDER BY p.id",
        posts_tbl, users_tbl
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(join_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        let author: String = row.try_get("author").unwrap();
        assert_eq!(author, "Alice");
    }

    // 清理
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", posts_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", users_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

/// 测试批量加载策略（IN 查询）替代 N+1：先查 posts，再用 IN 批量查 comments。
#[tokio::test]
async fn test_pg_eager_batch_load_in_query() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let posts_tbl = unique_table_name("e2e_el_posts");
    let comments_tbl = unique_table_name("e2e_el_comments");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, title TEXT)",
            posts_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, post_id BIGINT REFERENCES \"{}\"(id), body TEXT)",
            comments_tbl, posts_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    // 插入 3 篇帖子
    for i in 1..=3 {
        let sql = format!("INSERT INTO \"{}\" (title) VALUES ($1)", posts_tbl);
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(format!("Post{}", i))
            .execute(&pool)
            .await
            .unwrap();
    }
    let post_ids: Vec<(i64,)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT id FROM \"{}\" ORDER BY id", posts_tbl).as_str(),
    ))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(post_ids.len(), 3);

    // 每篇帖子 2 条评论
    for (pid,) in &post_ids {
        for j in 1..=2 {
            let sql = format!(
                "INSERT INTO \"{}\" (post_id, body) VALUES ($1, $2)",
                comments_tbl
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(pid)
                .bind(format!("Comment_{}", j))
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    // 批量加载：一次 IN 查询获取所有帖子的评论（替代 N+1）
    let batch_sql = format!(
        "SELECT post_id, COUNT(*) AS cnt FROM \"{}\" WHERE post_id = ANY($1) GROUP BY post_id",
        comments_tbl
    );
    let id_array: Vec<i64> = post_ids.iter().map(|(id,)| *id).collect();
    let rows = sqlx::query(sqlx::AssertSqlSafe(batch_sql.as_str()))
        .bind(&id_array)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    for row in &rows {
        let cnt: i64 = row.try_get("cnt").unwrap();
        assert_eq!(cnt, 2);
    }

    // 清理
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", comments_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", posts_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== MySQL HasMany JOIN ====================

/// 测试 MySQL HasMany JOIN 预加载。
#[tokio::test]
async fn test_mysql_eager_has_many_join() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => {
            eprintln!("MySQL 未配置，跳过");
            return;
        }
    };
    let users_tbl = unique_table_name("e2e_el_users");
    let posts_tbl = unique_table_name("e2e_el_posts");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
            users_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, user_id BIGINT, title VARCHAR(255))",
            posts_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let sql = format!("INSERT INTO `{}` (name) VALUES (?), (?)", users_tbl);
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind("Alice")
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();

    let user_ids: Vec<(i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT id, name FROM `{}` ORDER BY id", users_tbl).as_str(),
    ))
    .fetch_all(&pool)
    .await
    .unwrap();

    for (uid, uname) in &user_ids {
        let sql = format!("INSERT INTO `{}` (user_id, title) VALUES (?, ?)", posts_tbl);
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(uid)
            .bind(format!("{}_post", uname))
            .execute(&pool)
            .await
            .unwrap();
    }

    let join_sql = format!(
        "SELECT u.name AS user_name, p.title AS post_title \
         FROM `{}` u LEFT JOIN `{}` p ON p.user_id = u.id \
         ORDER BY u.id",
        users_tbl, posts_tbl
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(join_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    // 清理
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", posts_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", users_tbl).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== SQLite HasMany JOIN ====================

/// 测试 SQLite HasMany JOIN 预加载。
#[tokio::test]
async fn test_sqlite_eager_has_many_join() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => {
            eprintln!("SQLite 未配置，跳过");
            return;
        }
    };
    let users_tbl = unique_table_name("e2e_el_users");
    let posts_tbl = unique_table_name("e2e_el_posts");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)",
            users_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER, title TEXT)",
            posts_tbl
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let sql = format!("INSERT INTO \"{}\" (name) VALUES (?), (?)", users_tbl);
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind("Alice")
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();

    let user_ids: Vec<(i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT id, name FROM \"{}\" ORDER BY id", users_tbl).as_str(),
    ))
    .fetch_all(&pool)
    .await
    .unwrap();

    for (uid, uname) in &user_ids {
        for i in 1..=2 {
            let sql = format!(
                "INSERT INTO \"{}\" (user_id, title) VALUES (?, ?)",
                posts_tbl
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(uid)
                .bind(format!("{}_post_{}", uname, i))
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    let join_sql = format!(
        "SELECT u.name AS user_name, p.title AS post_title \
         FROM \"{}\" u LEFT JOIN \"{}\" p ON p.user_id = u.id \
         ORDER BY u.id, p.id",
        users_tbl, posts_tbl
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(join_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
}

// ==================== N+1 查询检测（sz-orm-core API） ====================

/// 测试 N1QueryDetector 检测 N+1 查询模式。
#[tokio::test]
async fn test_n1_query_detector() {
    use sz_orm_core::entity_graph::{N1DetectionConfig, N1QueryDetector};

    let detector = N1QueryDetector::new(N1DetectionConfig {
        threshold: 5,
        enabled: true,
    });
    detector.start_window();

    // 模拟 N+1：逐条加载 10 次
    for _ in 0..10 {
        detector.record_single_load("posts");
    }
    // 模拟批量加载：1 次批量
    detector.record_batch_load("comments", 10);

    let alerts = detector.end_window();
    assert!(!alerts.is_empty(), "应检测到 N+1 告警");

    let posts_alert = alerts
        .iter()
        .find(|a| a.relation == "posts")
        .expect("应有 posts 关联的 N+1 告警");
    assert!(posts_alert.query_count >= 10);
    assert!(posts_alert.threshold == 5);

    // comments 是批量加载，不应出现在告警中
    let comments_alert = alerts.iter().find(|a| a.relation == "comments");
    assert!(comments_alert.is_none(), "批量加载不应触发 N+1 告警");
}

/// 测试 N1QueryDetector 批量加载不触发告警。
#[tokio::test]
async fn test_n1_query_detector_batch_ok() {
    use sz_orm_core::entity_graph::{N1DetectionConfig, N1QueryDetector};

    let detector = N1QueryDetector::new(N1DetectionConfig {
        threshold: 5,
        enabled: true,
    });
    detector.start_window();

    // 全部使用批量加载
    detector.record_batch_load("posts", 10);
    detector.record_batch_load("posts", 20);

    let alerts = detector.end_window();
    assert!(alerts.is_empty(), "纯批量加载不应触发 N+1 告警");
    assert!(!detector.has_n_plus_one());
}

// ==================== M1-T11: 测试超时机制验证 ====================

/// 验证单方言超时常量为 60 秒。
#[tokio::test]
async fn test_timeout_single_dialect_constant() {
    use common::timeout::SINGLE_DIALECT_TIMEOUT;
    assert_eq!(SINGLE_DIALECT_TIMEOUT.as_secs(), 60, "单方言超时应为 60 秒");
}

/// 验证全方言超时常量为 300 秒。
#[tokio::test]
async fn test_timeout_all_dialect_constant() {
    use common::timeout::ALL_DIALECT_TIMEOUT;
    assert_eq!(ALL_DIALECT_TIMEOUT.as_secs(), 300, "全方言超时应为 300 秒");
}

/// 验证 run_with_timeout 正常完成不触发超时。
#[tokio::test]
async fn test_timeout_completes_within_limit() {
    use common::timeout::{run_with_timeout, SINGLE_DIALECT_TIMEOUT};

    let result = run_with_timeout("fast_test", SINGLE_DIALECT_TIMEOUT, async { 42 }).await;
    assert_eq!(result, 42);
}

/// 验证 run_with_timeout 超时后 panic。
#[tokio::test]
#[should_panic(expected = "测试超时")]
async fn test_timeout_panics_on_expiry() {
    use common::timeout::run_with_timeout;
    use std::time::Duration;

    let _ = run_with_timeout("slow_test", Duration::from_millis(50), async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        42
    })
    .await;
}
