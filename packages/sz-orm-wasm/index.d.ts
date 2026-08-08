// Type definitions for sz-orm-wasm
// Generated from packages/sz-orm-wasm/src/js_bindings.rs
// Project: https://github.com/ljclz/sz-orm

/**
 * JS 查询结果
 */
export class JsQueryResult {
    /** JSON 格式的行数据字符串 */
    readonly rowsJson: string;
    /** 受影响的行数 */
    readonly affected: number;
}

/**
 * JS 可调用的 WASM 数据库
 *
 * 内存级 SQL 引擎，支持 CREATE TABLE / INSERT / SELECT / UPDATE / DELETE。
 * 数据存储在 WASM 线性内存中，页面刷新后丢失。
 * 如需持久化，请启用 `persistence` feature 并使用 IndexedDB。
 */
export class JsWasmDatabase {
    /** 创建新的数据库实例 */
    constructor();

    /**
     * 执行 CREATE TABLE
     * @param sql - SQL DDL 语句
     * @returns 受影响行数（CREATE TABLE 总是 0）
     * @throws 错误消息字符串
     */
    createTable(sql: string): number;

    /**
     * 执行 INSERT
     * @param sql - SQL DML 语句（参数化占位符 `?`）
     * @param paramsJson - JSON 数组字符串，如 `'[1, "Alice"]'`
     * @returns 受影响行数
     * @throws 错误消息字符串
     */
    insert(sql: string, paramsJson: string): number;

    /**
     * 执行 SELECT 查询
     * @param sql - SQL 查询语句（参数化占位符 `?`）
     * @param paramsJson - JSON 数组字符串，如 `'[1]'` 或空字符串
     * @returns 查询结果，`rowsJson` 为 JSON 行数组
     * @throws 错误消息字符串
     */
    query(sql: string, paramsJson: string): JsQueryResult;

    /**
     * 执行 UPDATE
     * @param sql - SQL DML 语句（参数化占位符 `?`）
     * @param paramsJson - JSON 数组字符串
     * @returns 受影响行数
     * @throws 错误消息字符串
     */
    update(sql: string, paramsJson: string): number;

    /**
     * 执行 DELETE
     * @param sql - SQL DML 语句（参数化占位符 `?`）
     * @param paramsJson - JSON 数组字符串
     * @returns 受影响行数
     * @throws 错误消息字符串
     */
    delete(sql: string, paramsJson: string): number;

    /** 列出所有表名 */
    tableNames(): string[];

    /** 获取指定表的行数 */
    tableRowCount(table: string): number;
}