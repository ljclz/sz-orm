# sz-orm 多后端能力就绪清单

> 版本：v3.0.0 M1.1 交付物
> 日期：2026-08-07
> 目的：验证 sz-orm 上游已满足 sz-rust P2-1 多后端 ORM 启动条件，每项附 file:line 证据
> 审计规则：所有引用的 file:line 必须真实存在（可由 `scripts/audit-verify.ps1` 验证）

---

## 1. 验证项总览

| # | 验证项 | 对应需求 | 状态 | 证据 |
|---|--------|---------|------|------|
| V1 | AnyBackend 五方言枚举 | REQ-MB-001 | ✅ PASS | [any_driver.rs:57](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L57) |
| V2 | from_dsn() DSN 自动识别 | REQ-MB-001 | ✅ PASS | [any_driver.rs:80](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L80) |
| V3 | dialect() 方言映射 | REQ-MB-002 | ✅ PASS | [any_driver.rs:117](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L117) |
| V4 | AnyPool 后端无关连接工厂 | REQ-MB-002 | ✅ PASS | [any_driver.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L129) |
| V5 | UnifiedPool 统一连接池 | REQ-MB-003 | ✅ PASS | [unified_pool.rs:48](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L48) |

**总结论**：5/5 项全部 PASS，sz-orm 上游多后端 ORM 能力已就绪，sz-rust P2-1 启动条件已满足。

---

## 2. 逐项验证详情

### V1：AnyBackend 五方言枚举

**需求**：REQ-MB-001 — sz-orm 提供统一后端枚举，覆盖五种主流关系数据库

**证据**：
- [packages/sz-orm-sqlx/src/any_driver.rs:57](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L57)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnyBackend {
    MySql,
    Postgres,
    Sqlite,
    Oracle,
    Mssql,
}
```

**验证结果**：✅ PASS

**说明**：
- 枚举含 5 个变体，覆盖 MySQL/PostgreSQL/SQLite/Oracle/MSSQL
- `#[non_exhaustive]` 标注确保外部 crate match 时必须使用 wildcard 臂，未来新增变体不破坏现有代码
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` 支持比较和拷贝，可用于运行时判断后端类型

---

### V2：from_dsn() DSN 自动识别

**需求**：REQ-MB-001 — 从 DSN 字符串自动识别后端类型，无需调用方手动指定

**证据**：
- [packages/sz-orm-sqlx/src/any_driver.rs:80](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L80)

```rust
pub fn from_dsn(dsn: &str) -> Result<Self, DbError> {
    if dsn.starts_with("mysql://") || dsn.starts_with("mariadb://") {
        Ok(AnyBackend::MySql)
    } else if dsn.starts_with("postgres://") || dsn.starts_with("postgresql://") {
        Ok(AnyBackend::Postgres)
    } else if dsn.starts_with("sqlite://") || dsn.starts_with("sqlite:") {
        Ok(AnyBackend::Sqlite)
    } else if dsn.starts_with("oracle://") {
        Ok(AnyBackend::Oracle)
    } else if dsn.starts_with("mssql://") || dsn.starts_with("sqlserver://") {
        Ok(AnyBackend::Mssql)
    } else {
        Err(DbError::ConnectionRefused(...))
    }
}
```

**验证结果**：✅ PASS

**说明**：
- 支持 7 种 DSN scheme：`mysql://`、`mariadb://`、`postgres://`、`postgresql://`、`sqlite://`、`sqlite:`、`oracle://`、`mssql://`、`sqlserver://`
- 未知 scheme 返回 `DbError::ConnectionRefused`，错误信息含支持的 scheme 列表
- 调用方只需传入 DSN 字符串，无需手动匹配后端类型

---

### V3：dialect() 方言映射

**需求**：REQ-MB-002 — 根据后端类型返回对应的 Dialect 实例，用于 SQL 方言差异处理

**证据**：
- [packages/sz-orm-sqlx/src/any_driver.rs:117](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L117)

```rust
pub fn dialect(&self) -> Box<dyn Dialect> {
    match self {
        AnyBackend::MySql => Box::new(MySqlDialect),
        AnyBackend::Postgres => Box::new(PostgreSqlDialect),
        AnyBackend::Sqlite => Box::new(SqliteDialect),
        AnyBackend::Oracle => Box::new(OracleDialect),
        AnyBackend::Mssql => Box::new(SqlServerDialect),
    }
}
```

**Dialect trait 定义**：
- [packages/sz-orm-core/src/dialect.rs:23](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-core/src/dialect.rs#L23)

```rust
pub trait Dialect: Send + Sync {
    fn clone_box(&self) -> Box<dyn Dialect>;
    fn db_type(&self) -> DbType;
    fn quote(&self, identifier: &str) -> String;
    fn escape_string(&self, s: &str) -> String;
    fn supports_returning(&self) -> bool;
    fn build_pagination(&self, sql: &str, page: u64, limit: u64) -> String;
    fn json_type(&self) -> &'static str;
    fn json_extract(&self, column: &str, path: &str) -> String;
    fn full_text_search(&self, columns: &[&str], keyword: &str) -> String;
    fn bool_to_int(&self, expr: &str) -> String;
    fn concat(&self, parts: &[&str]) -> String;
    fn supports_if_exists(&self) -> bool;
    fn supports_if_not_exists(&self) -> bool;
    fn auto_increment_keyword(&self) -> &'static str;
    fn last_insert_id_sql(&self) -> Option<&'static str>;
    fn build_create_table(&self, table: &str, columns: &[ColumnDef]) -> String;
    fn build_alter_table(&self, table: &str, changes: &[TableChange]) -> String;
    fn build_upsert_on_conflict(...) -> Option<String>;
    fn build_lock_clause(&self, lock_type: LockType) -> Option<String>;
    fn supports_lock_for_update(&self) -> bool;
    // ...
}
```

**验证结果**：✅ PASS

**说明**：
- 5 后端各有独立 Dialect 实现：`MySqlDialect`、`PostgreSqlDialect`、`SqliteDialect`、`OracleDialect`、`SqlServerDialect`
- Dialect trait 覆盖 20+ 方言差异方法（标识符引用、字符串转义、RETURNING、分页、JSON、全文检索、upsert、行锁等）
- `dialect()` 返回 `Box<dyn Dialect>`，调用方可透明使用各方言特性

---

### V4：AnyPool 后端无关连接工厂

**需求**：REQ-MB-002 — 提供后端无关的连接工厂，从 DSN 自动创建对应后端连接

**证据**：
- 结构体定义：[packages/sz-orm-sqlx/src/any_driver.rs:129](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L129)
- connect 方法：[packages/sz-orm-sqlx/src/any_driver.rs:142](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L142)
- create 方法：[packages/sz-orm-sqlx/src/any_driver.rs:213](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L213)

```rust
pub struct AnyPool {
    backend: AnyBackend,
    factory: Arc<dyn ConnectionFactory>,
}

impl AnyPool {
    pub async fn connect(dsn: &str) -> Result<Self, DbError> {
        let backend = AnyBackend::from_dsn(dsn)?;
        let factory: Arc<dyn ConnectionFactory> = match backend {
            AnyBackend::MySql => { /* MySqlPoolHandle + SqlxMySqlConnectionFactory */ }
            AnyBackend::Postgres => { /* PgPoolHandle + SqlxPgConnectionFactory */ }
            AnyBackend::Sqlite => { /* SqlitePoolHandle + SqlxSqliteConnectionFactory */ }
            AnyBackend::Oracle => { /* OraclePoolHandle + OracleConnectionFactory */ }
            AnyBackend::Mssql => { /* MssqlPoolHandle + MssqlConnectionFactory */ }
        };
        Ok(Self { backend, factory })
    }

    pub async fn create(&self) -> Result<AnyConnection, DbError> { ... }
}
```

**AnyConnection 定义**：
- [packages/sz-orm-sqlx/src/any_driver.rs:274](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/any_driver.rs#L274)

```rust
pub struct AnyConnection {
    backend: AnyBackend,
    inner: Box<dyn Connection>,
}
```

**验证结果**：✅ PASS

**说明**：
- `AnyPool::connect(dsn)` 从 DSN 自动识别后端并创建对应连接工厂
- Oracle/MSSQL 后端通过 feature gate 隔离（`#[cfg(feature = "oracle")]` / `#[cfg(feature = "mssql")]`），未启用时返回明确错误提示
- `AnyConnection` 实现 `Connection` trait，委托内部具体后端连接，零能力丢失
- `AnyPool` 持有 `Arc<dyn ConnectionFactory>`，可共享连接池句柄

---

### V5：UnifiedPool 统一连接池

**需求**：REQ-MB-003 — 提供完整连接池抽象，供 sz-rust AppState 持有单一类型 `Arc<UnifiedPool>`

**证据**：
- 结构体定义：[packages/sz-orm-sqlx/src/unified_pool.rs:48](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L48)
- connect 方法：[packages/sz-orm-sqlx/src/unified_pool.rs:57](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L57)
- connect_with_config 方法：[packages/sz-orm-sqlx/src/unified_pool.rs:65](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L65)
- from_pool 零成本迁移：[packages/sz-orm-sqlx/src/unified_pool.rs:126](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L126)
- acquire 方法：[packages/sz-orm-sqlx/src/unified_pool.rs:144](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L144)
- dialect 方法：[packages/sz-orm-sqlx/src/unified_pool.rs:138](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L138)
- 单元测试：[packages/sz-orm-sqlx/src/unified_pool.rs:179](file:///E:/vue/test/鲜视达/rust/sz-orm/packages/sz-orm-sqlx/src/unified_pool.rs#L179)

```rust
pub struct UnifiedPool {
    backend: AnyBackend,
    pool: Pool,
}

impl UnifiedPool {
    pub async fn connect(dsn: &str) -> Result<Self, DbError> { ... }
    pub async fn connect_with_config(dsn: &str, config: PoolConfig) -> Result<Self, DbError> { ... }
    pub fn from_pool(pool: Pool, backend: AnyBackend) -> Self { ... }
    pub fn backend(&self) -> AnyBackend { ... }
    pub fn dialect(&self) -> Box<dyn Dialect> { ... }
    pub async fn acquire(&self) -> Result<PooledConnection, PoolError> { ... }
    pub fn resize(&self, new_max: usize) { ... }
    pub async fn close_all(&self) { ... }
    pub async fn status(&self) -> PoolStatus { ... }
}
```

**验证结果**：✅ PASS

**说明**：
- `UnifiedPool` 包装 `Pool`（完整连接池，含 AtomicU32 + crossbeam-queue ArrayQueue + Notify 自研连接池）+ `AnyBackend`
- 所有方法委托内部 `Pool`，零能力丢失
- `from_pool` 提供零成本迁移路径：sz-rust 从 `Arc<Pool>` 迁移到 `Arc<UnifiedPool>`
- `connect`/`connect_with_config` 根据 DSN 自动识别后端并创建完整连接池
- 单元测试覆盖：SQLite 连接、dialect 映射、from_pool 迁移、resize/close、无效 DSN 错误处理

---

## 3. sz-rust P2-1 启动条件评估

| P2-1 条件 | 对应验证项 | 状态 |
|-----------|-----------|------|
| 上游提供统一后端枚举 | V1 (AnyBackend) | ✅ 满足 |
| 上游提供 DSN 自动识别 | V2 (from_dsn) | ✅ 满足 |
| 上游提供方言映射 | V3 (dialect) | ✅ 满足 |
| 上游提供后端无关连接工厂 | V4 (AnyPool) | ✅ 满足 |
| 上游提供统一连接池 | V5 (UnifiedPool) | ✅ 满足 |

**结论**：sz-orm 上游多后端 ORM 能力已完全就绪，sz-rust P2-1 可启动下游透明适配层实现。

---

## 4. sz-rust 集成方式建议

### 4.1 AppState 持有 UnifiedPool

```rust
// sz-rust 推荐集成方式
use sz_orm_sqlx::UnifiedPool;
use std::sync::Arc;

pub struct AppState {
    pool: Arc<UnifiedPool>,
}

// 从配置 DSN 创建
let pool = UnifiedPool::connect(&config.database_dsn).await?;
let state = AppState { pool: Arc::new(pool) };
```

### 4.2 业务代码透明访问

```rust
// 业务代码无需感知后端类型
let mut conn = state.pool.acquire().await?;
let dialect = state.pool.dialect(); // 自动返回对应方言

// 根据方言特性生成 SQL
if dialect.supports_returning() {
    // 使用 RETURNING 子句
} else {
    // 使用 last_insert_id_sql() 回退
}
```

### 4.3 零成本迁移路径

```rust
// 从现有 Arc<Pool> 迁移到 Arc<UnifiedPool>
let unified = UnifiedPool::from_pool(existing_pool, AnyBackend::MySql);
// 所有方法委托内部 Pool，行为完全一致
```

---

## 5. Feature Gate 说明

| 后端 | Feature | 默认 | 说明 |
|------|---------|------|------|
| MySQL | （内置） | ✅ | 无需 feature gate |
| PostgreSQL | （内置） | ✅ | 无需 feature gate |
| SQLite | （内置） | ✅ | 无需 feature gate |
| Oracle | `oracle` | ❌ | 需在 Cargo.toml 启用 |
| MSSQL | `mssql` | ❌ | 需在 Cargo.toml 启用 |

**sz-rust Cargo.toml 示例**：
```toml
[dependencies]
sz-orm-sqlx = { version = "2.3", features = ["oracle", "mssql"] }
```

---

## 6. 验证脚本

本文档所有 file:line 引用可通过以下命令验证：

```powershell
.\scripts\audit-verify.ps1 docs\spec\v3.0.0\multi_backend_readiness.md
```

预期输出：所有引用 ✅ PASS（文件存在且行号在范围内）。