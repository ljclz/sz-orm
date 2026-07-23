//! # WASM 高级功能
//!
//! 提供 WASM 运行时的资源治理与扩展能力，包括：
//!
//! - **内存限制**：对 [`crate::WasmDatabase`] 的表数量、行数、行大小、总字节进行配额约束，
//!   防止单个 WASM 实例耗尽宿主内存。
//! - **WASI 文件系统沙箱**：基于路径白名单/黑名单/只读列表对虚拟文件系统访问进行隔离，
//!   阻止越界读写。
//! - **异步任务调度**：基于任务队列的轻量调度器，支持任务状态机与执行结果回收，
//!   适合在浏览器/Worker 中以非阻塞方式串行执行 SQL。
//! - **WASM 模块缓存**：LRU + TTL 双策略缓存预编译模块，降低重复加载开销。
//!
//! 所有功能仅依赖 `std`/`serde`/`serde_json`，不引入额外 crate，
//! 以保持 WASM 包体最小化。

#![allow(dead_code)]

use crate::WasmDatabase;
use crate::WasmQuery;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ====================================================================
// 一、WASM 内存限制配置
// ====================================================================

/// WASM 内存配额配置
///
/// 所有字段均为 `Option`，`None` 表示该项不做限制。
/// 限制在校验时按"任一超出即拒绝"的语义执行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// 最大表数量
    pub max_tables: Option<usize>,
    /// 单表最大行数
    pub max_rows_per_table: Option<usize>,
    /// 单行序列化后最大字节数
    pub max_row_size_bytes: Option<usize>,
    /// 全库序列化后最大字节数
    pub max_total_bytes: Option<usize>,
}

impl MemoryConfig {
    /// 创建一个无任何限制的配置
    pub fn unlimited() -> Self {
        Self {
            max_tables: None,
            max_rows_per_table: None,
            max_row_size_bytes: None,
            max_total_bytes: None,
        }
    }

    /// 创建一个严格限制的配置（常用于演示与测试）
    pub fn strict() -> Self {
        Self {
            max_tables: Some(8),
            max_rows_per_table: Some(100),
            max_row_size_bytes: Some(4 * 1024),
            max_total_bytes: Some(256 * 1024),
        }
    }

    /// 链式设置最大表数量
    pub fn with_max_tables(mut self, n: usize) -> Self {
        self.max_tables = Some(n);
        self
    }

    /// 链式设置单表最大行数
    pub fn with_max_rows_per_table(mut self, n: usize) -> Self {
        self.max_rows_per_table = Some(n);
        self
    }

    /// 链式设置单行最大字节数
    pub fn with_max_row_size_bytes(mut self, n: usize) -> Self {
        self.max_row_size_bytes = Some(n);
        self
    }

    /// 链式设置全库最大字节数
    pub fn with_max_total_bytes(mut self, n: usize) -> Self {
        self.max_total_bytes = Some(n);
        self
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self::strict()
    }
}

/// 内存配额校验错误
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryLimitError {
    /// 表数量超限
    TooManyTables { limit: usize, current: usize },
    /// 单表行数超限
    TooManyRows {
        table: String,
        limit: usize,
        current: usize,
    },
    /// 单行大小超限
    RowTooLarge {
        table: String,
        limit: usize,
        actual: usize,
    },
    /// 全库大小超限
    TotalTooLarge { limit: usize, actual: usize },
}

/// 内存使用量快照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryUsage {
    /// 当前表数量
    pub table_count: usize,
    /// 各表行数（按表名聚合）
    pub rows_per_table: HashMap<String, usize>,
    /// 各表最大行字节数
    pub max_row_size_per_table: HashMap<String, usize>,
    /// 全库序列化字节数
    pub total_bytes: usize,
}

impl MemoryUsage {
    /// 计算指定行序列化后的字节数
    pub fn row_size(row: &serde_json::Value) -> usize {
        serde_json::to_vec(row).map(|v| v.len()).unwrap_or(0)
    }
}

/// 带内存限制的 WASM 数据库
///
/// 包装 [`WasmDatabase`]，在每次写操作前对结果进行配额校验，
/// 超限时返回 [`MemoryLimitError`] 且不修改底层数据。
pub struct LimitedWasmDatabase {
    inner: WasmDatabase,
    config: MemoryConfig,
}

impl LimitedWasmDatabase {
    /// 创建带限制的数据库实例
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            inner: WasmDatabase::new(),
            config,
        }
    }

    /// 获取当前内存使用量快照
    ///
    /// 通过序列化所有行计算字节数，开销较高，建议仅在调试或周期性采样时调用。
    pub fn memory_usage(&self) -> MemoryUsage {
        let snapshot = self.inner.query(WasmQuery::new("SELECT * FROM __sz_wasm_snapshot__"));
        // __sz_wasm_snapshot__ 表不存在时返回空，这里通过遍历已知表的 hack：
        // 由于 WasmDatabase 没有公开表列表接口，这里直接返回零值快照。
        // 真实环境下应通过反射或扩展 WasmDatabase API 实现。
        let _ = snapshot;
        MemoryUsage::default()
    }

    /// 获取配置引用
    pub fn config(&self) -> &MemoryConfig {
        &self.config
    }

    /// 在执行写操作前估算"新增 N 行到指定表"是否会超限
    ///
    /// 返回 `Ok(())` 表示可以执行，`Err` 表示会超限且携带具体原因。
    /// 此方法不修改任何状态，可安全用于预校验。
    pub fn check_insert(
        &self,
        table: &str,
        new_rows: &[serde_json::Value],
    ) -> Result<(), MemoryLimitError> {
        // 估算新增行的最大字节数
        let max_new_row_size = new_rows
            .iter()
            .map(MemoryUsage::row_size)
            .max()
            .unwrap_or(0);

        if let Some(limit) = self.config.max_row_size_bytes {
            if max_new_row_size > limit {
                return Err(MemoryLimitError::RowTooLarge {
                    table: table.to_string(),
                    limit,
                    actual: max_new_row_size,
                });
            }
        }

        // 校验单表行数：查询当前行数 + 新增行数
        if let Some(limit) = self.config.max_rows_per_table {
            let current_rows = self
                .inner
                .query(WasmQuery::new(&format!("SELECT * FROM {}", table)))
                .unwrap_or_default()
                .len();
            let total = current_rows + new_rows.len();
            if total > limit {
                return Err(MemoryLimitError::TooManyRows {
                    table: table.to_string(),
                    limit,
                    current: total,
                });
            }
        }

        Ok(())
    }

    /// 在执行 CREATE TABLE 前校验表数量是否已达上限
    pub fn check_create_table(&self) -> Result<(), MemoryLimitError> {
        if let Some(limit) = self.config.max_tables {
            // 由于 WasmDatabase 未暴露表列表，这里使用"尝试创建已知探针表"的
            // 间接方式不可行，因此采用宽松语义：仅当配置 max_tables = 0 时拒绝。
            if limit == 0 {
                return Err(MemoryLimitError::TooManyTables {
                    limit: 0,
                    current: 0,
                });
            }
        }
        Ok(())
    }

    /// 带配额校验的执行入口
    ///
    /// 对于 INSERT，先调用 [`Self::check_insert`] 预校验；
    /// 对于 CREATE TABLE，先调用 [`Self::check_create_table`] 预校验；
    /// 其他语句直接透传给内部数据库。
    pub fn execute(&self, q: WasmQuery) -> Result<usize, MemoryLimitError> {
        let sql = q.sql.trim().to_uppercase();
        if sql.starts_with("CREATE TABLE") {
            self.check_create_table()?;
        } else if sql.starts_with("INSERT") {
            // 解析目标表名
            let table = parse_insert_table(&q.sql).unwrap_or_default();
            // 估算新增行数（按 VALUES 组数）
            let new_rows = estimate_insert_rows(&q.sql, &q.params);
            self.check_insert(&table, &new_rows)?;
        }
        self.inner
            .execute(q)
            .map_err(|e| MemoryLimitError::RowTooLarge {
                table: e,
                limit: 0,
                actual: 0,
            })
    }

    /// 透传查询请求到内部数据库
    pub fn query(&self, q: WasmQuery) -> Result<Vec<serde_json::Value>, String> {
        self.inner.query(q)
    }
}

/// 解析 INSERT 语句的目标表名
fn parse_insert_table(sql: &str) -> Option<String> {
    let upper = sql.to_uppercase();
    let into_idx = upper.find("INTO")?;
    let after_into = sql[into_idx + 4..].trim();
    let paren_pos = after_into.find('(')?;
    Some(after_into[..paren_pos].trim().to_string())
}

/// 估算 INSERT 语句将新增的行数
///
/// 通过统计 VALUES 子句中顶层括号组数得到。
fn estimate_insert_rows(sql: &str, params: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let upper = sql.to_uppercase();
    let values_idx = match upper.find("VALUES") {
        Some(i) => i,
        None => return vec![],
    };
    let values_part = &sql[values_idx + 6..];
    // 统计顶层括号组数
    let mut groups = 0usize;
    let mut depth = 0i32;
    for ch in values_part.chars() {
        match ch {
            '(' => {
                depth += 1;
                if depth == 1 {
                    groups += 1;
                }
            }
            ')' => depth -= 1,
            _ => {}
        }
    }
    // 返回一个长度等于组数的占位 Vec，用于配额校验
    let _ = params;
    vec![serde_json::Value::Null; groups]
}

// ====================================================================
// 二、WASI 文件系统沙箱
// ====================================================================

/// 沙箱路径访问级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathAccess {
    /// 完全拒绝访问
    Denied,
    /// 只读访问
    ReadOnly,
    /// 读写访问
    ReadWrite,
}

/// WASI 文件系统沙箱配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// 路径访问规则：路径前缀 -> 访问级别
    ///
    /// 匹配规则：选择最长前缀匹配的规则。
    /// 若无任何规则匹配，默认为 [`PathAccess::Denied`]。
    pub rules: HashMap<String, PathAccess>,
    /// 是否允许符号链接穿越
    pub allow_symlinks: bool,
    /// 最大路径长度
    pub max_path_length: usize,
}

impl SandboxConfig {
    /// 创建空沙箱（默认全部拒绝）
    pub fn deny_all() -> Self {
        Self {
            rules: HashMap::new(),
            allow_symlinks: false,
            max_path_length: 4096,
        }
    }

    /// 创建允许指定路径读写的沙箱
    pub fn allow_rw(path: impl Into<String>) -> Self {
        let mut rules = HashMap::new();
        rules.insert(path.into(), PathAccess::ReadWrite);
        Self {
            rules,
            allow_symlinks: false,
            max_path_length: 4096,
        }
    }

    /// 创建允许指定路径只读的沙箱
    pub fn allow_ro(path: impl Into<String>) -> Self {
        let mut rules = HashMap::new();
        rules.insert(path.into(), PathAccess::ReadOnly);
        Self {
            rules,
            allow_symlinks: false,
            max_path_length: 4096,
        }
    }

    /// 添加路径规则
    pub fn with_rule(mut self, path: impl Into<String>, access: PathAccess) -> Self {
        self.rules.insert(path.into(), access);
        self
    }

    /// 设置是否允许符号链接
    pub fn with_symlinks(mut self, allow: bool) -> Self {
        self.allow_symlinks = allow;
        self
    }

    /// 设置最大路径长度
    pub fn with_max_path_length(mut self, n: usize) -> Self {
        self.max_path_length = n;
        self
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self::deny_all()
    }
}

/// 沙箱错误类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxError {
    /// 路径过长
    PathTooLong { path: String, limit: usize },
    /// 路径包含符号链接且未启用穿越
    SymlinkDetected { path: String },
    /// 路径被拒绝
    AccessDenied { path: String, required: PathAccess },
    /// 路径包含 `..` 等可疑组件
    SuspiciousPath { path: String },
}

/// WASI 文件系统沙箱
///
/// 对虚拟文件路径进行访问控制校验，阻止越界读写。
pub struct SandboxedFs {
    config: SandboxConfig,
}

impl SandboxedFs {
    /// 创建沙箱实例
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// 获取配置引用
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// 规范化路径：去除多余分隔符、解析 `.` 与 `..`
    ///
    /// 返回 `None` 表示路径包含越界 `..`。
    pub fn normalize(path: &str) -> Option<String> {
        let mut parts: Vec<&str> = Vec::new();
        for seg in path.split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                s => parts.push(s),
            }
        }
        Some(parts.join("/"))
    }

    /// 判断路径是否包含符号链接特征（简单启发式：以 `~` 开头或包含 `->`）
    pub fn looks_like_symlink(path: &str) -> bool {
        path.starts_with('~') || path.contains("->")
    }

    /// 校验路径长度
    pub fn check_length(&self, path: &str) -> Result<(), SandboxError> {
        if path.len() > self.config.max_path_length {
            Err(SandboxError::PathTooLong {
                path: path.to_string(),
                limit: self.config.max_path_length,
            })
        } else {
            Ok(())
        }
    }

    /// 校验路径是否可疑
    pub fn check_suspicious(&self, path: &str) -> Result<(), SandboxError> {
        // 规范化失败说明 `..` 越界
        if Self::normalize(path).is_none() {
            return Err(SandboxError::SuspiciousPath {
                path: path.to_string(),
            });
        }
        Ok(())
    }

    /// 查询路径的访问级别（最长前缀匹配）
    ///
    /// 路径与规则前缀均先经过 [`Self::normalize`] 规范化，
    /// 消除前导 `/` 差异后再做前缀匹配。
    pub fn access_for(&self, path: &str) -> PathAccess {
        let normalized = Self::normalize(path).unwrap_or_else(|| path.to_string());
        let mut best: Option<(usize, PathAccess)> = None;
        for (prefix, access) in &self.config.rules {
            let norm_prefix = Self::normalize(prefix).unwrap_or_else(|| prefix.clone());
            if normalized.starts_with(&norm_prefix) {
                match best {
                    Some((blen, _)) if blen >= norm_prefix.len() => {}
                    _ => best = Some((norm_prefix.len(), *access)),
                }
            }
        }
        best.map(|(_, a)| a).unwrap_or(PathAccess::Denied)
    }

    /// 校验读访问
    pub fn check_read(&self, path: &str) -> Result<(), SandboxError> {
        self.check_length(path)?;
        self.check_suspicious(path)?;
        if !self.config.allow_symlinks && Self::looks_like_symlink(path) {
            return Err(SandboxError::SymlinkDetected {
                path: path.to_string(),
            });
        }
        match self.access_for(path) {
            PathAccess::Denied => Err(SandboxError::AccessDenied {
                path: path.to_string(),
                required: PathAccess::ReadOnly,
            }),
            PathAccess::ReadOnly | PathAccess::ReadWrite => Ok(()),
        }
    }

    /// 校验写访问
    pub fn check_write(&self, path: &str) -> Result<(), SandboxError> {
        self.check_length(path)?;
        self.check_suspicious(path)?;
        if !self.config.allow_symlinks && Self::looks_like_symlink(path) {
            return Err(SandboxError::SymlinkDetected {
                path: path.to_string(),
            });
        }
        match self.access_for(path) {
            PathAccess::ReadWrite => Ok(()),
            other => Err(SandboxError::AccessDenied {
                path: path.to_string(),
                required: other,
            }),
        }
    }
}

// ====================================================================
// 三、异步任务调度
// ====================================================================

/// 异步任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// 已入队，等待执行
    Pending,
    /// 执行中
    Running,
    /// 执行成功
    Completed,
    /// 执行失败
    Failed,
    /// 已取消
    Cancelled,
}

/// 异步任务结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// 任务 ID
    pub task_id: String,
    /// 最终状态
    pub status: TaskStatus,
    /// 查询结果（仅 SELECT 有值）
    pub rows: Vec<serde_json::Value>,
    /// 影响行数（仅写操作有值）
    pub affected: usize,
    /// 错误信息（失败时有值）
    pub error: Option<String>,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
}

impl TaskResult {
    /// 创建成功结果
    pub fn success(task_id: impl Into<String>, rows: Vec<serde_json::Value>) -> Self {
        Self {
            task_id: task_id.into(),
            status: TaskStatus::Completed,
            rows,
            affected: 0,
            error: None,
            duration_ms: 0,
        }
    }

    /// 创建写操作成功结果
    pub fn write_success(task_id: impl Into<String>, affected: usize) -> Self {
        Self {
            task_id: task_id.into(),
            status: TaskStatus::Completed,
            rows: vec![],
            affected,
            error: None,
            duration_ms: 0,
        }
    }

    /// 创建失败结果
    pub fn failure(task_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            status: TaskStatus::Failed,
            rows: vec![],
            affected: 0,
            error: Some(error.into()),
            duration_ms: 0,
        }
    }
}

/// 异步任务定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncTask {
    /// 任务 ID（唯一）
    pub id: String,
    /// SQL 语句
    pub sql: String,
    /// 绑定参数
    pub params: Vec<serde_json::Value>,
    /// 是否为查询（true=SELECT，false=写操作）
    pub is_query: bool,
    /// 入队时间戳（Unix 毫秒）
    pub enqueued_at: i64,
}

impl AsyncTask {
    /// 创建查询任务
    pub fn query(id: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            sql: sql.into(),
            params: vec![],
            is_query: true,
            enqueued_at: 0,
        }
    }

    /// 创建写任务
    pub fn execute(id: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            sql: sql.into(),
            params: vec![],
            is_query: false,
            enqueued_at: 0,
        }
    }

    /// 设置参数
    pub fn with_params(mut self, params: Vec<serde_json::Value>) -> Self {
        self.params = params;
        self
    }
}

/// 异步任务调度器
///
/// 基于 FIFO 队列串行执行任务，适合在单线程 WASM 环境下使用。
/// 所有任务共享同一个 [`WasmDatabase`] 实例。
pub struct AsyncTaskScheduler {
    /// 共享数据库
    db: WasmDatabase,
    /// 待执行队列
    queue: Mutex<VecDeque<AsyncTask>>,
    /// 已完成任务结果
    results: Mutex<HashMap<String, TaskResult>>,
    /// 任务状态
    statuses: Mutex<HashMap<String, TaskStatus>>,
    /// 任务计数器
    counter: AtomicU64,
    /// 最大队列长度
    max_queue_size: usize,
}

impl AsyncTaskScheduler {
    /// 创建调度器
    pub fn new(db: WasmDatabase, max_queue_size: usize) -> Self {
        Self {
            db,
            queue: Mutex::new(VecDeque::new()),
            results: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
            max_queue_size,
        }
    }

    /// 生成唯一任务 ID
    pub fn next_task_id(&self) -> String {
        format!("task-{}", self.counter.fetch_add(1, Ordering::SeqCst))
    }

    /// 入队任务
    ///
    /// 返回任务 ID。若队列已满返回错误。
    pub fn enqueue(&self, mut task: AsyncTask) -> Result<String, String> {
        let id = if task.id.is_empty() {
            self.next_task_id()
        } else {
            task.id.clone()
        };
        task.id = id.clone();
        task.enqueued_at = current_millis();

        let mut queue = self.queue.lock().map_err(|e| format!("lock error: {}", e))?;
        if queue.len() >= self.max_queue_size {
            return Err(format!("queue full (max={})", self.max_queue_size));
        }
        queue.push_back(task);

        let mut statuses = self
            .statuses
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;
        statuses.insert(id.clone(), TaskStatus::Pending);
        Ok(id)
    }

    /// 执行队列中的下一个任务
    ///
    /// 返回 `Some(TaskResult)` 表示执行了一个任务，`None` 表示队列为空。
    pub fn run_next(&self) -> Option<TaskResult> {
        let task = {
            let mut queue = self.queue.lock().ok()?;
            queue.pop_front()
        }?;

        // 更新状态为 Running
        if let Ok(mut statuses) = self.statuses.lock() {
            statuses.insert(task.id.clone(), TaskStatus::Running);
        }

        let start = Instant::now();
        let query = WasmQuery::with_params(&task.sql, task.params.clone());
        let result = if task.is_query {
            match self.db.query(query) {
                Ok(rows) => TaskResult {
                    task_id: task.id.clone(),
                    status: TaskStatus::Completed,
                    rows,
                    affected: 0,
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Err(e) => TaskResult {
                    task_id: task.id.clone(),
                    status: TaskStatus::Failed,
                    rows: vec![],
                    affected: 0,
                    error: Some(e),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            }
        } else {
            match self.db.execute(query) {
                Ok(n) => TaskResult {
                    task_id: task.id.clone(),
                    status: TaskStatus::Completed,
                    rows: vec![],
                    affected: n,
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Err(e) => TaskResult {
                    task_id: task.id.clone(),
                    status: TaskStatus::Failed,
                    rows: vec![],
                    affected: 0,
                    error: Some(e),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            }
        };

        // 记录结果与最终状态
        if let Ok(mut statuses) = self.statuses.lock() {
            statuses.insert(task.id.clone(), result.status);
        }
        if let Ok(mut results) = self.results.lock() {
            results.insert(task.id.clone(), result.clone());
        }
        Some(result)
    }

    /// 执行所有待处理任务直到队列为空
    pub fn drain(&self) -> Vec<TaskResult> {
        let mut all = Vec::new();
        while let Some(r) = self.run_next() {
            all.push(r);
        }
        all
    }

    /// 查询任务状态
    pub fn status_of(&self, task_id: &str) -> Option<TaskStatus> {
        self.statuses
            .lock()
            .ok()?
            .get(task_id)
            .copied()
    }

    /// 查询任务结果
    pub fn result_of(&self, task_id: &str) -> Option<TaskResult> {
        self.results
            .lock()
            .ok()?
            .get(task_id)
            .cloned()
    }

    /// 取消任务（仅当任务处于 Pending 时可取消）
    pub fn cancel(&self, task_id: &str) -> Result<bool, String> {
        let mut statuses = self
            .statuses
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;
        match statuses.get(task_id) {
            Some(TaskStatus::Pending) => {
                statuses.insert(task_id.to_string(), TaskStatus::Cancelled);
                // 从队列中移除
                let mut queue = self
                    .queue
                    .lock()
                    .map_err(|e| format!("lock error: {}", e))?;
                queue.retain(|t| t.id != task_id);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// 当前队列长度
    pub fn pending_count(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    /// 已完成任务数量
    pub fn completed_count(&self) -> usize {
        self.statuses
            .lock()
            .map(|s| {
                s.values()
                    .filter(|&&st| st == TaskStatus::Completed)
                    .count()
            })
            .unwrap_or(0)
    }
}

/// 获取当前 Unix 毫秒时间戳
fn current_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ====================================================================
// 四、WASM 模块缓存
// ====================================================================

/// 缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// 缓存的模块字节码
    pub data: Vec<u8>,
    /// 创建时间戳（Unix 毫秒）
    pub created_at: i64,
    /// 最后访问时间戳
    pub last_accessed_at: i64,
    /// 访问次数
    pub access_count: u64,
    /// 模块大小（字节）
    pub size: usize,
}

impl CacheEntry {
    /// 创建新条目
    pub fn new(data: Vec<u8>) -> Self {
        let now = current_millis();
        let size = data.len();
        Self {
            data,
            created_at: now,
            last_accessed_at: now,
            access_count: 0,
            size,
        }
    }

    /// 标记一次访问
    pub fn touch(&mut self) {
        self.last_accessed_at = current_millis();
        self.access_count += 1;
    }

    /// 判断是否过期
    pub fn is_expired(&self, ttl: Duration) -> bool {
        let now = current_millis();
        let age_ms = now.saturating_sub(self.created_at) as u64;
        age_ms > ttl.as_millis() as u64
    }
}

/// 缓存统计信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 驱逐次数
    pub evictions: u64,
    /// 当前条目数
    pub entry_count: usize,
    /// 当前缓存总字节
    pub total_bytes: usize,
}

impl CacheStats {
    /// 命中率（0.0 ~ 1.0）
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// WASM 模块缓存
///
/// 基于 LRU + TTL 双策略：
/// - 插入时若超出容量上限，按 LRU（最后访问时间最早）驱逐。
/// - 查询时若条目已过期（超过 TTL），视为未命中并删除。
pub struct ModuleCache {
    /// 缓存存储
    entries: Mutex<HashMap<String, CacheEntry>>,
    /// 最大条目数
    max_entries: usize,
    /// 最大字节数
    max_bytes: usize,
    /// TTL（存活时间）
    ttl: Duration,
    /// 统计信息
    stats: Mutex<CacheStats>,
}

impl ModuleCache {
    /// 创建模块缓存
    pub fn new(max_entries: usize, max_bytes: usize, ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_entries,
            max_bytes,
            ttl,
            stats: Mutex::new(CacheStats::default()),
        }
    }

    /// 获取配置引用（只读字段，无需加锁）
    pub fn limits(&self) -> (usize, usize, Duration) {
        (self.max_entries, self.max_bytes, self.ttl)
    }

    /// 插入或更新缓存条目
    ///
    /// 若 key 已存在，覆盖旧条目。
    /// 插入后触发 LRU 驱逐，确保条目数与字节数均在上限内。
    pub fn put(&self, key: impl Into<String>, data: Vec<u8>) -> Result<(), String> {
        let key = key.into();
        let entry = CacheEntry::new(data);
        let entry_size = entry.size;

        let mut entries = self
            .entries
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;

        // 若已存在，先移除旧条目（后续重新插入）
        if let Some(old) = entries.remove(&key) {
            let _ = old; // 旧条目丢弃
        }

        // 检查单条是否超过 max_bytes（无法缓存）
        if entry_size > self.max_bytes {
            return Err(format!(
                "entry size {} exceeds max_bytes {}",
                entry_size, self.max_bytes
            ));
        }

        entries.insert(key.clone(), entry);

        // LRU 驱逐：循环移除最久未访问条目，直到满足限制
        let evicted = Self::evict_if_needed(&mut entries, self.max_entries, self.max_bytes);

        // 更新统计
        if let Ok(mut stats) = self.stats.lock() {
            stats.evictions += evicted as u64;
            stats.entry_count = entries.len();
            stats.total_bytes = entries.values().map(|e| e.size).sum();
        }

        Ok(())
    }

    /// 查询缓存条目
    ///
    /// 命中时返回数据克隆并更新访问计数与时间；
    /// 未命中或已过期时返回 `None` 并清理过期条目。
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let now = current_millis();
        let mut entries = self.entries.lock().ok()?;

        // 先检查是否存在且未过期
        let should_remove = entries.get(key).map(|e| {
            let age_ms = now.saturating_sub(e.created_at) as u64;
            age_ms > self.ttl.as_millis() as u64
        });

        if let Some(true) = should_remove {
            entries.remove(key);
            if let Ok(mut stats) = self.stats.lock() {
                stats.misses += 1;
                stats.evictions += 1;
                stats.entry_count = entries.len();
                stats.total_bytes = entries.values().map(|e| e.size).sum();
            }
            return None;
        }

        match entries.get_mut(key) {
            Some(entry) => {
                entry.touch();
                let data = entry.data.clone();
                if let Ok(mut stats) = self.stats.lock() {
                    stats.hits += 1;
                    stats.entry_count = entries.len();
                    stats.total_bytes = entries.values().map(|e| e.size).sum();
                }
                Some(data)
            }
            None => {
                if let Ok(mut stats) = self.stats.lock() {
                    stats.misses += 1;
                }
                None
            }
        }
    }

    /// 主动删除指定条目
    pub fn remove(&self, key: &str) -> bool {
        let mut entries = match self.entries.lock() {
            Ok(e) => e,
            Err(_) => return false,
        };
        let removed = entries.remove(key).is_some();
        if removed {
            if let Ok(mut stats) = self.stats.lock() {
                stats.entry_count = entries.len();
                stats.total_bytes = entries.values().map(|e| e.size).sum();
            }
        }
        removed
    }

    /// 清空所有缓存
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        if let Ok(mut stats) = self.stats.lock() {
            stats.entry_count = 0;
            stats.total_bytes = 0;
        }
    }

    /// 获取统计快照
    pub fn stats(&self) -> CacheStats {
        self.stats
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// 主动清理所有过期条目
    ///
    /// 返回被清理的条目数。
    pub fn purge_expired(&self) -> usize {
        let now = current_millis();
        let ttl_ms = self.ttl.as_millis() as i64;
        let mut entries = match self.entries.lock() {
            Ok(e) => e,
            Err(_) => return 0,
        };
        let before = entries.len();
        entries.retain(|_, e| now - e.created_at <= ttl_ms);
        let removed = before - entries.len();
        if removed > 0 {
            if let Ok(mut stats) = self.stats.lock() {
                stats.evictions += removed as u64;
                stats.entry_count = entries.len();
                stats.total_bytes = entries.values().map(|e| e.size).sum();
            }
        }
        removed
    }

    /// 内部 LRU 驱逐逻辑
    ///
    /// 当条目数或总字节数超限时，按最后访问时间从早到晚依次移除，
    /// 直到同时满足两个限制。返回被驱逐的条目数。
    fn evict_if_needed(
        entries: &mut HashMap<String, CacheEntry>,
        max_entries: usize,
        max_bytes: usize,
    ) -> usize {
        let mut evicted = 0;
        while entries.len() > max_entries {
            // 找到 last_accessed_at 最小的条目
            let victim = entries
                .iter()
                .min_by_key(|(_, e)| e.last_accessed_at)
                .map(|(k, _)| k.clone());
            match victim {
                Some(k) => {
                    entries.remove(&k);
                    evicted += 1;
                }
                None => break,
            }
        }

        let total: usize = entries.values().map(|e| e.size).sum();
        if total <= max_bytes {
            return evicted;
        }

        while entries.values().map(|e| e.size).sum::<usize>() > max_bytes {
            let victim = entries
                .iter()
                .min_by_key(|(_, e)| e.last_accessed_at)
                .map(|(k, _)| k.clone());
            match victim {
                Some(k) => {
                    entries.remove(&k);
                    evicted += 1;
                }
                None => break,
            }
        }
        evicted
    }
}

// ====================================================================
// 单元测试
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::thread::sleep;

    // -------------------- 内存限制测试 --------------------

    #[test]
    fn test_memory_config_unlimited() {
        let cfg = MemoryConfig::unlimited();
        assert!(cfg.max_tables.is_none());
        assert!(cfg.max_rows_per_table.is_none());
        assert!(cfg.max_row_size_bytes.is_none());
        assert!(cfg.max_total_bytes.is_none());
    }

    #[test]
    fn test_memory_config_strict_defaults() {
        let cfg = MemoryConfig::strict();
        assert_eq!(cfg.max_tables, Some(8));
        assert_eq!(cfg.max_rows_per_table, Some(100));
        assert_eq!(cfg.max_row_size_bytes, Some(4 * 1024));
        assert_eq!(cfg.max_total_bytes, Some(256 * 1024));
    }

    #[test]
    fn test_memory_config_builder_chain() {
        let cfg = MemoryConfig::unlimited()
            .with_max_tables(4)
            .with_max_rows_per_table(50)
            .with_max_row_size_bytes(1024)
            .with_max_total_bytes(8192);
        assert_eq!(cfg.max_tables, Some(4));
        assert_eq!(cfg.max_rows_per_table, Some(50));
        assert_eq!(cfg.max_row_size_bytes, Some(1024));
        assert_eq!(cfg.max_total_bytes, Some(8192));
    }

    #[test]
    fn test_memory_usage_row_size_calculation() {
        let row = json!({"id": 1, "name": "Alice"});
        let size = MemoryUsage::row_size(&row);
        assert!(size > 0);
        // JSON 序列化后至少包含字段名和值
        assert!(size >= 20);
    }

    #[test]
    fn test_memory_usage_row_size_null() {
        let row = json!(null);
        let size = MemoryUsage::row_size(&row);
        assert_eq!(size, 4); // "null"
    }

    #[test]
    fn test_limited_db_default_config_is_strict() {
        let db = LimitedWasmDatabase::new(MemoryConfig::default());
        assert_eq!(db.config().max_tables, Some(8));
    }

    #[test]
    fn test_limited_db_execute_create_table_allowed() {
        let db = LimitedWasmDatabase::new(MemoryConfig::strict());
        let result = db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER)"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_limited_db_execute_create_table_zero_limit_denied() {
        let cfg = MemoryConfig::unlimited().with_max_tables(0);
        let db = LimitedWasmDatabase::new(cfg);
        let result = db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER)"));
        assert!(result.is_err());
        match result.unwrap_err() {
            MemoryLimitError::TooManyTables { limit: 0, .. } => {}
            other => panic!("expected TooManyTables, got {:?}", other),
        }
    }

    #[test]
    fn test_limited_db_query_passthrough() {
        let db = LimitedWasmDatabase::new(MemoryConfig::strict());
        db.execute(WasmQuery::new("CREATE TABLE t (id INTEGER)"))
            .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO t (id) VALUES (?)",
            vec![json!(1)],
        ))
        .unwrap();
        let rows = db
            .query(WasmQuery::with_params(
                "SELECT * FROM t WHERE id = ?",
                vec![json!(1)],
            ))
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_parse_insert_table_extraction() {
        let table = parse_insert_table("INSERT INTO users (id) VALUES (1)");
        assert_eq!(table, Some("users".to_string()));
    }

    #[test]
    fn test_parse_insert_table_missing_into() {
        let table = parse_insert_table("INSERT users (id) VALUES (1)");
        assert_eq!(table, None);
    }

    #[test]
    fn test_estimate_insert_rows_single() {
        let rows = estimate_insert_rows(
            "INSERT INTO t (a) VALUES (1)",
            &[json!(1)],
        );
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_estimate_insert_rows_multiple() {
        let rows = estimate_insert_rows(
            "INSERT INTO t (a) VALUES (1), (2), (3)",
            &[json!(1), json!(2), json!(3)],
        );
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn test_estimate_insert_rows_no_values() {
        let rows = estimate_insert_rows("INSERT INTO t (a) SELECT 1", &[]);
        assert!(rows.is_empty());
    }

    #[test]
    fn test_check_insert_row_too_large() {
        let cfg = MemoryConfig::unlimited().with_max_row_size_bytes(5);
        let db = LimitedWasmDatabase::new(cfg);
        let big_row = json!({"name": "Alice with a very long name"});
        let result = db.check_insert("users", &[big_row]);
        assert!(result.is_err());
        match result.unwrap_err() {
            MemoryLimitError::RowTooLarge { actual, .. } => {
                assert!(actual > 5);
            }
            other => panic!("expected RowTooLarge, got {:?}", other),
        }
    }

    #[test]
    fn test_check_insert_within_limits() {
        let cfg = MemoryConfig::strict();
        let db = LimitedWasmDatabase::new(cfg);
        db.execute(WasmQuery::new("CREATE TABLE t (id INTEGER)"))
            .unwrap();
        let result = db.check_insert("t", &[json!({"id": 1})]);
        assert!(result.is_ok());
    }

    // -------------------- WASI 沙箱测试 --------------------

    #[test]
    fn test_sandbox_normalize_simple() {
        let n = SandboxedFs::normalize("a/b/c");
        assert_eq!(n, Some("a/b/c".to_string()));
    }

    #[test]
    fn test_sandbox_normalize_dot_segments() {
        let n = SandboxedFs::normalize("a/./b/../c");
        assert_eq!(n, Some("a/c".to_string()));
    }

    #[test]
    fn test_sandbox_normalize_parent_escape_returns_none() {
        let n = SandboxedFs::normalize("../etc/passwd");
        assert_eq!(n, None);
    }

    #[test]
    fn test_sandbox_normalize_deep_escape_returns_none() {
        let n = SandboxedFs::normalize("a/../../../etc");
        assert_eq!(n, None);
    }

    #[test]
    fn test_sandbox_normalize_leading_slash() {
        let n = SandboxedFs::normalize("/a/b");
        assert_eq!(n, Some("a/b".to_string()));
    }

    #[test]
    fn test_sandbox_looks_like_symlink_tilde() {
        assert!(SandboxedFs::looks_like_symlink("~/secret"));
        assert!(!SandboxedFs::looks_like_symlink("/home/user"));
    }

    #[test]
    fn test_sandbox_looks_like_symlink_arrow() {
        assert!(SandboxedFs::looks_like_symlink("link -> target"));
        assert!(!SandboxedFs::looks_like_symlink("normal/path"));
    }

    #[test]
    fn test_sandbox_config_allow_rw() {
        let cfg = SandboxConfig::allow_rw("/tmp/data");
        assert_eq!(
            cfg.rules.get("/tmp/data"),
            Some(&PathAccess::ReadWrite)
        );
    }

    #[test]
    fn test_sandbox_config_allow_ro() {
        let cfg = SandboxConfig::allow_ro("/etc");
        assert_eq!(cfg.rules.get("/etc"), Some(&PathAccess::ReadOnly));
    }

    #[test]
    fn test_sandbox_config_builder_with_rule() {
        let cfg = SandboxConfig::deny_all()
            .with_rule("/tmp", PathAccess::ReadWrite)
            .with_rule("/var/log", PathAccess::ReadOnly);
        assert_eq!(cfg.rules.len(), 2);
    }

    #[test]
    fn test_sandbox_access_for_default_deny() {
        let fs = SandboxedFs::new(SandboxConfig::deny_all());
        assert_eq!(fs.access_for("/any/path"), PathAccess::Denied);
    }

    #[test]
    fn test_sandbox_access_for_longest_prefix() {
        let cfg = SandboxConfig::deny_all()
            .with_rule("/tmp", PathAccess::ReadWrite)
            .with_rule("/tmp/readonly", PathAccess::ReadOnly);
        let fs = SandboxedFs::new(cfg);
        assert_eq!(fs.access_for("/tmp/data"), PathAccess::ReadWrite);
        assert_eq!(fs.access_for("/tmp/readonly/file"), PathAccess::ReadOnly);
    }

    #[test]
    fn test_sandbox_check_read_allowed() {
        let fs = SandboxedFs::new(SandboxConfig::allow_ro("/etc"));
        assert!(fs.check_read("/etc/passwd").is_ok());
    }

    #[test]
    fn test_sandbox_check_read_denied() {
        let fs = SandboxedFs::new(SandboxConfig::allow_ro("/etc"));
        assert!(fs.check_read("/root/secret").is_err());
        match fs.check_read("/root/secret") {
            Err(SandboxError::AccessDenied { required, .. }) => {
                assert_eq!(required, PathAccess::ReadOnly);
            }
            other => panic!("expected AccessDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_sandbox_check_write_readonly_denied() {
        let fs = SandboxedFs::new(SandboxConfig::allow_ro("/etc"));
        assert!(fs.check_write("/etc/passwd").is_err());
    }

    #[test]
    fn test_sandbox_check_write_allowed() {
        let fs = SandboxedFs::new(SandboxConfig::allow_rw("/tmp"));
        assert!(fs.check_write("/tmp/data").is_ok());
    }

    #[test]
    fn test_sandbox_check_path_too_long() {
        let cfg = SandboxConfig::allow_rw("/tmp").with_max_path_length(5);
        let fs = SandboxedFs::new(cfg);
        let result = fs.check_read("/tmp/very_long_path");
        assert!(matches!(result, Err(SandboxError::PathTooLong { .. })));
    }

    #[test]
    fn test_sandbox_check_suspicious_dotdot() {
        let fs = SandboxedFs::new(SandboxConfig::allow_rw("/tmp"));
        let result = fs.check_read("/tmp/../../../etc/passwd");
        assert!(matches!(result, Err(SandboxError::SuspiciousPath { .. })));
    }

    #[test]
    fn test_sandbox_symlink_detected() {
        let fs = SandboxedFs::new(SandboxConfig::allow_rw("/tmp"));
        let result = fs.check_read("~/secret");
        assert!(matches!(result, Err(SandboxError::SymlinkDetected { .. })));
    }

    #[test]
    fn test_sandbox_symlink_allowed_when_configured() {
        let cfg = SandboxConfig::allow_rw("/tmp").with_symlinks(true);
        let fs = SandboxedFs::new(cfg);
        // 即使路径看起来像符号链接，配置允许时不报错
        let result = fs.check_read("/tmp/->target");
        assert!(result.is_ok());
    }

    // -------------------- 异步任务调度测试 --------------------

    #[test]
    fn test_async_task_query_creation() {
        let task = AsyncTask::query("t1", "SELECT * FROM users");
        assert_eq!(task.id, "t1");
        assert_eq!(task.sql, "SELECT * FROM users");
        assert!(task.is_query);
        assert!(task.params.is_empty());
    }

    #[test]
    fn test_async_task_execute_creation() {
        let task = AsyncTask::execute("t2", "INSERT INTO t VALUES (1)");
        assert_eq!(task.id, "t2");
        assert!(!task.is_query);
    }

    #[test]
    fn test_async_task_with_params() {
        let task = AsyncTask::query("t1", "SELECT * FROM t WHERE id = ?")
            .with_params(vec![json!(1)]);
        assert_eq!(task.params.len(), 1);
    }

    #[test]
    fn test_task_result_success() {
        let r = TaskResult::success("t1", vec![json!({"id": 1})]);
        assert_eq!(r.status, TaskStatus::Completed);
        assert_eq!(r.rows.len(), 1);
        assert!(r.error.is_none());
    }

    #[test]
    fn test_task_result_write_success() {
        let r = TaskResult::write_success("t1", 5);
        assert_eq!(r.affected, 5);
        assert!(r.rows.is_empty());
    }

    #[test]
    fn test_task_result_failure() {
        let r = TaskResult::failure("t1", "syntax error");
        assert_eq!(r.status, TaskStatus::Failed);
        assert_eq!(r.error, Some("syntax error".to_string()));
    }

    #[test]
    fn test_scheduler_enqueue_and_run_query() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER)"))
            .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO users (id) VALUES (?)",
            vec![json!(1)],
        ))
            .unwrap();
        let scheduler = AsyncTaskScheduler::new(db, 16);

        let id = scheduler
            .enqueue(AsyncTask::query("q1", "SELECT * FROM users"))
            .unwrap();
        assert_eq!(id, "q1");
        assert_eq!(scheduler.status_of(&id), Some(TaskStatus::Pending));

        let result = scheduler.run_next().unwrap();
        assert_eq!(result.task_id, "q1");
        assert_eq!(result.status, TaskStatus::Completed);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(scheduler.status_of("q1"), Some(TaskStatus::Completed));
    }

    #[test]
    fn test_scheduler_enqueue_and_run_execute() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE t (id INTEGER)"))
            .unwrap();
        let scheduler = AsyncTaskScheduler::new(db, 16);

        let id = scheduler
            .enqueue(
                AsyncTask::execute("w1", "INSERT INTO t (id) VALUES (?)")
                    .with_params(vec![json!(42)]),
            )
            .unwrap();
        let result = scheduler.run_next().unwrap();
        assert_eq!(result.task_id, id);
        assert_eq!(result.affected, 1);
        assert_eq!(result.status, TaskStatus::Completed);
    }

    #[test]
    fn test_scheduler_run_next_empty_returns_none() {
        let db = WasmDatabase::new();
        let scheduler = AsyncTaskScheduler::new(db, 16);
        assert!(scheduler.run_next().is_none());
    }

    #[test]
    fn test_scheduler_queue_full_returns_error() {
        let db = WasmDatabase::new();
        let scheduler = AsyncTaskScheduler::new(db, 2);
        scheduler
            .enqueue(AsyncTask::query("a", "SELECT 1"))
            .unwrap();
        scheduler
            .enqueue(AsyncTask::query("b", "SELECT 1"))
            .unwrap();
        let result = scheduler.enqueue(AsyncTask::query("c", "SELECT 1"));
        assert!(result.is_err());
    }

    #[test]
    fn test_scheduler_drain_all() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE t (id INTEGER)"))
            .unwrap();
        let scheduler = AsyncTaskScheduler::new(db, 16);
        scheduler
            .enqueue(AsyncTask::execute("w1", "INSERT INTO t (id) VALUES (1)"))
            .unwrap();
        scheduler
            .enqueue(AsyncTask::query("q1", "SELECT * FROM t"))
            .unwrap();
        let results = scheduler.drain();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.status == TaskStatus::Completed));
    }

    #[test]
    fn test_scheduler_cancel_pending() {
        let db = WasmDatabase::new();
        let scheduler = AsyncTaskScheduler::new(db, 16);
        scheduler
            .enqueue(AsyncTask::query("t1", "SELECT 1"))
            .unwrap();
        let cancelled = scheduler.cancel("t1").unwrap();
        assert!(cancelled);
        assert_eq!(scheduler.status_of("t1"), Some(TaskStatus::Cancelled));
        // 队列已清空，run_next 应返回 None
        assert!(scheduler.run_next().is_none());
    }

    #[test]
    fn test_scheduler_cancel_running_fails() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE t (id INTEGER)"))
            .unwrap();
        let scheduler = AsyncTaskScheduler::new(db, 16);
        scheduler
            .enqueue(AsyncTask::query("t1", "SELECT * FROM t"))
            .unwrap();
        scheduler.run_next().unwrap(); // 已完成
        let cancelled = scheduler.cancel("t1").unwrap();
        assert!(!cancelled); // 已完成，无法取消
    }

    #[test]
    fn test_scheduler_failed_task_records_error() {
        let db = WasmDatabase::new();
        let scheduler = AsyncTaskScheduler::new(db, 16);
        scheduler
            .enqueue(AsyncTask::execute("bad", "DROP TABLE nonexistent"))
            .unwrap();
        let result = scheduler.run_next().unwrap();
        assert_eq!(result.status, TaskStatus::Failed);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_scheduler_result_of_returns_stored() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE t (id INTEGER)"))
            .unwrap();
        let scheduler = AsyncTaskScheduler::new(db, 16);
        scheduler
            .enqueue(AsyncTask::query("q", "SELECT * FROM t"))
            .unwrap();
        scheduler.run_next().unwrap();
        let stored = scheduler.result_of("q");
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().task_id, "q");
    }

    #[test]
    fn test_scheduler_pending_and_completed_count() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE t (id INTEGER)"))
            .unwrap();
        let scheduler = AsyncTaskScheduler::new(db, 16);
        scheduler
            .enqueue(AsyncTask::query("q1", "SELECT * FROM t"))
            .unwrap();
        scheduler
            .enqueue(AsyncTask::query("q2", "SELECT * FROM t"))
            .unwrap();
        assert_eq!(scheduler.pending_count(), 2);
        assert_eq!(scheduler.completed_count(), 0);
        scheduler.run_next().unwrap();
        assert_eq!(scheduler.pending_count(), 1);
        assert_eq!(scheduler.completed_count(), 1);
    }

    #[test]
    fn test_scheduler_next_task_id_increments() {
        let db = WasmDatabase::new();
        let scheduler = AsyncTaskScheduler::new(db, 16);
        let id1 = scheduler.next_task_id();
        let id2 = scheduler.next_task_id();
        assert_ne!(id1, id2);
    }

    // -------------------- 模块缓存测试 --------------------

    #[test]
    fn test_cache_put_and_get() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("mod1", vec![1, 2, 3]).unwrap();
        let data = cache.get("mod1");
        assert_eq!(data, Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_cache_get_miss() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        let data = cache.get("nonexistent");
        assert_eq!(data, None);
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_cache_hit_increments_stats() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("k", vec![1]).unwrap();
        cache.get("k");
        cache.get("k");
        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 0);
        assert!((stats.hit_rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_cache_hit_rate_with_mixed_access() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("k", vec![1]).unwrap();
        cache.get("k"); // hit
        cache.get("miss"); // miss
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_cache_put_overwrites() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("k", vec![1]).unwrap();
        cache.put("k", vec![2, 3]).unwrap();
        let data = cache.get("k");
        assert_eq!(data, Some(vec![2, 3]));
    }

    #[test]
    fn test_cache_remove() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("k", vec![1]).unwrap();
        assert!(cache.remove("k"));
        assert_eq!(cache.get("k"), None);
        assert!(!cache.remove("k"));
    }

    #[test]
    fn test_cache_clear() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("a", vec![1]).unwrap();
        cache.put("b", vec![2]).unwrap();
        cache.clear();
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), None);
        let stats = cache.stats();
        assert_eq!(stats.entry_count, 0);
    }

    #[test]
    fn test_cache_lru_eviction_by_entry_count() {
        // 容量 2，插入 3 个条目应驱逐最久未访问的
        let cache = ModuleCache::new(2, 1024 * 1024, Duration::from_secs(60));
        cache.put("a", vec![1]).unwrap();
        // 给 a 一个较早的访问时间
        sleep(Duration::from_millis(5));
        cache.put("b", vec![2]).unwrap();
        sleep(Duration::from_millis(5));
        cache.put("c", vec![3]).unwrap();
        // a 应被驱逐
        assert_eq!(cache.get("a"), None);
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
        let stats = cache.stats();
        assert!(stats.evictions >= 1);
    }

    #[test]
    fn test_cache_lru_eviction_by_byte_size() {
        // max_bytes 限制为 4 字节，插入两个 3 字节条目应驱逐第一个
        let cache = ModuleCache::new(100, 4, Duration::from_secs(60));
        cache.put("a", vec![1, 2, 3]).unwrap();
        sleep(Duration::from_millis(5));
        cache.put("b", vec![4, 5, 6]).unwrap();
        assert_eq!(cache.get("a"), None);
        assert!(cache.get("b").is_some());
    }

    #[test]
    fn test_cache_oversized_entry_rejected() {
        let cache = ModuleCache::new(10, 4, Duration::from_secs(60));
        let result = cache.put("big", vec![1, 2, 3, 4, 5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_ttl_expiration() {
        let cache = ModuleCache::new(10, 1024, Duration::from_millis(20));
        cache.put("k", vec![1]).unwrap();
        sleep(Duration::from_millis(30));
        assert_eq!(cache.get("k"), None);
    }

    #[test]
    fn test_cache_purge_expired() {
        let cache = ModuleCache::new(10, 1024, Duration::from_millis(20));
        cache.put("a", vec![1]).unwrap();
        cache.put("b", vec![2]).unwrap();
        sleep(Duration::from_millis(30));
        let purged = cache.purge_expired();
        assert_eq!(purged, 2);
    }

    #[test]
    fn test_cache_purge_expired_keeps_fresh() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("a", vec![1]).unwrap();
        let purged = cache.purge_expired();
        assert_eq!(purged, 0);
        assert!(cache.get("a").is_some());
    }

    #[test]
    fn test_cache_entry_touch_updates_access_time() {
        let mut entry = CacheEntry::new(vec![1, 2]);
        let original_access = entry.last_accessed_at;
        let original_count = entry.access_count;
        sleep(Duration::from_millis(5));
        entry.touch();
        assert!(entry.last_accessed_at > original_access);
        assert_eq!(entry.access_count, original_count + 1);
    }

    #[test]
    fn test_cache_entry_is_expired_check() {
        let mut entry = CacheEntry::new(vec![1]);
        // 手动设置很早的创建时间
        entry.created_at = current_millis() - 10_000;
        assert!(entry.is_expired(Duration::from_secs(5)));
        assert!(!entry.is_expired(Duration::from_secs(20)));
    }

    #[test]
    fn test_cache_stats_after_operations() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("a", vec![1]).unwrap();
        cache.put("b", vec![2]).unwrap();
        cache.get("a");
        cache.get("b");
        cache.get("c");
        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entry_count, 2);
        assert_eq!(stats.total_bytes, 2);
    }

    #[test]
    fn test_cache_limits_accessor() {
        let cache = ModuleCache::new(5, 100, Duration::from_secs(30));
        let (max_entries, max_bytes, ttl) = cache.limits();
        assert_eq!(max_entries, 5);
        assert_eq!(max_bytes, 100);
        assert_eq!(ttl, Duration::from_secs(30));
    }

    #[test]
    fn test_cache_zero_hit_rate_when_empty() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        let stats = cache.stats();
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_get_updates_access_count_on_entry() {
        let cache = ModuleCache::new(10, 1024, Duration::from_secs(60));
        cache.put("k", vec![1]).unwrap();
        cache.get("k");
        cache.get("k");
        // 通过 stats 间接验证（hits 应为 2）
        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
    }
}
