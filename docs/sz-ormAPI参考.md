# SZ-ORM API 参考手册

> 项目名称：SZ-ORM（鲜视达 ORM）
> 文档版本：v4.0（新增 sz-orm-vector + NL→SQL API + 生态扩展包）
> 适用版本：SZ-ORM v0.2.1（工作空间 39 个成员：37 个 lib + cli + examples）
> 测试：1970+ passed / 0 failed（112 个测试套件）
> 代码规模：85,834 LOC（src/ 18,430 + tests/ 67,404）
> 成熟度：L4 金融级（评分 4.98/5，CMMI Level 5 - 持续优化级，已知 Bug 0）
> 更新日期：2026-07-20
> 文档定位：核心 trait/结构体说明 + 各包公开 API 速查 + 错误处理指南

---

## 一、核心 trait 与结构体（sz-orm-core）

`use sz_orm_core::*;` 导入全部公共符号。重导出：`async_trait`、`bytes::Bytes`、`chrono::{DateTime, Utc}`、`serde::{Deserialize, Serialize}`、`sz_orm_macros::sql_string`。

### 1.1 Model / ModelExt（model.rs）

```rust
pub trait Model: Send + Sync + Sized + 'static {
    type PrimaryKey: Send + Sync + Debug + Display + Clone + Default;
    fn table_name() -> &'static str;              // 表名（必需）
    fn pk_name() -> &'static str { "id" }         // 主键列名
    fn pk(&self) -> Self::PrimaryKey;             // 获取主键值
    fn set_pk(&mut self, pk: Self::PrimaryKey);   // 设置主键值
    fn foreign_key(relation: &str) -> String;     // 外键命名，如 "user_id"
    fn timestamp_fields() -> Option<TimestampFields>; // 自动时间戳
    fn soft_delete_field() -> Option<&'static str>;   // 软删除字段
}

pub trait ModelExt: Model {
    fn columns() -> Vec<&'static str>;            // 所有列
    fn fillable() -> Vec<&'static str>;           // 可填充列
    fn guarded() -> Vec<&'static str>;            // 保护列（默认含主键）
    fn hidden() -> Vec<&'static str>;             // 隐藏列（不序列化）
    fn relations() -> HashMap<&str, Relation>;    // 关联关系
    fn fill(&mut self, data: HashMap<String, Value>); // 批量赋值
    fn to_json(&self) -> serde_json::Value;       // 序列化
}
```

关联关系枚举 `Relation`：`BelongsTo`（多对一）、`HasOne`（一对一）、`HasMany`（一对多）、`BelongsToMany`（多对多，经中间表），支持 eager loading。

### 1.2 QueryBuilder\<M\>（query.rs）

所有链式方法返回 `Self`，构建方法返回 `String`（SQL）。

| 类别 | 方法签名 | 说明 |
|------|---------|------|
| 构造 | `new(dialect: Box<dyn Dialect>) -> Self` | 创建构建器 |
| 表/列 | `table(impl Into<String>)` / `select(Vec<&str>)` | 指定表与查询列 |
| 条件 | `where_cond(impl Into<String>)` | AND 条件 |
| | `or_where(impl Into<String>)` | OR 条件 |
| | `where_in(field, Vec<Value>)` / `where_not_in(field, Vec<Value>)` | IN / NOT IN |
| | `where_between(field, Value, Value)` / `where_not_between(...)` | BETWEEN |
| | `where_null(field)` / `where_not_null(field)` | NULL 判断 |
| 排序分组 | `order_by(field)` / `order_desc(field)` | ASC / DESC |
| | `group_by(field)` / `having(cond)` | GROUP BY / HAVING |
| 分页 | `limit(usize)` / `offset(usize)` / `page(page, page_size)` | 分页 |
| JOIN | `join_inner(table, left_col, right_col)` | INNER JOIN |
| | `join_left(table, left_col, right_col)` | LEFT JOIN |
| | `join_right(table, left_col, right_col)` | RIGHT JOIN |
| 构建 | `build_select() -> String` | SELECT |
| | `build_insert(&HashMap<String, Value>) -> String` | INSERT |
| | `build_update(&HashMap<String, Value>) -> String` | UPDATE |
| | `build_delete() -> String` | DELETE |
| 聚合 | `build_count()` / `build_exists()` | COUNT(*) / EXISTS |
| | `build_max(field)` / `build_min(field)` / `build_sum(field)` / `build_avg(field)` | 聚合函数 |
| 校验 | `validate() -> Result<(), Vec<SqlValidationError>>` | 校验 SELECT |
| | `validate_insert(&data)` / `validate_update(&data)` / `validate_delete()` | 校验 DML（含空数据检测） |

### 1.3 Dialect（dialect.rs）

```rust
pub fn get_dialect(db_type: DbType) -> Result<Box<dyn Dialect>, DbError>
```

实现：`MySqlDialect`、`PostgresDialect`、`SqliteDialect`、`OracleDialect`。方言职责：标识符引用、字符串转义（`escape_string`）、占位符、分页（`build_pagination`）、DDL 生成（`build_create_table`/`build_alter_table`）、JSON 提取、全文搜索、布尔转整数、自增关键字。

### 1.4 Pool / Connection（pool.rs）

```rust
pub trait Connection: Send + Sync {
    fn execute<'a>(&'a mut self, sql: &'a str)
        -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>>;
    fn query<'a>(&'a mut self, sql: &'a str)
        -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>>;
    fn begin_transaction<'a>(&'a mut self) -> ...;
    fn commit<'a>(&'a mut self) -> ...;
    fn rollback<'a>(&'a mut self) -> ...;
}

pub trait ConnectionFactory: Send + Sync {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError>;
}
```

| 类型 | 关键方法 | 说明 |
|------|---------|------|
| `PoolConfig` | `validate() -> Result<(), PoolError>` | 配置校验 |
| `PoolConfigBuilder` | `new()` → `max_size(u32)` → `min_idle(u32)` → `acquire_timeout(u64)` → `idle_timeout(u64)` → `max_lifetime(u64)` → `build() -> Result<PoolConfig, PoolError>` | 构建器 |
| `Pool` | `new(config, Arc<dyn ConnectionFactory>) -> Result<Self, PoolError>` | 创建池 |
| | `acquire().await -> Result<Connection, PoolError>` | 带超时获取 |
| | `release(conn).await` | 归还 |
| | `status().await -> PoolStatus` | `{ idle, active, max, min }` |
| | `reap_idle().await` / `close_all().await` | 回收/关闭 |

### 1.5 Transaction（transaction.rs）

| 类型 | 关键方法 |
|------|---------|
| `TransactOptions` | `with_isolation(IsolationLevel)` / `read_only()` / `with_timeout(Duration)` |
| `Transaction` | `new(conn, options)` / `execute(sql)` / `query(sql)` / `commit()` / `rollback()` / `savepoint()` / `rollback_to_savepoint(&sp)` / `release_savepoint(&sp)` / `state()` / `is_active()` / `options()` |
| `TransactionManager` | `new()` / `begin(name, conn, opts)` / `commit(name)` / `rollback(name)` / `list()` / `state(name)` |
| `IsolationLevel` | `ReadUncommitted` / `ReadCommitted` / `RepeatableRead` / `Serializable` |

### 1.6 Migration（migration.rs）

| 类型 | 关键方法 |
|------|---------|
| `Migration` | `new(version, name, sql_up, sql_down)` / `with_batch(i32)` / `with_executed_at(DateTime<Utc>)` |
| `FileMigrationResolver` | `new(PathBuf)` / `resolve(DbType) -> Result<Vec<Migration>>` |
| `Migrator` | `new(MigrationContext)` / `add_migration(m)` / `add_migrations(vec)` / `migrate()` / `up(Option<ver>)` / `down(Option<ver>)` / `rollback(ver)` / `reset()` / `refresh()` / `progress() -> MigrationProgress` |
| `MigrationProgress` | `new(total, applied)` / `percent_complete() -> f64` |
| `SchemaBuilder` | `new(table)` / `add_column(ColumnDef)` / `add_index(IndexDef)` / `add_foreign_key(ForeignKeyDef)` / `if_not_exists(bool)` / `build(DbType) -> String` |
| `ColumnDef` | `new(name, type)` / `not_null()` / `default(v)` / `auto_increment()` / `unique()` / `comment(c)` / `length(n)` |
| `IndexDef` | `new(name, Vec<&str>)` / `unique()` |
| `ForeignKeyDef` | `new(name, col, ref_table, ref_col)` / `on_delete(action)` / `on_update(action)` |

### 1.7 Value（value.rs）

20 种变体：`Null | Bool | I8..I64 | U8..U64 | F32 | F64 | String | Bytes | Uuid | Date | DateTime | Time | Json | Array`。

| 方法 | 签名 | 说明 |
|------|------|------|
| 类型判断 | `is_null()` / `is_bool()` / `is_i64()` / `is_f64()` / `is_string()` / `is_bytes()` / `is_object()` | — |
| 取值 | `as_str() -> Option<&str>` | — |
| | `as_i64() -> Option<i64>` | 支持 F32/F64/Bool/String 转换 |
| | `as_f64() -> Option<f64>` | — |
| | `as_bool() -> Option<bool>` | 支持 "true"/"1"/"yes"/"on" |
| | `as_bytes() -> Option<&[u8]>` | — |
| 参数化 | `to_param() -> Cow<str>` | SQL 参数格式 |
| 构造 | `from_map(HashMap<String, Value>)` / `From<T>`（i64/&str/Vec\<u8\> 等） | — |

### 1.8 Cache（cache.rs）

| 类型 | 关键方法 |
|------|---------|
| `Cache`（trait） | get/set/delete/clear 等异步缓存接口 |
| `MemoryCache` | `new()` / `with_ttl(Duration)` |
| `MultiLevelCache` | `new()` / `add_cache(Box<dyn Cache>)` |
| `CacheStats` | 命中/未命中统计 |

### 1.9 类型别名与常量

```rust
pub type Shared<T> = Arc<T>;
pub type Boxed<T> = Box<T>;
pub type DbResult<T>    = Result<T, DbError>;
pub type PoolResult<T>  = Result<T, PoolError>;
pub type CacheResult<T> = Result<T, CacheError>;
pub type TxResult<T>    = Result<T, TxError>;

pub const DEFAULT_BATCH_SIZE: usize = 1000;
pub const DEFAULT_ACQUIRE_TIMEOUT: u64 = 30;
pub const DEFAULT_IDLE_TIMEOUT: u64 = 600;
pub const DEFAULT_MAX_LIFETIME: u64 = 1800;
pub const DEFAULT_MIN_IDLE: u32 = 5;
pub const DEFAULT_MAX_SIZE: u32 = 100;
```

---

## 二、各包公开 API 列表

### 2.1 sz-orm-sqlx（sqlx 适配器）

| 导出 | 说明 |
|------|------|
| `MySqlPoolHandle` / `PgPoolHandle` / `SqlitePoolHandle` | `connect(url).await` 建立底层 sqlx 池 |
| `SqlxMySqlConnection` / `SqlxPgConnection` / `SqlxSqliteConnection` | `Connection` trait 实现 |
| `SqlxMySqlConnectionFactory` / `SqlxPgConnectionFactory` / `SqlxSqliteConnectionFactory` | `ConnectionFactory` 实现，`new(Arc<Handle>)` |
| `map_sqlx_error(sqlx::Error) -> DbError` | 错误映射 |
| `pub use sz_orm_core;` | 重导出核心包 |

### 2.2 sz-orm-sql-validator（SQL 校验）

| 函数 | 签名 | 说明 |
|------|------|------|
| `validate` | `(sql: &str) -> ValidationResult` | 自动识别语句类型并校验 |
| `validate_select` / `validate_insert` / `validate_update` / `validate_delete` | `(sql: &str) -> ValidationResult` | 分类校验（DELETE 要求 WHERE） |
| `validate_sql` | `(sql: &str) -> ValidationResult` | 通用结构校验 |
| `detect_statement_type` | `(sql: &str) -> SqlStatementType` | Select/Insert/Update/Delete/Create/Drop/Alter |
| `validate_parameter_count` | `(sql: &str, expected: usize) -> ValidationResult` | 占位符个数 |
| `validate_table_name` / `validate_column_name` | `(name: &str) -> ValidationResult` | 标识符合法性 |
| 类型 | `SqlValidationError`（12 变体）、`ValidationResult = Result<(), SqlValidationError>` | — |

### 2.3 sz-orm-macros（编译时宏）

| 宏 | 语法 | 说明 |
|----|------|------|
| `sql_string!` | `sql_string!("SQL")` / `sql_string!("SQL"; params: N)` | 编译期校验 SQL：SELECT 必含 FROM、INSERT 必含 INTO/VALUES、UPDATE 必含 SET、DELETE 必含 FROM、括号平衡、字符串闭合、注入模式、参数个数 |

### 2.4 sz-orm-crypto（加密原语，RustCrypto 审计栈）

| 导出 | 说明 |
|------|------|
| `sha256(&[u8]) -> [u8; 32]` / `sha256_hex(&[u8]) -> String` | SHA-256 |
| `hmac_sha256(key, msg) -> [u8; 32]` / `hmac_sha256_hex(key, msg) -> String` | HMAC-SHA256 |
| `Crypter`（trait）→ `AesGcmCrypter` | AES-256-GCM 加解密 |
| `PasswordHasher`（trait）→ `Pbkdf2Hasher` | PBKDF2 口令散列 |
| `ApiSigner`（trait）→ `HmacSigner` | API 签名 |
| `CryptoError` | 错误类型 |

### 2.5 sz-orm-auth（JWT 鉴权）

| 导出 | 说明 |
|------|------|
| `JwtAuthenticator` | 登录认证、令牌签发/校验 |
| `JwtEncoder` / `JwtHeader` / `JwtClaims` | JWT 编解码（HS256，sha2+hmac+base64） |
| `Credentials` / `Token` / `User` / `Claims` | 认证数据模型 |
| `Authorizer`（trait）→ `RbacAuthorizer` | RBAC 授权 |
| `AuthError` | 错误类型；签名比较使用 `constant_time_eq` 防时序攻击 |

### 2.6 sz-orm-scheduler（定时任务）

| 导出 | 说明 |
|------|------|
| `Scheduler`（trait）→ `CronScheduler` | 秒级 Cron 调度 |
| `ScheduledTask` / `CronExpr` | 任务与表达式 |
| `JobHandler`（trait）→ `CounterJobHandler` / `RecordingJobHandler` | 任务处理器 |
| `SchedulerError` | 错误类型 |

### 2.7 sz-orm-mqtt

| 导出 | 说明 |
|------|------|
| `MqttPlugin` / `MqttConfig` / `MqttMessage` / `MqttTopic` | 内存实现与配置 |
| `QoS` | `AtMostOnce(0)` / `AtLeastOnce(1)` / `ExactlyOnce(2)` |
| `TopicFilter` / `topic_matches(topic, filter)` | 主题匹配 |
| `RealMqttClient`（feature `real-broker`） | rumqttc 0.25 真实 broker 客户端 |
| `MqttError` | 错误类型 |

### 2.8 sz-orm-websocket

| 导出 | 说明 |
|------|------|
| `RealtimePusher` / `InMemorySender` / `PushResult` | 消息推送（内存） |
| `WsServer`（feature `server`） | tokio-tungstenite 0.30 真实服务端 |
| `WebSocketHandler`（trait）→ `DefaultWebSocketHandler` | 连接/消息回调 |
| `WebSocketMessage` / `MessageType` / `WebSocketConnection` / `WsContext` / `WsMessageBuilder` | 消息模型 |
| `UserId = i64` / `WsError` | 类型别名与错误 |

### 2.9 sz-orm-queue（消息队列）

```rust
pub trait MessageQueue: Send + Sync {
    async fn publish(&self, topic: &str, message: &[u8]) -> Result<(), MqError>;
    async fn consume(&self, topic: &str) -> Result<Option<Message>, MqError>;
    async fn ack(&self, message_id: &str) -> Result<(), MqError>;
    async fn subscribe(&self, topic: &str) -> Result<(), MqError>;
}
```

| 导出 | 说明 |
|------|------|
| `Message` | `new(topic, payload)` / `with_key(k)` / `text_message()` / `json_message()` / `text()` / `json()` |
| `QueueConfig` / `MqProvider` / `QueueWrapper` | 配置与统一封装 |
| `KafkaConfig` / `RabbitConfig` / `NatsConfig` / `ActiveConfig` / `RocketConfig` / `PulsarConfig` | 6 种提供商配置 |
| `InMemoryKafkaQueue` / `InMemoryRabbitmqQueue` / `InMemoryNatsQueue` / `InMemoryActivemqQueue` / `InMemoryRocketmqQueue` / `InMemoryPulsarQueue` | 内存实现 |
| `LapinRabbitmqQueue`（feature `rabbitmq`） | lapin 4.10 真实 RabbitMQ（AMQP 0.9.1） |
| `MqError` | 错误类型 |

### 2.10 sz-orm-storage（对象存储）

```rust
pub trait Storage: Send + Sync {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<String, StorageError>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
}
```

| 导出 | 说明 |
|------|------|
| `StorageBuilder` / `StorageConfig` / `StorageProvider` / `StorageWrapper` | 构建器与统一封装 |
| `S3Config` / `AliyunConfig` / `TencentConfig` / `QiniuConfig` / `HuaweiConfig` / `UpYunConfig` | 各云配置 |
| `S3Storage` / `AliyunOssStorage` / `TencentCosStorage` / `QiniuKodoStorage` / `HuaweiObsStorage` / `UpYunStorage` / `LocalStorage` | 7 种实现 |
| `S3SdkStorage`（feature `s3-sdk`） | rust-s3 0.37 真实 S3（MinIO/AWS） |
| `StorageError` | 错误类型 |

### 2.11 sz-orm-ai

| 模块 | 导出 |
|------|------|
| `embedding` | `EmbeddingModel`（trait）、`SimpleEmbeddingModel`、`EmbeddingRecord`、`EmbeddingBatch`、`EmbeddingError` |
| `vector` | `VectorStore`（trait）、`InMemoryVectorStore`、`VectorRecord`、`SearchResult`、`VectorFilter`、`VectorMetric`、`CollectionMeta`、`VectorError` |
| `rag` | `RagEngine<E, V>`、`RagConfig`、`Document`、`Chunk`、`RagSearchResult` |
| 错误 | `AiError` |

### 2.12 sz-orm-back（备份与灾备）

| 导出 | 说明 |
|------|------|
| `BackupManager` / `BackupConfig` / `BackupResult` / `BackupManifest` / `BackupTable` / `BackupCatalog` / `ExportResult` | 备份（flate2 压缩 + sha2 校验） |
| `RestoreManager` / `RestoreResult` / `ImportResult` | 恢复 |
| `DisasterRecoveryDrill` / `DrillScenario` / `DrillReport` | 灾备演练 |
| `DegradationPolicy` / `DegradationAction` / `HealthStatus` | 降级预案 |
| `BkError` | 错误类型 |

### 2.13 sz-orm-mig（数据迁移增强）

| 导出 | 说明 |
|------|------|
| `DataMigrator<R, W>` / `MigConfig` / `DatabaseConfig` / `MigReport` | 迁移执行器 |
| `TableReader` / `TableWriter`（trait）→ `InMemoryTableStore` | 读写抽象 |
| `RowData` / `ColumnInfo` | 数据模型 |
| `DataTransformer`（trait）→ `TypeTransformer` / `ColumnMapper` / `ChainTransformer` / `FilterTransformer` | 转换管线 |
| `MigError` | 错误类型 |

### 2.14 其余扩展包速查

| 包 | 公开 API |
|----|---------|
| sz-orm-rw | `ReadWriteRouter`、`LoadBalanceStrategy` |
| sz-orm-sharding | `ShardingRouter`、`ShardingStrategy`、`ShardingError` |
| sz-orm-limit | `RateLimiter`（trait）、`TokenBucketRateLimiter`、`SlidingWindowRateLimiter`、`RateLimitResult`、`RateLimitError` |
| sz-orm-config | `ConfigCenter`（trait）、`ConsulConfigCenter`、`NacosConfigCenter`、`ConfigChangeEvent`、`ConfigChangeCallback` |
| sz-orm-es | `EsSyncManager`、`EsSync`（trait）、`InMemoryEsSync`、`EsDocument`、`EsSearchRequest`、`EsQuery`、`EsBoolQuery`、`EsRangeQuery`、`EsSort`、`EsSearchResult`、`EsHit`、`EsFieldType`、`EsError` |
| sz-orm-tracing | `Tracer`（trait）、`SzTracer`、`OtelTracer`、`Span`、`SpanLog`、`LatencyHistogram`、`ErrorRateCounter`、`ErrorBudget`、`SlaMonitor`、`SlaReport`、`AlertHook`、`LogAlertHook`、`WebhookAlertHook`、`Alert`、`AlertLevel`、`SaturationGauge`、`TracingError` |
| sz-orm-logger | `Logger`（trait）、`StructuredLogger`、`LoggerFactory`、`LogEntry`、`LogLevel`、`Metrics`、`MetricsSnapshot` |
| sz-orm-health | `HealthReport`、`HealthSnapshot`、`HealthStatus`、`DbHealthChecker`、`DefaultHealthChecker`、`HealthStatusProvider`、`StaticStatusProvider`、`ThresholdProvider`、`AlertManager`、`AlertChannel`、`LogAlertChannel`、`WebhookAlertChannel`、`ImAlertChannel`、`HealthAlert`、`AlertLevel`、`FailoverPolicy`、`FailoverAction`、`MultiRegionHealthView`、`CircuitBreaker`、`CircuitState`、`BackupHealthProvider` |
| sz-orm-grpc | `GrpcServer`、`GrpcServerHandle`、`GrpcServiceDef`、`GrpcMethod`、`UserGrpcService`（trait）、`InMemoryUserService`、`UserGrpcClient`、`GrpcChannel`、`UserRequest`、`UserResponse`、`GrpcError` |
| sz-orm-graphql | `GraphQLSchema`、`GraphQLType`、`GraphQLField`、`GraphQLSchemaGenerator`、`GraphQLServer` |
| sz-orm-swagger | `OpenAPIGenerator`、`OpenAPISpec`、`PathInfo`、`SwaggerUi` |
| sz-orm-masking | `DataMasker`、`MaskingRule` |
| sz-orm-audit | `SqlAuditor`、`SqlAuditContext` |
| sz-orm-batch | `BatchOperations`（trait）、`DefaultBatchOps`、`BatchResult`、`UpsertMode` |
| sz-orm-wasm | `WasmDatabase`、`WasmQuery` |
| sz-orm-lc | `LowCodeEngine`、`ModelDefinition`、`FieldDef`、`RelationDefinition` |
| sz-orm-vector | `PgVectorStore`（trait）、`InMemoryVectorStore`、`RealPgVectorStore`（feature `real-pg`）、`StubVectorStore`、`VectorRecord`、`SearchResult`、`VectorMetric`、`VectorError` |
| sz-orm-search | `SearchExt`（trait）、`SearchBuilder`、`SearchWrapper`、`SearchProvider`、`SearchQuery`、`SearchHit`、`SearchResult`、`MemorySearch`、`StubSearch`、`ElasticsearchProvider`（feature `real-es`）、`OpensearchProvider`（feature `real-opensearch`）、`MeilisearchProvider`（feature `real-meilisearch`）、`SearchError` |
| sz-orm-timeseries | `TimeseriesExt`（trait）、`TimeseriesBuilder`、`TimeseriesWrapper`、`TimeseriesProvider`、`Metric`、`TimeBucket`、`Aggregation`、`DownsampleConfig`、`MemoryTimeseries`、`StubTimeseries`、`RealTimescale`（feature `real-timescale`）、`TimescaleError` |
| sz-orm-postgis | `PostgisExt`（trait）、`PostgisBuilder`、`PostgisWrapper`、`PostgisProvider`、`Geometry`、`Point`、`LineString`、`Polygon`、`DEFAULT_SRID`、`MemoryPostgis`、`StubPostgis`、`RealPostgis`（feature `real-postgis`）、`PostgisError` |
| sz-orm-observability | `MetricsRegistry`、`MetricKind`、`MetricMeta`、`Counter`、`Gauge`、`Histogram`、`SloMonitor`、`SloConfig`、`SloBurnRate`（另有 feature `prometheus` / `otlp` 可选 exporter） |

> sz-orm-dtx 包包含 TCC / Saga / CrossShard 三个分布式事务子模块，详见 §2.15。

### 2.15 sz-orm-dtx（分布式事务扩展）

`sz_orm_dtx` 包在原有 2PC（`DistributedTransaction` / `TransactionParticipant` / `TransactionState` / `ParticipantState` / `DtxManager`）基础上扩展了三个子模块：`tcc`、`cross_shard`、`saga`。

#### 2.15.1 基础 2PC（sz_orm_dtx 顶层）

| 类型 | 关键方法 |
|------|---------|
| `TransactionState` | `Active` / `Preparing` / `Prepared` / `Committing` / `Committed` / `RollingBack` / `RolledBack` / `Failed` |
| `ParticipantState` | `Active` / `Prepared` / `Committed` / `RolledBack` / `Failed` |
| `TransactionParticipant` | `new(id)` / `with_prepare(f)` / `with_commit(f)` / `with_rollback(f)` / `prepare()` / `commit()` / `rollback()` / `fail()`；`pub resource_id`、`pub state` |
| `DistributedTransaction` | `new(id)` / `state()` / `participants()` / `add_participant(p)` / `prepare()` / `commit()` / `rollback()`；`pub id` |
| `DtxManager` | `new()` / `begin(id)` / `add_participant(id, p)` / `prepare(id)` / `commit(id)` / `rollback(id)` / `get(id)` / `list()` / `participant_states(id)` |
| `ParticipantCallback` | `Arc<dyn Fn() -> Result<(), String> + Send + Sync>` |

#### 2.15.2 tcc 子模块（`sz_orm_dtx::tcc`）

TCC（Try-Confirm-Cancel）补偿型分布式事务，每个分支需实现 try / confirm / cancel 三个回调，且 confirm/cancel 必须幂等。

**类型与状态枚举**

```rust
pub enum TccState {
    Init, Trying, Tried, Confirming, Confirmed,
    Cancelling, Cancelled, Failed,
}

impl TccState {
    pub fn is_terminal(&self) -> bool;            // Confirmed | Cancelled | Failed
    pub fn can_retry_confirm(&self) -> bool;      // Confirming | Failed
    pub fn can_retry_cancel(&self) -> bool;       // Cancelling | Failed
}

pub enum TccParticipantState {
    Init, Tried, Confirmed, Cancelled, Failed,
}

pub type TccCallback = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
```

**TccParticipant（分支事务）**

| 字段/方法 | 签名 | 说明 |
|----------|------|------|
| pub 字段 | `resource_id: String` / `state: TccParticipantState` / `try_attempts: u32` / `confirm_attempts: u32` / `cancel_attempts: u32` | 公开状态 |
| `new` | `(resource_id: impl Into<String>) -> Self` | 创建分支 |
| `with_try` | `<F: Fn() -> Result<(), String> + Send + Sync + 'static>(self, f: F) -> Self` | 设置 try 回调（builder） |
| `with_confirm` | 同上 | 设置 confirm 回调（必须幂等） |
| `with_cancel` | 同上 | 设置 cancel 回调（必须幂等） |
| `try_phase` | `(&mut self) -> Result<(), String>` | 执行 try，成功 → `Tried`，失败 → `Failed` |
| `confirm_phase` | `(&mut self) -> Result<(), String>` | 执行 confirm，成功 → `Confirmed` |
| `cancel_phase` | `(&mut self) -> Result<(), String>` | 执行 cancel，成功 → `Cancelled` |
| `fail` | `(&mut self)` | 标记分支失败 |
| `is_tried` | `(&self) -> bool` | Try 曾成功（含已 confirm/cancel） |

**TccCoordinator（协调器）**

| 字段/方法 | 签名 | 说明 |
|----------|------|------|
| pub 字段 | `tx_id: String` | 事务 ID |
| `new` | `(tx_id: impl Into<String>) -> Self` | 创建协调器 |
| `state` | `(&self) -> TccState` | 当前事务状态 |
| `participants` | `(&self) -> &[TccParticipant]` | 所有分支 |
| `add_participant` | `(&mut self, p: TccParticipant)` | 注册分支 |
| `execute` | `(&mut self) -> Result<(), TccError>` | 完整 TCC 流程：try 全成功 → confirm；任一失败 → 自动 cancel |
| `try_phase` | `(&mut self) -> Result<(), TccError>` | 仅执行 Try 阶段 |
| `confirm_phase` | `(&mut self) -> Result<(), TccError>` | 执行 Confirm（须 Tried/Confirming/Failed） |
| `cancel_phase` | `(&mut self) -> Result<(), TccError>` | 执行 Cancel（须 Tried/Cancelling/Failed/Trying） |
| `retry_confirm` | `(&mut self) -> Result<(), TccError>` | 重试 Confirm（跳过已 Confirmed 分支） |
| `retry_cancel` | `(&mut self) -> Result<(), TccError>` | 重试 Cancel（跳过已 Cancelled/Confirmed/Init 分支） |

**TccError**

```rust
pub enum TccError {
    TryFailed { resource_id: String, reason: String, cancelled_count: usize },
    ConfirmFailed { resource_id: String, reason: String },
    CancelFailed { reason: String },
    InvalidState { current: TccState, expected: &'static str },
    ParticipantNotFound { resource_id: String },
}
```

**TccManager（全局管理器）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `() -> Self` | 创建管理器（`Default` 已实现） |
| `register` | `(&self, tx_id: impl Into<String>) -> Result<(), TccError>` | 注册新事务 |
| `add_participant` | `(&self, tx_id: &str, p: TccParticipant) -> Result<(), TccError>` | 添加分支 |
| `execute` | `(&self, tx_id: &str) -> Result<(), TccError>` | 执行完整 TCC |
| `get_state` | `(&self, tx_id: &str) -> Option<TccState>` | 查询状态 |
| `retry_confirm` | `(&self, tx_id: &str) -> Result<(), TccError>` | 重试 Confirm |
| `retry_cancel` | `(&self, tx_id: &str) -> Result<(), TccError>` | 重试 Cancel |
| `list_failed` | `(&self) -> Vec<String>` | 列出 Failed 事务（按 ID 排序） |
| `list_all` | `(&self) -> Vec<String>` | 列出所有事务 ID（按 ID 排序） |

**用法示例**

```rust
use sz_orm_dtx::tcc::{TccCoordinator, TccParticipant, TccState};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

let mut coord = TccCoordinator::new("tx-transfer-001");

let frozen = Arc::new(AtomicU32::new(0));
let confirmed = Arc::new(AtomicU32::new(0));
let cancelled = Arc::new(AtomicU32::new(0));

let f1 = frozen.clone();
let c1 = confirmed.clone();
let ca1 = cancelled.clone();
coord.add_participant(
    TccParticipant::new("account-deduct")
        .with_try(move || { f1.fetch_add(1, Ordering::SeqCst); Ok(()) })
        .with_confirm(move || { c1.fetch_add(1, Ordering::SeqCst); Ok(()) })
        .with_cancel(move || { ca1.fetch_add(1, Ordering::SeqCst); Ok(()) }),
);

// 全部 try 成功 → 自动 confirm
coord.execute().unwrap();
assert_eq!(frozen.load(Ordering::SeqCst), 1);
assert_eq!(confirmed.load(Ordering::SeqCst), 1);
assert_eq!(cancelled.load(Ordering::SeqCst), 0);
assert_eq!(coord.state(), TccState::Confirmed);
```

#### 2.15.3 cross_shard 子模块（`sz_orm_dtx::cross_shard`）

跨分片 ACID 协调器，基于 2PC 实现。按 `shard_id` 自动分组操作，每个分片合并为一个 `TransactionParticipant`。流程：`prepare 全部分片` → 全部成功 → `commit`；任一失败 → `rollback` 已 prepare 的分片。

**类型与枚举**

```rust
pub type OperationCallback = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

pub enum CrossShardError {
    NoOperations,
    NotPrepared,
    PrepareFailed(String),
    CommitFailed(String),
    RollbackFailed(String),
}
```

**ShardOperation（单分片操作）**

| 字段/方法 | 签名 | 说明 |
|----------|------|------|
| pub 字段 | `shard_id: String` / `name: String` | 分片与操作名 |
| `new` | `(shard_id: impl Into<String>, name: impl Into<String>) -> Self` | 创建操作 |
| `with_prepare` | `<F: Fn() -> Result<(), String> + Send + Sync + 'static>(self, f: F) -> Self` | 设置 prepare 回调 |
| `with_commit` | 同上 | 设置 commit 回调 |
| `with_rollback` | 同上 | 设置 rollback 回调 |

**CrossShardCoordinator（协调器）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `(tx_id: impl Into<String>) -> Self` | 创建协调器 |
| `tx_id` | `(&self) -> &str` | 事务 ID |
| `add_shard_operation` | `(&mut self, op: ShardOperation) -> &mut Self` | 注册操作（builder 风格） |
| `add_operation` | `(&mut self, shard_id, prepare, commit, rollback) -> &mut Self` | 注册操作（闭包风格） |
| `operations_by_shard` | `(&self) -> HashMap<String, Vec<&ShardOperation>>` | 按分片分组 |
| `operation_count` | `(&self) -> usize` | 已注册操作数（未去重） |
| `shard_count` | `(&self) -> usize` | 涉及分片数（去重） |
| `execute` | `(&mut self) -> Result<(), CrossShardError>` | 完整 2PC：prepare → commit |
| `prepare_only` | `(&mut self) -> Result<(), CrossShardError>` | 仅 prepare（手动两阶段） |
| `commit` | `(&mut self) -> Result<(), CrossShardError>` | prepare 后手动 commit |
| `rollback` | `(&mut self) -> Result<(), CrossShardError>` | prepare 后手动 rollback |
| `state` | `(&self) -> Option<TransactionState>` | 底层 2PC 事务状态 |
| `participant_states` | `(&self) -> Option<Vec<ParticipantState>>` | 各分片状态 |
| `participant_ids` | `(&self) -> Option<Vec<String>>` | 各分片资源 ID |

**用法示例**

```rust
use sz_orm_dtx::cross_shard::CrossShardCoordinator;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

let prepared = Arc::new(AtomicU32::new(0));
let committed = Arc::new(AtomicU32::new(0));

let mut coord = CrossShardCoordinator::new("tx-order-001");

let p1 = prepared.clone();
let c1 = committed.clone();
coord.add_operation("shard-orders", move || { p1.fetch_add(1, Ordering::SeqCst); Ok(()) },
    move || { c1.fetch_add(1, Ordering::SeqCst); Ok(()) },
    || Ok(()));

let p2 = prepared.clone();
let c2 = committed.clone();
coord.add_operation("shard-inventory", move || { p2.fetch_add(1, Ordering::SeqCst); Ok(()) },
    move || { c2.fetch_add(1, Ordering::SeqCst); Ok(()) },
    || Ok(()));

coord.execute().unwrap();
assert_eq!(prepared.load(Ordering::SeqCst), 2);
assert_eq!(committed.load(Ordering::SeqCst), 2);
```

#### 2.15.4 saga 子模块（`sz_orm_dtx::saga`）

协调式（Orchestration）Saga，将长事务拆分为有序步骤，每步包含 action 与 compensation；任一步骤失败时，按反向顺序对已成功步骤执行补偿。

**类型与枚举**

```rust
pub type SagaAction       = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
pub type SagaCompensation = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

pub enum SagaState {
    New, Running, Completed,
    Compensating, Compensated,
    CompensationFailed, Failed,
}

pub enum StepState {
    Pending, Completed, Compensated, Failed, CompensationFailed,
}

pub enum SagaResult {
    Success,
    Compensated { failed_step: String, reason: String },
    CompensationFailed {
        failed_step: String,
        failure_reason: String,
        compensation_failed_step: String,
        compensation_reason: String,
    },
}
```

**SagaStep（步骤）**

| 字段/方法 | 签名 | 说明 |
|----------|------|------|
| pub 字段 | `name: String` / `state: StepState` | 步骤名与状态 |
| `new` | `(name: &str) -> Self` | 创建步骤 |
| `with_action` | `<F: Fn() -> Result<(), String> + Send + Sync + 'static>(self, f: F) -> Self` | 设置动作回调 |
| `with_compensation` | 同上 | 设置补偿回调 |
| `execute_action` | `(&mut self) -> Result<(), String>` | 执行动作（成功 → `Completed`，失败 → `Failed`） |
| `execute_compensation` | `(&mut self) -> Result<(), String>` | 执行补偿（成功 → `Compensated`，失败 → `CompensationFailed`） |
| `is_completed` | `(&self) -> bool` | 是否已成功 |
| `needs_compensation` | `(&self) -> bool` | 是否需要补偿（`Completed` 状态） |

**Saga（协调器）**

| 字段/方法 | 签名 | 说明 |
|----------|------|------|
| pub 字段 | `id: String` | Saga 标识 |
| `new` | `(id: &str) -> Self` | 创建 Saga |
| `state` | `(&self) -> SagaState` | 当前状态 |
| `steps` | `(&self) -> &[SagaStep]` | 所有步骤 |
| `last_result` | `(&self) -> Option<&SagaResult>` | 最近执行结果 |
| `completed_count` | `(&self) -> usize` | 已成功步骤数 |
| `add_step` | `(&mut self, step: SagaStep) -> Result<(), String>` | 添加步骤（仅 New 状态） |
| `with_step` | `(self, step: SagaStep) -> Self` | 链式添加步骤（非 New 状态静默忽略） |
| `execute` | `(&mut self) -> Result<SagaResult, String>` | 执行 Saga；任一步骤失败按反向顺序补偿 |
| `reset` | `(&mut self)` | 重置为 New 状态（清除步骤状态与结果） |

**SagaManager（管理器）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `() -> Self` | 创建管理器（`Default` 已实现） |
| `register` | `(&self, saga: Saga) -> Result<(), String>` | 注册 Saga |
| `execute` | `(&self, id: &str) -> Result<SagaResult, String>` | 执行指定 Saga |
| `state` | `(&self, id: &str) -> Option<SagaState>` | 查询状态 |
| `step_states` | `(&self, id: &str) -> Option<Vec<StepState>>` | 查询步骤状态 |
| `list` | `(&self) -> Vec<String>` | 列出所有 Saga ID（排序） |
| `remove` | `(&self, id: &str) -> Option<SagaState>` | 删除 Saga |
| `reset` | `(&self, id: &str) -> Result<(), String>` | 重置 Saga |

**用法示例**

```rust
use sz_orm_dtx::saga::{Saga, SagaStep, SagaState};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

let counter = Arc::new(AtomicU32::new(0));
let c1 = counter.clone();
let c2 = counter.clone();
let c1r = counter.clone();
let c2r = counter.clone();

let mut saga = Saga::new("order-create");
saga.add_step(SagaStep::new("step1")
    .with_action(move || { c1.fetch_add(1, Ordering::SeqCst); Ok(()) })
    .with_compensation(move || { c1r.fetch_sub(1, Ordering::SeqCst); Ok(()) })).unwrap();
saga.add_step(SagaStep::new("step2")
    .with_action(move || { c2.fetch_add(1, Ordering::SeqCst); Ok(()) })
    .with_compensation(move || { c2r.fetch_sub(1, Ordering::SeqCst); Ok(()) })).unwrap();

saga.execute().unwrap();
assert_eq!(counter.load(Ordering::SeqCst), 2);
assert_eq!(saga.state(), SagaState::Completed);
```

### 2.20 sz-orm-vector（pgvector 向量数据库）

提供 pgvector 向量数据库集成，支持 PostgreSQL 向量扩展。

**核心 trait**：

```rust
pub trait PgVectorStore: Send + Sync {
    async fn create_collection(&self, name: &str, dimension: usize, metric: Option<VectorMetric>) -> Result<(), VectorError>;
    async fn delete_collection(&self, name: &str) -> Result<(), VectorError>;
    async fn insert(&self, collection: &str, records: Vec<VectorRecord>) -> Result<(), VectorError>;
    async fn search(&self, collection: &str, query: &[f32], top_k: usize) -> Result<Vec<SearchResult>, VectorError>;
    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRecord>, VectorError>;
    async fn delete(&self, collection: &str, ids: Vec<String>) -> Result<u64, VectorError>;
    async fn count(&self, collection: &str) -> Result<usize, VectorError>;
}
```

**数据结构**：

| 类型 | 说明 |
|------|------|
| `VectorRecord` | 向量记录：id（String）、vector（Vec\<f32\>）、score（Option\<f32\>）、metadata（Option\<HashMap\<String, serde_json::Value\>\>）；插入为 upsert 语义（相同 id 覆盖） |
| `SearchResult` | 搜索结果：id（String）、score（f32，相似度，越大越相似）、vector（Vec\<f32\>）、text（Option\<String\>）、metadata |
| `VectorMetric` | 相似度度量：Cosine（余弦，默认，操作符 `<=>`）、Euclidean（欧几里得，`<->`）、DotProduct（点积，`<#>`） |

**实现**：

| 实现 | 说明 |
|------|------|
| `InMemoryVectorStore` | 内存实现，支持 cosine/euclidean/dot 三种度量，无需外部依赖 |
| `RealPgVectorStore` | 真实 PG + pgvector（feature `real-pg`），tokio-postgres 延迟连接（`OnceCell`，首次查询时建立），参数化 SQL；配套配置 `RealPgConfig` |
| `StubVectorStore` | Stub 实现，所有方法返回 `VectorError::Unsupported` |

**安全性**：
- 所有数据查询使用参数化查询（$1, $2），表名/集合名禁止字符串拼接
- 集合名严格校验（仅 ASCII 字母数字+下划线，须以字母或下划线开头，最大 63 字符）
- 维度校验（1-16000）

**pgvector 表结构**：
```sql
-- 集合元信息表
CREATE TABLE IF NOT EXISTS collections (
    name TEXT PRIMARY KEY,
    dimension INT NOT NULL,
    metric TEXT NOT NULL
);

-- 向量表（每个集合独立一张）
CREATE TABLE IF NOT EXISTS vectors_{name} (
    id TEXT NOT NULL,
    embedding vector({dimension}),
    metadata JSONB DEFAULT '{}'::jsonb,
    text TEXT DEFAULT '',
    PRIMARY KEY (id)
);

-- 相似度搜索（按集合度量自动选择 <=> / <-> / <#> 操作符）
SELECT id, embedding, metadata, text, (embedding <=> $1::vector) AS distance
FROM vectors_{name}
ORDER BY distance
LIMIT $2;
```

`VectorError` 变体：`CollectionNotFound` / `DimensionMismatch` / `Unsupported` / `Query` / `Connection` / `InvalidConfig` / `InvalidIdentifier`。

### 2.21 NL→SQL 自然语言转 SQL（sz-orm-ai 包）

提供自然语言到 SQL 的转换，所有输出经过安全验证。

**核心 trait**：

```rust
pub trait Nl2SqlEngine: Send + Sync {
    async fn generate(&self, nl_query: &str, schema: &SchemaContext) -> Result<SqlQuery, Nl2SqlError>;
    async fn validate(&self, query: &SqlQuery) -> Result<bool, Nl2SqlError>;
}
```

**数据结构**：

| 类型 | 说明 |
|------|------|
| `SqlQuery` | 生成的 SQL：sql（String）、explanation（String）、confidence（f32，0.0-1.0） |
| `SchemaContext` | 数据库 schema 上下文：tables（Vec\<TableInfo\>） |
| `TableInfo` | 表信息：name（String）、columns（Vec\<ColumnInfo\>） |
| `ColumnInfo` | 列信息：name（String）、data_type（String）、nullable（bool）、is_primary_key（bool） |

**实现**：

| 实现 | 说明 |
|------|------|
| `SimpleNl2SqlEngine` | 规则引擎（内存），关键词匹配，支持 SELECT/COUNT/聚合/WHERE/ORDER BY/GROUP BY/JOIN/LIMIT，参数化 SQL；支持 `with_alias` 表别名 |
| `OpenAINl2SqlEngine` | OpenAI 兼容 API（feature `real`），GPT-4o-mini 默认，system prompt 含 schema，双重安全验证 |

**安全验证（safety 模块）**：

| 函数 | 说明 |
|------|------|
| `validate_select_only(sql)` | 只允许 SELECT（trim 后以 SELECT 开头，不区分大小写） |
| `validate_no_injection(sql)` | 检测注释、UNION、布尔注入、写入关键字、引号逃逸等 |
| `sanitize_sql(sql)` | 清理行注释/块注释/控制字符 |

**生成的 SQL 规则**：
- 只允许 SELECT（禁止 DROP/ALTER/TRUNCATE/INSERT/UPDATE/DELETE）
- 所有值使用 $1, $2 参数化占位符，禁止字符串拼接
- 自动检测 SQL 注入模式

### 2.16 sz-orm-core：JSON 查询（`sz_orm_core::json_query`）

提供 think-orm 风格的链式 JSON 字段查询与更新构造器，支持 MySQL / PostgreSQL / SQLite 三种方言。

**三方言映射表**

| 操作 | MySQL | PostgreSQL | SQLite |
|------|-------|-----------|--------|
| 取字段 | `` `col`->'$.field' `` | `"col"->>'field'` | `json_extract(col, '$.field')` |
| 取路径 | `` `col`->'$.a.b' `` | `"col"->>'a'->>'b'` | `json_extract(col, '$.a.b')` |
| 键存在 | `JSON_CONTAINS_PATH(col, 'one', '$.k')` | `"col" ? 'k'` / `#>` 路径 | `json_type(col, '$.k') IS NOT NULL` |
| 数组长度 | `JSON_LENGTH(col->'$.p')` | `jsonb_array_length(col#>>'{p}')` | `json_array_length(json_extract(col,'$.p'))` |
| 数组包含 | `JSON_CONTAINS(col, '"v"', '$.p')` | `col @> '{"p":"v"}'` | `EXISTS (SELECT 1 FROM json_each(...))` |
| SET 字段 | `JSON_SET(col, '$.k', v)` | `jsonb_set(col, '{k}', v)` | `json_set(col, '$.k', v)` |
| REMOVE 字段 | `JSON_REMOVE(col, '$.k')` | `col - 'k'` | `json_remove(col, '$.k')` |
| 数组追加 | `JSON_ARRAY_APPEND(col, '$.k', v)` | `jsonb_set(col, '{k}', (col#>'{k}') \|\| to_jsonb(v))` | `json_set(col, '$.k', json_insert(...))` |

**JsonQuery（查询构造器）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `(db_type: DbType, column: impl Into<String>) -> Self` | 创建构造器 |
| `path` | `(self, path: impl Into<String>) -> Self` | 指定 JSON 路径（如 `theme` 或 `a.b.c`） |
| `build_extract` | `(&self) -> String` | 构建取字段表达式（左侧值） |
| `eq_string` / `eq_i64` / `eq_f64` | `(self, v) -> String` | `=` 比较 |
| `ne_string` | `(self, v: &str) -> String` | `!=` 比较 |
| `gt_string` / `lt_string` / `ge_string` / `le_string` | `(self, v: &str) -> String` | 字符串 `>` / `<` / `>=` / `<=` |
| `gt_i64` / `lt_i64` / `ge_i64` / `le_i64` | `(self, v: i64) -> String` | 整数 `>` / `<` / `>=` / `<=` |
| `between_i64` | `(self, low: i64, high: i64) -> String` | `BETWEEN` |
| `in_strs` | `(self, &[&str]) -> String` | `IN (字符串列表)` |
| `in_i64s` | `(self, &[i64]) -> String` | `IN (整数列表)` |
| `like` | `(self, v: &str) -> String` | `LIKE '%v%'` |
| `is_null` / `is_not_null` | `(self) -> String` | NULL 判断 |
| `has_key` | `(self) -> String` | 键存在性检查 |
| `json_type_eq` | `(self, expected: &str) -> String` | JSON 类型检查（`'integer'`/`'string'`/`'array'` 等） |
| `contains` | `(self, v: &str) -> String` | 数组包含某元素 |
| `array_length_eq` | `(self, length: i64) -> String` | 数组长度比较 |
| `column` | `(&self) -> &str` | 返回列名 |
| `db_type` | `(&self) -> DbType` | 返回数据库类型 |

**JsonUpdate（更新构造器）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `(db_type: DbType, column: impl Into<String>) -> Self` | 创建更新构造器 |
| `set_str` | `(self, key: impl Into<String>, value: &str) -> Self` | SET 字符串字段 |
| `set_i64` | `(self, key: impl Into<String>, value: i64) -> Self` | SET 整数字段 |
| `set_bool` | `(self, key: impl Into<String>, value: bool) -> Self` | SET 布尔字段 |
| `array_append_str` | `(self, key: impl Into<String>, value: &str) -> Self` | 数组追加字符串元素 |
| `array_append_i64` | `(self, key: impl Into<String>, value: i64) -> Self` | 数组追加整数元素 |
| `remove_key` | `(self, key: impl Into<String>) -> Self` | 删除指定 JSON 路径字段 |
| `build_set` | `(&self) -> String` | 构建 SET 子句片段（不含 `SET` 关键字） |

**用法示例**

```rust
use sz_orm_core::json_query::JsonQuery;
use sz_orm_core::DbType;

// MySQL: WHERE `prefs`->'$.theme' = 'dark'
let cond = JsonQuery::new(DbType::MySQL, "prefs")
    .path("theme")
    .eq_string("dark");

// MySQL: UPDATE ... SET `prefs` = JSON_SET(`prefs`, '$.theme', 'dark')
use sz_orm_core::json_query::JsonUpdate;
let set_clause = JsonUpdate::new(DbType::MySQL, "prefs")
    .set_str("theme", "dark")
    .set_bool("dark_mode", true)
    .build_set();
```

### 2.17 sz-orm-core：find_with_related（`sz_orm_core::find_with_related`）

SeaORM 风格的关联查询 API。由于 QueryBuilder 主要生成 SQL（不直接执行），本模块提供"生成关联查询 SQL"的辅助 API。

**FindWithRelated\<\'a\>（JOIN 模式构造器，适合 1:1 / N:1）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `(dialect: &'a dyn Dialect, main_table, related_table, foreign_key, primary_key, left_join: bool) -> Self` | 创建构造器 |
| `where_cond` | `(self, cond: impl Into<String>) -> Self` | 追加 WHERE（AND 连接） |
| `order_by` | `(self, field: impl Into<String>) -> Self` | ORDER BY ASC |
| `order_desc` | `(self, field: impl Into<String>) -> Self` | ORDER BY DESC |
| `limit` | `(self, n: usize) -> Self` | LIMIT |
| `offset` | `(self, n: usize) -> Self` | OFFSET |
| `build` | `(&self) -> String` | 构建 JOIN SELECT SQL |

**自由函数**

| 函数 | 签名 | 说明 |
|------|------|------|
| `inspect_relation` | `(relations: &'a HashMap<&'a str, Relation>, name: &'a str) -> Option<(&'a str, &'a str, &'a str, bool)>` | 从 relations map 提取 `(related_table, foreign_key, primary_key, is_many)` |
| `find_with_related_join` | `(dialect, main_table, related_table, foreign_key, primary_key, left_join) -> FindWithRelated<'a>` | 便捷创建 JOIN 构造器（等价 `FindWithRelated::new(...)`） |
| `find_with_related_eager_sql` | `(dialect, main_table, related_table, foreign_key, main_where: Option<&str>) -> (String, String)` | 生成 eager load 两条 SQL（主表 SQL + 关联表 IN(?) 模板） |
| `find_with_related_subquery` | `(dialect, main_table, related_table, foreign_key, primary_key, related_where: Option<&str>) -> String` | 生成子查询 SQL（适合 1:N，避免行膨胀） |

**WithRelation\<\'a\>（SeaORM find_with_related 风格）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `(dialect: &'a dyn Dialect, main_table: impl Into<String>) -> Self` | 创建加载器 |
| `with_has_many` | `(self, related: &'a str, foreign_key, primary_key) -> Self` | 添加 HasMany 关联 |
| `with_has_one` | 同上 | 添加 HasOne 关联 |
| `with_belongs_to` | 同上 | 添加 BelongsTo 关联 |
| `load_eager` | `(self, main_where: Option<&str>) -> Self` | 标记 eager load 模式 |
| `load_join` | `(&self, main_where: Option<&str>) -> String` | 生成 JOIN 模式 SQL（HasMany/HasOne → LEFT JOIN；BelongsTo → INNER JOIN） |
| `main_sql` | `(&self) -> String` | 主表 SQL |
| `related_sql` | `(&self, name: &str) -> Option<String>` | 关联表 SQL（默认占位符 `?`） |
| `related_sql_with_ids` | `(&self, name: &str, ids: impl IntoIterator<Item = impl ToString>) -> Option<String>` | 关联表 SQL（用具体 ID 列表填充） |
| `relation_names` | `(&self) -> Vec<&str>` | 所有已注册关联名 |

**用法示例**

```rust
use sz_orm_core::find_with_related::find_with_related_join;
use sz_orm_core::{get_dialect, DbType};

let dialect = get_dialect(DbType::MySQL).unwrap();
let sql = find_with_related_join(
    &*dialect, "users", "orders", "user_id", "id", true,
)
    .where_cond("users.id = 1")
    .build();
```

### 2.18 sz-orm-core：强类型 AST（`sz_orm_core::typed_ast`）

Diesel 风格的强类型 SQL 表达式 AST，让列类型不匹配、跨表列引用等错误在编译期被捕获。所有表达式为零成本抽象（ZST），仅在编译期携带类型信息。

**类型标记与 trait**

```rust
pub trait SqlType: 'static {}
pub struct Bool;    impl SqlType for Bool {}
pub struct Integer; impl SqlType for Integer {}
pub struct Text;    impl SqlType for Text {}

pub trait TypedExpression {
    type SqlType: SqlType;
    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>);
}
```

**表达式类型**

| 类型 | 签名 | 说明 |
|------|------|------|
| `ColumnExpr<C: TypedColumn>` | `new() -> Self` | 列引用表达式 |
| `Literal<T: ToString + Clone>` | `new(value: T) -> Self` | 字面量表达式（参数化） |
| `Eq<C, V>` / `Ne<C, V>` | `new(col: C, value: V) -> Self` | `=` / `!=` 比较，`SqlType = Bool` |
| `Lt<C, V>` / `Gt<C, V>` | 同上 | `<` / `>` 比较 |
| `Le<C, V>` / `Ge<C, V>` | 同上 | `<=` / `>=` 比较 |
| `And<L, R>` / `Or<L, R>` | `new(left: L, right: R) -> Self` | 逻辑 AND / OR；子表达式均要求 `SqlType = Bool` |

**TypedSelectQuery\<T\>（类型安全 SELECT 构造器）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `() -> Self`（`Default` 已实现） | 创建查询，泛型 `T: TypedTable` 锁定主表 |
| `filter` | `<E: TypedExpression<SqlType = Bool> + 'static>(self, expr: E) -> Self` | 添加 WHERE（AND 连接） |
| `limit` | `(self, n: usize) -> Self` | LIMIT |
| `offset` | `(self, n: usize) -> Self` | OFFSET |
| `build` | `(&self, dialect: &dyn Dialect) -> (String, Vec<String>)` | 构建 SQL 与参数 |

**TypedColumnExt trait（列扩展便捷方法）**

```rust
pub trait TypedColumnExt: TypedColumn + Sized {
    fn eq<V: Clone + ToString>(self, value: V) -> Eq<Self, V>;
    fn ne<V: Clone + ToString>(self, value: V) -> Ne<Self, V>;
    fn lt<V: Clone + ToString>(self, value: V) -> Lt<Self, V>;
    fn gt<V: Clone + ToString>(self, value: V) -> Gt<Self, V>;
    fn le<V: Clone + ToString>(self, value: V) -> Le<Self, V>;
    fn ge<V: Clone + ToString>(self, value: V) -> Ge<Self, V>;
}
```

**类型安全保证**

- `Eq<C, T>` 要求 `C: TypedColumn<RustType = T>`，列类型必须与值类型匹配
- `And<L, R>` 要求 `L: TypedExpression<SqlType = Bool>`，`R: TypedExpression<SqlType = Bool>`
- `TypedSelectQuery::filter<E>` 要求 `E: TypedExpression<SqlType = Bool>`
- 跨表列引用：通过 `TypedColumn::Table` 关联类型 + `TypedSelectQuery<T>::filter` 的 `T` 约束

**用法示例**

```ignore
use sz_orm_core::typed::{TypedTable, TypedColumn};
use sz_orm_core::typed_ast::*;

struct users;
impl TypedTable for users { const NAME: &'static str = "users"; }

mod users {
    use super::*;
    pub struct id;
    impl TypedColumn for id {
        const NAME: &'static str = "id";
        type Table = super::users;
        type RustType = i64;
    }
    pub struct name;
    impl TypedColumn for name {
        const NAME: &'static str = "name";
        type Table = super::users;
        type RustType = String;
    }
}

let q = TypedSelectQuery::<users>::new()
    .filter(users::id.eq(42))         // ✅ i64 列与 i64 值
    .filter(users::name.eq("Alice")); // ✅ String 列与 &str 值

// 编译期拒绝：
// q.filter(users::id.eq("Alice"));  // ❌ 类型不匹配
```

### 2.19 sz-orm-core：动态 SQL（`sz_orm_core::dynamic_sql`）

rbatis 风格的 XML 动态 SQL 模板构造器。支持 `<select>` / `<insert>` / `<update>` / `<delete>` 顶层语句，以及 `<if>` / `<where>` / `<set>` / `<foreach>` / `<choose><when><otherwise>` / `<trim>` 等动态标签。

**模板语法表**

| 语法 | 含义 | 安全性 |
|------|------|--------|
| `#{name}` | 命名参数绑定（生成占位符 `?` 并收集绑定值） | ✅ 安全，自动参数化 |
| `${name}` | 字符串插值（直接拼入 SQL） | ⚠️ 注入风险，仅在受控场景使用 |

**XML 标签**

| 标签 | 作用 |
|------|------|
| `<select id>` / `<insert id>` / `<update id>` / `<delete id>` | 语句容器 |
| `<if test="expr">` | 条件包含 |
| `<where>` | WHERE 子句（自动剥离首个 AND/OR） |
| `<set>` | SET 子句（自动剥离末尾逗号） |
| `<foreach collection="x" item="i" separator=",">` | 循环展开（用于 IN 子句） |
| `<choose>` / `<when test="expr">` / `<otherwise>` | 多分支选择 |
| `<trim prefix="..." suffix="..." prefixOverrides="AND">` | 通用前后缀修剪 |

**if test 表达式语法**：`name != null`、`name == 'Alice'`、`age > 18`、`status != null && status != ''` 等。

**类型与枚举**

```rust
pub enum DynamicSqlError {
    ParseError(String),
    StatementNotFound(String),
    EvalError(String),
    MissingParam(String),
}

pub enum ParamValue {
    Null, String(String), Int(i64), Float(f64), Bool(bool), Array(Vec<ParamValue>),
}
```

**SqlParams（命名参数容器）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `() -> Self`（`Default` 已实现） | 创建空参数集 |
| `set` | `(&mut self, name: &str, value: &str)` | 设置字符串参数 |
| `set_int` | `(&mut self, name: &str, value: i64)` | 设置整数 |
| `set_float` | `(&mut self, name: &str, value: f64)` | 设置浮点 |
| `set_bool` | `(&mut self, name: &str, value: bool)` | 设置布尔 |
| `set_null` | `(&mut self, name: &str)` | 设置 null |
| `set_array` | `(&mut self, name: &str, values: Vec<ParamValue>)` | 设置数组（foreach 用） |
| `get` | `(&self, name: &str) -> Option<&ParamValue>` | 获取参数值 |
| `contains` | `(&self, name: &str) -> bool` | 是否存在 |
| `is_null` | `(&self, name: &str) -> bool` | 为 null 或不存在 |
| `is_not_null` | `(&self, name: &str) -> bool` | 不为 null |
| `names` | `(&self) -> Vec<String>` | 所有参数名 |

**DynamicSqlParser（解析器）**

| 方法 | 签名 | 说明 |
|------|------|------|
| `new` | `() -> Self` | 创建空解析器 |
| `from_xml` | `(xml: &str) -> Result<Self, DynamicSqlError>` | 从 XML 字符串解析 |
| `build` | `(&self, id: &str, params: &SqlParams) -> Result<String, DynamicSqlError>` | 构建 SQL（仅 SQL 文本） |
| `build_with_binds` | `(&self, id: &str, params: &SqlParams) -> Result<(String, Vec<ParamValue>), DynamicSqlError>` | 构建 SQL + 绑定参数（按出现顺序） |
| `statement_ids` | `(&self) -> Vec<String>` | 列出所有已注册语句 ID（排序） |

**用法示例**

```ignore
use sz_orm_core::dynamic_sql::{DynamicSqlParser, SqlParams};

let xml = r#"
<select id="find_users">
    SELECT * FROM users
    <where>
        <if test="name != null">AND name = #{name}</if>
        <if test="age != null">AND age &gt; #{age}</if>
    </where>
</select>
"#;

let parser = DynamicSqlParser::from_xml(xml).unwrap();
let mut params = SqlParams::new();
params.set("name", "Alice");
// params.set_int("age", 18);  // 不设置则 if 不生效

let sql = parser.build("find_users", &params).unwrap();
// SELECT * FROM users WHERE name = ?

let (sql, binds) = parser.build_with_binds("find_users", &params).unwrap();
// binds == vec![ParamValue::String("Alice".into())]
```

---

## 三、错误处理指南

### 3.1 错误码总表

**DbError（DB001–DB018）**

| 错误码 | 变体 | 说明 | 可重试 |
|--------|------|------|--------|
| DB001 | `QueryError(String)` | 查询执行失败 | 视情况 |
| DB002 | `ConnectionError(String)` | 连接错误 | ✅ |
| DB003 | `ConnectionRefused(String)` | 连接被拒绝 | ✅ |
| DB004 | `ConnectionTimeout(String)` | 连接超时 | ✅ |
| DB007 | `TxError(TxError)` | 事务错误（包装） | ❌ |
| DB008 | `MigrationError(String)` | 迁移失败 | ❌ |
| DB009 | `Unsupported(String)` | 不支持的操作 | ❌ |
| DB010 | `ConfigError(String)` | 配置错误 | ❌ |
| DB011 | `SerdeError(String)` | 序列化错误 | ❌ |
| DB012 | `NotFound(String)` | 记录不存在 | ❌ |
| DB013 | `AlreadyExists(String)` | 记录已存在 | ❌ |
| DB014 | `ConstraintViolation(String)` | 约束冲突 | ❌ |
| DB015 | `NullValue(String)` | 非空约束 | ❌ |
| DB016 | `InvalidInput(String)` | 非法输入 | ❌ |
| DB017 | `Internal(String)` | 内部错误 | ❌ |
| DB018 | `IoError(String)` | IO 错误 | ✅ |

**PoolError（PL001–PL006）**：`Exhausted`(PL001)、`Timeout`(PL002)、`AlreadyAcquired`(PL003)、`NotAcquired`(PL004)、`InvalidConfig(String)`(PL005)、`Internal(String)`(PL006)

**CacheError（CH001–CH006）**：`NotFound`(CH001)、`SerializationError`(CH002)、`DeserializationError`(CH003)、`ConnectionError`(CH004)、`Timeout`(CH005)、`Internal`(CH006)

**TxError**：`NotStarted`、`CommitFailed`、`RollbackFailed`、`SavepointError` 等 6 变体。

### 3.2 便捷构造与判定

```rust
DbError::query("test failed");      // 构造查询错误
DbError::connection("timeout");     // 构造连接错误
DbError::not_found("user #42");     // 构造未找到错误

err.is_retryable();                 // 是否可重试（连接/超时/IO 类为 true）
err.error_code();                   // "DB001" / "PL002" / "CH001"
```

### 3.3 推荐处理模式

```rust
use sz_orm_core::*;

async fn run(pool: &Pool) -> DbResult<()> {
    let mut conn = match pool.acquire().await {
        Ok(c) => c,
        Err(PoolError::Timeout) => return Err(DbError::connection("pool acquire timeout")),
        Err(e) => return Err(DbError::PoolError(e)),
    };
    match conn.query("SELECT id FROM users").await {
        Ok(rows) => { /* ... */ Ok(()) }
        Err(e) if e.is_retryable() => {
            // 指数退避后重试，最多 3 次
            Err(e)
        }
        Err(e) => Err(e), // 约束/输入类错误直接上抛，不重试
    }
}
```

### 3.4 各扩展包错误类型

| 包 | 错误类型 |
|----|---------|
| sz-orm-sql-validator | `SqlValidationError`（12 变体：SyntaxError/UnbalancedParentheses/UnclosedString/MissingKeyword/ParameterCountMismatch/InvalidTableName/EmptySelectColumns/EmptyInsertData/EmptyUpdateData/DeleteWithoutWhere/InvalidIdentifier/InjectionDetected） |
| sz-orm-auth | `AuthError` |
| sz-orm-crypto | `CryptoError` |
| sz-orm-mqtt | `MqttError` |
| sz-orm-websocket | `WsError` |
| sz-orm-queue | `MqError` |
| sz-orm-storage | `StorageError` |
| sz-orm-back | `BkError` |
| sz-orm-mig | `MigError` |
| sz-orm-ai | `AiError` |
| sz-orm-es | `EsError` |
| sz-orm-grpc | `GrpcError` |
| sz-orm-tracing | `TracingError` |
| sz-orm-limit | `RateLimitError` |
| sz-orm-scheduler | `SchedulerError` |
| sz-orm-sharding | `ShardingError` |
| sz-orm-dtx::tcc | `TccError`（TryFailed / ConfirmFailed / CancelFailed / InvalidState / ParticipantNotFound） |
| sz-orm-dtx::cross_shard | `CrossShardError`（NoOperations / NotPrepared / PrepareFailed / CommitFailed / RollbackFailed） |
| sz-orm-core::dynamic_sql | `DynamicSqlError`（ParseError / StatementNotFound / EvalError / MissingParam） |

所有错误类型均基于 `thiserror` 派生 `std::error::Error`，可用 `?` 向上传播或用 `Box<dyn Error>` 统一兜底。

---

## 四、钩子系统（v3.0）

### 4.1 hooks 模块概述

`use sz_orm_core::hooks::*;` 导入钩子系统全部公共符号。

- **HookContext**（builder 模式：`with_tenant` / `with_operator` / `with_timestamp` / `set_meta` / `get_meta`）
- **HookEvent** 16 种事件枚举（详见 §4.2）
- **Hookable** trait（16 个生命周期钩子，默认 no-op，详见 §4.3）
- **HookDispatcher** 静态辅助（封装常见触发顺序，详见 §4.4）
- **SoftDelete** trait + **SoftDeleteScope**
- **GlobalScope** trait + **TenantModel** + **TenantScope**
- **HookRegistry**（运行时钩子注册表，lock poisoned 降级 no-op）
- **ScopeRegistry**（disable/enable/without_scope 临时禁用）

### 4.2 HookEvent 16 种事件枚举

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    // 细粒度写入事件（6 个，原始 v2.0）
    BeforeInsert, AfterInsert,
    BeforeUpdate, AfterUpdate,
    BeforeDelete, AfterDelete,
    // 通用写入事件（4 个，v3.0 新增）
    BeforeWrite, AfterWrite,       // insert 或 update 前后均触发
    BeforeSave, AfterSave,         // 与 write 等价，命名借用 Rails/ActiveRecord
    // 软删除恢复事件（2 个，v3.0 新增）
    BeforeRestore, AfterRestore,
    // 查询事件（2 个，v3.0 新增）
    BeforeFind, AfterFind,
    // 数据验证事件（2 个，v3.0 新增）
    BeforeValidate, AfterValidate,
}
```

**HookEvent 方法**

| 方法 | 签名 | 说明 |
|------|------|------|
| `is_before` | `(&self) -> bool` | 是否为 before 事件 |
| `is_after` | `(&self) -> bool` | 是否为 after 事件 |
| `is_write_level` | `(&self) -> bool` | 是否为通用写入事件（write/save） |
| `is_find_level` | `(&self) -> bool` | 是否为查询事件（find） |
| `is_validate_level` | `(&self) -> bool` | 是否为验证事件（validate） |
| `is_fine_grained` | `(&self) -> bool` | 是否为 v3.0 新增的细粒度事件（write/save/restore/find/validate） |

### 4.3 Hookable trait（16 个生命周期钩子）

```rust
pub trait Hookable: crate::model::Model {
    fn before_insert(_ctx: &mut HookContext) -> HookResult<()> { Ok(()) }
    fn after_insert(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_update(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_update(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_delete(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_delete(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_write(_ctx: &mut HookContext) -> HookResult<()> { Ok(()) }
    fn after_write(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_save(_ctx: &mut HookContext) -> HookResult<()> { Ok(()) }
    fn after_save(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_restore(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_restore(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_find(_ctx: &mut HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn after_find(_ctx: &HookContext, _id: &Self::PrimaryKey) -> HookResult<()> { Ok(()) }
    fn before_validate(_ctx: &mut HookContext) -> HookResult<()> { Ok(()) }
    fn after_validate(_ctx: &mut HookContext) -> HookResult<()> { Ok(()) }
}
```

### 4.4 HookDispatcher 静态辅助

封装常见钩子触发顺序，避免业务代码手动逐个调用。所有方法均为泛型静态方法，泛型 `M: Hookable`。

| 方法 | 签名 | 触发顺序 |
|------|------|---------|
| `insert<M, F>` | `(ctx: &mut HookContext, f: F) -> HookResult<M::PrimaryKey>` | `before_write` → `before_save` → `before_validate` → `after_validate` → `before_insert` → (执行 f) → `after_insert` → `after_save` → `after_write` |
| `update<M, F>` | `(ctx: &mut HookContext, id: &M::PrimaryKey, f: F) -> HookResult<()>` | `before_write` → `before_save` → `before_validate` → `after_validate` → `before_update` → (执行 f) → `after_update` → `after_save` → `after_write` |
| `delete<M, F>` | `(ctx: &mut HookContext, id: &M::PrimaryKey, f: F) -> HookResult<()>` | `before_delete` → (执行 f) → `after_delete` |
| `restore<M, F>` | `(ctx: &mut HookContext, id: &M::PrimaryKey, f: F) -> HookResult<()>` | `before_restore` → (执行 f) → `after_restore` |
| `find<M, F>` | `(ctx: &mut HookContext, id: &M::PrimaryKey, f: F) -> HookResult<()>` | `before_find` → (执行 SELECT) → `after_find` |
| `validate<M>` | `(ctx: &mut HookContext) -> HookResult<()>` | `before_validate` → `after_validate`（独立校验，不写入） |

**用法示例**

```rust
use sz_orm_core::hooks::{HookContext, HookDispatcher, Hookable};

// 自定义 Model 实现 Hookable 后，调用 HookDispatcher::insert 完成完整钩子序列
let mut ctx = HookContext::new().with_tenant(100).with_operator(1001);
let id = HookDispatcher::insert::<MyModel, _>(&mut ctx, |_ctx| {
    // 实际 INSERT 逻辑，返回插入后的主键
    Ok(42_i64)
})?;
```

### 4.5 软删除与多租户作用域

| 类型 | 关键方法/说明 |
|------|--------------|
| `SoftDelete`（trait） | `soft_delete_field() -> &'static str` / `is_deleted(&self) -> bool` |
| `SoftDeleteScope` | 自动追加 `AND {soft_delete_field} IS NULL` |
| `GlobalScope`（trait） | `scope_name() -> &'static str` / `apply_scope(ctx) -> Option<(String, Vec<Value>)>` |
| `TenantModel`（trait） | `tenant_field() -> &'static str`（默认 `tenant_id`）/ `tenant_id(&self) -> i64` / `set_tenant_id(&mut self, i64)` |
| `TenantScope` | 自动追加 `AND tenant_id = ?`，绑定 `ctx.tenant_id`（None 时不追加） |

---

## 五、CLI 工具与示例集

- **cli/**：SZ-ORM 命令行工具，提供迁移、Schema 导出、SQL 校验等子命令，便于在工程化流程中集成。
- **examples/**：覆盖核心引擎、sqlx 适配器与扩展生态包的端到端示例集，可作为集成参考。
