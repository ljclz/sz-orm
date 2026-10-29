/* Type definitions for @sz-orm/core (sz-orm-js napi bindings)
 *
 * 手工对齐 `packages/sz-orm-js/src/*.rs` 的全部 #[napi] 导出编写（2026-10-27，
 * 整改 P1-3）。命名遵循 napi-rs v2 默认约定：snake_case → camelCase，
 * #[napi(getter)] → 只读属性。安装 @napi-rs/cli 后执行 `napi build` 可生成
 * 等价的权威 .d.ts（届时以生成版本为准）。
 */

/* ============ 通用对象 ============ */

/** 查询构建结果：参数化 SQL 与占位符参数（query.rs SqlWithParams） */
export interface SqlWithParams {
  sql: string
  params: string[]
}

/** 增强查询结果（enhanced.rs EnhancedQueryResult） */
export interface EnhancedQueryResult {
  sql: string
  paramCount: number
}

/** 连接池配置（enhanced.rs PoolConfig，全部字段经 validate 校验） */
export interface PoolConfig {
  maxSize: number
  minIdle: number
  acquireTimeoutSecs: number
  idleTimeoutSecs: number
  maxLifetimeSecs: number
  healthCheckIntervalSecs: number
  maxQueueSize: number
}

/** 批量插入结果（batch.rs BatchInsertResult） */
export interface BatchInsertResult {
  sql: string
  paramCount: number
  rowCount: number
}

/** 批量更新结果（batch.rs BatchUpdateResult） */
export interface BatchUpdateResult {
  sqls: string[]
  statementCount: number
}

/** 批量删除结果（batch.rs BatchDeleteResult） */
export interface BatchDeleteResult {
  sql: string
  paramCount: number
}

/** 批量操作统计（batch.rs BatchStats） */
export interface BatchStats {
  totalOperations: number
  totalParams: number
  avgParamsPerOp: number
}

/** 模型差异结果（migration.rs SchemaDiffResult） */
export interface SchemaDiffResult {
  addedColumns: string[]
  removedColumns: string[]
  migrationSqls: string[]
}

/** 数据库错误信息（error.rs DbErrorInfo） */
export interface DbErrorInfo {
  message: string
  code: string
}

/* ============ 枚举 ============ */

/** JOIN 类型（enhanced.rs JoinType） */
export enum JoinType {
  Inner = 'Inner',
  Left = 'Left',
  Right = 'Right',
  Full = 'Full',
}

/** 迁移操作类型（migration.rs MigrationAction） */
export enum MigrationAction {
  CreateTable = 'CreateTable',
  DropTable = 'DropTable',
  AddColumn = 'AddColumn',
  DropColumn = 'DropColumn',
  AlterColumn = 'AlterColumn',
  AddIndex = 'AddIndex',
  DropIndex = 'DropIndex',
  RenameTable = 'RenameTable',
}

/** 字段数据类型（model_def.rs FieldType） */
export enum FieldType {
  AutoIncrement = 'AutoIncrement',
  Integer = 'Integer',
  BigInteger = 'BigInteger',
  Float = 'Float',
  Double = 'Double',
  String = 'String',
  Text = 'Text',
  Boolean = 'Boolean',
  Date = 'Date',
  DateTime = 'DateTime',
  Time = 'Time',
  Json = 'Json',
  Bytes = 'Bytes',
  Uuid = 'Uuid',
  Decimal = 'Decimal',
}

/** 关系类型（model_def.rs RelationType） */
export enum RelationType {
  OneToMany = 'OneToMany',
  ManyToOne = 'ManyToOne',
  OneToOne = 'OneToOne',
}

/* ============ 连接池（pool.rs） ============ */

/**
 * 异步连接池。构造后为「配置态」，需 `connect(dsn)` 建立真实 SQLite 连接池
 * （dsn 示例："sqlite::memory:" 或 "sqlite://path/to/db.sqlite"）。
 */
export declare class Pool {
  constructor(
    dbType?: string,
    maxSize?: number,
    minIdle?: number,
    acquireTimeout?: number,
    idleTimeout?: number,
    maxLifetime?: number,
  )
  /** 建立真实连接池（重复调用幂等返回） */
  connect(dsn: string): Promise<void>
  readonly dbType: string
  readonly maxSize: number
  readonly minIdle: number
  readonly acquireTimeout: number
  readonly idleTimeout: number
  readonly maxLifetime: number
  readonly isConnected: boolean
  status(): string
  asyncPing(): Promise<boolean>
  /** 执行查询，返回 JSON 行数组字符串 */
  asyncQuery(sql: string): Promise<string>
  /** 执行写语句，返回影响行数 */
  asyncExecute(sql: string): Promise<number>
  close(): Promise<void>
  metrics(): Promise<string>
  asyncQueryOne(sql: string): Promise<string>
  asyncExecuteBatch(sql: string): Promise<number>
  tableExists(table: string): Promise<boolean>
  asyncCount(table: string): Promise<number>
  /** 开启事务（真实 BEGIN，独占一条连接直到 commit/rollback） */
  asyncBegin(): Promise<boolean>
  asyncCommit(): Promise<boolean>
  asyncRollback(): Promise<boolean>
  asyncExecuteTransaction(sql: string): Promise<number>
  /** 插入记录（data 为 JSON 对象字符串，值参数化绑定；表名经标识符校验） */
  asyncInsert(table: string, data: string): Promise<number>
  /** 更新记录（whereClause 非空；set 值参数化绑定） */
  asyncUpdate(table: string, data: string, whereClause: string): Promise<number>
  asyncDelete(table: string, whereClause: string): Promise<number>
  asyncFind(table: string, whereClause: string): Promise<string>
  /** 以下 *_tx 方法须在 asyncBegin 之后的活动事务内调用 */
  asyncInsertTx(table: string, data: string): Promise<number>
  asyncUpdateTx(table: string, data: string, whereClause: string): Promise<number>
  asyncDeleteTx(table: string, whereClause: string): Promise<number>
  asyncFindTx(table: string, whereClause: string): Promise<string>
  /** 兼容占位：JS 侧由 GC 管理内存（no-op） */
  queryResultFree(): void
  stringFree(): void
  /** 绑定版本（Rust crate 版本） */
  static version(): string
  /** 错误码描述 */
  static errorMessage(code: number): string
}

/* ============ 查询构建器（query.rs） ============ */

export declare class QueryBuilder {
  constructor(dbType?: string)
  setTable(table: string): void
  setSelect(columns: string[]): void
  whereEqStr(field: string, value: string): void
  whereEqI64(field: string, value: number): void
  whereEqF64(field: string, value: number): void
  whereEqBool(field: string, value: boolean): void
  orWhereEqStr(field: string, value: string): void
  orWhereEqI64(field: string, value: number): void
  addOrderBy(field: string): void
  addOrderDesc(field: string): void
  setLimit(limit: number): void
  setOffset(offset: number): void
  buildSelect(): SqlWithParams
  buildDelete(): SqlWithParams
  asyncBuildSelect(): Promise<SqlWithParams>
  asyncBuildDelete(): Promise<SqlWithParams>
}

/* ============ 增强查询（enhanced.rs） ============ */

export declare class JoinClause {
  constructor(joinType: JoinType, table: string, onCondition: string)
  setAlias(alias: string): void
}

export declare class QueryBuilderEnhanced {
  constructor(dbType?: string)
  setTable(table: string): void
  setAlias(alias: string): void
  setSelect(columns: string[]): void
  setDistinct(): void
  addWhere(condition: string): void
  addGroupBy(column: string): void
  addHaving(condition: string): void
  addOrderBy(column: string): void
  addOrderDesc(column: string): void
  setLimit(limit: number): void
  setOffset(offset: number): void
  build(): EnhancedQueryResult
  buildCount(): EnhancedQueryResult
}

export declare class PoolConfigBuilder {
  constructor()
  setMaxSize(size: number): void
  setMinIdle(idle: number): void
  setAcquireTimeout(secs: number): void
  setIdleTimeout(secs: number): void
  setMaxLifetime(secs: number): void
  setHealthCheckInterval(secs: number): void
  setMaxQueueSize(size: number): void
  /** 校验并返回配置（非法时抛错） */
  build(): PoolConfig
}

/* ============ 模型值对象（model.rs） ============ */

export declare class Model {
  constructor(tableName: string, pkName?: string)
  readonly tableName: string
  readonly pkName: string
  setStr(key: string, value: string): void
  setI64(key: string, value: number): void
  setF64(key: string, value: number): void
  setBool(key: string, value: boolean): void
  setNull(key: string): void
  toJsonString(): string
  status(): string
}

/* ============ Schema 定义（model_def.rs） ============ */

export declare class FieldDefinition {
  constructor(name: string, fieldType: FieldType)
  notNull(): void
  setUnique(): void
  setPrimaryKey(): void
  setDefault(value: string): void
  setIndexed(): void
  setComment(comment: string): void
  readonly name: string
  readonly isNullable: boolean
  readonly isUnique: boolean
  readonly isPrimaryKey: boolean
  readonly isIndexed: boolean
  readonly defaultValue: string | null
}

export declare class IndexDefinition {
  constructor(name: string, columns: string[])
  setUnique(): void
  readonly name: string
  readonly columns: string[]
  readonly isUnique: boolean
}

export declare class RelationDefinition {
  constructor(
    name: string,
    relationType: RelationType,
    targetModel: string,
    foreignKey: string,
  )
  setTargetKey(key: string): void
  readonly name: string
  readonly targetModel: string
  readonly foreignKey: string
  readonly targetKey: string
}

export declare class ModelDefinition {
  constructor(dbType?: string, tableName?: string)
  readonly tableName: string
  readonly fieldCount: number
  readonly indexCount: number
  readonly relationCount: number
  toCreateTableSql(): string
  toCreateIndexSqls(): string[]
}

/* ============ 迁移（migration.rs） ============ */

export declare class MigrationStep {
  constructor(action: MigrationAction, description: string, sql: string)
  readonly actionName: string
  readonly description: string
  readonly sql: string
  readonly isReversible: boolean
  readonly rollbackSql: string
}

export declare class MigrationTool {
  constructor(dbType?: string, version?: number)
  readonly version: number
  readonly stepCount: number
  allSql(): string[]
  allRollbackSql(): string[]
  dropColumn(table: string, columnName: string): void
  dropIndex(indexName: string): void
  renameTable(oldName: string, newName: string): void
  clear(): void
  summary(): string
}

export declare class SchemaDiff {
  constructor(dbType?: string)
}
