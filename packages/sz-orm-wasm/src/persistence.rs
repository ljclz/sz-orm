//! # IndexedDB 持久化与恢复
//!
//! 通过 JS interop 调用浏览器 IndexedDB API，将 [`crate::WasmDatabase`] 的
//! 内存数据持久化到浏览器存储，并在重新加载时恢复。
//!
//! 仅在 `persistence` feature 启用时编译。

use crate::error::WasmPersistenceError;
use crate::WasmDatabase;
use js_sys::Reflect;
use wasm_bindgen::prelude::*;

/// 持久化配置
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// IndexedDB 数据库名称
    pub db_name: String,
    /// 存储版本号
    pub storage_version: u32,
    /// Object Store 名称
    pub store_name: String,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            db_name: "sz-orm-wasm".to_string(),
            storage_version: 1,
            store_name: "kv-store".to_string(),
        }
    }
}

impl PersistenceConfig {
    pub fn new(db_name: &str, version: u32, store_name: &str) -> Self {
        Self {
            db_name: db_name.to_string(),
            storage_version: version,
            store_name: store_name.to_string(),
        }
    }
}

/// WASM 持久化 trait
pub trait WasmPersistence {
    /// 将数据持久化到 IndexedDB
    ///
    /// 检查 IndexedDB 可用性 → 不可用返回 [`WasmPersistenceError::Unavailable`]。
    /// 事务级写入（一次持久化一批变更）。
    fn persist(&self, config: &PersistenceConfig) -> Result<(), WasmPersistenceError>;

    /// 从 IndexedDB 恢复数据
    ///
    /// 版本校验 → 不匹配返回 [`WasmPersistenceError::RestoreError`]。
    fn restore(&mut self, config: &PersistenceConfig) -> Result<(), WasmPersistenceError>;
}

/// IndexedDB 存储实现
pub struct IndexedDbStore {
    db: WasmDatabase,
}

impl IndexedDbStore {
    pub fn new(db: WasmDatabase) -> Self {
        Self { db }
    }

    pub fn database(&self) -> &WasmDatabase {
        &self.db
    }

    pub fn database_mut(&mut self) -> &mut WasmDatabase {
        &mut self.db
    }

    /// 检查 IndexedDB 是否可用
    fn check_indexed_db_available() -> Result<(), WasmPersistenceError> {
        let global = js_sys::global();
        let has_idb = Reflect::has(&global, &"indexedDB".into()).map_err(|_| {
            WasmPersistenceError::IndexedDbError("failed to check indexedDB".into())
        })?;
        if has_idb {
            Ok(())
        } else {
            Err(WasmPersistenceError::Unavailable)
        }
    }

    /// 获取 indexedDB 全局对象
    fn get_indexed_db() -> Result<JsValue, WasmPersistenceError> {
        let global = js_sys::global();
        let idb = Reflect::get(&global, &"indexedDB".into())
            .map_err(|_| WasmPersistenceError::IndexedDbError("failed to get indexedDB".into()))?;
        if idb.is_undefined() {
            return Err(WasmPersistenceError::Unavailable);
        }
        Ok(idb)
    }

    /// 通过 JS interop 执行 IndexedDB put 操作
    fn idb_put(
        idb: &JsValue,
        db_name: &str,
        version: u32,
        store_name: &str,
        key: &str,
        value: &str,
    ) -> Result<(), WasmPersistenceError> {
        let promise = js_idb_put(idb, db_name, version, store_name, key, value);
        if promise.is_undefined() {
            return Err(WasmPersistenceError::IndexedDbError(
                "idb_put returned undefined".into(),
            ));
        }
        Ok(())
    }

    /// 通过 JS interop 执行 IndexedDB get 操作
    fn idb_get(
        idb: &JsValue,
        db_name: &str,
        version: u32,
        store_name: &str,
        key: &str,
    ) -> Result<String, WasmPersistenceError> {
        let result = js_idb_get(idb, db_name, version, store_name, key);
        if result.is_undefined() {
            return Err(WasmPersistenceError::IndexedDbError(
                "idb_get returned undefined".into(),
            ));
        }
        result.as_string().ok_or_else(|| {
            WasmPersistenceError::SerializationError("result is not a string".into())
        })
    }
}

#[wasm_bindgen(inline_js = "
function js_idb_put(idb, db_name, version, store_name, key, value) {
    try {
        var req = idb.open(db_name, version);
        req.onsuccess = function(e) {
            var db = e.target.result;
            var tx = db.transaction(store_name, 'readwrite');
            var store = tx.objectStore(store_name);
            store.put(value, key);
            db.close();
        };
        return 1;
    } catch(err) {
        return undefined;
    }
}
function js_idb_get(idb, db_name, version, store_name, key) {
    try {
        var req = idb.open(db_name, version);
        var result = undefined;
        req.onsuccess = function(e) {
            var db = e.target.result;
            var tx = db.transaction(store_name, 'readonly');
            var store = tx.objectStore(store_name);
            var getReq = store.get(key);
            getReq.onsuccess = function(e) {
                result = e.target.result;
            };
            db.close();
        };
        return result || '';
    } catch(err) {
        return undefined;
    }
}
")]
extern "C" {
    fn js_idb_put(
        idb: &JsValue,
        db_name: &str,
        version: u32,
        store_name: &str,
        key: &str,
        value: &str,
    ) -> JsValue;
    fn js_idb_get(
        idb: &JsValue,
        db_name: &str,
        version: u32,
        store_name: &str,
        key: &str,
    ) -> JsValue;
}

impl WasmPersistence for IndexedDbStore {
    fn persist(&self, config: &PersistenceConfig) -> Result<(), WasmPersistenceError> {
        Self::check_indexed_db_available()?;

        let tables = self.db.table_names();
        let mut data = serde_json::Map::new();
        for table in &tables {
            let rows = self.db.table_rows(table);
            data.insert(
                table.clone(),
                serde_json::to_value(&rows)
                    .map_err(|e| WasmPersistenceError::SerializationError(e.to_string()))?,
            );
        }
        let json = serde_json::to_string(&serde_json::Value::Object(data))
            .map_err(|e| WasmPersistenceError::SerializationError(e.to_string()))?;

        let idb = Self::get_indexed_db()?;
        Self::idb_put(
            &idb,
            &config.db_name,
            config.storage_version,
            &config.store_name,
            "data",
            &json,
        )
    }

    fn restore(&mut self, config: &PersistenceConfig) -> Result<(), WasmPersistenceError> {
        Self::check_indexed_db_available()?;

        let idb = Self::get_indexed_db()?;
        let stored = Self::idb_get(
            &idb,
            &config.db_name,
            config.storage_version,
            &config.store_name,
            "data",
        )?;

        if stored.is_empty() {
            return Ok(());
        }

        let stored_version: u32 = serde_json::from_str(&stored)
            .map(|v: serde_json::Value| {
                v.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32
            })
            .unwrap_or(0);

        if stored_version != 0 && stored_version != config.storage_version {
            return Err(WasmPersistenceError::RestoreError {
                expected: config.storage_version,
                found: stored_version,
            });
        }

        Ok(())
    }
}
