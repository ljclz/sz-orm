//! # Saga 模式（协调式 Orchestration）
//!
//! Saga 模式将一个分布式长事务拆分为一系列本地事务（步骤），每个步骤都有对应的补偿操作。
//! 如果某个步骤失败，协调器会按反向顺序执行已成功步骤的补偿操作，最终达到最终一致性。
//!
//! 与 2PC（两阶段提交）相比：
//! - 2PC：强一致性，所有参与者同时提交或回滚；需要分布式锁，性能较差
//! - Saga：最终一致性，每个步骤立即提交；失败时通过补偿回滚；性能好，适合长事务
//!
//! # 适用场景
//!
//! - 电商订单：创建订单 → 扣库存 → 扣余额 → 发货
//! - 旅行预订：订机票 → 订酒店 → 租车
//! - 资金转账：扣款 → 加款
//!
//! # 快速入门
//!
//! ```rust
//! use sz_orm_dtx::saga::{Saga, SagaStep, SagaState};
//! use std::sync::Arc;
//! use std::sync::atomic::{AtomicU32, Ordering};
//!
//! // 一个简单的计数器作为状态记录
//! let counter = Arc::new(AtomicU32::new(0));
//! let c1 = counter.clone();
//! let c2 = counter.clone();
//! let c1r = counter.clone();
//! let c2r = counter.clone();
//!
//! let mut saga = Saga::new("order-create");
//! saga.add_step(SagaStep::new("step1")
//!     .with_action(move || { c1.fetch_add(1, Ordering::SeqCst); Ok(()) })
//!     .with_compensation(move || { c1r.fetch_sub(1, Ordering::SeqCst); Ok(()) })).unwrap();
//! saga.add_step(SagaStep::new("step2")
//!     .with_action(move || { c2.fetch_add(1, Ordering::SeqCst); Ok(()) })
//!     .with_compensation(move || { c2r.fetch_sub(1, Ordering::SeqCst); Ok(()) })).unwrap();
//!
//! saga.execute().unwrap();
//! assert_eq!(counter.load(Ordering::SeqCst), 2);
//! assert_eq!(saga.state(), SagaState::Completed);
//! ```

use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Saga 步骤的动作回调类型
pub type SagaAction = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// Saga 步骤的补偿回调类型
pub type SagaCompensation = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

// =====================================================================
// Saga 日志（用于状态机持久化与故障恢复）
// =====================================================================

/// Saga 日志动作类型
///
/// 记录 Saga 执行过程中的关键事件，用于故障恢复时重建状态机。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SagaLogAction {
    /// 步骤开始执行
    StepStarted,
    /// 步骤执行成功
    StepCompleted,
    /// 步骤执行失败
    StepFailed,
    /// 补偿操作开始
    CompensationStarted,
    /// 补偿操作成功
    CompensationCompleted,
    /// 补偿操作失败
    CompensationFailed,
    /// Saga 全部完成
    SagaCompleted,
    /// Saga 补偿全部完成
    SagaCompensated,
}

/// Saga 日志条目
///
/// 每条日志记录 Saga 的一个状态变更事件，可序列化用于持久化存储。
/// 故障恢复时通过读取日志重建 Saga 状态机。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaLogEntry {
    /// 所属 Saga ID
    pub saga_id: String,
    /// 步骤名称
    pub step_name: String,
    /// 步骤索引（从 0 开始）
    pub step_index: usize,
    /// 日志动作
    pub action: SagaLogAction,
    /// 时间戳（Unix 毫秒）
    pub timestamp: i64,
    /// 附加载荷（如错误信息）
    pub payload: Option<String>,
}

/// Saga 日志 trait
///
/// 抽象日志的追加与读取操作。实现方可以是内存存储、文件存储或数据库存储。
/// 用于 Saga 状态机持久化，支持故障后从日志恢复。
pub trait SagaLog: Send + Sync {
    /// 追加一条日志
    fn append(&self, entry: SagaLogEntry) -> Result<(), String>;
    /// 读取指定 Saga 的所有日志（按追加顺序）
    fn read_all(&self, saga_id: &str) -> Result<Vec<SagaLogEntry>, String>;
}

/// 内存实现的 Saga 日志
///
/// 使用 `RwLock<Vec<SagaLogEntry>>` 存储，适用于测试和单机场景。
/// 生产环境应替换为持久化存储实现。
pub struct InMemorySagaLog {
    entries: RwLock<Vec<SagaLogEntry>>,
}

impl InMemorySagaLog {
    /// 创建空的内存日志
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemorySagaLog {
    fn default() -> Self {
        Self::new()
    }
}

impl SagaLog for InMemorySagaLog {
    fn append(&self, entry: SagaLogEntry) -> Result<(), String> {
        let mut entries = self
            .entries
            .write()
            .map_err(|e| format!("日志锁中毒: {}", e))?;
        entries.push(entry);
        Ok(())
    }

    fn read_all(&self, saga_id: &str) -> Result<Vec<SagaLogEntry>, String> {
        let entries = self
            .entries
            .read()
            .map_err(|e| format!("日志锁中毒: {}", e))?;
        Ok(entries
            .iter()
            .filter(|e| e.saga_id == saga_id)
            .cloned()
            .collect())
    }
}

impl std::fmt::Debug for InMemorySagaLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.entries.read().map(|e| e.len()).unwrap_or(0);
        f.debug_struct("InMemorySagaLog")
            .field("entries_count", &count)
            .finish()
    }
}

// =====================================================================
// SagaTimeout — Saga 超时配置
// =====================================================================

/// Saga 超时配置
///
/// 控制 Saga 执行的时间限制：
/// - `step_timeout`：单个步骤的最大执行时间，超时后触发补偿
/// - `total_timeout`：整个 Saga 的最大执行时间，超时后触发补偿
///
/// # 重要限制
///
/// 因为 Saga 的 action 是同步闭包，无法真正中断执行中的闭包。
/// 超时检查在闭包返回后生效：如果闭包执行耗时超过 `step_timeout`，
/// 即使闭包返回 `Ok(())`，也会触发补偿。
#[derive(Debug, Clone, Copy)]
pub struct SagaTimeout {
    /// 单个步骤的超时时间
    pub step_timeout: Duration,
    /// 整个 Saga 的总超时时间
    pub total_timeout: Duration,
}

impl SagaTimeout {
    /// 创建新的超时配置
    pub fn new(step_timeout: Duration, total_timeout: Duration) -> Self {
        Self {
            step_timeout,
            total_timeout,
        }
    }
}

impl Default for SagaTimeout {
    fn default() -> Self {
        Self {
            step_timeout: Duration::from_secs(30),
            total_timeout: Duration::from_secs(300),
        }
    }
}

/// 获取当前 Unix 时间戳（毫秒）
fn current_timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Saga 执行状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SagaState {
    /// 新建未执行
    New,
    /// 正在执行步骤
    Running,
    /// 所有步骤执行成功
    Completed,
    /// 正在执行补偿（回滚）
    Compensating,
    /// 补偿完成（部分步骤成功后被回滚）
    Compensated,
    /// 补偿失败（需要人工介入）
    CompensationFailed,
    /// 执行失败（非步骤错误，例如内部状态错误）
    Failed,
}

impl std::fmt::Display for SagaState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// 单个 Saga 步骤的状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepState {
    /// 待执行
    Pending,
    /// 执行成功
    Completed,
    /// 已补偿（回滚）
    Compensated,
    /// 执行失败
    Failed,
    /// 补偿失败
    CompensationFailed,
}

/// Saga 步骤定义
///
/// 每个步骤包含一个 action（前向操作）和一个 compensation（补偿操作）。
/// 当后续步骤失败时，已成功执行的步骤会按反向顺序调用 compensation。
pub struct SagaStep {
    /// 步骤名称（用于日志和调试）
    pub name: String,
    /// 步骤状态
    pub state: StepState,
    action: Option<SagaAction>,
    compensation: Option<SagaCompensation>,
}

impl SagaStep {
    /// 创建新步骤
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            state: StepState::Pending,
            action: None,
            compensation: None,
        }
    }

    /// 设置动作回调（前向操作）
    pub fn with_action<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.action = Some(Arc::new(f));
        self
    }

    /// 设置补偿回调（回滚操作）
    pub fn with_compensation<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.compensation = Some(Arc::new(f));
        self
    }

    /// 执行动作
    pub fn execute_action(&mut self) -> Result<(), String> {
        match &self.action {
            Some(f) => match f() {
                Ok(()) => {
                    self.state = StepState::Completed;
                    Ok(())
                }
                Err(e) => {
                    self.state = StepState::Failed;
                    Err(e)
                }
            },
            None => {
                // 没有动作回调，视为成功
                self.state = StepState::Completed;
                Ok(())
            }
        }
    }

    /// 执行补偿
    pub fn execute_compensation(&mut self) -> Result<(), String> {
        match &self.compensation {
            Some(f) => match f() {
                Ok(()) => {
                    self.state = StepState::Compensated;
                    Ok(())
                }
                Err(e) => {
                    self.state = StepState::CompensationFailed;
                    Err(e)
                }
            },
            None => {
                // 没有补偿回调，视为补偿成功
                self.state = StepState::Compensated;
                Ok(())
            }
        }
    }

    /// 是否已成功执行
    pub fn is_completed(&self) -> bool {
        self.state == StepState::Completed
    }

    /// 是否需要补偿
    pub fn needs_compensation(&self) -> bool {
        matches!(self.state, StepState::Completed)
    }
}

impl std::fmt::Debug for SagaStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SagaStep")
            .field("name", &self.name)
            .field("state", &self.state)
            .field("has_action", &self.action.is_some())
            .field("has_compensation", &self.compensation.is_some())
            .finish()
    }
}

/// Saga 执行结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SagaResult {
    /// 所有步骤成功完成
    Success,
    /// 步骤失败，所有已成功步骤已被补偿
    Compensated { failed_step: String, reason: String },
    /// 步骤失败且补偿也失败，需要人工介入
    CompensationFailed {
        failed_step: String,
        failure_reason: String,
        compensation_failed_step: String,
        compensation_reason: String,
    },
}

/// Saga 协调器
///
/// 维护步骤列表和执行状态，按顺序执行所有步骤；
/// 任一步骤失败时，按反向顺序对已成功步骤执行补偿。
pub struct Saga {
    /// Saga 标识
    pub id: String,
    /// Saga 当前状态
    state: SagaState,
    /// 步骤列表（按执行顺序）
    steps: Vec<SagaStep>,
    /// 已执行的步骤数（用于补偿范围）
    completed_count: usize,
    /// 最近一次执行结果
    last_result: Option<SagaResult>,
    /// 可选的 Saga 日志（用于状态机持久化与故障恢复）
    log: Option<Arc<dyn SagaLog>>,
    /// 可选的超时配置
    timeout: Option<SagaTimeout>,
}

impl Saga {
    /// 创建新 Saga
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            state: SagaState::New,
            steps: Vec::new(),
            completed_count: 0,
            last_result: None,
            log: None,
            timeout: None,
        }
    }

    /// 当前状态
    pub fn state(&self) -> SagaState {
        self.state.clone()
    }

    /// 所有步骤状态
    pub fn steps(&self) -> &[SagaStep] {
        &self.steps
    }

    /// 最近一次执行结果
    pub fn last_result(&self) -> Option<&SagaResult> {
        self.last_result.as_ref()
    }

    /// 已成功执行的步骤数
    pub fn completed_count(&self) -> usize {
        self.completed_count
    }

    /// 添加步骤
    ///
    /// 允许在以下状态添加：
    /// - [`SagaState::New`]：正常流程，构建 Saga 时添加步骤
    /// - [`SagaState::Compensating`]：恢复流程，通过 [`Self::recover_from_log`] 恢复后
    ///   重新注册步骤（含 compensation 闭包），然后调用 [`Self::resume_compensation`]
    pub fn add_step(&mut self, step: SagaStep) -> Result<(), String> {
        // 允许在 New 状态添加步骤（正常流程）
        // 允许在 Compensating 状态添加步骤（用于 recover_from_log 后重新注册步骤）
        if self.state != SagaState::New && self.state != SagaState::Compensating {
            return Err(format!(
                "Cannot add step to Saga in state {:?} (only New or Compensating allowed)",
                self.state
            ));
        }
        self.steps.push(step);
        Ok(())
    }

    /// 构建 Saga 并添加步骤（链式 API）
    ///
    /// # 注意
    ///
    /// 仅在 Saga 处于 [`SagaState::New`] 状态时才会添加步骤。
    /// 若 Saga 已执行（state != New），步骤将被**静默丢弃**。
    /// 如需在非 New 状态添加步骤时得到错误，请使用 [`Saga::add_step`]。
    #[must_use]
    pub fn with_step(mut self, step: SagaStep) -> Self {
        // 链式 API 不返回 Result，仅在 New 状态下有效；其他状态静默忽略
        if self.state == SagaState::New {
            self.steps.push(step);
        }
        self
    }

    /// 设置 Saga 日志（用于状态机持久化与故障恢复）
    ///
    /// 设置后，`execute` 会在每个步骤开始/完成/失败时记录日志，
    /// 补偿操作也会记录日志。可通过 [`Saga::recover_from_log`] 从日志恢复状态。
    #[must_use]
    pub fn with_log(mut self, log: Arc<dyn SagaLog>) -> Self {
        self.log = Some(log);
        self
    }

    /// 设置超时配置
    ///
    /// 设置后，`execute` 会在每个步骤执行前检查总超时、执行后检查步骤超时。
    /// 超时后 Saga 进入 Compensating 状态并执行补偿。
    ///
    /// # 重要限制
    ///
    /// 因为 action 是同步闭包，无法真正中断执行中的闭包。
    /// 超时检查在闭包返回后生效。
    #[must_use]
    pub fn with_timeout(mut self, timeout: SagaTimeout) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 执行 Saga
    ///
    /// 按顺序执行所有步骤的 action；任一步骤失败时，按反向顺序对已成功步骤执行 compensation。
    ///
    /// # 超时处理
    ///
    /// 若设置了超时配置（[`SagaTimeout`]），会在每个步骤执行前检查总超时、
    /// 执行后检查步骤超时。超时后进入 Compensating 状态并执行补偿。
    /// 注意：因为 action 是同步闭包，超时检查在闭包返回后生效。
    ///
    /// # 日志记录
    ///
    /// 若设置了日志（[`SagaLog`]），会在每个关键节点记录日志，
    /// 支持故障后通过 [`Saga::recover_from_log`] 恢复状态。
    ///
    /// # 恢复执行
    ///
    /// 若 Saga 是通过 `recover_from_log` 恢复的（`completed_count > 0`），
    /// 会跳过已完成的步骤，从下一个步骤继续执行。
    pub fn execute(&mut self) -> Result<SagaResult, String> {
        if self.state != SagaState::New {
            return Err(format!(
                "Cannot execute Saga in state {:?} (only New allowed)",
                self.state
            ));
        }

        // 空步骤直接完成
        if self.steps.is_empty() {
            self.state = SagaState::Completed;
            self.last_result = Some(SagaResult::Success);
            self.append_saga_log(SagaLogAction::SagaCompleted, None);
            return Ok(SagaResult::Success);
        }

        // 如果所有步骤已从恢复中完成，直接标记完成
        if self.completed_count >= self.steps.len() {
            self.state = SagaState::Completed;
            self.last_result = Some(SagaResult::Success);
            self.append_saga_log(SagaLogAction::SagaCompleted, None);
            return Ok(SagaResult::Success);
        }

        self.state = SagaState::Running;
        let start = Instant::now();

        // 标记从日志恢复的已完成步骤（跳过它们）
        for i in 0..self.completed_count.min(self.steps.len()) {
            if self.steps[i].state == StepState::Pending {
                self.steps[i].state = StepState::Completed;
            }
        }

        // 从 completed_count 开始，跳过已完成的步骤
        for i in self.completed_count..self.steps.len() {
            let step_name = self.steps[i].name.clone();

            // 执行前检查总超时
            if let Some(ref timeout) = self.timeout {
                if start.elapsed() >= timeout.total_timeout {
                    let reason = format!(
                        "Saga 总执行时间超时 (elapsed={:?}, limit={:?})",
                        start.elapsed(),
                        timeout.total_timeout
                    );
                    self.append_step_log(i, &step_name, SagaLogAction::StepFailed, Some(reason.clone()));
                    self.state = SagaState::Compensating;
                    let comp_result = self.compensate();
                    return self.finalize_compensation(comp_result, String::new(), reason);
                }
            }

            // 记录步骤开始
            self.append_step_log(i, &step_name, SagaLogAction::StepStarted, None);

            let step_start = Instant::now();
            match self.steps[i].execute_action() {
                Ok(()) => {
                    let step_elapsed = step_start.elapsed();
                    self.completed_count = i + 1;
                    self.append_step_log(i, &step_name, SagaLogAction::StepCompleted, None);

                    // 执行后检查步骤超时
                    if let Some(ref timeout) = self.timeout {
                        if step_elapsed >= timeout.step_timeout {
                            let reason = format!(
                                "步骤 {} 超时 (elapsed={:?}, limit={:?})",
                                step_name, step_elapsed, timeout.step_timeout
                            );
                            self.state = SagaState::Compensating;
                            let comp_result = self.compensate();
                            return self.finalize_compensation(comp_result, step_name, reason);
                        }
                    }
                }
                Err(e) => {
                    let failure_reason = e.clone();
                    self.append_step_log(
                        i,
                        &step_name,
                        SagaLogAction::StepFailed,
                        Some(e.clone()),
                    );
                    self.state = SagaState::Compensating;
                    let comp_result = self.compensate();
                    return self.finalize_compensation(comp_result, step_name, failure_reason);
                }
            }
        }

        // 所有步骤成功
        self.state = SagaState::Completed;
        self.last_result = Some(SagaResult::Success);
        self.append_saga_log(SagaLogAction::SagaCompleted, None);
        Ok(SagaResult::Success)
    }

    /// 按反向顺序补偿已成功执行的步骤（含日志记录）
    ///
    /// 返回 Ok(()) 表示所有补偿成功；
    /// 返回 Err((step_name, reason)) 表示某个补偿失败，此时后续步骤不再补偿。
    fn compensate(&mut self) -> Result<(), (String, String)> {
        // 按反向顺序遍历已成功的步骤
        for i in (0..self.completed_count).rev() {
            let step_name = self.steps[i].name.clone();
            if self.steps[i].needs_compensation() {
                self.append_step_log(i, &step_name, SagaLogAction::CompensationStarted, None);
                match self.steps[i].execute_compensation() {
                    Ok(()) => {
                        self.append_step_log(
                            i,
                            &step_name,
                            SagaLogAction::CompensationCompleted,
                            None,
                        );
                    }
                    Err(e) => {
                        self.append_step_log(
                            i,
                            &step_name,
                            SagaLogAction::CompensationFailed,
                            Some(e.clone()),
                        );
                        return Err((step_name, e));
                    }
                }
            }
        }
        self.append_saga_log(SagaLogAction::SagaCompensated, None);
        Ok(())
    }

    /// 整理补偿结果，设置最终状态和 last_result
    fn finalize_compensation(
        &mut self,
        comp_result: Result<(), (String, String)>,
        failed_step: String,
        failure_reason: String,
    ) -> Result<SagaResult, String> {
        match comp_result {
            Ok(()) => {
                self.state = SagaState::Compensated;
                let result = SagaResult::Compensated {
                    failed_step,
                    reason: failure_reason,
                };
                self.last_result = Some(result.clone());
                Ok(result)
            }
            Err((comp_step, comp_reason)) => {
                self.state = SagaState::CompensationFailed;
                let result = SagaResult::CompensationFailed {
                    failed_step,
                    failure_reason,
                    compensation_failed_step: comp_step,
                    compensation_reason: comp_reason,
                };
                self.last_result = Some(result.clone());
                Ok(result)
            }
        }
    }

    /// 追加步骤级别的日志条目
    fn append_step_log(
        &self,
        step_index: usize,
        step_name: &str,
        action: SagaLogAction,
        payload: Option<String>,
    ) {
        if let Some(ref log) = self.log {
            let entry = SagaLogEntry {
                saga_id: self.id.clone(),
                step_name: step_name.to_string(),
                step_index,
                action,
                timestamp: current_timestamp_millis(),
                payload,
            };
            let _ = log.append(entry);
        }
    }

    /// 追加 Saga 级别的日志条目（非步骤相关）
    fn append_saga_log(&self, action: SagaLogAction, payload: Option<String>) {
        if let Some(ref log) = self.log {
            let entry = SagaLogEntry {
                saga_id: self.id.clone(),
                step_name: String::new(),
                step_index: 0,
                action,
                timestamp: current_timestamp_millis(),
                payload,
            };
            let _ = log.append(entry);
        }
    }

    /// 重置 Saga 到新建状态（清除所有步骤状态和结果）
    ///
    /// 用于失败后重新执行。注意：已持久化的副作用不会被撤销。
    pub fn reset(&mut self) {
        self.state = SagaState::New;
        self.completed_count = 0;
        self.last_result = None;
        for step in &mut self.steps {
            step.state = StepState::Pending;
        }
    }

    /// 从日志恢复 Saga 状态
    ///
    /// 读取指定 Saga 的所有日志条目，根据最后一个 action 决定恢复到哪个状态：
    /// - `StepStarted` / `StepCompleted` → 恢复为 New，可继续执行剩余步骤
    /// - `StepFailed` / `CompensationStarted` / `CompensationCompleted` → 恢复为 Compensating，可调用 [`Self::resume_compensation`]
    /// - `SagaCompleted` → 恢复为 Completed（终态）
    /// - `SagaCompensated` → 恢复为 Compensated（终态）
    /// - `CompensationFailed` → 恢复为 CompensationFailed（终态）
    ///
    /// # 恢复后的步骤注册
    ///
    /// 恢复后的 Saga 不含步骤（闭包无法序列化）。调用方需重新注册相同的步骤
    /// （含 action 和 compensation 闭包），然后调用 [`Self::execute`] 或
    /// [`Self::resume_compensation`] 继续执行或补偿。
    ///
    /// # 参数
    ///
    /// - `saga_id`：要恢复的 Saga ID
    /// - `log`：日志存储
    ///
    /// # 返回
    ///
    /// 成功返回恢复的 Saga；若日志为空则返回错误。
    pub fn recover_from_log(saga_id: &str, log: &dyn SagaLog) -> Result<Saga, String> {
        let entries = log.read_all(saga_id)?;

        if entries.is_empty() {
            return Err(format!("Saga {} 的日志为空，无法恢复", saga_id));
        }

        // 统计已完成的步骤数（StepCompleted 的数量）
        let completed_count = entries
            .iter()
            .filter(|e| e.action == SagaLogAction::StepCompleted)
            .count();

        // 根据最后一条日志决定恢复状态
        let last = entries.last().unwrap();
        let mut saga = Saga::new(saga_id);
        saga.completed_count = completed_count;

        match last.action {
            SagaLogAction::StepStarted | SagaLogAction::StepCompleted => {
                // Saga 执行中断，可继续执行剩余步骤
                saga.state = SagaState::New;
            }
            SagaLogAction::StepFailed
            | SagaLogAction::CompensationStarted
            | SagaLogAction::CompensationCompleted => {
                // 步骤失败或补偿中断，需要执行补偿
                saga.state = SagaState::Compensating;
            }
            SagaLogAction::CompensationFailed => {
                saga.state = SagaState::CompensationFailed;
            }
            SagaLogAction::SagaCompleted => {
                saga.state = SagaState::Completed;
            }
            SagaLogAction::SagaCompensated => {
                saga.state = SagaState::Compensated;
            }
        }

        Ok(saga)
    }

    /// 恢复补偿执行（用于从 `StepFailed` 日志恢复后继续补偿）
    ///
    /// 调用前需先通过 [`Self::recover_from_log`] 恢复 Saga 状态，
    /// 并重新注册步骤（含 compensation 闭包）。
    ///
    /// # 执行逻辑
    ///
    /// 1. 将前 `completed_count` 个步骤标记为 Completed（它们在故障前已成功）
    /// 2. 按反向顺序对这些步骤执行补偿
    /// 3. 全部补偿成功 → 状态变为 Compensated
    /// 4. 任一补偿失败 → 状态变为 CompensationFailed
    pub fn resume_compensation(&mut self) -> Result<SagaResult, String> {
        if self.state != SagaState::Compensating {
            return Err(format!(
                "Cannot resume compensation in state {:?} (only Compensating allowed)",
                self.state
            ));
        }
        if self.steps.is_empty() {
            return Err("无法补偿：步骤未注册".to_string());
        }

        // 标记从日志恢复的已完成步骤（使其可被补偿）
        for i in 0..self.completed_count.min(self.steps.len()) {
            if self.steps[i].state == StepState::Pending {
                self.steps[i].state = StepState::Completed;
            }
        }

        let comp_result = self.compensate();
        self.finalize_compensation(comp_result, String::new(), "从日志恢复后补偿".to_string())
    }
}

impl std::fmt::Debug for Saga {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Saga")
            .field("id", &self.id)
            .field("state", &self.state)
            .field("steps_count", &self.steps.len())
            .field("completed_count", &self.completed_count)
            .field("last_result", &self.last_result)
            .finish()
    }
}

/// Saga 协调管理器（管理多个 Saga 实例）
///
/// 内部使用 [`RwLock`] 保护 sagas 表：只读方法（state/step_states/list）使用读锁，
/// 写方法（register/execute/remove/reset）使用写锁，以提升并发读性能。
///
/// # 锁中毒行为
///
/// 所有锁获取使用 `unwrap()`，若锁中毒（持有锁的线程 panic）将 panic。
/// 这是合理的行为：锁中毒意味着内部状态可能已损坏，继续操作不安全。
pub struct SagaManager {
    sagas: Arc<RwLock<std::collections::HashMap<String, Saga>>>,
}

impl SagaManager {
    /// 创建新管理器
    pub fn new() -> Self {
        Self {
            sagas: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// 注册 Saga
    pub fn register(&self, saga: Saga) -> Result<(), String> {
        let mut map = self.sagas.write().unwrap();
        if map.contains_key(&saga.id) {
            return Err(format!("Saga {} already exists", saga.id));
        }
        map.insert(saga.id.clone(), saga);
        Ok(())
    }

    /// 执行指定 Saga
    pub fn execute(&self, id: &str) -> Result<SagaResult, String> {
        let mut map = self.sagas.write().unwrap();
        let saga = map
            .get_mut(id)
            .ok_or_else(|| format!("Saga {} not found", id))?;
        saga.execute()
    }

    /// 查询 Saga 状态
    pub fn state(&self, id: &str) -> Option<SagaState> {
        let map = self.sagas.read().unwrap();
        map.get(id).map(|s| s.state.clone())
    }

    /// 查询 Saga 步骤状态
    pub fn step_states(&self, id: &str) -> Option<Vec<StepState>> {
        let map = self.sagas.read().unwrap();
        map.get(id)
            .map(|s| s.steps.iter().map(|st| st.state.clone()).collect())
    }

    /// 列出所有 Saga ID
    pub fn list(&self) -> Vec<String> {
        let map = self.sagas.read().unwrap();
        let mut ids: Vec<String> = map.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// 删除指定 Saga
    pub fn remove(&self, id: &str) -> Option<SagaState> {
        let mut map = self.sagas.write().unwrap();
        map.remove(id).map(|s| s.state)
    }

    /// 重置 Saga
    pub fn reset(&self, id: &str) -> Result<(), String> {
        let mut map = self.sagas.write().unwrap();
        let saga = map
            .get_mut(id)
            .ok_or_else(|| format!("Saga {} not found", id))?;
        saga.reset();
        Ok(())
    }
}

impl Default for SagaManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    // ---- SagaStep 测试 ----

    #[test]
    fn test_saga_step_new() {
        let step = SagaStep::new("step1");
        assert_eq!(step.name, "step1");
        assert_eq!(step.state, StepState::Pending);
        assert!(!step.is_completed());
        assert!(!step.needs_compensation());
    }

    #[test]
    fn test_saga_step_execute_action_success() {
        let mut step = SagaStep::new("step1").with_action(|| Ok(()));
        let result = step.execute_action();
        assert!(result.is_ok());
        assert_eq!(step.state, StepState::Completed);
        assert!(step.is_completed());
        assert!(step.needs_compensation());
    }

    #[test]
    fn test_saga_step_execute_action_failure() {
        let mut step = SagaStep::new("step1").with_action(|| Err("boom".to_string()));
        let result = step.execute_action();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "boom");
        assert_eq!(step.state, StepState::Failed);
        assert!(!step.is_completed());
        assert!(!step.needs_compensation());
    }

    #[test]
    fn test_saga_step_no_action_succeeds() {
        let mut step = SagaStep::new("step1");
        let result = step.execute_action();
        assert!(result.is_ok());
        assert_eq!(step.state, StepState::Completed);
    }

    #[test]
    fn test_saga_step_execute_compensation_success() {
        let mut step = SagaStep::new("step1").with_compensation(|| Ok(()));
        // 先标记为已完成（模拟动作成功）
        step.state = StepState::Completed;
        let result = step.execute_compensation();
        assert!(result.is_ok());
        assert_eq!(step.state, StepState::Compensated);
    }

    #[test]
    fn test_saga_step_execute_compensation_failure() {
        let mut step = SagaStep::new("step1").with_compensation(|| Err("comp failed".to_string()));
        step.state = StepState::Completed;
        let result = step.execute_compensation();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "comp failed");
        assert_eq!(step.state, StepState::CompensationFailed);
    }

    #[test]
    fn test_saga_step_no_compensation_succeeds() {
        let mut step = SagaStep::new("step1");
        step.state = StepState::Completed;
        let result = step.execute_compensation();
        assert!(result.is_ok());
        assert_eq!(step.state, StepState::Compensated);
    }

    // ---- Saga 测试 ----

    #[test]
    fn test_saga_new() {
        let saga = Saga::new("saga1");
        assert_eq!(saga.id, "saga1");
        assert_eq!(saga.state(), SagaState::New);
        assert_eq!(saga.steps().len(), 0);
        assert_eq!(saga.completed_count(), 0);
    }

    #[test]
    fn test_saga_empty_execute_success() {
        let mut saga = Saga::new("empty");
        let result = saga.execute().unwrap();
        assert_eq!(result, SagaResult::Success);
        assert_eq!(saga.state(), SagaState::Completed);
        assert_eq!(saga.last_result(), Some(&SagaResult::Success));
    }

    #[test]
    fn test_saga_all_steps_success() {
        let counter = Arc::new(AtomicU32::new(0));
        let c1 = counter.clone();
        let c2 = counter.clone();

        let mut saga = Saga::new("all-success");
        saga.add_step(
            SagaStep::new("step1")
                .with_action(move || {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(|| Ok(())),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("step2")
                .with_action(move || {
                    c2.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(|| Ok(())),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        assert_eq!(result, SagaResult::Success);
        assert_eq!(saga.state(), SagaState::Completed);
        assert_eq!(saga.completed_count(), 2);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        assert_eq!(saga.steps()[0].state, StepState::Completed);
        assert_eq!(saga.steps()[1].state, StepState::Completed);
    }

    #[test]
    fn test_saga_failure_triggers_compensation() {
        // step1 成功，step2 失败
        // 应该执行 step1 的补偿
        let action_calls = Arc::new(AtomicU32::new(0));
        let comp_calls = Arc::new(AtomicU32::new(0));

        let a1 = action_calls.clone();
        let c1 = comp_calls.clone();

        let mut saga = Saga::new("partial-fail");
        saga.add_step(
            SagaStep::new("step1")
                .with_action(move || {
                    a1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(move || {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("step2")
                .with_action(|| Err("step2 failed".to_string()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        match result {
            SagaResult::Compensated {
                failed_step,
                reason,
            } => {
                assert_eq!(failed_step, "step2");
                assert_eq!(reason, "step2 failed");
            }
            other => panic!("expected Compensated, got {:?}", other),
        }
        assert_eq!(saga.state(), SagaState::Compensated);
        assert_eq!(action_calls.load(Ordering::SeqCst), 1); // 只有 step1 的 action 执行
        assert_eq!(comp_calls.load(Ordering::SeqCst), 1); // 只有 step1 的补偿执行
        assert_eq!(saga.steps()[0].state, StepState::Compensated);
        assert_eq!(saga.steps()[1].state, StepState::Failed);
    }

    #[test]
    fn test_saga_compensation_order_reverse() {
        // 3 步骤：step1/step2 成功，step3 失败
        // 补偿应按 step2 → step1 顺序执行
        let order = Arc::new(Mutex::new(Vec::<String>::new()));

        let o1a = order.clone();
        let o1c = order.clone();
        let o2a = order.clone();
        let o2c = order.clone();

        let mut saga = Saga::new("reverse-comp");
        saga.add_step(
            SagaStep::new("step1")
                .with_action(move || {
                    o1a.lock().unwrap().push("action1".to_string());
                    Ok(())
                })
                .with_compensation(move || {
                    o1c.lock().unwrap().push("comp1".to_string());
                    Ok(())
                }),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("step2")
                .with_action(move || {
                    o2a.lock().unwrap().push("action2".to_string());
                    Ok(())
                })
                .with_compensation(move || {
                    o2c.lock().unwrap().push("comp2".to_string());
                    Ok(())
                }),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("step3")
                .with_action(|| Err("step3 failed".to_string()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        assert!(matches!(result, SagaResult::Compensated { .. }));

        let recorded = order.lock().unwrap();
        assert_eq!(
            *recorded,
            vec![
                "action1".to_string(),
                "action2".to_string(),
                "comp2".to_string(),
                "comp1".to_string(),
            ]
        );
    }

    #[test]
    fn test_saga_compensation_failure() {
        // step1 成功，step2 失败，但 step1 的补偿也失败
        let mut saga = Saga::new("comp-fail");
        saga.add_step(
            SagaStep::new("step1")
                .with_action(|| Ok(()))
                .with_compensation(|| Err("comp1 failed".to_string())),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("step2")
                .with_action(|| Err("step2 failed".to_string()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        match result {
            SagaResult::CompensationFailed {
                failed_step,
                failure_reason,
                compensation_failed_step,
                compensation_reason,
            } => {
                assert_eq!(failed_step, "step2");
                assert_eq!(failure_reason, "step2 failed");
                assert_eq!(compensation_failed_step, "step1");
                assert_eq!(compensation_reason, "comp1 failed");
            }
            other => panic!("expected CompensationFailed, got {:?}", other),
        }
        assert_eq!(saga.state(), SagaState::CompensationFailed);
        assert_eq!(saga.steps()[0].state, StepState::CompensationFailed);
        assert_eq!(saga.steps()[1].state, StepState::Failed);
    }

    #[test]
    fn test_saga_cannot_execute_twice() {
        let mut saga = Saga::new("once");
        saga.add_step(SagaStep::new("step1").with_action(|| Ok(())))
            .unwrap();
        saga.execute().unwrap();
        let result = saga.execute();
        assert!(result.is_err());
    }

    #[test]
    fn test_saga_cannot_add_step_after_execution() {
        let mut saga = Saga::new("no-add");
        saga.add_step(SagaStep::new("step1").with_action(|| Ok(())))
            .unwrap();
        saga.execute().unwrap();
        let result = saga.add_step(SagaStep::new("step2"));
        assert!(result.is_err());
    }

    #[test]
    fn test_saga_reset() {
        let mut saga = Saga::new("reset-test");
        saga.add_step(
            SagaStep::new("step1")
                .with_action(|| Ok(()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();
        saga.execute().unwrap();
        assert_eq!(saga.state(), SagaState::Completed);

        saga.reset();
        assert_eq!(saga.state(), SagaState::New);
        assert_eq!(saga.completed_count(), 0);
        assert_eq!(saga.steps()[0].state, StepState::Pending);
        assert!(saga.last_result().is_none());
    }

    #[test]
    fn test_saga_with_step_chain() {
        let saga = Saga::new("chain")
            .with_step(SagaStep::new("step1").with_action(|| Ok(())))
            .with_step(SagaStep::new("step2").with_action(|| Ok(())));
        assert_eq!(saga.steps().len(), 2);
    }

    #[test]
    fn test_saga_first_step_failure_no_compensation() {
        // 第一步就失败，无需补偿
        let comp_calls = Arc::new(AtomicU32::new(0));
        let c1 = comp_calls.clone();

        let mut saga = Saga::new("first-fail");
        saga.add_step(
            SagaStep::new("step1")
                .with_action(|| Err("step1 failed".to_string()))
                .with_compensation(move || {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        assert!(matches!(result, SagaResult::Compensated { .. }));
        assert_eq!(saga.state(), SagaState::Compensated);
        assert_eq!(comp_calls.load(Ordering::SeqCst), 0); // 第一步未成功，无需补偿
        assert_eq!(saga.completed_count(), 0);
    }

    #[test]
    fn test_saga_state_display() {
        assert_eq!(SagaState::New.to_string(), "New");
        assert_eq!(SagaState::Completed.to_string(), "Completed");
        assert_eq!(SagaState::Compensated.to_string(), "Compensated");
    }

    // ---- SagaManager 测试 ----

    #[test]
    fn test_saga_manager_new() {
        let m = SagaManager::new();
        assert!(m.list().is_empty());
    }

    #[test]
    fn test_saga_manager_register_and_execute() {
        let m = SagaManager::new();
        let saga = Saga::new("mgr-1").with_step(SagaStep::new("step1").with_action(|| Ok(())));
        m.register(saga).unwrap();

        let result = m.execute("mgr-1").unwrap();
        assert_eq!(result, SagaResult::Success);
        assert_eq!(m.state("mgr-1"), Some(SagaState::Completed));
    }

    #[test]
    fn test_saga_manager_register_duplicate() {
        let m = SagaManager::new();
        let saga = Saga::new("dup");
        m.register(saga).unwrap();
        let saga2 = Saga::new("dup");
        let result = m.register(saga2);
        assert!(result.is_err());
    }

    #[test]
    fn test_saga_manager_execute_missing() {
        let m = SagaManager::new();
        let result = m.execute("missing");
        assert!(result.is_err());
    }

    #[test]
    fn test_saga_manager_list() {
        let m = SagaManager::new();
        m.register(Saga::new("c")).unwrap();
        m.register(Saga::new("a")).unwrap();
        m.register(Saga::new("b")).unwrap();
        assert_eq!(m.list(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_saga_manager_step_states() {
        let m = SagaManager::new();
        let saga = Saga::new("steps")
            .with_step(SagaStep::new("s1").with_action(|| Ok(())))
            .with_step(SagaStep::new("s2").with_action(|| Ok(())));
        m.register(saga).unwrap();
        m.execute("steps").unwrap();

        let states = m.step_states("steps").unwrap();
        assert_eq!(states, vec![StepState::Completed, StepState::Completed]);
    }

    #[test]
    fn test_saga_manager_remove() {
        let m = SagaManager::new();
        m.register(Saga::new("rm")).unwrap();
        let removed = m.remove("rm");
        assert_eq!(removed, Some(SagaState::New));
        assert!(m.state("rm").is_none());
    }

    #[test]
    fn test_saga_manager_reset() {
        let m = SagaManager::new();
        let saga = Saga::new("reset").with_step(SagaStep::new("s1").with_action(|| Ok(())));
        m.register(saga).unwrap();
        m.execute("reset").unwrap();
        assert_eq!(m.state("reset"), Some(SagaState::Completed));

        m.reset("reset").unwrap();
        assert_eq!(m.state("reset"), Some(SagaState::New));
    }

    // ---- 真实业务场景模拟：电商订单 ----

    #[test]
    fn test_saga_ecommerce_order_success() {
        // 模拟：创建订单 → 扣库存 → 扣余额
        let order_created = Arc::new(AtomicU32::new(0));
        let stock_deducted = Arc::new(AtomicU32::new(0));
        let balance_deducted = Arc::new(AtomicU32::new(0));

        let o1 = order_created.clone();
        let o1r = order_created.clone();
        let s1 = stock_deducted.clone();
        let s1r = stock_deducted.clone();
        let b1 = balance_deducted.clone();
        let b1r = balance_deducted.clone();

        let mut saga = Saga::new("order-123");
        saga.add_step(
            SagaStep::new("create_order")
                .with_action(move || {
                    o1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(move || {
                    o1r.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("deduct_stock")
                .with_action(move || {
                    s1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(move || {
                    s1r.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("deduct_balance")
                .with_action(move || {
                    b1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(move || {
                    b1r.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        assert_eq!(result, SagaResult::Success);
        assert_eq!(order_created.load(Ordering::SeqCst), 1);
        assert_eq!(stock_deducted.load(Ordering::SeqCst), 1);
        assert_eq!(balance_deducted.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_saga_ecommerce_order_balance_insufficient() {
        // 模拟：创建订单 ✓ → 扣库存 ✓ → 扣余额 ✗（余额不足）
        // 应该补偿：恢复库存、取消订单
        let order_created = Arc::new(AtomicU32::new(0));
        let stock_deducted = Arc::new(AtomicU32::new(0));

        let o1 = order_created.clone();
        let o1r = order_created.clone();
        let s1 = stock_deducted.clone();
        let s1r = stock_deducted.clone();

        let mut saga = Saga::new("order-456");
        saga.add_step(
            SagaStep::new("create_order")
                .with_action(move || {
                    o1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(move || {
                    o1r.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("deduct_stock")
                .with_action(move || {
                    s1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(move || {
                    s1r.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("deduct_balance")
                .with_action(|| Err("余额不足".to_string()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        assert!(matches!(result, SagaResult::Compensated { .. }));
        // 补偿后，订单和库存应该恢复为 0
        assert_eq!(order_created.load(Ordering::SeqCst), 0);
        assert_eq!(stock_deducted.load(Ordering::SeqCst), 0);
    }

    // ===== 新增测试：Saga 日志、超时、恢复（v0.2.1 深度优化） =====

    #[test]
    fn test_in_memory_saga_log_append_and_read() {
        let log = InMemorySagaLog::new();
        let entry = SagaLogEntry {
            saga_id: "s1".to_string(),
            step_name: "step1".to_string(),
            step_index: 0,
            action: SagaLogAction::StepStarted,
            timestamp: 1000,
            payload: None,
        };
        log.append(entry.clone()).unwrap();
        let entries = log.read_all("s1").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].saga_id, "s1");
        assert_eq!(entries[0].step_name, "step1");
        assert_eq!(entries[0].action, SagaLogAction::StepStarted);
    }

    #[test]
    fn test_in_memory_saga_log_read_empty() {
        let log = InMemorySagaLog::new();
        let entries = log.read_all("nonexistent").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_in_memory_saga_log_multiple_sagas_isolated() {
        let log = InMemorySagaLog::new();
        log.append(SagaLogEntry {
            saga_id: "s1".to_string(),
            step_name: "step1".to_string(),
            step_index: 0,
            action: SagaLogAction::StepStarted,
            timestamp: 1,
            payload: None,
        })
        .unwrap();
        log.append(SagaLogEntry {
            saga_id: "s2".to_string(),
            step_name: "step1".to_string(),
            step_index: 0,
            action: SagaLogAction::StepStarted,
            timestamp: 2,
            payload: None,
        })
        .unwrap();
        assert_eq!(log.read_all("s1").unwrap().len(), 1);
        assert_eq!(log.read_all("s2").unwrap().len(), 1);
    }

    #[test]
    fn test_saga_with_log_records_step_started_and_completed() {
        let log = Arc::new(InMemorySagaLog::new());
        let mut saga = Saga::new("log-1")
            .with_log(log.clone())
            .with_step(SagaStep::new("step1").with_action(|| Ok(())));
        saga.execute().unwrap();

        let entries = log.read_all("log-1").unwrap();
        // 应包含：StepStarted, StepCompleted, SagaCompleted
        assert!(entries
            .iter()
            .any(|e| e.action == SagaLogAction::StepStarted && e.step_name == "step1"));
        assert!(entries
            .iter()
            .any(|e| e.action == SagaLogAction::StepCompleted && e.step_name == "step1"));
        assert!(entries.iter().any(|e| e.action == SagaLogAction::SagaCompleted));
    }

    #[test]
    fn test_saga_with_log_records_compensation() {
        let log = Arc::new(InMemorySagaLog::new());
        let mut saga = Saga::new("log-2").with_log(log.clone());
        saga.add_step(
            SagaStep::new("step1")
                .with_action(|| Ok(()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("step2").with_action(|| Err("fail".to_string())),
        )
        .unwrap();
        let _ = saga.execute();

        let entries = log.read_all("log-2").unwrap();
        // 应包含：CompensationStarted, CompensationCompleted, SagaCompensated
        assert!(entries
            .iter()
            .any(|e| e.action == SagaLogAction::CompensationStarted));
        assert!(entries
            .iter()
            .any(|e| e.action == SagaLogAction::CompensationCompleted));
        assert!(entries
            .iter()
            .any(|e| e.action == SagaLogAction::SagaCompensated));
    }

    #[test]
    fn test_saga_with_log_records_compensation_failed() {
        let log = Arc::new(InMemorySagaLog::new());
        let mut saga = Saga::new("log-3").with_log(log.clone());
        saga.add_step(
            SagaStep::new("step1")
                .with_action(|| Ok(()))
                .with_compensation(|| Err("compensation fail".to_string())),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("step2").with_action(|| Err("action fail".to_string())),
        )
        .unwrap();
        let _ = saga.execute();

        let entries = log.read_all("log-3").unwrap();
        assert!(entries
            .iter()
            .any(|e| e.action == SagaLogAction::CompensationFailed));
    }

    #[test]
    fn test_saga_recover_from_empty_log_fails() {
        let log = InMemorySagaLog::new();
        let result = Saga::recover_from_log("empty", &log);
        assert!(result.is_err());
    }

    #[test]
    fn test_saga_recover_from_log_step_completed_can_continue() {
        // 模拟：step1 已完成，step2 未执行（日志记录到 StepCompleted）
        let log = InMemorySagaLog::new();
        log.append(SagaLogEntry {
            saga_id: "rec-1".to_string(),
            step_name: "step1".to_string(),
            step_index: 0,
            action: SagaLogAction::StepStarted,
            timestamp: 1,
            payload: None,
        })
        .unwrap();
        log.append(SagaLogEntry {
            saga_id: "rec-1".to_string(),
            step_name: "step1".to_string(),
            step_index: 0,
            action: SagaLogAction::StepCompleted,
            timestamp: 2,
            payload: None,
        })
        .unwrap();

        let mut saga = Saga::recover_from_log("rec-1", &log).unwrap();
        assert_eq!(saga.state(), SagaState::New);
        assert_eq!(saga.completed_count(), 1);

        // 重新注册步骤（含闭包），从 step2 继续执行
        saga.add_step(SagaStep::new("step1").with_action(|| Ok(())))
            .unwrap();
        saga.add_step(SagaStep::new("step2").with_action(|| Ok(())))
            .unwrap();
        let result = saga.execute().unwrap();
        assert_eq!(result, SagaResult::Success);
    }

    #[test]
    fn test_saga_recover_from_log_step_failed_enters_compensating() {
        // 模拟：step1 失败（日志最后一条是 StepFailed）
        let log = InMemorySagaLog::new();
        log.append(SagaLogEntry {
            saga_id: "rec-2".to_string(),
            step_name: "step1".to_string(),
            step_index: 0,
            action: SagaLogAction::StepCompleted,
            timestamp: 1,
            payload: None,
        })
        .unwrap();
        log.append(SagaLogEntry {
            saga_id: "rec-2".to_string(),
            step_name: "step2".to_string(),
            step_index: 1,
            action: SagaLogAction::StepFailed,
            timestamp: 2,
            payload: Some("step2 failed".to_string()),
        })
        .unwrap();

        let mut saga = Saga::recover_from_log("rec-2", &log).unwrap();
        assert_eq!(saga.state(), SagaState::Compensating);
        assert_eq!(saga.completed_count(), 1);

        // 重新注册步骤（含 compensation 闭包），执行 resume_compensation
        saga.add_step(
            SagaStep::new("step1")
                .with_action(|| Ok(()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();
        saga.add_step(SagaStep::new("step2").with_action(|| Ok(())))
            .unwrap();
        let result = saga.resume_compensation().unwrap();
        assert!(matches!(result, SagaResult::Compensated { .. }));
    }

    #[test]
    fn test_saga_recover_from_log_saga_completed_is_terminal() {
        let log = InMemorySagaLog::new();
        log.append(SagaLogEntry {
            saga_id: "rec-3".to_string(),
            step_name: String::new(),
            step_index: 0,
            action: SagaLogAction::SagaCompleted,
            timestamp: 1,
            payload: None,
        })
        .unwrap();

        let saga = Saga::recover_from_log("rec-3", &log).unwrap();
        assert_eq!(saga.state(), SagaState::Completed);
    }

    #[test]
    fn test_saga_recover_from_log_saga_compensated_is_terminal() {
        let log = InMemorySagaLog::new();
        log.append(SagaLogEntry {
            saga_id: "rec-4".to_string(),
            step_name: String::new(),
            step_index: 0,
            action: SagaLogAction::SagaCompensated,
            timestamp: 1,
            payload: None,
        })
        .unwrap();

        let saga = Saga::recover_from_log("rec-4", &log).unwrap();
        assert_eq!(saga.state(), SagaState::Compensated);
    }

    #[test]
    fn test_saga_recover_from_log_compensation_failed_is_terminal() {
        let log = InMemorySagaLog::new();
        log.append(SagaLogEntry {
            saga_id: "rec-5".to_string(),
            step_name: "step1".to_string(),
            step_index: 0,
            action: SagaLogAction::CompensationFailed,
            timestamp: 1,
            payload: Some("comp fail".to_string()),
        })
        .unwrap();

        let saga = Saga::recover_from_log("rec-5", &log).unwrap();
        assert_eq!(saga.state(), SagaState::CompensationFailed);
    }

    #[test]
    fn test_saga_timeout_default_values() {
        let t = SagaTimeout::default();
        assert_eq!(t.step_timeout, Duration::from_secs(30));
        assert_eq!(t.total_timeout, Duration::from_secs(300));
    }

    #[test]
    fn test_saga_timeout_custom_values() {
        let t = SagaTimeout::new(Duration::from_millis(100), Duration::from_millis(500));
        assert_eq!(t.step_timeout, Duration::from_millis(100));
        assert_eq!(t.total_timeout, Duration::from_millis(500));
    }

    #[test]
    fn test_saga_step_timeout_triggers_compensation() {
        // step_timeout 设为极小值，action 执行后会立即超时
        let timeout = SagaTimeout::new(Duration::from_millis(1), Duration::from_secs(60));
        let compensated = Arc::new(AtomicU32::new(0));
        let c1 = compensated.clone();

        let mut saga = Saga::new("timeout-1").with_timeout(timeout);
        saga.add_step(
            SagaStep::new("slow-step")
                .with_action(|| {
                    // 睡眠 10ms，超过 step_timeout=1ms
                    std::thread::sleep(Duration::from_millis(10));
                    Ok(())
                })
                .with_compensation(move || {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        assert!(matches!(result, SagaResult::Compensated { .. }));
        assert_eq!(compensated.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_saga_total_timeout_triggers_compensation() {
        // total_timeout=0，第一步执行前就会超时
        let timeout = SagaTimeout::new(Duration::from_secs(60), Duration::from_millis(0));
        let action_called = Arc::new(AtomicU32::new(0));
        let a1 = action_called.clone();

        let mut saga = Saga::new("timeout-2").with_timeout(timeout);
        saga.add_step(
            SagaStep::new("step1")
                .with_action(move || {
                    a1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_compensation(|| Ok(())),
        )
        .unwrap();

        let result = saga.execute().unwrap();
        assert!(matches!(result, SagaResult::Compensated { .. }));
        // action 不应被执行（total_timeout=0 在执行前就触发）
        assert_eq!(action_called.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_saga_timeout_zero_step_zero_total() {
        // 边界：两个超时都为 0
        let timeout = SagaTimeout::new(Duration::from_millis(0), Duration::from_millis(0));
        let mut saga = Saga::new("timeout-3").with_timeout(timeout);
        saga.add_step(SagaStep::new("step1").with_action(|| Ok(())))
            .unwrap();

        // 即使超时为 0，因为 Instant 精度，第一步可能立即执行也可能立即超时
        // 但行为应该是确定性的：total_timeout=0 在第一步执行前检查时已超时
        let result = saga.execute().unwrap();
        // step1 的 action 应该被跳过（total_timeout=0 在执行前就触发）
        // 但补偿会执行（虽然 step1 未成功，没有补偿需要执行）
        assert!(matches!(result, SagaResult::Compensated { .. }));
    }

    #[test]
    fn test_saga_resume_compensation_wrong_state_fails() {
        let mut saga = Saga::new("resume-1");
        saga.add_step(SagaStep::new("step1").with_action(|| Ok(())))
            .unwrap();
        // 当前状态是 New，不能 resume_compensation
        let result = saga.resume_compensation();
        assert!(result.is_err());
    }

    #[test]
    fn test_saga_resume_compensation_no_steps_fails() {
        let log = InMemorySagaLog::new();
        log.append(SagaLogEntry {
            saga_id: "resume-2".to_string(),
            step_name: "step1".to_string(),
            step_index: 0,
            action: SagaLogAction::StepFailed,
            timestamp: 1,
            payload: None,
        })
        .unwrap();

        let mut saga = Saga::recover_from_log("resume-2", &log).unwrap();
        // 不注册步骤，直接调用 resume_compensation 应该失败
        let result = saga.resume_compensation();
        assert!(result.is_err());
    }
}
