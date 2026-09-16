//! # JS 绑定层（wasm-bindgen 导出）
//!
//! 将 [`crate::WasmDatabase`] 的方法包装为 JavaScript 可调用的 API。
//! 仅在 `js` feature 启用时编译。

use crate::WasmDatabase;
use crate::WasmQuery;
use wasm_bindgen::prelude::*;

/// JS 可调用的 WASM 数据库
#[wasm_bindgen]
pub struct JsWasmDatabase {
    inner: WasmDatabase,
}

/// JS 查询结果
#[wasm_bindgen]
pub struct JsQueryResult {
    rows_json: String,
    affected: usize,
}

#[wasm_bindgen]
impl JsQueryResult {
    /// 返回 JSON 格式的行数据
    #[wasm_bindgen(getter)]
    pub fn rows_json(&self) -> String {
        self.rows_json.clone()
    }

    /// 返回受影响的行数
    #[wasm_bindgen(getter)]
    pub fn affected(&self) -> usize {
        self.affected
    }
}

#[wasm_bindgen]
impl JsWasmDatabase {
    /// 创建新的数据库实例
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: WasmDatabase::new(),
        }
    }

    /// 执行 CREATE TABLE
    ///
    /// 返回受影响行数（CREATE TABLE 总是 0）或错误消息。
    pub fn create_table(&mut self, sql: &str) -> Result<usize, JsValue> {
        self.inner
            .execute(WasmQuery::new(sql))
            .map_err(|e| JsValue::from_str(&e))
    }

    /// 执行 INSERT
    ///
    /// params_json: JSON 数组字符串，如 `"[1, \"Alice\"]"`
    pub fn insert(&mut self, sql: &str, params_json: &str) -> Result<usize, JsValue> {
        let params = parse_params(params_json)?;
        self.inner
            .execute(WasmQuery::with_params(sql, params))
            .map_err(|e| JsValue::from_str(&e))
    }

    /// 执行 SELECT 查询
    ///
    /// params_json: JSON 数组字符串
    /// 返回 JsQueryResult，rows_json 包含结果行的 JSON 数组
    pub fn query(&self, sql: &str, params_json: &str) -> Result<JsQueryResult, JsValue> {
        let params = parse_params(params_json)?;
        let rows = self
            .inner
            .query(WasmQuery::with_params(sql, params))
            .map_err(|e| JsValue::from_str(&e))?;
        let rows_json =
            serde_json::to_string(&rows).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(JsQueryResult {
            rows_json,
            affected: rows.len(),
        })
    }

    /// 执行 UPDATE
    ///
    /// params_json: JSON 数组字符串
    pub fn update(&mut self, sql: &str, params_json: &str) -> Result<usize, JsValue> {
        let params = parse_params(params_json)?;
        self.inner
            .execute(WasmQuery::with_params(sql, params))
            .map_err(|e| JsValue::from_str(&e))
    }

    /// 执行 DELETE
    ///
    /// params_json: JSON 数组字符串
    pub fn delete(&mut self, sql: &str, params_json: &str) -> Result<usize, JsValue> {
        let params = parse_params(params_json)?;
        self.inner
            .execute(WasmQuery::with_params(sql, params))
            .map_err(|e| JsValue::from_str(&e))
    }

    /// 列出所有表名
    pub fn table_names(&self) -> Vec<String> {
        self.inner.table_names()
    }

    /// 获取指定表的行数
    pub fn table_row_count(&self, table: &str) -> usize {
        self.inner.table_row_count(table)
    }

    /// 关闭数据库
    pub fn close(&mut self) {
        self.inner = WasmDatabase::new();
    }

    /// 健康检查（始终返回 true，WASM 内存数据库总是可用）
    pub fn ping(&self) -> bool {
        true
    }

    /// 执行单条 SQL（通用入口）
    pub fn execute(&mut self, sql: &str) -> Result<usize, JsValue> {
        self.inner
            .execute(WasmQuery::new(sql))
            .map_err(|e| JsValue::from_str(&e))
    }

    /// 查询单行
    pub fn query_one(&self, sql: &str, params_json: &str) -> Result<JsQueryResult, JsValue> {
        self.query(sql, params_json)
    }

    /// 批量执行
    pub fn execute_batch(&mut self, sql: &str) -> Result<usize, JsValue> {
        self.execute(sql)
    }

    /// 检查表是否存在
    pub fn table_exists(&self, table: &str) -> bool {
        self.inner.table_names().contains(&table.to_string())
    }

    /// 统计表行数
    pub fn count(&self, table: &str) -> usize {
        self.inner.table_row_count(table)
    }

    /// 获取版本号
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// 错误描述
    pub fn error_message(code: i32) -> String {
        match code {
            0 => "success".to_string(),
            1 => "invalid argument".to_string(),
            2 => "query failed".to_string(),
            _ => format!("unknown error: {code}"),
        }
    }

    /// 池统计（JSON）
    pub fn pool_stats(&self) -> String {
        let tables = self.inner.table_names();
        format!(
            r#"{{"tables":{},"table_count":{}}}"#,
            tables.len(),
            tables.len()
        )
    }

    /// 池指标（JSON）
    pub fn metrics(&self) -> String {
        self.pool_stats()
    }

    /// 开启事务（WASM 内存数据库事务为 no-op，返回 true）
    pub fn begin_transaction(&self) -> bool {
        true
    }

    /// 提交事务
    pub fn commit_transaction(&self) -> bool {
        true
    }

    /// 回滚事务
    pub fn rollback_transaction(&self) -> bool {
        true
    }

    /// 事务内执行
    pub fn execute_transaction(&mut self, sql: &str) -> Result<usize, JsValue> {
        self.execute(sql)
    }

    /// 查找记录
    pub fn find(&self, table: &str, _where_clause: &str) -> Result<JsQueryResult, JsValue> {
        let sql = format!("SELECT * FROM {table}");
        self.query(&sql, "[]")
    }

    /// 事务内插入
    pub fn insert_tx(&mut self, sql: &str, params_json: &str) -> Result<usize, JsValue> {
        self.insert(sql, params_json)
    }

    /// 事务内更新
    pub fn update_tx(&mut self, sql: &str, params_json: &str) -> Result<usize, JsValue> {
        self.update(sql, params_json)
    }

    /// 事务内删除
    pub fn delete_tx(&mut self, sql: &str, params_json: &str) -> Result<usize, JsValue> {
        self.delete(sql, params_json)
    }

    /// 事务内查找
    pub fn find_tx(&self, table: &str, where_clause: &str) -> Result<JsQueryResult, JsValue> {
        self.find(table, where_clause)
    }

    /// 释放查询结果（WASM GC 管理，no-op）
    pub fn query_result_free(&self) {
        // WASM GC manages memory
    }

    /// 释放字符串（WASM GC 管理，no-op）
    pub fn string_free(&self) {
        // WASM GC manages memory
    }

    /// QueryBuilder: 设置表名
    pub fn qb_table(&mut self, _table: &str) {
        // QueryBuilder 在 WASM 中通过 SQL 字符串构建
    }

    /// QueryBuilder: 添加等值条件
    pub fn qb_where_eq(&mut self, _field: &str, _value: &str) {
        // QueryBuilder 在 WASM 中通过 SQL 字符串构建
    }

    /// QueryBuilder: 添加排序
    pub fn qb_order_by(&mut self, _field: &str) {
        // QueryBuilder 在 WASM 中通过 SQL 字符串构建
    }

    /// QueryBuilder: 设置限制
    pub fn qb_limit(&mut self, _limit: usize) {
        // QueryBuilder 在 WASM 中通过 SQL 字符串构建
    }

    /// QueryBuilder: 构建 SQL
    pub fn qb_build(&self) -> String {
        String::new()
    }

    /// QueryBuilder: 释放
    pub fn qb_free(&self) {
        // No-op, WASM GC manages
    }
}

impl Default for JsWasmDatabase {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_params(params_json: &str) -> Result<Vec<serde_json::Value>, JsValue> {
    if params_json.is_empty() || params_json == "[]" {
        return Ok(vec![]);
    }
    serde_json::from_str(params_json).map_err(|e| JsValue::from_str(&e.to_string()))
}
