//! HTTP 请求处理器

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use parking_lot::RwLock;

/// 表信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    /// 表名
    pub name: String,
    /// 列名列表
    pub columns: Vec<String>,
    /// 行数
    pub row_count: usize,
}

/// 表行数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRow {
    /// 行 ID
    pub id: String,
    /// 列值
    pub data: HashMap<String, serde_json::Value>,
}

/// 关系信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationInfo {
    /// 关系名称
    pub name: String,
    /// 源表
    pub from_table: String,
    /// 源列
    pub from_column: String,
    /// 目标表
    pub to_table: String,
    /// 目标列
    pub to_column: String,
}

/// 编辑请求
#[derive(Debug, Clone, Deserialize)]
pub struct EditRequest {
    /// 要更新的字段
    pub data: HashMap<String, serde_json::Value>,
}

/// 筛选参数
#[derive(Debug, Clone, Deserialize)]
pub struct FilterParams {
    /// 列名
    pub column: Option<String>,
    /// 筛选值
    pub value: Option<String>,
    /// LIMIT
    pub limit: Option<usize>,
}

/// 共享数据存储
pub type DataStore = Arc<RwLock<StudioData>>;

/// Studio 数据
///
/// 支持两种模式（v10.13.0 增强）：
/// - 内存模式：直接操作 `tables`/`rows`/`relations` HashMap（默认，测试与演示用）；
/// - 真实 SQLite 模式：通过 `sqlite` 连接读写真实数据库。
#[derive(Debug, Clone, Default)]
pub struct StudioData {
    /// 表列表（内存模式）
    pub tables: HashMap<String, TableInfo>,
    /// 表数据（内存模式）
    pub rows: HashMap<String, Vec<TableRow>>,
    /// 关系（内存模式）
    pub relations: HashMap<String, Vec<RelationInfo>>,
    /// 真实 SQLite 数据源（可选）
    pub sqlite: Option<Arc<Mutex<rusqlite::Connection>>>,
}

impl StudioData {
    /// 创建带真实 SQLite 数据源的 Studio 数据存储
    pub fn with_sqlite(conn: rusqlite::Connection) -> Self {
        Self {
            tables: HashMap::new(),
            rows: HashMap::new(),
            relations: HashMap::new(),
            sqlite: Some(Arc::new(Mutex::new(conn))),
        }
    }
}

/// 校验表名存在于 SQLite（防注入：仅允许 sqlite_master 中的真实表名）
fn sqlite_table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// 从 SQLite 读取表信息（列名 + 行数）
fn sqlite_table_info(conn: &rusqlite::Connection, table: &str) -> Option<TableInfo> {
    if !sqlite_table_exists(conn, table) {
        return None;
    }
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", quote_ident(table)))
        .ok()?;
    let columns: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;
    let row_count: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", quote_ident(table)),
            [],
            |r| r.get(0),
        )
        .ok()?;
    Some(TableInfo {
        name: table.to_string(),
        columns,
        row_count: row_count as usize,
    })
}

/// SQLite 标识符加双引号（仅用于表名/列名；调用前已用 sqlite_master 校验表存在）
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// 从 SQLite 读取表数据
fn sqlite_table_rows(
    conn: &rusqlite::Connection,
    table: &str,
    limit: Option<usize>,
) -> Result<Vec<TableRow>, String> {
    if !sqlite_table_exists(conn, table) {
        return Err(format!("表 {} 不存在", table));
    }
    let sql = format!(
        "SELECT rowid AS __rowid__, * FROM {} {}",
        quote_ident(table),
        limit.map(|n| format!("LIMIT {}", n)).unwrap_or_default()
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("查询失败: {}", e))?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows_iter = stmt
        .query_map([], |row| {
            let mut data = HashMap::new();
            for (i, col) in cols.iter().enumerate() {
                if col == "__rowid__" {
                    continue;
                }
                let val: rusqlite::types::Value = row.get(i)?;
                data.insert(col.clone(), sqlite_value_to_json(&val));
            }
            Ok(data)
        })
        .map_err(|e| format!("查询失败: {}", e))?;
    let mut result = Vec::new();
    for r in rows_iter {
        let data = r.map_err(|e| format!("读取失败: {}", e))?;
        let id = data
            .get("id")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0".to_string());
        result.push(TableRow { id, data });
    }
    Ok(result)
}

/// 将 rusqlite 值转换为 JSON
fn sqlite_value_to_json(v: &rusqlite::types::Value) -> serde_json::Value {
    use rusqlite::types::Value;
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Integer(i) => serde_json::json!(i),
        Value::Real(f) => serde_json::json!(f),
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::Blob(b) => serde_json::Value::String(format!("0x{}", hex_encode(b))),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 获取表列表（真实 SQLite 优先）
pub async fn get_tables(
    axum::extract::State(store): axum::extract::State<DataStore>,
) -> Json<Vec<TableInfo>> {
    let data = store.read();
    if let Some(conn_arc) = &data.sqlite {
        let conn = conn_arc.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .ok();
        if let Some(stmt) = stmt.as_mut() {
            let names: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .ok()
                .and_then(|it| it.collect::<Result<_, _>>().ok())
                .unwrap_or_default();
            let tables: Vec<TableInfo> = names
                .iter()
                .filter_map(|n| sqlite_table_info(&conn, n))
                .collect();
            return Json(tables);
        }
    }
    let mut tables: Vec<TableInfo> = data.tables.values().cloned().collect();
    tables.sort_by(|a, b| a.name.cmp(&b.name));
    Json(tables)
}

/// 获取表数据（真实 SQLite 优先）
pub async fn get_table_data(
    axum::extract::State(store): axum::extract::State<DataStore>,
    Path(table_name): Path<String>,
    Query(params): Query<FilterParams>,
) -> Result<Json<Vec<TableRow>>, (StatusCode, String)> {
    let data = store.read();
    if let Some(conn_arc) = &data.sqlite {
        let conn = conn_arc.lock().unwrap();
        let mut rows = sqlite_table_rows(&conn, &table_name, None)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

        if let (Some(col), Some(val)) = (&params.column, &params.value) {
            rows.retain(|r| {
                r.data
                    .get(col)
                    .map(|v| v.to_string().contains(val))
                    .unwrap_or(false)
            });
        }
        if let Some(limit) = params.limit {
            rows.truncate(limit);
        }
        return Ok(Json(rows));
    }

    let rows = data
        .rows
        .get(&table_name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("表 {} 不存在", table_name)))?;

    let mut filtered: Vec<TableRow> = rows.clone();

    if let (Some(col), Some(val)) = (&params.column, &params.value) {
        filtered.retain(|r| {
            r.data
                .get(col)
                .map(|v| v.to_string().contains(val))
                .unwrap_or(false)
        });
    }

    if let Some(limit) = params.limit {
        filtered.truncate(limit);
    }

    Ok(Json(filtered))
}

/// 编辑记录（真实 SQLite 优先）
pub async fn edit_record(
    axum::extract::State(store): axum::extract::State<DataStore>,
    Path((table_name, id)): Path<(String, String)>,
    Json(edit): Json<EditRequest>,
) -> Result<Json<TableRow>, (StatusCode, String)> {
    let mut data = store.write();
    if let Some(conn_arc) = &data.sqlite {
        let conn = conn_arc.lock().unwrap();
        if !sqlite_table_exists(&conn, &table_name) {
            return Err((StatusCode::NOT_FOUND, format!("表 {} 不存在", table_name)));
        }
        // 构造 SET 子句（列名来自请求 data 的键，须先通过 PRAGMA table_info 白名单）
        let info = sqlite_table_info(&conn, &table_name).unwrap();
        let valid_cols: std::collections::HashSet<String> = info.columns.iter().cloned().collect();
        let sets: Vec<String> = edit
            .data
            .keys()
            .filter(|k| valid_cols.contains(*k))
            .cloned()
            .collect();
        if sets.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "没有可更新的合法字段".to_string()));
        }
        let set_sql = sets
            .iter()
            .map(|c| format!("{} = ?", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let mut params: Vec<serde_json::Value> =
            sets.iter().map(|c| edit.data[c].clone()).collect();
        // id 字段：优先用整型解析
        let id_int: i64 = id.parse().unwrap_or(0);
        params.push(serde_json::json!(id_int));
        let sql = format!(
            "UPDATE {} SET {} WHERE id = ?",
            quote_ident(&table_name),
            set_sql
        );
        conn.execute(
            &sql,
            rusqlite::params_from_iter(params.iter().map(sqlite_json_to_param)),
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新失败: {}", e),
            )
        })?;
        let rows = sqlite_table_rows(&conn, &table_name, None)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
        return rows
            .into_iter()
            .find(|r| r.id == id)
            .map(Json)
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("记录 {} 不存在", id)));
    }

    let rows = data
        .rows
        .get_mut(&table_name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("表 {} 不存在", table_name)))?;

    let row = rows
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("记录 {} 不存在", id)))?;

    for (k, v) in &edit.data {
        row.data.insert(k.clone(), v.clone());
    }

    Ok(Json(row.clone()))
}

/// 将 serde_json 值转换为 rusqlite 参数
fn sqlite_json_to_param(v: &serde_json::Value) -> rusqlite::types::Value {
    use rusqlite::types::Value;
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Integer(if *b { 1 } else { 0 }),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Real(f)
            } else {
                Value::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::Text(s.clone()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => Value::Text(v.to_string()),
    }
}

/// 获取表关系
pub async fn get_table_relations(
    axum::extract::State(store): axum::extract::State<DataStore>,
    Path(table_name): Path<String>,
) -> Result<Json<Vec<RelationInfo>>, (StatusCode, String)> {
    let data = store.read();
    let relations = data.relations.get(&table_name).cloned().unwrap_or_default();
    Ok(Json(relations))
}
