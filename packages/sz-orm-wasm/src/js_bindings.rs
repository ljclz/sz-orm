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
