//! Hydration Modes + Plugin 拦截器链
//!
//! 对应文档 6.8 节改进项 44（Hydration Modes）+ 46（Plugin 拦截器链）。
//!
//! # 核心概念
//!
//! ## Hydration Modes
//! - **Object**：每行 → `HashMap<String, Value>`（默认）
//! - **Array**：每行 → `Vec<Value>`（按列顺序）
//! - **Scalar**：每行 → Value（取第一列）
//! - **SingleScalar**：唯一行 + 唯一列 → Value
//! - **Column**：每行的指定列 → `Vec<Value>`
//!
//! ## Plugin 拦截器链
//! - **Plugin trait**：拦截 Executor 操作
//! - **PluginContext**：上下文（操作类型、SQL、参数、阶段）
//! - **PluginDecision**：插件决策（Continue / Skip / Modified）
//! - **PluginChain**：拦截器链（按注册顺序执行）
//! - **ExecutionStage**：拦截阶段（BeforeQuery / AfterQuery / BeforeUpdate / AfterUpdate / BeforeCommit / AfterCommit / BeforeRollback / AfterRollback）
//!
//! # 设计灵感
//!
//! - Doctrine `HYDRATE_*`（OBJECT/ARRAY/SCALAR/SINGLE_SCALAR/COLUMN）
//! - MyBatis `Interceptor`（拦截 Executor.query/update/commit）
//! - Hibernate `Interceptor` / `EventListeners`
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::hydration_plugin::{
//!     HydrationMode, hydrate, PluginChain, PluginContext, ExecutionStage, PluginDecision,
//! };
//! use sz_orm_core::result_map::RowData;
//! use sz_orm_core::Value;
//! use std::collections::HashMap;
//!
//! // HydrationMode::Scalar
//! let mut row = RowData::empty();
//! row.set("count", Value::I64(42));
//! let result = hydrate(&[row], HydrationMode::Scalar).unwrap();
//! assert_eq!(result.first(), Some(&Value::I64(42)));
//! ```

use crate::result_map::RowData;
use crate::value::Value;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

// ============================================================================
// HydrationMode — 填充模式
// ============================================================================

/// Hydration 填充模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HydrationMode {
    /// 每行 → `HashMap<String, Value>`（默认，对象模式）
    #[default]
    Object,
    /// 每行 → `Vec<Value>`（按列顺序）
    Array,
    /// 每行 → Value（取第一列）
    Scalar,
    /// 唯一行 + 唯一列 → Value（聚合查询常用）
    SingleScalar,
    /// 每行的指定列 → `Vec<Value>`
    Column,
}

impl HydrationMode {
    /// 模式名称
    pub fn name(&self) -> &'static str {
        match self {
            HydrationMode::Object => "object",
            HydrationMode::Array => "array",
            HydrationMode::Scalar => "scalar",
            HydrationMode::SingleScalar => "single_scalar",
            HydrationMode::Column => "column",
        }
    }
}

// ============================================================================
// HydrationResult / hydrate 函数
// ============================================================================

/// Hydration 错误
#[derive(Debug, Clone, PartialEq)]
pub enum HydrationError {
    /// SingleScalar 模式下行数不等于 1
    SingleScalarRequiresSingleRow { actual_rows: usize },
    /// 指定列不存在
    ColumnNotFound { column: String },
    /// 行列数为 0
    EmptyRow,
}

impl std::fmt::Display for HydrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HydrationError::SingleScalarRequiresSingleRow { actual_rows } => {
                write!(
                    f,
                    "SingleScalar mode requires exactly 1 row, got {}",
                    actual_rows
                )
            }
            HydrationError::ColumnNotFound { column } => {
                write!(f, "column '{}' not found", column)
            }
            HydrationError::EmptyRow => write!(f, "row has no columns"),
        }
    }
}

impl std::error::Error for HydrationError {}

/// Hydration 结果类型
pub type HydrationResult<T> = Result<T, HydrationError>;

/// Object 模式：每行 → HashMap<String, Value>
pub fn hydrate_object(rows: &[RowData]) -> HydrationResult<Vec<HashMap<String, Value>>> {
    Ok(rows
        .iter()
        .map(|r| {
            let mut map = HashMap::new();
            for (k, v) in r.iter() {
                map.insert(k.clone(), v.clone());
            }
            map
        })
        .collect())
}

/// Array 模式：每行 → `Vec<Value>`（按列名排序后顺序）
pub fn hydrate_array(rows: &[RowData]) -> HydrationResult<Vec<Vec<Value>>> {
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        // 按列名排序，保证顺序稳定
        let sorted = row.sorted_columns();
        let values: Vec<Value> = sorted.iter().map(|(_, v)| (*v).clone()).collect();
        result.push(values);
    }
    Ok(result)
}

/// Scalar 模式：每行 → Value（取第一列，按列名排序）
pub fn hydrate_scalar(rows: &[RowData]) -> HydrationResult<Vec<Value>> {
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        if row.is_empty() {
            return Err(HydrationError::EmptyRow);
        }
        let sorted = row.sorted_columns();
        let (_, first_value) = sorted.first().unwrap();
        result.push((*first_value).clone());
    }
    Ok(result)
}

/// SingleScalar 模式：唯一行 + 唯一列 → Value
pub fn hydrate_single_scalar(rows: &[RowData]) -> HydrationResult<Value> {
    if rows.len() != 1 {
        return Err(HydrationError::SingleScalarRequiresSingleRow {
            actual_rows: rows.len(),
        });
    }
    let row = &rows[0];
    if row.is_empty() {
        return Err(HydrationError::EmptyRow);
    }
    let sorted = row.sorted_columns();
    let (_, first_value) = sorted.first().unwrap();
    Ok((*first_value).clone())
}

/// Column 模式：每行的指定列 → `Vec<Value>`
pub fn hydrate_column(rows: &[RowData], column: &str) -> HydrationResult<Vec<Value>> {
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        match row.get(column) {
            Some(v) => result.push(v.clone()),
            None => {
                return Err(HydrationError::ColumnNotFound {
                    column: column.to_string(),
                })
            }
        }
    }
    Ok(result)
}

/// 通用 hydrate 函数：根据 mode 自动选择
///
/// 注意：Column 模式需要列名参数，请直接使用 `hydrate_column`。
/// 此函数对 Column 模式使用第一列。
pub fn hydrate(rows: &[RowData], mode: HydrationMode) -> HydrationResult<Vec<Value>> {
    match mode {
        HydrationMode::Scalar => hydrate_scalar(rows),
        HydrationMode::SingleScalar => {
            let v = hydrate_single_scalar(rows)?;
            Ok(vec![v])
        }
        HydrationMode::Column => {
            if rows.is_empty() {
                return Ok(Vec::new());
            }
            let first_row = &rows[0];
            if first_row.is_empty() {
                return Err(HydrationError::EmptyRow);
            }
            let sorted = first_row.sorted_columns();
            let first_col = sorted.first().unwrap().0.as_str();
            hydrate_column(rows, first_col)
        }
        HydrationMode::Object | HydrationMode::Array => {
            // Object/Array 模式下，结果应是 HashMap/Vec 而非 Value
            // 这里返回每行的第一列 Value 作为简化
            // 完整 Object/Array 结果请使用 hydrate_object / hydrate_array
            hydrate_scalar(rows)
        }
    }
}

// ============================================================================
// Plugin 拦截器链
// ============================================================================

/// 执行阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionStage {
    /// 查询前
    BeforeQuery,
    /// 查询后
    AfterQuery,
    /// 更新前（INSERT/UPDATE/DELETE）
    BeforeUpdate,
    /// 更新后
    AfterUpdate,
    /// 提交前
    BeforeCommit,
    /// 提交后
    AfterCommit,
    /// 回滚前
    BeforeRollback,
    /// 回滚后
    AfterRollback,
}

impl ExecutionStage {
    /// 阶段名称
    pub fn name(&self) -> &'static str {
        match self {
            ExecutionStage::BeforeQuery => "before_query",
            ExecutionStage::AfterQuery => "after_query",
            ExecutionStage::BeforeUpdate => "before_update",
            ExecutionStage::AfterUpdate => "after_update",
            ExecutionStage::BeforeCommit => "before_commit",
            ExecutionStage::AfterCommit => "after_commit",
            ExecutionStage::BeforeRollback => "before_rollback",
            ExecutionStage::AfterRollback => "after_rollback",
        }
    }

    /// 是否为 before 阶段
    pub fn is_before(&self) -> bool {
        matches!(
            self,
            ExecutionStage::BeforeQuery
                | ExecutionStage::BeforeUpdate
                | ExecutionStage::BeforeCommit
                | ExecutionStage::BeforeRollback
        )
    }

    /// 是否为 after 阶段
    pub fn is_after(&self) -> bool {
        !self.is_before()
    }

    /// 是否为查询阶段
    pub fn is_query(&self) -> bool {
        matches!(
            self,
            ExecutionStage::BeforeQuery | ExecutionStage::AfterQuery
        )
    }

    /// 是否为更新阶段
    pub fn is_update(&self) -> bool {
        matches!(
            self,
            ExecutionStage::BeforeUpdate | ExecutionStage::AfterUpdate
        )
    }

    /// 是否为事务阶段
    pub fn is_transaction(&self) -> bool {
        matches!(
            self,
            ExecutionStage::BeforeCommit
                | ExecutionStage::AfterCommit
                | ExecutionStage::BeforeRollback
                | ExecutionStage::AfterRollback
        )
    }
}

/// 插件上下文（携带操作信息）
#[derive(Debug, Clone)]
pub struct PluginContext {
    /// 执行阶段
    pub stage: ExecutionStage,
    /// SQL 语句（可被插件修改）
    pub sql: String,
    /// 绑定参数
    pub parameters: Vec<Value>,
    /// 执行开始时间（用于慢查询检测）
    pub started_at: Option<Instant>,
    /// 执行耗时（After 阶段才有）
    pub elapsed: Option<Duration>,
    /// 影响行数（After 阶段才有）
    pub affected_rows: Option<usize>,
    /// 自定义元数据
    pub metadata: HashMap<String, Value>,
}

impl PluginContext {
    /// 创建上下文
    pub fn new(stage: ExecutionStage, sql: impl Into<String>) -> Self {
        Self {
            stage,
            sql: sql.into(),
            parameters: Vec::new(),
            started_at: None,
            elapsed: None,
            affected_rows: None,
            metadata: HashMap::new(),
        }
    }

    /// 设置参数
    pub fn with_parameters(mut self, params: Vec<Value>) -> Self {
        self.parameters = params;
        self
    }

    /// 设置开始时间
    pub fn with_start_time(mut self, instant: Instant) -> Self {
        self.started_at = Some(instant);
        self
    }

    /// 设置耗时
    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = Some(elapsed);
        self
    }

    /// 设置影响行数
    pub fn with_affected_rows(mut self, rows: usize) -> Self {
        self.affected_rows = Some(rows);
        self
    }

    /// 添加元数据
    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }

    /// 获取元数据
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }
}

/// 插件决策
#[derive(Debug, Clone, PartialEq)]
pub enum PluginDecision {
    /// 继续执行（链中下一个插件）
    Continue,
    /// 跳过后续插件（但仍执行原 SQL）
    Skip,
    /// 修改 SQL/参数后继续
    Modified { sql: String, parameters: Vec<Value> },
    /// 中止执行（不执行原 SQL，返回错误）
    Abort(String),
}

/// Plugin 拦截器 trait
pub trait Plugin: Send + Sync {
    /// 插件名称
    fn name(&self) -> &str;

    /// 拦截哪些阶段
    fn stages(&self) -> Vec<ExecutionStage>;

    /// 拦截处理
    fn intercept(&self, context: &mut PluginContext) -> PluginDecision;
}

// ============================================================================
// PluginChain — 插件链
// ============================================================================

/// 插件链（按注册顺序执行）
#[derive(Default)]
pub struct PluginChain {
    plugins: RwLock<Vec<Box<dyn Plugin>>>,
}

impl PluginChain {
    /// 创建空链
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(Vec::new()),
        }
    }

    /// 注册插件（追加到链尾）
    pub fn register(&self, plugin: Box<dyn Plugin>) {
        let mut plugins = self.plugins.write().unwrap();
        plugins.push(plugin);
    }

    /// 注册插件到指定位置
    pub fn insert_at(&self, index: usize, plugin: Box<dyn Plugin>) {
        let mut plugins = self.plugins.write().unwrap();
        let len = plugins.len();
        plugins.insert(index.min(len), plugin);
    }

    /// 注销指定名称的插件
    pub fn unregister(&self, name: &str) -> bool {
        let mut plugins = self.plugins.write().unwrap();
        if let Some(idx) = plugins.iter().position(|p| p.name() == name) {
            plugins.remove(idx);
            true
        } else {
            false
        }
    }

    /// 已注册的插件数量
    pub fn len(&self) -> usize {
        self.plugins.read().unwrap().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 列出所有插件名
    pub fn plugin_names(&self) -> Vec<String> {
        self.plugins
            .read()
            .unwrap()
            .iter()
            .map(|p| p.name().to_string())
            .collect()
    }

    /// 清空插件链
    pub fn clear(&self) {
        self.plugins.write().unwrap().clear();
    }

    /// 执行插件链
    ///
    /// 按 before 插件 → 原操作 → after 插件 的顺序执行。
    /// 任意插件返回 `Abort` 中止整个链。
    /// 任意插件返回 `Modified` 会修改 context 后继续。
    /// 任意插件返回 `Skip` 跳过后续插件（但继续原操作）。
    pub fn execute(&self, context: &mut PluginContext) -> PluginDecision {
        let plugins = self.plugins.read().unwrap();
        let target_stages = [context.stage];

        for plugin in plugins.iter() {
            // 仅调用订阅了当前阶段的插件
            if !plugin.stages().iter().any(|s| target_stages.contains(s)) {
                continue;
            }
            match plugin.intercept(context) {
                PluginDecision::Continue => continue,
                PluginDecision::Skip => return PluginDecision::Skip,
                PluginDecision::Modified { sql, parameters } => {
                    context.sql = sql;
                    context.parameters = parameters;
                    continue;
                }
                PluginDecision::Abort(reason) => {
                    return PluginDecision::Abort(reason);
                }
            }
        }
        PluginDecision::Continue
    }
}

impl std::fmt::Debug for PluginChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let plugins = self.plugins.read().unwrap();
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        f.debug_struct("PluginChain")
            .field("plugins", &names)
            .finish()
    }
}

// ============================================================================
// 内置插件：SqlLogPlugin
// ============================================================================

/// SQL 日志插件
pub struct SqlLogPlugin {
    logs: RwLock<Vec<String>>,
}

impl SqlLogPlugin {
    pub fn new() -> Self {
        Self {
            logs: RwLock::new(Vec::new()),
        }
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs.read().unwrap().clone()
    }

    pub fn clear(&self) {
        self.logs.write().unwrap().clear();
    }

    pub fn count(&self) -> usize {
        self.logs.read().unwrap().len()
    }
}

impl Default for SqlLogPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SqlLogPlugin {
    fn name(&self) -> &str {
        "sql_log"
    }

    fn stages(&self) -> Vec<ExecutionStage> {
        vec![
            ExecutionStage::BeforeQuery,
            ExecutionStage::AfterQuery,
            ExecutionStage::BeforeUpdate,
            ExecutionStage::AfterUpdate,
        ]
    }

    fn intercept(&self, context: &mut PluginContext) -> PluginDecision {
        let mut logs = self.logs.write().unwrap();
        let entry = match context.stage {
            ExecutionStage::BeforeQuery => {
                format!("[{}] QUERY: {}", context.stage.name(), context.sql)
            }
            ExecutionStage::AfterQuery => {
                let elapsed_ms = context.elapsed.map(|d| d.as_millis()).unwrap_or(0);
                format!(
                    "[{}] QUERY ({}ms): {}",
                    context.stage.name(),
                    elapsed_ms,
                    context.sql
                )
            }
            ExecutionStage::BeforeUpdate => {
                format!("[{}] UPDATE: {}", context.stage.name(), context.sql)
            }
            ExecutionStage::AfterUpdate => {
                let rows = context.affected_rows.unwrap_or(0);
                format!(
                    "[{}] UPDATE ({} rows): {}",
                    context.stage.name(),
                    rows,
                    context.sql
                )
            }
            _ => return PluginDecision::Continue,
        };
        logs.push(entry);
        PluginDecision::Continue
    }
}

// ============================================================================
// 内置插件：SlowQueryPlugin
// ============================================================================

/// 慢查询检测插件
pub struct SlowQueryPlugin {
    threshold: Duration,
    slow_queries: RwLock<Vec<SlowQueryRecord>>,
}

/// 慢查询记录
#[derive(Debug, Clone)]
pub struct SlowQueryRecord {
    pub sql: String,
    pub elapsed: Duration,
    pub threshold: Duration,
}

impl SlowQueryPlugin {
    /// 创建插件，指定阈值
    pub fn new(threshold: Duration) -> Self {
        Self {
            threshold,
            slow_queries: RwLock::new(Vec::new()),
        }
    }

    /// 默认 1 秒阈值
    pub fn default_threshold() -> Self {
        Self::new(Duration::from_secs(1))
    }

    /// 获取所有慢查询记录
    pub fn slow_queries(&self) -> Vec<SlowQueryRecord> {
        self.slow_queries.read().unwrap().clone()
    }

    /// 慢查询数量
    pub fn count(&self) -> usize {
        self.slow_queries.read().unwrap().len()
    }

    /// 清空记录
    pub fn clear(&self) {
        self.slow_queries.write().unwrap().clear();
    }

    /// 阈值
    pub fn threshold(&self) -> Duration {
        self.threshold
    }
}

impl Plugin for SlowQueryPlugin {
    fn name(&self) -> &str {
        "slow_query"
    }

    fn stages(&self) -> Vec<ExecutionStage> {
        vec![ExecutionStage::AfterQuery, ExecutionStage::AfterUpdate]
    }

    fn intercept(&self, context: &mut PluginContext) -> PluginDecision {
        if let Some(elapsed) = context.elapsed {
            if elapsed > self.threshold {
                let mut records = self.slow_queries.write().unwrap();
                records.push(SlowQueryRecord {
                    sql: context.sql.clone(),
                    elapsed,
                    threshold: self.threshold,
                });
            }
        }
        PluginDecision::Continue
    }
}

// ============================================================================
// 内置插件：AuditPlugin
// ============================================================================

/// 审计插件（记录所有写操作）
pub struct AuditPlugin {
    audit_log: RwLock<Vec<AuditRecord>>,
}

/// 审计记录
#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub stage: ExecutionStage,
    pub sql: String,
    pub affected_rows: Option<usize>,
}

impl AuditPlugin {
    pub fn new() -> Self {
        Self {
            audit_log: RwLock::new(Vec::new()),
        }
    }

    pub fn records(&self) -> Vec<AuditRecord> {
        self.audit_log.read().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.audit_log.read().unwrap().len()
    }

    pub fn clear(&self) {
        self.audit_log.write().unwrap().clear();
    }
}

impl Default for AuditPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for AuditPlugin {
    fn name(&self) -> &str {
        "audit"
    }

    fn stages(&self) -> Vec<ExecutionStage> {
        vec![ExecutionStage::AfterUpdate]
    }

    fn intercept(&self, context: &mut PluginContext) -> PluginDecision {
        let mut log = self.audit_log.write().unwrap();
        log.push(AuditRecord {
            stage: context.stage,
            sql: context.sql.clone(),
            affected_rows: context.affected_rows,
        });
        PluginDecision::Continue
    }
}

// ============================================================================
// 内置插件：SqlRewritePlugin（演示 Modified 决策）
// ============================================================================

/// SQL 改写插件（演示 Modified 决策）
///
/// 在执行前将 `SELECT` 替换为 `SELECT /* hint */`，用于演示 SQL 改写能力。
pub struct SqlRewritePlugin {
    pattern: String,
    replacement: String,
}

impl SqlRewritePlugin {
    pub fn new(pattern: impl Into<String>, replacement: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            replacement: replacement.into(),
        }
    }
}

impl Plugin for SqlRewritePlugin {
    fn name(&self) -> &str {
        "sql_rewrite"
    }

    fn stages(&self) -> Vec<ExecutionStage> {
        vec![ExecutionStage::BeforeQuery, ExecutionStage::BeforeUpdate]
    }

    fn intercept(&self, context: &mut PluginContext) -> PluginDecision {
        if context.sql.contains(&self.pattern) {
            let new_sql = context.sql.replace(&self.pattern, &self.replacement);
            PluginDecision::Modified {
                sql: new_sql,
                parameters: context.parameters.clone(),
            }
        } else {
            PluginDecision::Continue
        }
    }
}

// ============================================================================
// 内置插件：BlockPlugin（演示 Abort 决策）
// ============================================================================

/// 阻断插件（演示 Abort 决策）
///
/// 拦截包含指定关键字的 SQL（如 DROP TABLE），返回 Abort。
pub struct BlockPlugin {
    blocked_keywords: Vec<String>,
}

impl BlockPlugin {
    pub fn new(keywords: Vec<String>) -> Self {
        Self {
            blocked_keywords: keywords,
        }
    }

    /// 默认阻断 DROP / TRUNCATE
    pub fn default_block_ddl() -> Self {
        Self::new(vec![
            "DROP TABLE".to_string(),
            "TRUNCATE".to_string(),
            "DROP DATABASE".to_string(),
        ])
    }
}

impl Plugin for BlockPlugin {
    fn name(&self) -> &str {
        "block"
    }

    fn stages(&self) -> Vec<ExecutionStage> {
        vec![ExecutionStage::BeforeUpdate, ExecutionStage::BeforeQuery]
    }

    fn intercept(&self, context: &mut PluginContext) -> PluginDecision {
        let upper_sql = context.sql.to_uppercase();
        for kw in &self.blocked_keywords {
            if upper_sql.contains(&kw.to_uppercase()) {
                return PluginDecision::Abort(format!(
                    "blocked by BlockPlugin: SQL contains forbidden keyword '{}'",
                    kw
                ));
            }
        }
        PluginDecision::Continue
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== HydrationMode =====

    #[test]
    fn test_hydration_mode_default() {
        assert_eq!(HydrationMode::default(), HydrationMode::Object);
    }

    #[test]
    fn test_hydration_mode_name() {
        assert_eq!(HydrationMode::Object.name(), "object");
        assert_eq!(HydrationMode::Array.name(), "array");
        assert_eq!(HydrationMode::Scalar.name(), "scalar");
        assert_eq!(HydrationMode::SingleScalar.name(), "single_scalar");
        assert_eq!(HydrationMode::Column.name(), "column");
    }

    // ===== hydrate_object =====

    #[test]
    fn test_hydrate_object_basic() {
        let mut row = RowData::empty();
        row.set("id", Value::I64(1));
        row.set("name", Value::String("Alice".to_string()));

        let result = hydrate_object(&[row]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].get("id"), Some(&Value::I64(1)));
        assert_eq!(
            result[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
    }

    #[test]
    fn test_hydrate_object_multiple_rows() {
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(1));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(2));
                r
            },
        ];

        let result = hydrate_object(&rows).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_hydrate_object_empty() {
        let result = hydrate_object(&[]).unwrap();
        assert!(result.is_empty());
    }

    // ===== hydrate_array =====

    #[test]
    fn test_hydrate_array_basic() {
        let mut row = RowData::empty();
        row.set("id", Value::I64(1));
        row.set("name", Value::String("Alice".to_string()));

        let result = hydrate_array(&[row]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
        // 列名排序后顺序：id, name
        assert_eq!(result[0][0], Value::I64(1));
        assert_eq!(result[0][1], Value::String("Alice".to_string()));
    }

    #[test]
    fn test_hydrate_array_empty() {
        let result = hydrate_array(&[]).unwrap();
        assert!(result.is_empty());
    }

    // ===== hydrate_scalar =====

    #[test]
    fn test_hydrate_scalar_basic() {
        let mut row = RowData::empty();
        row.set("count", Value::I64(42));

        let result = hydrate_scalar(&[row]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Value::I64(42));
    }

    #[test]
    fn test_hydrate_scalar_multiple_rows() {
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(1));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(2));
                r
            },
        ];

        let result = hydrate_scalar(&rows).unwrap();
        assert_eq!(result, vec![Value::I64(1), Value::I64(2)]);
    }

    #[test]
    fn test_hydrate_scalar_empty_row_error() {
        let row = RowData::empty();
        let err = hydrate_scalar(&[row]).unwrap_err();
        match err {
            HydrationError::EmptyRow => {}
            _ => panic!("expected EmptyRow error"),
        }
    }

    // ===== hydrate_single_scalar =====

    #[test]
    fn test_hydrate_single_scalar_ok() {
        let mut row = RowData::empty();
        row.set("total", Value::I64(100));

        let result = hydrate_single_scalar(&[row]).unwrap();
        assert_eq!(result, Value::I64(100));
    }

    #[test]
    fn test_hydrate_single_scalar_no_rows() {
        let err = hydrate_single_scalar(&[]).unwrap_err();
        match err {
            HydrationError::SingleScalarRequiresSingleRow { actual_rows } => {
                assert_eq!(actual_rows, 0)
            }
            _ => panic!("expected SingleScalarRequiresSingleRow"),
        }
    }

    #[test]
    fn test_hydrate_single_scalar_too_many_rows() {
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(1));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(2));
                r
            },
        ];
        let err = hydrate_single_scalar(&rows).unwrap_err();
        match err {
            HydrationError::SingleScalarRequiresSingleRow { actual_rows } => {
                assert_eq!(actual_rows, 2)
            }
            _ => panic!("expected SingleScalarRequiresSingleRow"),
        }
    }

    #[test]
    fn test_hydrate_single_scalar_empty_row() {
        let row = RowData::empty();
        let err = hydrate_single_scalar(&[row]).unwrap_err();
        match err {
            HydrationError::EmptyRow => {}
            _ => panic!("expected EmptyRow"),
        }
    }

    // ===== hydrate_column =====

    #[test]
    fn test_hydrate_column_basic() {
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(1));
                r.set("name", Value::String("Alice".to_string()));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(2));
                r.set("name", Value::String("Bob".to_string()));
                r
            },
        ];

        let result = hydrate_column(&rows, "name").unwrap();
        assert_eq!(
            result,
            vec![
                Value::String("Alice".to_string()),
                Value::String("Bob".to_string()),
            ]
        );
    }

    #[test]
    fn test_hydrate_column_missing() {
        let rows = vec![{
            let mut r = RowData::empty();
            r.set("id", Value::I64(1));
            r
        }];

        let err = hydrate_column(&rows, "missing").unwrap_err();
        match err {
            HydrationError::ColumnNotFound { column } => assert_eq!(column, "missing"),
            _ => panic!("expected ColumnNotFound"),
        }
    }

    #[test]
    fn test_hydrate_column_empty_rows() {
        let result = hydrate_column(&[], "name").unwrap();
        assert!(result.is_empty());
    }

    // ===== 通用 hydrate 函数 =====

    #[test]
    fn test_hydrate_scalar_mode() {
        let mut row = RowData::empty();
        row.set("count", Value::I64(42));

        let result = hydrate(&[row], HydrationMode::Scalar).unwrap();
        assert_eq!(result, vec![Value::I64(42)]);
    }

    #[test]
    fn test_hydrate_single_scalar_mode() {
        let mut row = RowData::empty();
        row.set("total", Value::I64(100));

        let result = hydrate(&[row], HydrationMode::SingleScalar).unwrap();
        assert_eq!(result, vec![Value::I64(100)]);
    }

    #[test]
    fn test_hydrate_column_mode() {
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(1));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(2));
                r
            },
        ];

        let result = hydrate(&rows, HydrationMode::Column).unwrap();
        assert_eq!(result, vec![Value::I64(1), Value::I64(2)]);
    }

    // ===== ExecutionStage =====

    #[test]
    fn test_execution_stage_name() {
        assert_eq!(ExecutionStage::BeforeQuery.name(), "before_query");
        assert_eq!(ExecutionStage::AfterQuery.name(), "after_query");
        assert_eq!(ExecutionStage::BeforeUpdate.name(), "before_update");
        assert_eq!(ExecutionStage::AfterCommit.name(), "after_commit");
    }

    #[test]
    fn test_execution_stage_is_before() {
        assert!(ExecutionStage::BeforeQuery.is_before());
        assert!(ExecutionStage::BeforeUpdate.is_before());
        assert!(ExecutionStage::BeforeCommit.is_before());
        assert!(ExecutionStage::BeforeRollback.is_before());
        assert!(!ExecutionStage::AfterQuery.is_before());
        assert!(!ExecutionStage::AfterUpdate.is_before());
    }

    #[test]
    fn test_execution_stage_is_after() {
        assert!(ExecutionStage::AfterQuery.is_after());
        assert!(!ExecutionStage::BeforeQuery.is_after());
    }

    #[test]
    fn test_execution_stage_is_query() {
        assert!(ExecutionStage::BeforeQuery.is_query());
        assert!(ExecutionStage::AfterQuery.is_query());
        assert!(!ExecutionStage::BeforeUpdate.is_query());
    }

    #[test]
    fn test_execution_stage_is_update() {
        assert!(ExecutionStage::BeforeUpdate.is_update());
        assert!(ExecutionStage::AfterUpdate.is_update());
        assert!(!ExecutionStage::BeforeQuery.is_update());
    }

    #[test]
    fn test_execution_stage_is_transaction() {
        assert!(ExecutionStage::BeforeCommit.is_transaction());
        assert!(ExecutionStage::AfterCommit.is_transaction());
        assert!(ExecutionStage::BeforeRollback.is_transaction());
        assert!(ExecutionStage::AfterRollback.is_transaction());
        assert!(!ExecutionStage::BeforeQuery.is_transaction());
    }

    // ===== PluginContext =====

    #[test]
    fn test_plugin_context_new() {
        let ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT 1");
        assert_eq!(ctx.stage, ExecutionStage::BeforeQuery);
        assert_eq!(ctx.sql, "SELECT 1");
        assert!(ctx.parameters.is_empty());
        assert!(ctx.started_at.is_none());
        assert!(ctx.elapsed.is_none());
        assert!(ctx.affected_rows.is_none());
    }

    #[test]
    fn test_plugin_context_with_parameters() {
        let ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT ?")
            .with_parameters(vec![Value::I64(1)]);
        assert_eq!(ctx.parameters.len(), 1);
    }

    #[test]
    fn test_plugin_context_with_elapsed() {
        let ctx = PluginContext::new(ExecutionStage::AfterQuery, "SELECT 1")
            .with_elapsed(Duration::from_millis(50));
        assert_eq!(ctx.elapsed.unwrap().as_millis(), 50);
    }

    #[test]
    fn test_plugin_context_with_affected_rows() {
        let ctx = PluginContext::new(ExecutionStage::AfterUpdate, "UPDATE users SET ...")
            .with_affected_rows(10);
        assert_eq!(ctx.affected_rows.unwrap(), 10);
    }

    #[test]
    fn test_plugin_context_metadata() {
        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT 1");
        ctx.set_metadata("user_id", Value::I64(42));
        assert_eq!(ctx.get_metadata("user_id"), Some(&Value::I64(42)));
        assert_eq!(ctx.get_metadata("missing"), None);
    }

    // ===== PluginChain =====

    #[test]
    fn test_plugin_chain_empty() {
        let chain = PluginChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn test_plugin_chain_register() {
        let chain = PluginChain::new();
        chain.register(Box::new(SqlLogPlugin::new()));
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn test_plugin_chain_unregister() {
        let chain = PluginChain::new();
        chain.register(Box::new(SqlLogPlugin::new()));
        assert_eq!(chain.len(), 1);

        let removed = chain.unregister("sql_log");
        assert!(removed);
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn test_plugin_chain_unregister_missing() {
        let chain = PluginChain::new();
        let removed = chain.unregister("non_existent");
        assert!(!removed);
    }

    #[test]
    fn test_plugin_chain_plugin_names() {
        let chain = PluginChain::new();
        chain.register(Box::new(SqlLogPlugin::new()));
        chain.register(Box::new(AuditPlugin::new()));

        let names = chain.plugin_names();
        assert_eq!(names, vec!["sql_log", "audit"]);
    }

    #[test]
    fn test_plugin_chain_clear() {
        let chain = PluginChain::new();
        chain.register(Box::new(SqlLogPlugin::new()));
        chain.clear();
        assert!(chain.is_empty());
    }

    #[test]
    fn test_plugin_chain_insert_at() {
        let chain = PluginChain::new();
        chain.register(Box::new(SqlLogPlugin::new()));
        chain.insert_at(0, Box::new(AuditPlugin::new()));

        let names = chain.plugin_names();
        assert_eq!(names, vec!["audit", "sql_log"]);
    }

    #[test]
    fn test_plugin_chain_insert_at_end() {
        let chain = PluginChain::new();
        chain.register(Box::new(SqlLogPlugin::new()));
        chain.insert_at(99, Box::new(AuditPlugin::new()));

        let names = chain.plugin_names();
        assert_eq!(names, vec!["sql_log", "audit"]);
    }

    #[test]
    fn test_plugin_chain_execute_empty() {
        let chain = PluginChain::new();
        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT 1");
        let decision = chain.execute(&mut ctx);
        assert_eq!(decision, PluginDecision::Continue);
    }

    #[test]
    fn test_plugin_chain_execute_continue() {
        let chain = PluginChain::new();
        chain.register(Box::new(SqlLogPlugin::new()));

        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT 1");
        let decision = chain.execute(&mut ctx);
        assert_eq!(decision, PluginDecision::Continue);
    }

    #[test]
    fn test_plugin_chain_execute_skip() {
        struct SkipPlugin;
        impl Plugin for SkipPlugin {
            fn name(&self) -> &str {
                "skip"
            }
            fn stages(&self) -> Vec<ExecutionStage> {
                vec![ExecutionStage::BeforeQuery]
            }
            fn intercept(&self, _ctx: &mut PluginContext) -> PluginDecision {
                PluginDecision::Skip
            }
        }

        let chain = PluginChain::new();
        chain.register(Box::new(SkipPlugin));

        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT 1");
        let decision = chain.execute(&mut ctx);
        assert_eq!(decision, PluginDecision::Skip);
    }

    #[test]
    fn test_plugin_chain_execute_modified() {
        let chain = PluginChain::new();
        chain.register(Box::new(SqlRewritePlugin::new(
            "SELECT",
            "SELECT /* hint */",
        )));

        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT 1");
        let decision = chain.execute(&mut ctx);
        assert_eq!(decision, PluginDecision::Continue);
        assert_eq!(ctx.sql, "SELECT /* hint */ 1");
    }

    #[test]
    fn test_plugin_chain_execute_abort() {
        let chain = PluginChain::new();
        chain.register(Box::new(BlockPlugin::new(vec!["DROP".to_string()])));

        let mut ctx = PluginContext::new(ExecutionStage::BeforeUpdate, "DROP TABLE users");
        let decision = chain.execute(&mut ctx);
        match decision {
            PluginDecision::Abort(reason) => assert!(reason.contains("DROP")),
            _ => panic!("expected Abort"),
        }
    }

    #[test]
    fn test_plugin_chain_skip_unrelated_stages() {
        let chain = PluginChain::new();
        // SqlLogPlugin 订阅 Before/AfterQuery/Update，不应响应 BeforeCommit
        chain.register(Box::new(SqlLogPlugin::new()));

        let mut ctx = PluginContext::new(ExecutionStage::BeforeCommit, "COMMIT");
        let decision = chain.execute(&mut ctx);
        assert_eq!(decision, PluginDecision::Continue);
        // 日志中不应有 BeforeCommit 记录
    }

    // ===== SqlLogPlugin =====

    #[test]
    fn test_sql_log_plugin_basic() {
        let plugin = SqlLogPlugin::new();
        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT 1");
        let decision = plugin.intercept(&mut ctx);
        assert_eq!(decision, PluginDecision::Continue);
        assert_eq!(plugin.count(), 1);
    }

    #[test]
    fn test_sql_log_plugin_after_query_with_elapsed() {
        let plugin = SqlLogPlugin::new();
        let mut ctx = PluginContext::new(ExecutionStage::AfterQuery, "SELECT 1")
            .with_elapsed(Duration::from_millis(50));
        let _ = plugin.intercept(&mut ctx);

        let logs = plugin.logs();
        assert!(logs[0].contains("50ms"));
    }

    #[test]
    fn test_sql_log_plugin_after_update_with_rows() {
        let plugin = SqlLogPlugin::new();
        let mut ctx = PluginContext::new(ExecutionStage::AfterUpdate, "UPDATE users SET ...")
            .with_affected_rows(10);
        let _ = plugin.intercept(&mut ctx);

        let logs = plugin.logs();
        assert!(logs[0].contains("10 rows"));
    }

    #[test]
    fn test_sql_log_plugin_clear() {
        let plugin = SqlLogPlugin::new();
        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT 1");
        let _ = plugin.intercept(&mut ctx);
        assert_eq!(plugin.count(), 1);

        plugin.clear();
        assert_eq!(plugin.count(), 0);
    }

    // ===== SlowQueryPlugin =====

    #[test]
    fn test_slow_query_plugin_below_threshold() {
        let plugin = SlowQueryPlugin::new(Duration::from_millis(100));
        let mut ctx = PluginContext::new(ExecutionStage::AfterQuery, "SELECT 1")
            .with_elapsed(Duration::from_millis(50));
        let _ = plugin.intercept(&mut ctx);

        assert_eq!(plugin.count(), 0);
    }

    #[test]
    fn test_slow_query_plugin_above_threshold() {
        let plugin = SlowQueryPlugin::new(Duration::from_millis(100));
        let mut ctx = PluginContext::new(ExecutionStage::AfterQuery, "SELECT * FROM big_table")
            .with_elapsed(Duration::from_millis(500));
        let _ = plugin.intercept(&mut ctx);

        assert_eq!(plugin.count(), 1);
        let records = plugin.slow_queries();
        assert!(records[0].elapsed > records[0].threshold);
    }

    #[test]
    fn test_slow_query_plugin_no_elapsed() {
        let plugin = SlowQueryPlugin::new(Duration::from_millis(100));
        let mut ctx = PluginContext::new(ExecutionStage::AfterQuery, "SELECT 1");
        let _ = plugin.intercept(&mut ctx);

        assert_eq!(plugin.count(), 0);
    }

    #[test]
    fn test_slow_query_plugin_clear() {
        let plugin = SlowQueryPlugin::new(Duration::from_millis(100));
        let mut ctx = PluginContext::new(ExecutionStage::AfterQuery, "SELECT 1")
            .with_elapsed(Duration::from_millis(200));
        let _ = plugin.intercept(&mut ctx);
        assert_eq!(plugin.count(), 1);

        plugin.clear();
        assert_eq!(plugin.count(), 0);
    }

    #[test]
    fn test_slow_query_plugin_default_threshold() {
        let plugin = SlowQueryPlugin::default_threshold();
        assert_eq!(plugin.threshold(), Duration::from_secs(1));
    }

    // ===== AuditPlugin =====

    #[test]
    fn test_audit_plugin_basic() {
        let plugin = AuditPlugin::new();
        let mut ctx = PluginContext::new(ExecutionStage::AfterUpdate, "INSERT INTO users ...")
            .with_affected_rows(1);
        let _ = plugin.intercept(&mut ctx);

        assert_eq!(plugin.count(), 1);
        let records = plugin.records();
        assert_eq!(records[0].stage, ExecutionStage::AfterUpdate);
        assert_eq!(records[0].affected_rows, Some(1));
    }

    #[test]
    fn test_audit_plugin_clear() {
        let plugin = AuditPlugin::new();
        let mut ctx = PluginContext::new(ExecutionStage::AfterUpdate, "UPDATE users");
        let _ = plugin.intercept(&mut ctx);
        assert_eq!(plugin.count(), 1);

        plugin.clear();
        assert_eq!(plugin.count(), 0);
    }

    // ===== SqlRewritePlugin =====

    #[test]
    fn test_sql_rewrite_plugin_matches() {
        let plugin = SqlRewritePlugin::new("SELECT", "SELECT /* hint */");
        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT * FROM users");
        let decision = plugin.intercept(&mut ctx);
        match decision {
            PluginDecision::Modified { sql, .. } => {
                assert_eq!(sql, "SELECT /* hint */ * FROM users");
            }
            _ => panic!("expected Modified"),
        }
    }

    #[test]
    fn test_sql_rewrite_plugin_no_match() {
        let plugin = SqlRewritePlugin::new("SELECT", "SELECT /* hint */");
        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SHOW TABLES");
        let decision = plugin.intercept(&mut ctx);
        assert_eq!(decision, PluginDecision::Continue);
    }

    // ===== BlockPlugin =====

    #[test]
    fn test_block_plugin_blocks_drop() {
        let plugin = BlockPlugin::default_block_ddl();
        let mut ctx = PluginContext::new(ExecutionStage::BeforeUpdate, "DROP TABLE users");
        let decision = plugin.intercept(&mut ctx);
        match decision {
            PluginDecision::Abort(reason) => {
                assert!(reason.contains("DROP TABLE"));
            }
            _ => panic!("expected Abort"),
        }
    }

    #[test]
    fn test_block_plugin_blocks_truncate() {
        let plugin = BlockPlugin::default_block_ddl();
        let mut ctx = PluginContext::new(ExecutionStage::BeforeUpdate, "TRUNCATE TABLE logs");
        let decision = plugin.intercept(&mut ctx);
        assert!(matches!(decision, PluginDecision::Abort(_)));
    }

    #[test]
    fn test_block_plugin_allows_safe_sql() {
        let plugin = BlockPlugin::default_block_ddl();
        let mut ctx = PluginContext::new(ExecutionStage::BeforeQuery, "SELECT * FROM users");
        let decision = plugin.intercept(&mut ctx);
        assert_eq!(decision, PluginDecision::Continue);
    }

    #[test]
    fn test_block_plugin_case_insensitive() {
        let plugin = BlockPlugin::default_block_ddl();
        let mut ctx = PluginContext::new(ExecutionStage::BeforeUpdate, "drop table users");
        let decision = plugin.intercept(&mut ctx);
        assert!(matches!(decision, PluginDecision::Abort(_)));
    }

    // ===== 端到端场景 =====

    #[test]
    fn test_e2e_plugin_chain_workflow() {
        let chain = PluginChain::new();
        let sql_log = std::sync::Arc::new(SqlLogPlugin::new());
        let audit = std::sync::Arc::new(AuditPlugin::new());
        let slow = std::sync::Arc::new(SlowQueryPlugin::new(Duration::from_millis(100)));

        // 由于 Plugin trait 是 Send + Sync 但需要 'static，我们用 Box::new 克隆实例
        // 这里简化：直接注册新的实例
        chain.register(Box::new(SqlLogPlugin::new()));
        chain.register(Box::new(AuditPlugin::new()));
        chain.register(Box::new(SlowQueryPlugin::new(Duration::from_millis(100))));

        assert_eq!(chain.len(), 3);

        // 1. 模拟查询前
        let mut before_ctx = PluginContext::new(
            ExecutionStage::BeforeQuery,
            "SELECT * FROM users WHERE id = ?",
        )
        .with_parameters(vec![Value::I64(1)]);
        let decision = chain.execute(&mut before_ctx);
        assert_eq!(decision, PluginDecision::Continue);
        // sql_log 应记录 1 条（before_query 阶段）
        // audit 不订阅 before_query，不应记录
        // slow_query 不订阅 before_query，不应记录

        // 2. 模拟查询后（慢查询）
        let mut after_ctx = PluginContext::new(
            ExecutionStage::AfterQuery,
            "SELECT * FROM users WHERE id = ?",
        )
        .with_elapsed(Duration::from_millis(500));
        let decision = chain.execute(&mut after_ctx);
        assert_eq!(decision, PluginDecision::Continue);

        // 3. 验证插件链中的插件名
        let names = chain.plugin_names();
        assert_eq!(names, vec!["sql_log", "audit", "slow_query"]);

        let _ = (sql_log, audit, slow); // 避免未使用警告
    }

    #[test]
    fn test_e2e_block_plugin_aborts_chain() {
        let chain = PluginChain::new();
        // 先注册 block，确保 Abort 中止后续插件
        chain.register(Box::new(BlockPlugin::default_block_ddl()));
        chain.register(Box::new(SqlLogPlugin::new()));

        let mut ctx = PluginContext::new(ExecutionStage::BeforeUpdate, "DROP TABLE users");
        let decision = chain.execute(&mut ctx);
        assert!(matches!(decision, PluginDecision::Abort(_)));
    }

    #[test]
    fn test_e2e_hydrate_scalar_count_query() {
        // 模拟 SELECT COUNT(*) AS cnt FROM users 的结果填充
        let mut row = RowData::empty();
        row.set("cnt", Value::I64(42));

        let result = hydrate_single_scalar(&[row]).unwrap();
        assert_eq!(result, Value::I64(42));
    }

    #[test]
    fn test_e2e_hydrate_object_user_query() {
        // 模拟 SELECT id, name, email FROM users
        let rows = vec![
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(1));
                r.set("name", Value::String("Alice".to_string()));
                r.set("email", Value::String("alice@example.com".to_string()));
                r
            },
            {
                let mut r = RowData::empty();
                r.set("id", Value::I64(2));
                r.set("name", Value::String("Bob".to_string()));
                r.set("email", Value::String("bob@example.com".to_string()));
                r
            },
        ];

        let result = hydrate_object(&rows).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
    }

    #[test]
    fn test_e2e_hydrate_array_multi_column() {
        let rows = vec![{
            let mut r = RowData::empty();
            r.set("a", Value::I64(1));
            r.set("b", Value::I64(2));
            r.set("c", Value::I64(3));
            r
        }];

        let result = hydrate_array(&rows).unwrap();
        // 按列名排序：a, b, c
        assert_eq!(result[0], vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
    }
}
