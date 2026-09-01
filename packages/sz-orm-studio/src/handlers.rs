//! HTTP 请求处理器

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

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
#[derive(Debug, Clone, Default)]
pub struct StudioData {
    /// 表列表
    pub tables: HashMap<String, TableInfo>,
    /// 表数据
    pub rows: HashMap<String, Vec<TableRow>>,
    /// 关系
    pub relations: HashMap<String, Vec<RelationInfo>>,
}

/// 获取表列表
pub async fn get_tables(
    axum::extract::State(store): axum::extract::State<DataStore>,
) -> Json<Vec<TableInfo>> {
    let data = store.read();
    let mut tables: Vec<TableInfo> = data.tables.values().cloned().collect();
    tables.sort_by(|a, b| a.name.cmp(&b.name));
    Json(tables)
}

/// 获取表数据
pub async fn get_table_data(
    axum::extract::State(store): axum::extract::State<DataStore>,
    Path(table_name): Path<String>,
    Query(params): Query<FilterParams>,
) -> Result<Json<Vec<TableRow>>, (StatusCode, String)> {
    let data = store.read();
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

/// 编辑记录
pub async fn edit_record(
    axum::extract::State(store): axum::extract::State<DataStore>,
    Path((table_name, id)): Path<(String, String)>,
    Json(edit): Json<EditRequest>,
) -> Result<Json<TableRow>, (StatusCode, String)> {
    let mut data = store.write();
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

/// 获取表关系
pub async fn get_table_relations(
    axum::extract::State(store): axum::extract::State<DataStore>,
    Path(table_name): Path<String>,
) -> Result<Json<Vec<RelationInfo>>, (StatusCode, String)> {
    let data = store.read();
    let relations = data.relations.get(&table_name).cloned().unwrap_or_default();
    Ok(Json(relations))
}
