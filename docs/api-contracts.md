# SZ-ORM 公共 API 行为契约清单

> **版本**: v0.2.0
> **更新日期**: 2026-07-19
> **维护规则**: 任何修改公共 API 行为的 PR 必须同步更新本文档
> **关联测试**: [packages/sz-orm-core/tests/contracts/](../packages/sz-orm-core/tests/contracts/)
> **关联审计**: [scripts/audit-api-changes.ps1](../scripts/audit-api-changes.ps1) / [.sh](../scripts/audit-api-changes.sh)

---

## 目录

1. [value 模块 — Value 枚举](#1-value-模块--value-枚举)
2. [db_type 模块 — DbType](#2-db_type-模块--dbtype)
3. [error 模块 — 错误类型体系](#3-error-模块--错误类型体系)
4. [dialect 模块 — 方言系统](#4-dialect-模块--方言系统)
5. [pool 模块 — 连接池](#5-pool-模块--连接池)
6. [transaction 模块 — 事务](#6-transaction-模块--事务)
7. [model 模块 — Model trait 与关联关系](#7-model-模块--model-trait-与关联关系)
8. [query 模块 — QueryBuilder](#8-query-模块--querybuilder)
9. [migration 模块 — 迁移系统](#9-migration-模块--迁移系统)
10. [cache 模块 — 缓存](#10-cache-模块--缓存)
11. [hooks 模块 — 钩子系统](#11-hooks-模块--钩子系统)
12. [dynamic_sql 模块 — 动态 SQL](#12-dynamic_sql-模块--动态-sql)
13. [phinx_migration 模块 — Phinx 迁移](#13-phinx_migration-模块--phinx-迁移)
14. [find_with_related 模块 — 关联加载](#14-find_with_related-模块--关联加载)
15. [join_dsl 模块 — JOIN DSL](#15-join_dsl-模块--join-dsl)
16. [json_query 模块 — JSON 查询](#16-json_query-模块--json-查询)
17. [queryable 模块 — Queryable/FromRow](#17-queryable-模块--queryablefromrow)
18. [quick_query 模块 — 快捷查询](#18-quick_query-模块--快捷查询)
19. [schema_gen 模块 — Schema 生成](#19-schema_gen-模块--schema-生成)
20. [typed 模块 — 强类型列](#20-typed-模块--强类型列)
21. [typed_ast 模块 — 类型化 AST](#21-typed_ast-模块--类型化-ast)
22. [宏: sql_string! / typed_query!](#22-宏-sql_string--typed_query)

---

## 1. value 模块 — Value 枚举

### 1.1 `Value` 枚举（20 种变体）

**签名**: `pub enum Value { Null, Bool(bool), I8..I64, U8..U64, F32, F64, String(String), Bytes(Vec<u8>), Uuid(String), Date(String), DateTime(String), Time(String), Json(String), Array(Vec<Value>) }`

**不变量**:
- `Value` 是 `Clone + Debug + Send + Sync`
- 所有变体可序列化为 SQL 参数（通过 `to_param()`）
- `Value::Null` 在 SQL 参数中渲染为 `NULL`

**契约**:

| 方法 | 前置条件 | 后置条件 | 错误条件 |
|------|---------|---------|---------|
| `Value::is_null()` | — | `Value::Null` 返回 true，其他变体返回 false | 不 panic |
| `Value::as_i64()` | — | `I8..I64/U8..U64` 直接返回；`F32/F64` 截断返回；`Bool` true→1 false→0；`String` 解析数字；其他返回 None | 不 panic |
| `Value::as_f64()` | — | 数值类型转换返回；`String` 解析返回；其他 None | 不 panic |
| `Value::as_bool()` | — | `Bool` 直接返回；`I64` 0→false 非0→true；`String` 支持 "true"/"1"/"yes"/"on" | 不 panic |
| `Value::as_str()` | — | `String` 返回 `Some(&str)`；`Null` 返回 None；其他变体返回 None（不转换） | 不 panic |
| `Value::to_param()` | — | 返回 `Cow<str>`；字符串加单引号并转义；NULL 字面量；数字直接 | 不 panic |

**已知陷阱**:
- `as_i64()` 对 `F64(3.14)` 会截断为 `Some(3)`，**不是** `None`
- `as_str()` 对 `I64(42)` 返回 `None`，**不会**自动转字符串
- `String("true")` 通过 `as_bool()` 返回 `Some(true)`，但 `String("TRUE")` 也返回 `Some(true)`（不区分大小写）

### 1.2 `From` 实现

**契约**: `i64/u64/f64/bool/&str/String/Vec<u8>` 均可 `into()` 为 `Value`

**已知陷阱**:
- `let v: Value = 42i32.into()` **不编译** — 没有为 `i32` 实现 `From`，需用 `Value::I64(42)`
- `let v: Value = vec![1u8, 2].into()` 转换为 `Value::Bytes`，不是 `Value::Array`

---

## 2. db_type 模块 — DbType

### 2.1 `DbType` 枚举（11 种数据库）

**签名**: `pub enum DbType { MySQL, PostgreSQL, Sqlite, Redis, MongoDB, ClickHouse, Oracle, OceanBase, SqlServer, VectorDb, PureJsDb }`

**契约**:

| 方法 | 返回 | 不变量 |
|------|------|-------|
| `DbType::as_str()` | `"mysql"`/`"postgres"`/`"sqlite"` 等 | 小写字符串，稳定不变 |
| `DbType::default_port()` | 3306/5432/0 等 | Redis=6379, MongoDB=27017, ClickHouse=8123, Oracle=1521 |
| `DbType::supports_transaction()` | bool | SQL 数据库（MySQL/PG/SQLite/Oracle/OceanBase/SqlServer）返回 true；NoSQL 返回 false |
| `DbType::supports_foreign_key()` | bool | 同上 |

**不变量**:
- 枚举顺序稳定，不重新排序（影响序列化）
- 新增 DbType 必须实现 `as_str/default_port/supports_transaction/supports_foreign_key`

---

## 3. error 模块 — 错误类型体系

### 3.1 `DbError`（20 变体，错误码 DB001-DB020）

**契约**:
- 每个变体有唯一错误码（`error_code()` 返回 `"DB001"` 等）
- `is_retryable()` 对 `ConnectionTimeout`/`ConnectionRefused` 返回 true，其他 false
- `Display` 实现稳定（影响日志/告警匹配）

**已知陷阱**:
- `DbError::PoolError(_)` 包装 `PoolError`，`source()` 返回 `Some(&PoolError)`
- 错误码字符串不可变（破坏会断日志匹配）

### 3.2 `PoolError`（6 变体，PL001-PL006）

**变体**: `Exhausted | Timeout | AlreadyAcquired | InvalidConfig | Closed | Other(String)`

**关键契约（v0.2.0 新增）**:
- `Pool::close_all()` 后调用 `acquire()` 必须返回 `Err(PoolError::Closed)`
- **此契约在 v0.2.0 由"仍可创建新连接"改为"拒绝创建"，属于行为变更**

**契约测试**: `tests/contracts/pool_contract.rs::test_close_all_blocks_acquire`

### 3.3 `TxError`（6 变体）

**变体**: `NotStarted | CommitFailed | RollbackFailed | SavepointError(String) | NotActive(TransactionState) | Other(String)`

**关键契约（v0.2.0 新增）**:
- 已 `commit`/`rollback` 的事务再调用 `savepoint()` 必须返回 `Err(TxError::NotActive(state))`
- **此契约在 v0.2.0 由 `SavepointError` 改为 `NotActive`，属于错误类型变更**

**契约测试**: `tests/contracts/transaction_contract.rs::test_savepoint_after_commit_returns_not_active`

### 3.4 `CacheError`（6 变体，CH001-CH006）

**契约**:
- `CacheError::NotFound` 仅在 `get()` 时返回；`set()` 不会返回此错误
- `CacheError::InvalidConfig` 在 `MemoryCache::new(capacity=0)` 时返回

---

## 4. dialect 模块 — 方言系统

### 4.1 `Dialect` trait

**契约**:

| 方法 | 不变量 |
|------|-------|
| `quote_identifier(name)` | MySQL 用 `` ` ``、PG/SQLite 用 `"`、Oracle 用 `"` |
| `quote_string(s)` | 单引号包裹，内部 `'` 转义为 `''` |
| `limit_offset_sql(limit, offset)` | MySQL/PG/SQLite 用 `LIMIT .. OFFSET ..`；Oracle 12c+ 用 `OFFSET .. ROWS FETCH NEXT .. ROWS ONLY` |
| `auto_increment_keyword()` | MySQL=`AUTO_INCREMENT`；PG=`SERIAL`/`GENERATED`；Oracle=`GENERATED BY DEFAULT AS IDENTITY` |
| `json_extract(column, path)` | MySQL=`JSON_EXTRACT(col, '$.path')`；PG=`col #>> '{path}'`；SQLite=`json_extract(col, '$.path')`；Oracle=`JSON_VALUE(col, '$.path')` |

### 4.2 `get_dialect(db_type)`

**契约**:
- 返回 `Result<Box<dyn Dialect>, DbError>`
- NoSQL 数据库（Redis/MongoDB/ClickHouse/VectorDb/PureJsDb）返回 `Err(DbError::UnsupportedDialect)`

**已知陷阱**:
- `get_dialect(DbType::OceanBase)` 返回 MySQL 方言（OceanBase 兼容 MySQL 协议）
- `get_dialect(DbType::SqlServer)` 返回 `Ok` 但功能受限

---

## 5. pool 模块 — 连接池

### 5.1 `Pool` 结构体

**契约**:

| 方法 | 前置条件 | 后置条件 | 错误条件 |
|------|---------|---------|---------|
| `Pool::new(config, factory)` | `config.validate()` 通过 | 创建空池 | `InvalidConfig` |
| `Pool::acquire().await` | 池未关闭 | 返回 `PooledConnection` 或创建新连接 | `Exhausted`/`Timeout`/`Closed` |
| `Pool::release(conn).await` | conn 来自此池 | 连接归还 idle 队列或关闭（超过 max_lifetime） | 不返回错误 |
| `Pool::status().await` | — | 返回 `PoolStatus { idle, active, max, min }` | 不 panic |
| `Pool::reap_idle().await` | — | 关闭超过 `idle_timeout` 的连接 | 不返回错误 |
| `Pool::close_all().await` | — | 关闭所有连接，标记池为 Closed | 不返回错误 |

**关键不变量（v0.2.0）**:
1. `close_all()` 后，`acquire()` 必须返回 `Err(PoolError::Closed)` — **不再创建新连接**
2. `release()` 后立即 `status().active` 必须减 1（除非连接超时被丢弃）
3. `acquire()` 必须遵守 `acquire_timeout`，超时返回 `Err(PoolError::Timeout)`
4. `PooledConnection::into_inner(self)` 消费连接，返回 `Box<dyn Connection>`
5. `release()` 接受 `PooledConnection`（不是 `Box<dyn Connection>`）

**契约测试**:
- `tests/contracts/pool_contract.rs::test_close_all_blocks_acquire`
- `tests/contracts/pool_contract.rs::test_release_decreases_active`
- `tests/contracts/pool_contract.rs::test_acquire_respects_timeout`
- `tests/contracts/pool_contract.rs::test_pooled_connection_into_inner`

### 5.2 `PoolConfig` 与 `PoolConfigBuilder`

**契约**:
- `max_size >= 1`，否则 `validate()` 返回 `Err(InvalidConfig)`
- `min_idle <= max_size`
- `acquire_timeout > 0`
- 默认值常量：`DEFAULT_MAX_SIZE=100`, `DEFAULT_MIN_IDLE=5`, `DEFAULT_ACQUIRE_TIMEOUT=30`, `DEFAULT_IDLE_TIMEOUT=600`, `DEFAULT_MAX_LIFETIME=1800`

### 5.3 `Connection` trait

**契约**:
- `execute(sql) -> Result<u64, DbError>` 返回受影响行数
- `query(sql) -> Result<QueryRows, DbError>` 返回行列表
- `ping() -> Result<(), DbError>` 健康检查

### 5.4 `PooledConnection`

**关键契约（v0.2.0 新增）**:
- `PooledConnection` 实现 `Deref<Target = dyn Connection>` 和 `DerefMut`
- `PooledConnection::into_inner(self) -> Box<dyn Connection>` 消费 self 提取内部连接
- **从 `Box<dyn Connection>` 到 `PooledConnection` 没有 `From` 实现**（必须通过 `Pool::acquire()`）

**已知陷阱**:
- `Transaction::new(conn, opts)` 接受 `Box<dyn Connection>`，不接 `PooledConnection`
- 模式：`let conn = pool.acquire().await?; let tx = Transaction::new(conn.into_inner(), opts);`

---

## 6. transaction 模块 — 事务

### 6.1 `Transaction`

**契约**:

| 方法 | 前置条件 | 后置条件 | 错误条件 |
|------|---------|---------|---------|
| `Transaction::new(conn, opts)` | conn 是有效连接 | state=`Active` | 不返回错误 |
| `tx.execute(sql).await` | state=`Active` | 返回受影响行数 | `NotActive` |
| `tx.query(sql).await` | state=`Active` | 返回行列表 | `NotActive` |
| `tx.commit().await` | state=`Active` | state=`Committed`，连接被消费 | `NotActive`/`CommitFailed` |
| `tx.rollback().await` | state=`Active` | state=`RolledBack`，连接被消费 | `NotActive`/`RollbackFailed` |
| `tx.savepoint().await` | state=`Active` | 返回 savepoint 名称 `"sp_N"` | `NotActive(state)` |
| `tx.rollback_to_savepoint(name).await` | state=`Active`, name 来自 `savepoint()` | 回滚到 savepoint | `NotActive`/`SavepointError` |
| `tx.release_savepoint(name).await` | state=`Active`, name 来自 `savepoint()` | 释放 savepoint | `NotActive`/`SavepointError` |
| `tx.take_connection()` | state=`Committed` 或 `RolledBack` | 返回原连接 | `NotStarted` |
| `tx.options()` | — | 返回 `&TransactOptions` | 不 panic |
| `tx.state()` | — | 返回当前 `TransactionState` | 不 panic |
| `tx.is_active()` | — | state=`Active` 时 true | 不 panic |

**关键不变量（v0.2.0）**:
1. `commit()`/`rollback()` 后 state 必须变更，**不能再次 commit/rollback**
2. `commit()`/`rollback()` 后调用 `savepoint()` 必须返回 `Err(TxError::NotActive(state))` — **不再返回 `SavepointError`**
3. savepoint 名称格式为 `"sp_N"`，N 从 1 递增
4. `Transaction::new` 接受 `Box<dyn Connection>`（**不接受 `PooledConnection`**）
5. `take_connection()` 仅在 `Committed`/`RolledBack` 状态可用，`Active` 状态返回 `Err(NotStarted)`

**契约测试**:
- `tests/contracts/transaction_contract.rs::test_commit_transitions_state`
- `tests/contracts/transaction_contract.rs::test_double_commit_returns_not_active`
- `tests/contracts/transaction_contract.rs::test_savepoint_after_commit_returns_not_active`
- `tests/contracts/transaction_contract.rs::test_savepoint_name_format`
- `tests/contracts/transaction_contract.rs::test_take_connection_after_commit`

### 6.2 `TransactOptions`

**契约**:
- `TransactOptions::default()` 返回 `ReadCommitted` 隔离级别，非只读，无超时
- `with_isolation(level)` 链式设置隔离级别
- `read_only()` 设置只读
- `with_timeout(d)` 设置超时

### 6.3 `TransactionManager`

**契约**:
- `begin(id, conn, opts)` 创建命名事务，状态 `Active`
- `commit(id)`/`rollback(id)` 操作命名事务
- `state(id)` 返回 `Option<TransactionState>`
- `list()` 返回所有事务 ID
- `remove(id)` 移除已结束的事务，返回 `Option<Transaction>`
- 并发安全（内部 `Mutex<HashMap<String, Transaction>>`）

---

## 7. model 模块 — Model trait 与关联关系

### 7.1 `Model` trait

**契约**:

| 方法 | 必需 | 不变量 |
|------|------|-------|
| `table_name()` | 是 | 返回静态表名字符串 |
| `pk_name()` | 否 | 默认 `"id"` |
| `pk()` | 是 | 返回主键值 |
| `set_pk(pk)` | 是 | 设置主键值 |
| `foreign_key(relation)` | 否 | 默认 `"{relation}_id"` |
| `timestamp_fields()` | 否 | 默认 `None` |
| `soft_delete_field()` | 否 | 默认 `None` |

**约束**:
- `type PrimaryKey: Send + Sync + Debug + Display + Clone + Default`
- `Model: Send + Sync + Sized + 'static`

### 7.2 `ModelExt` trait

**契约**:
- `columns()` 返回所有列名
- `fillable()` 返回可批量赋值的列
- `guarded()` 返回受保护列（默认含主键）
- `hidden()` 返回序列化时隐藏的列
- `relations()` 返回关联关系映射
- `fill(data)` 批量赋值（仅 fillable 列，跳过 guarded）
- `to_json()` 序列化为 `serde_json::Value`

### 7.3 关联关系

| 类型 | 关系 | 外键位置 |
|------|------|---------|
| `BelongsTo` | 多对一（Order → User） | 当前表 |
| `HasMany` | 一对多（User → Orders） | 关联表 |
| `HasOne` | 一对多（User → Profile） | 关联表 |
| `BelongsToMany` | 多对多（User ↔ Role） | 中间表 |
| `MorphMany` | 多态一对多（Comment → Post/Video） | 关联表 + type 字段 |
| `MorphTo` | 多态所属（Comment → Post/Video） | 当前表 + type 字段 |

### 7.4 `RelationError`

**变体**: `NotFound | InvalidRelation | LoaderFailed(String)`

---

## 8. query 模块 — QueryBuilder

### 8.1 `QueryBuilder<M>`

**契约**:
- 所有 builder 方法返回 `Self`（链式）
- `new(dialect)` 创建空 builder
- `table(name)` 设置表名
- `select(cols)`/`where_cond(s)`/`or_where(s)`/`where_in(col, vals)`/`where_between(col, a, b)`/`where_null(col)` 条件构造
- `order_by(col)`/`order_desc(col)`/`group_by(col)`/`having(s)` 排序分组
- `limit(n)`/`offset(n)`/`page(n, size)` 分页
- `join_inner(table, left, right)`/`join_left(table, left, right)` 连接
- `build_select()`/`build_insert(data)`/`build_update(data)`/`build_delete()` 生成 SQL
- `build_count()`/`build_max(col)`/`build_min(col)`/`build_sum(col)`/`build_avg(col)` 聚合
- `validate()`/`validate_insert(data)`/`validate_update(data)`/`validate_delete()` 校验

**不变量**:
- `build_insert(&data)` 当 data 为空时返回错误（不生成 `INSERT INTO t () VALUES ()`）
- `build_update(&data)` 当 data 为空时返回错误
- `validate()` 检查 SQL 注入、括号平衡、表名/列名合法性

---

## 9. migration 模块 — 迁移系统

### 9.1 `Migrator`

**契约**:
- `new(ctx)` 创建迁移器
- `add_migrations(migs)` 链式添加迁移
- `migrate().await` 执行所有待迁移
- `up(Some(version)).await` 执行到指定版本
- `down(Some(version)).await` 回滚到指定版本
- `rollback(version).await` 回滚单个迁移
- `reset().await` 全部回滚 + 重新执行
- `refresh().await` 同 reset
- `progress()` 返回 `MigrationProgress { total, applied, pending }`
- `get_pending_migrations()` 返回待执行迁移列表

### 9.2 `FileMigrationResolver`

**契约**:
- 文件命名：`<version>_<name>_up.sql` / `<version>_<name>_down.sql`
- `resolve(db_type)` 返回 `Result<Vec<Migration>, DbError>`
- 缺少 `_down.sql` 时该迁移被跳过（不报错）

### 9.3 `SchemaBuilder`

**契约**:
- `new(table)` 创建建表 builder
- `add_column(col)`/`add_index(idx)`/`add_foreign_key(fk)` 链式添加
- `build(db_type)` 生成对应方言的 DDL

### 9.4 `ColumnDef` / `IndexDef` / `ForeignKeyDef`

**契约**:
- `ColumnDef::new(name, type_str)` 创建列定义
- `.length(n)`/`.not_null()`/`.auto_increment()`/`.default(value)` 链式配置
- `IndexDef::new(name, cols)`/`.unique()` 创建索引
- `ForeignKeyDef::new(name, col, ref_table, ref_col)`/`.on_delete(action)`/`.on_update(action)` 创建外键

---

## 10. cache 模块 — 缓存

### 10.1 `Cache` trait

**契约**:
- `get(key) -> Result<Option<Vec<u8>>, CacheError>`（命中返回 `Ok(Some(bytes))`，未命中返回 `Ok(None)`）
- `set(key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<(), CacheError>`（TTL 为 None 时持久存储）
- `delete(key) -> Result<(), CacheError>`（删除键；对不存在的键也返回 `Ok(())`）
- `clear() -> Result<(), CacheError>`（清空所有键）
- `exists(key) -> Result<bool, CacheError>`（判断键是否存在）
- `expire(key, ttl: Duration) -> Result<(), CacheError>`（为已存在键设置 TTL；键不存在时返回 `Err(CacheError::NotFound)`）
- `ttl(key) -> Result<Option<Duration>, CacheError>`（返回剩余 TTL；无 TTL 时返回 `Ok(None)`；键不存在时返回 `Err(CacheError::NotFound)`）

**已知行为细节**:
- TTL 过期项在下次 `get()` / `exists()` / `ttl()` 时惰性清理
- `CacheError::NotFound(String)` 用于 `expire` / `ttl` 操作的键不存在场景

### 10.2 `MemoryCache`

**契约**:
- `new() -> Self`（不接 capacity 参数，返回 `Self` 而非 `Result`）
- `Default::default() -> Self`（与 `new()` 等价）
- `with_ttl(default_ttl: Duration) -> Self`（设置默认 TTL；`set` 时未显式传 TTL 时使用此默认值）
- 内部使用 `DashMap`，并发安全
- TTL 过期项在下次访问时惰性清理

### 10.3 `MultiLevelCache`

**契约**:
- `new() -> Self`（不接参数，返回 `Self`）
- `get(key) -> Result<Option<Vec<u8>>, CacheError>`（空的多级缓存对所有键返回 `Ok(None)`）
- 串联 L1(Memory) + L2(可选 Redis/MongoDB)
- `get()` 依次查询 L1 → L2，命中时回填上层
- `set()` 同时写入所有层

---

## 11. hooks 模块 — 钩子系统（v0.2.0 新增）

### 11.1 `HookContext`

**契约**:
- `new()` 创建默认上下文
- `with_tenant(id)`/`with_operator(id)`/`with_timestamp(ts)` 链式设置
- `set_meta(k, v)`/`get_meta(k)` 自定义元数据

### 11.2 `HookEvent`

**变体**: `BeforeInsert/AfterInsert/BeforeUpdate/AfterUpdate/BeforeDelete/AfterDelete/BeforeFind/AfterFind/BeforeValidate/AfterValidate`

**方法**:
- `is_before()` / `is_after()` — 区分前后
- `is_write_level()` — Insert/Update/Delete
- `is_find_level()` — Find
- `is_validate_level()` — Validate

### 11.3 `Hookable` trait

**契约**:
- 实现 `Model + ModelExt`
- 提供 `before_insert/after_insert/before_update/...` 等钩子方法（默认空实现）
- 钩子返回 `HookResult<T>` = `Result<T, DbError>`

### 11.4 `HookDispatcher`

**契约**:
- 静态方法 `insert/update/delete/restore/find/validate`
- 调用顺序：`before_validate` → `validate` → `after_validate` → `before_insert` → 实际插入 → `after_insert`
- 任何钩子返回 `Err` 中断后续流程

### 11.5 `SoftDelete` trait

**契约**:
- 实现 `Model`
- `soft_delete_field()` 返回字段名（如 `"deleted_at"`）
- `delete()` 实际执行 `UPDATE SET deleted_at = NOW()` 而非 `DELETE`
- `restore()` 反向操作
- `force_delete()` 真正删除

### 11.6 `TenantModel` trait

**契约**:
- 实现 `Model`
- `tenant_field()` 返回字段名（如 `"tenant_id"`）
- `tenant_id() -> i64` 返回当前模型的租户 ID
- `set_tenant_id(&mut self, tenant_id: i64)` 设置租户 ID
- 所有查询自动添加 `WHERE tenant_id = ?`
- `insert` 时自动填充 `tenant_id` 从 `HookContext`

### 11.7 `HookRegistry` / `ScopeRegistry`

**契约**:
- `HookRegistry::register(event, hook_fn)` 注册全局钩子
- `dispatch(event, ctx)` 派发事件，任何钩子失败则返回错误
- `ScopeRegistry::disable(name)`/`enable(name)` 启用禁用全局作用域
- `without_scope(name, f)` 临时禁用作用域执行 f

---

## 12. dynamic_sql 模块 — 动态 SQL

### 12.1 `DynamicSqlParser`

**契约**:
- `from_xml(xml)` 解析 XML 模板
- `build(id, params)` 根据参数构造 SQL
- 支持 `<if test="...">`、`<where>`、`<set>`、`<foreach>` 标签

### 12.2 `SqlParams`

**契约**:
- `new()` 创建空参数
- `set_int/set_float/set_str/set(name, value)` 设置参数
- `get(name)` 返回 `Option<&ParamValue>`
- `is_null(name)` 当参数不存在或为 `ParamValue::Null` 时返回 true
- `contains(name)` 当参数存在时返回 true（**不**检查是否为 Null）

**已知陷阱**:
- `is_null("x")` 对不存在的 key 返回 true
- `is_not_null("x") = !is_null("x")`

### 12.3 表达式语法

**契约**:
- `name != null` → 参数不为 null
- `name == null` → 参数为 null
- `name == 'value'` → 字符串相等
- `name != 'value'` → 字符串不等
- `name > 18` / `>= 18` / `< 18` / `<= 18` → 数值比较
- `expr1 and expr2` / `expr1 or expr2` → 逻辑组合

---

## 13. phinx_migration 模块 — Phinx 迁移

### 13.1 `IndexOptions` / `ForeignKeyOptions`

**契约**:
- 实现 `Default`（v0.2.0 改为 `#[derive(Default)]`，移除手动 impl）
- 链式 API：`IndexOptions::default().unique().name("idx_x")`

---

## 14. find_with_related 模块 — 关联加载

**契约**:
- `find_with_related<M, R>(conn, id, relation)` 加载主模型 + 关联
- 返回 `WithRelation<M>` 包含主模型和关联数据
- 支持链式 `.with(relation).with(relation2).load(conn).await`

---

## 15. join_dsl 模块 — JOIN DSL

**契约**:
- `JoinBuilder::new(table, kind)` 创建 JOIN 子句
- `.on(left, right)`/`.using(cols)` 设置连接条件
- 支持 INNER/LEFT/RIGHT/FULL/CROSS JOIN

---

## 16. json_query 模块 — JSON 查询

**契约**:
- `json_extract(col, path, dialect)` 生成方言相关的 JSON 提取 SQL
- `json_set(col, path, value, dialect)` 生成 JSON 设置 SQL
- `json_contains(col, value, dialect)` 生成 JSON 包含判断

---

## 17. queryable 模块 — Queryable/FromRow

### 17.1 `Queryable` trait

**契约**:
- `from_values(values: Vec<Value>) -> Result<Self, QueryError>` 按列顺序反序列化
- `from_values_with_desc(values, desc)` 带行描述反序列化
- 实现：`Value`、`(Value, Value)`、`(Value, Value, Value)`、用户结构体

### 17.2 `FromRow` trait

**契约**:
- `from_row(row: HashMap<String, Value>) -> Result<Self, QueryError>` 按列名反序列化
- 列名匹配大小写敏感
- 缺少列返回 `Err(QueryError::MissingColumn)`

### 17.3 `QueryError`

**变体**: `ColumnCountMismatch { expected, actual }` / `TypeMismatch { column, expected }` / `MissingColumn { column }` / `Custom(String)`

---

## 18. quick_query 模块 — 快捷查询

**契约**:
- `find_by_id<M>(conn, id)` 按主键查
- `find_all<M>(conn)` 查全部
- `find_where<M>(conn, "age > ?", vec![18.into()])` 条件查
- `count<M>(conn)` 计数
- `exists<M>(conn, id)` 判断存在

---

## 19. schema_gen 模块 — Schema 生成

**契约**:
- `generate_from_model::<M>(db_type)` 从 Model 生成 DDL
- `generate_diff::<M>(current_schema, db_type)` 生成 ALTER 语句

---

## 20. typed 模块 — 强类型列

### 20.1 `TypedColumn` trait

**契约（v0.2.0 新增 `type SqlType`）**:
- `const NAME: &'static str` — 列名
- `type Table` — 所属表
- `type RustType` — Rust 类型
- `type SqlType` — SQL 类型（实现 `SqlType` trait，可以是 `Untyped`）

**已知陷阱**:
- 手动实现 `TypedColumn` 必须包含 `type SqlType`，否则编译错误
- 使用 `typed_query!` 宏自动生成完整实现

### 20.2 `typed_query!` 宏

**契约**:
- 输入：表名、列定义（name: type 对）
- 输出：表结构体 + 每列的 `TypedColumn` 实现 + 便捷常量
- 生成的 `TypedColumn` 实现包含 `type SqlType = Untyped`

---

## 21. typed_ast 模块 — 类型化 AST

### 21.1 `Untyped`

**契约**: 标记类型，表示未指定 SQL 类型。实现 `SqlType` trait。

---

## 22. 宏: sql_string! / typed_query!

### 22.1 `sql_string!`

**契约**:
- 编译时校验 SQL 语法
- 支持 `params:` 子句绑定参数
- 返回 `&'static str` 或包含参数的结构体

**已知陷阱**:
- 不平衡的括号会编译错误
- 空字符串不编译错误

### 22.2 `typed_query!`

**契约**:
- 生成表结构体 + 列实现
- 列实现自动包含 `type SqlType = Untyped`
- 支持自定义 SQL 类型：`typed_query!(users { id: i64 as Int, name: String as Varchar })`

---

## 附录 A：行为变更历史

| 版本 | 变更 | 影响 |
|------|------|------|
| v0.2.0 | `Pool::close_all()` 后 `acquire()` 改为返回 `PoolError::Closed` | 调用方需处理 Closed 错误 |
| v0.2.0 | `Transaction::savepoint()` 在非 Active 状态改为返回 `TxError::NotActive(state)` | 错误匹配从 `SavepointError` 改为 `NotActive` |
| v0.2.0 | `Pool::acquire()` 返回 `PooledConnection` 而非 `Box<dyn Connection>` | 调用方需用 `into_inner()` 提取 |
| v0.2.0 | `TypedColumn` 新增 `type SqlType` 关联类型 | 手动实现需补充此字段 |
| v0.2.0 | `IndexOptions`/`ForeignKeyOptions` 改用 `#[derive(Default)]` | API 无变化，仅实现简化 |

## 附录 B：契约测试索引

| 测试文件 | 覆盖契约 |
|---------|---------|
| `tests/contracts/pool_contract.rs` | §5.1 Pool 不变量 |
| `tests/contracts/transaction_contract.rs` | §6.1 Transaction 不变量 |
| `tests/contracts/model_contract.rs` | §7.1 Model trait |
| `tests/contracts/hooks_contract.rs` | §11 钩子系统 |
| `tests/contracts/value_contract.rs` | §1 Value 枚举 |
| `tests/contracts/error_contract.rs` | §3 错误类型 |
| `tests/contracts/dialect_contract.rs` | §4 方言系统 |
| `tests/contracts/query_contract.rs` | §8 QueryBuilder |
| `tests/contracts/migration_contract.rs` | §9 迁移系统 |
| `tests/contracts/cache_contract.rs` | §10 缓存 |
| `tests/contracts/dynamic_sql_contract.rs` | §12 动态 SQL |
| `tests/contracts/queryable_contract.rs` | §17 Queryable/FromRow |
