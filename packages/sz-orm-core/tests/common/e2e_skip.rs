//! 真实数据库 e2e 测试的“跳过记账”守卫（R6）
//!
//! 历史问题：e2e 套件在未注入 DB URL 时直接 `return`，测试“全绿”但一行没跑。
//!
//! 统一处置：
//! - 默认（本地开发）：打印一行 `E2E_SKIP[<service>]: <原因>` 并跳过——
//!   跳过行为可见、可按 service 统计；
//! - `SZ_ORM_E2E_STRICT=1`：所有 service 都必须可用，任何跳过即 panic；
//! - `SZ_ORM_E2E_REQUIRED=mysql,postgres`：只强制这些 service 必须可用
//!   （CI 只准备 MySQL/PG，Oracle 等本机服务允许跳过，但仍输出记账行）。
//!
//! CI 断言方式：`SZ_ORM_E2E_STRICT=1 SZ_ORM_E2E_REQUIRED=mysql,postgres` 运行后
//! grep 输出中的 `E2E_SKIP[mysql]` / `E2E_SKIP[postgres]`，出现任何一行即失败
//! （见 `.github/workflows/integration.yml`）。
//!
//! 环境变量优先级：`SZ_ORM_{MYSQL,PG}_URL` > 旧变量 `MYSQL_URL` / `POSTGRES_URL`。

/// 是否处于严格模式
pub fn strict() -> bool {
    matches!(
        std::env::var("SZ_ORM_E2E_STRICT").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

/// 指定 service 是否必须可用（决定跳过是 panic 还是记账）
fn required(service: &str) -> bool {
    match std::env::var("SZ_ORM_E2E_REQUIRED") {
        Ok(v) if !v.trim().is_empty() => v
            .split(',')
            .map(str::trim)
            .any(|s| s.eq_ignore_ascii_case(service)),
        _ => strict(),
    }
}

/// 记录一次 service 跳过并返回 `None`；该 service 必须可用时 panic
pub fn skip_for<T>(service: &str, reason: &str) -> Option<T> {
    if required(service) {
        panic!("E2E_STRICT 模式禁止跳过真实数据库 e2e 测试（service={service}）：{reason}");
    }
    eprintln!("E2E_SKIP[{service}]: {reason}");
    None
}

/// 不区分 service 的跳过记账（内存库等无外部依赖场景）
pub fn skip<T>(reason: &str) -> Option<T> {
    skip_for("internal", reason)
}

fn env_first(keys: &[&'static str]) -> Option<(String, &'static str)> {
    for key in keys {
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Some((v, key));
            }
        }
    }
    None
}

/// MySQL 连接池；未配置或连接失败 → 跳过记账（该 service 必须可用时 panic）
pub async fn mysql_pool() -> Option<sqlx::MySqlPool> {
    let (url, key) = match env_first(&["SZ_ORM_MYSQL_URL", "MYSQL_URL"]) {
        Some(x) => x,
        None => return skip_for("mysql", "MySQL URL 未设置（SZ_ORM_MYSQL_URL / MYSQL_URL）"),
    };
    match sqlx::MySqlPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(e) => skip_for("mysql", &format!("MySQL 连接失败（{key}）: {e}")),
    }
}

/// 读取 PostgreSQL 连接串（不建连）；未配置时按 skip 记账
///
/// 供需要自建专用池（如固定会话锁的并发用例）的测试使用。
pub fn pg_url() -> Option<String> {
    match env_first(&["SZ_ORM_PG_URL", "POSTGRES_URL"]) {
        Some((url, _)) => Some(url),
        None => skip_for(
            "postgres",
            "PostgreSQL URL 未设置（SZ_ORM_PG_URL / POSTGRES_URL）",
        ),
    }
}

/// PostgreSQL 连接池；未配置或连接失败 → 跳过记账（该 service 必须可用时 panic）
pub async fn pg_pool() -> Option<sqlx::PgPool> {
    let url = pg_url()?;
    match sqlx::PgPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(e) => skip_for("postgres", &format!("PostgreSQL 连接失败: {e}")),
    }
}

/// 内存 SQLite 连接池（不依赖外部服务，失败即真实故障 → 记账）
pub async fn sqlite_pool() -> Option<sqlx::SqlitePool> {
    match sqlx::SqlitePool::connect("sqlite::memory:").await {
        Ok(pool) => Some(pool),
        Err(e) => skip_for("sqlite", &format!("SQLite 内存库连接失败: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未设置严格模式时，skip 返回 None 并打印记账行
    #[test]
    fn skip_returns_none_without_strict_mode() {
        if strict() {
            return; // 严格模式下本用例无意义
        }
        let v: Option<()> = skip("unit-test-reason");
        assert!(v.is_none());
        let v2: Option<()> = skip_for("mysql", "unit-test-reason");
        assert!(v2.is_none());
    }
}
