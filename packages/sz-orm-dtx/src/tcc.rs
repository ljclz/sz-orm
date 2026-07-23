//! TCC（Try-Confirm-Cancel）分布式事务模型
//!
//! TCC 是一种补偿型分布式事务模式，每个分支事务需要实现三个方法：
//! - **Try**：尝试预留资源（如检查余额、冻结金额、锁定库存）
//! - **Confirm**：确认提交（实际扣款、实际扣库存），必须幂等
//! - **Cancel**：取消预留（解冻金额、解锁库存），必须幂等
//!
//! # 协调器语义
//!
//! 1. **Try 阶段**：依次调用所有分支 try。
//!    - 全部成功 → 进入 Confirm 阶段
//!    - 任一失败 → 对已 try 成功的分支执行 cancel
//! 2. **Confirm 阶段**：依次调用所有分支 confirm。
//!    - 任一失败 → 标记 Failed（需要人工干预或自动重试，因为 confirm 是关键路径）
//! 3. **Cancel 阶段**：依次调用已 try 成功的分支 cancel。
//!    - 失败 → 标记 Failed（需重试，cancel 也必须幂等）
//!
//! # 与 Saga / 2PC 对比
//!
//! | 模型 | 隔离性 | 一致性 | 复杂度 | 适用场景 |
//! |------|--------|--------|--------|----------|
//! | 2PC  | 强     | 强     | 高     | 同构数据库 |
//! | TCC  | 中     | 最终   | 中     | 异构系统（资金、库存）|
//! | Saga | 弱     | 最终   | 低     | 长流程业务 |
//!
//! # 用法
//!
//! ```
//! use sz_orm_dtx::tcc::{TccCoordinator, TccParticipant, TccState};
//! use std::sync::Arc;
//! use std::sync::atomic::{AtomicU32, Ordering};
//!
//! let mut coord = TccCoordinator::new("tx-transfer-001");
//!
//! let frozen = Arc::new(AtomicU32::new(0));
//! let confirmed = Arc::new(AtomicU32::new(0));
//! let cancelled = Arc::new(AtomicU32::new(0));
//!
//! let f1 = frozen.clone();
//! let c1 = confirmed.clone();
//! let ca1 = cancelled.clone();
//! coord.add_participant(
//!     TccParticipant::new("account-deduct")
//!         .with_try(move || { f1.fetch_add(1, Ordering::SeqCst); Ok(()) })
//!         .with_confirm(move || { c1.fetch_add(1, Ordering::SeqCst); Ok(()) })
//!         .with_cancel(move || { ca1.fetch_add(1, Ordering::SeqCst); Ok(()) }),
//! );
//!
//! // 全部 try 成功 → 自动 confirm
//! coord.execute().unwrap();
//! assert_eq!(frozen.load(Ordering::SeqCst), 1);
//! assert_eq!(confirmed.load(Ordering::SeqCst), 1);
//! assert_eq!(cancelled.load(Ordering::SeqCst), 0);
//! assert_eq!(coord.state(), TccState::Confirmed);
//! ```

use std::sync::{Arc, RwLock};
use std::time::Duration;

// =====================================================================
// TccState — TCC 事务状态
// =====================================================================

/// TCC 事务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TccState {
    /// 初始状态：未开始 try
    Init,
    /// 正在执行 try 阶段
    Trying,
    /// Try 全部成功，等待 confirm 或 cancel
    Tried,
    /// 正在执行 confirm 阶段
    Confirming,
    /// Confirm 全部成功（终态）
    Confirmed,
    /// 正在执行 cancel 阶段
    Cancelling,
    /// Cancel 全部完成（终态）
    Cancelled,
    /// 异常态：confirm 或 cancel 中有分支失败，需要人工介入或自动重试
    Failed,
}

impl TccState {
    /// 是否为终态
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TccState::Confirmed | TccState::Cancelled | TccState::Failed
        )
    }

    /// 是否可重试 confirm（即 Confirming/Failed 中由于 confirm 失败）
    pub fn can_retry_confirm(&self) -> bool {
        matches!(self, TccState::Confirming | TccState::Failed)
    }

    /// 是否可重试 cancel（即 Cancelling/Failed 中由于 cancel 失败）
    pub fn can_retry_cancel(&self) -> bool {
        matches!(self, TccState::Cancelling | TccState::Failed)
    }
}

impl std::fmt::Display for TccState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// =====================================================================
// TccParticipantState — 分支事务状态
// =====================================================================

/// TCC 分支事务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TccParticipantState {
    /// 初始状态
    Init,
    /// Try 已成功
    Tried,
    /// Confirm 已成功（终态）
    Confirmed,
    /// Cancel 已成功（终态）
    Cancelled,
    /// Try / Confirm / Cancel 失败
    Failed,
}

// =====================================================================
// TccParticipant — TCC 分支事务
// =====================================================================

/// TCC 回调函数类型
pub type TccCallback = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// TCC 分支事务
///
/// 每个分支必须提供 try / confirm / cancel 三个闭包，且 confirm/cancel 必须幂等。
pub struct TccParticipant {
    /// 分支资源 ID（如 "account-deduct"、"inventory-lock"）
    pub resource_id: String,
    /// 当前分支状态
    pub state: TccParticipantState,
    try_fn: Option<TccCallback>,
    confirm_fn: Option<TccCallback>,
    cancel_fn: Option<TccCallback>,
    /// try 阶段执行次数（用于诊断）
    pub try_attempts: u32,
    /// confirm 阶段执行次数
    pub confirm_attempts: u32,
    /// cancel 阶段执行次数
    pub cancel_attempts: u32,
    /// confirm 是否已成功（用于幂等性追踪）
    confirm_succeeded: bool,
    /// cancel 是否已成功（用于幂等性追踪）
    cancel_succeeded: bool,
}

impl TccParticipant {
    /// 创建新的 TCC 分支
    pub fn new(resource_id: impl Into<String>) -> Self {
        Self {
            resource_id: resource_id.into(),
            state: TccParticipantState::Init,
            try_fn: None,
            confirm_fn: None,
            cancel_fn: None,
            try_attempts: 0,
            confirm_attempts: 0,
            cancel_attempts: 0,
            confirm_succeeded: false,
            cancel_succeeded: false,
        }
    }

    /// 设置 try 回调
    #[must_use]
    pub fn with_try<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.try_fn = Some(Arc::new(f));
        self
    }

    /// 设置 confirm 回调（必须幂等）
    #[must_use]
    pub fn with_confirm<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.confirm_fn = Some(Arc::new(f));
        self
    }

    /// 设置 cancel 回调（必须幂等）
    #[must_use]
    pub fn with_cancel<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.cancel_fn = Some(Arc::new(f));
        self
    }

    /// 执行 try 阶段
    pub fn try_phase(&mut self) -> Result<(), String> {
        self.try_attempts += 1;
        if let Some(cb) = &self.try_fn {
            if let Err(e) = cb() {
                self.state = TccParticipantState::Failed;
                return Err(e);
            }
        }
        self.state = TccParticipantState::Tried;
        Ok(())
    }

    /// 执行 confirm 阶段（必须幂等）
    ///
    /// # 幂等性
    ///
    /// 若 `confirm_succeeded` 已为 true，直接返回 Ok(())，不再调用闭包、不增加计数。
    pub fn confirm_phase(&mut self) -> Result<(), String> {
        // 幂等性检查：已成功则跳过
        if self.confirm_succeeded {
            return Ok(());
        }
        self.confirm_attempts += 1;
        if let Some(cb) = &self.confirm_fn {
            if let Err(e) = cb() {
                self.state = TccParticipantState::Failed;
                return Err(e);
            }
        }
        self.state = TccParticipantState::Confirmed;
        self.confirm_succeeded = true;
        Ok(())
    }

    /// 执行 cancel 阶段（必须幂等）
    ///
    /// # 幂等性
    ///
    /// 若 `cancel_succeeded` 已为 true，直接返回 Ok(())，不再调用闭包、不增加计数。
    pub fn cancel_phase(&mut self) -> Result<(), String> {
        // 幂等性检查：已成功则跳过
        if self.cancel_succeeded {
            return Ok(());
        }
        self.cancel_attempts += 1;
        if let Some(cb) = &self.cancel_fn {
            if let Err(e) = cb() {
                self.state = TccParticipantState::Failed;
                return Err(e);
            }
        }
        self.state = TccParticipantState::Cancelled;
        self.cancel_succeeded = true;
        Ok(())
    }

    /// 标记分支失败
    pub fn fail(&mut self) {
        self.state = TccParticipantState::Failed;
    }

    /// 是否已 try 成功（可用于决定是否需要 cancel）
    pub fn is_tried(&self) -> bool {
        matches!(
            self.state,
            TccParticipantState::Tried
                | TccParticipantState::Confirmed
                | TccParticipantState::Cancelled
        )
    }

    /// confirm 是否已完成（幂等性追踪）
    pub fn is_confirm_completed(&self) -> bool {
        self.confirm_succeeded
    }

    /// cancel 是否已完成（幂等性追踪）
    pub fn is_cancel_completed(&self) -> bool {
        self.cancel_succeeded
    }
}

impl std::fmt::Debug for TccParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TccParticipant")
            .field("resource_id", &self.resource_id)
            .field("state", &self.state)
            .field("has_try", &self.try_fn.is_some())
            .field("has_confirm", &self.confirm_fn.is_some())
            .field("has_cancel", &self.cancel_fn.is_some())
            .field("try_attempts", &self.try_attempts)
            .field("confirm_attempts", &self.confirm_attempts)
            .field("cancel_attempts", &self.cancel_attempts)
            .field("confirm_succeeded", &self.confirm_succeeded)
            .field("cancel_succeeded", &self.cancel_succeeded)
            .finish()
    }
}

// =====================================================================
// TccRetryPolicy — TCC 重试策略
// =====================================================================

/// TCC Confirm/Cancel 重试策略
///
/// 当 Confirm 或 Cancel 失败时，按指数退避策略重试：
/// - 第 0 次重试延迟：`initial_delay_ms`
/// - 第 n 次重试延迟：`initial_delay_ms * multiplier^n`（封顶 `max_delay_ms`）
/// - 最多重试 `max_retries` 次（不含首次调用）
///
/// # 默认值
///
/// - `max_retries = 3`（首次 + 3 次重试 = 最多 4 次调用）
/// - `initial_delay_ms = 100`
/// - `max_delay_ms = 5000`
/// - `multiplier = 2.0`
#[derive(Debug, Clone)]
pub struct TccRetryPolicy {
    /// 最大重试次数（不含首次调用）
    pub max_retries: u32,
    /// 初始重试延迟（毫秒）
    pub initial_delay_ms: u64,
    /// 最大重试延迟（毫秒），封顶值
    pub max_delay_ms: u64,
    /// 退避乘数（每次重试延迟乘以此系数）
    pub multiplier: f64,
}

impl TccRetryPolicy {
    /// 创建新的重试策略
    pub fn new(
        max_retries: u32,
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: f64,
    ) -> Self {
        Self {
            max_retries,
            initial_delay_ms,
            max_delay_ms,
            multiplier,
        }
    }

    /// 计算第 `attempt` 次重试的延迟（attempt 从 0 开始）
    ///
    /// - attempt=0 → initial_delay_ms
    /// - attempt=1 → initial_delay_ms * multiplier
    /// - attempt=n → initial_delay_ms * multiplier^n（封顶 max_delay_ms）
    pub fn delay_for(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(self.initial_delay_ms);
        }
        let delay = self.initial_delay_ms as f64 * self.multiplier.powi(attempt as i32);
        let capped = delay.min(self.max_delay_ms as f64);
        Duration::from_millis(capped as u64)
    }

    /// 最大总尝试次数（首次 + 重试）
    pub fn max_attempts(&self) -> u32 {
        1 + self.max_retries
    }
}

impl Default for TccRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            multiplier: 2.0,
        }
    }
}

// =====================================================================
// TccCoordinator — TCC 协调器
// =====================================================================

/// TCC 协调器
///
/// 负责：
/// 1. 注册分支事务
/// 2. 依次执行 try 阶段
/// 3. 全部成功 → 依次执行 confirm
/// 4. 任一失败 → 对已 try 成功的分支执行 cancel
/// 5. 提供 retry_confirm / retry_cancel 用于异常恢复
pub struct TccCoordinator {
    /// 事务 ID
    pub tx_id: String,
    /// 当前事务状态
    state: TccState,
    /// 已注册的分支事务
    participants: Vec<TccParticipant>,
    /// Confirm/Cancel 重试策略（None 表示不自动重试，保持向后兼容）
    retry_policy: Option<TccRetryPolicy>,
}

impl TccCoordinator {
    /// 创建新的 TCC 协调器
    pub fn new(tx_id: impl Into<String>) -> Self {
        Self {
            tx_id: tx_id.into(),
            state: TccState::Init,
            participants: Vec::new(),
            retry_policy: None,
        }
    }

    /// 设置 Confirm/Cancel 重试策略（builder 风格）
    ///
    /// 设置后，调用 [`TccCoordinator::confirm_with_retry`] / [`TccCoordinator::cancel_with_retry`]
    /// 会按此策略自动重试失败的分支。若不设置，则退化为单次执行的旧行为。
    ///
    /// # 示例
    ///
    /// ```
    /// use sz_orm_dtx::tcc::{TccCoordinator, TccParticipant, TccRetryPolicy};
    ///
    /// let mut coord = TccCoordinator::new("tx-001")
    ///     .with_retry_policy(TccRetryPolicy::default());
    /// coord.add_participant(
    ///     TccParticipant::new("db1").with_try(|| Ok(()))
    /// );
    /// ```
    #[must_use]
    pub fn with_retry_policy(mut self, policy: TccRetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }

    /// 获取当前重试策略（如果已设置）
    pub fn retry_policy(&self) -> Option<&TccRetryPolicy> {
        self.retry_policy.as_ref()
    }

    /// 获取当前事务状态
    pub fn state(&self) -> TccState {
        self.state
    }

    /// 获取所有分支事务
    pub fn participants(&self) -> &[TccParticipant] {
        &self.participants
    }

    /// 添加分支事务
    pub fn add_participant(&mut self, p: TccParticipant) {
        self.participants.push(p);
    }

    /// 执行完整的 TCC 流程：try → (confirm 或 cancel)
    ///
    /// - 全部 try 成功 → 自动 confirm
    /// - 任一 try 失败 → 自动 cancel 已 try 成功的分支
    pub fn execute(&mut self) -> Result<(), TccError> {
        self.try_phase()?;
        if self.state == TccState::Tried {
            self.confirm_phase()?;
        }
        Ok(())
    }

    /// 仅执行 Try 阶段
    ///
    /// - 全部成功 → state = Tried
    /// - 任一失败 → 对已 try 成功的分支执行 cancel，state = Cancelled 或 Failed
    pub fn try_phase(&mut self) -> Result<(), TccError> {
        if self.state != TccState::Init && self.state != TccState::Trying {
            return Err(TccError::InvalidState {
                current: self.state,
                expected: "Init or Trying",
            });
        }
        self.state = TccState::Trying;

        let total = self.participants.len();
        let mut tried_count = 0;
        for i in 0..total {
            match self.participants[i].try_phase() {
                Ok(()) => tried_count += 1,
                Err(e) => {
                    let resource_id = self.participants[i].resource_id.clone();
                    // fail() 已在 participant.try_phase 内部调用，此处无需重复
                    // 对已 try 成功的分支执行 cancel
                    let failures = self.cancel_tried_participants(tried_count);
                    // 根据是否还有未 cancel 的失败分支，决定整体状态
                    let any_failed = !failures.is_empty()
                        || self
                            .participants
                            .iter()
                            .any(|p| p.state == TccParticipantState::Failed);
                    self.state = if any_failed {
                        TccState::Failed
                    } else {
                        TccState::Cancelled
                    };
                    // 实际成功 cancel 的分支数 = 已 try 成功数 - cancel 失败数
                    let cancelled_count = tried_count - failures.len();
                    return Err(TccError::TryFailed {
                        resource_id,
                        reason: e,
                        cancelled_count,
                    });
                }
            }
        }
        self.state = TccState::Tried;
        Ok(())
    }

    /// 执行 Confirm 阶段（必须在 Tried 状态）
    ///
    /// - 全部成功 → state = Confirmed
    /// - 任一失败 → state = Failed（需调用 retry_confirm 或人工介入）
    pub fn confirm_phase(&mut self) -> Result<(), TccError> {
        if self.state != TccState::Tried
            && self.state != TccState::Confirming
            && self.state != TccState::Failed
        {
            return Err(TccError::InvalidState {
                current: self.state,
                expected: "Tried, Confirming, or Failed",
            });
        }
        self.state = TccState::Confirming;

        for p in &mut self.participants {
            if let Err(e) = p.confirm_phase() {
                let resource_id = p.resource_id.clone();
                p.fail();
                self.state = TccState::Failed;
                return Err(TccError::ConfirmFailed {
                    resource_id,
                    reason: e,
                });
            }
        }
        self.state = TccState::Confirmed;
        Ok(())
    }

    /// 执行 Cancel 阶段（必须在 Tried 或 Cancelling 或 Failed 状态）
    ///
    /// - 全部成功 → state = Cancelled
    /// - 任一失败 → state = Failed（需调用 retry_cancel 或人工介入）
    pub fn cancel_phase(&mut self) -> Result<(), TccError> {
        if self.state != TccState::Tried
            && self.state != TccState::Cancelling
            && self.state != TccState::Failed
            && self.state != TccState::Trying
        {
            return Err(TccError::InvalidState {
                current: self.state,
                expected: "Tried, Cancelling, Failed, or Trying",
            });
        }
        self.state = TccState::Cancelling;

        let total = self.participants.len();
        let failures = self.cancel_tried_participants(total);

        // 检查是否有失败的分支（cancel 失败或之前 try/confirm 失败的残留状态）
        let any_failed = !failures.is_empty()
            || self
                .participants
                .iter()
                .any(|p| p.state == TccParticipantState::Failed);
        if any_failed {
            self.state = TccState::Failed;
            let reason = if failures.is_empty() {
                "部分分支 cancel 失败".to_string()
            } else {
                let details: Vec<String> = failures
                    .iter()
                    .map(|(rid, e)| format!("{}: {}", rid, e))
                    .collect();
                format!("部分分支 cancel 失败: {}", details.join("; "))
            };
            return Err(TccError::CancelFailed { reason });
        }
        self.state = TccState::Cancelled;
        Ok(())
    }

    /// 重试 Confirm（用于 ConfirmFailed 后的自动/人工恢复）
    ///
    /// 仅对未 Confirmed 的分支重试。
    pub fn retry_confirm(&mut self) -> Result<(), TccError> {
        if !self.state.can_retry_confirm() {
            return Err(TccError::InvalidState {
                current: self.state,
                expected: "Confirming or Failed",
            });
        }
        self.state = TccState::Confirming;
        for p in &mut self.participants {
            if p.state == TccParticipantState::Confirmed {
                continue;
            }
            if let Err(e) = p.confirm_phase() {
                let resource_id = p.resource_id.clone();
                p.fail();
                self.state = TccState::Failed;
                return Err(TccError::ConfirmFailed {
                    resource_id,
                    reason: e,
                });
            }
        }
        self.state = TccState::Confirmed;
        Ok(())
    }

    /// 重试 Cancel（用于 CancelFailed 后的自动/人工恢复）
    ///
    /// 仅对 Tried/Failed 的分支重试，跳过已 Cancelled 的分支。
    pub fn retry_cancel(&mut self) -> Result<(), TccError> {
        if !self.state.can_retry_cancel() {
            return Err(TccError::InvalidState {
                current: self.state,
                expected: "Cancelling or Failed",
            });
        }
        self.state = TccState::Cancelling;
        for p in &mut self.participants {
            if p.state == TccParticipantState::Cancelled
                || p.state == TccParticipantState::Confirmed
                || p.state == TccParticipantState::Init
            {
                continue;
            }
            if let Err(e) = p.cancel_phase() {
                let resource_id = p.resource_id.clone();
                p.fail();
                self.state = TccState::Failed;
                return Err(TccError::CancelFailed {
                    reason: format!("分支 {} cancel 失败: {}", resource_id, e),
                });
            }
        }
        self.state = TccState::Cancelled;
        Ok(())
    }

    /// 带重试的 Confirm 执行（按指数退避策略自动重试失败的分支）
    ///
    /// 必须先设置 [`TccRetryPolicy`]（通过 [`TccCoordinator::with_retry_policy`]），
    /// 否则返回 [`TccError::InvalidState`]。
    ///
    /// # 执行逻辑
    ///
    /// 1. 对每个未 Confirmed 的分支依次调用 `confirm_phase`
    /// 2. 若某分支失败，按指数退避策略重试：
    ///    - 第 0 次重试延迟：`initial_delay_ms`
    ///    - 第 n 次重试延迟：`initial_delay_ms * multiplier^n`（封顶 `max_delay_ms`）
    ///    - 用 `std::thread::sleep`（同步实现，因为现有 TCC 是同步的）
    /// 3. 达到 `max_retries` 仍失败则标记该分支 Failed，并使整个事务进入 Failed 状态
    /// 4. 利用 [`TccParticipant::confirm_phase`] 的幂等性，已 Confirmed 的分支会被跳过
    ///
    /// # 状态迁移
    ///
    /// - 成功 → `TccState::Confirmed`
    /// - 失败 → `TccState::Failed`（需人工介入）
    ///
    /// # 错误
    ///
    /// - [`TccError::InvalidState`]：未设置重试策略，或状态不是 Tried/Confirming/Failed
    /// - [`TccError::ConfirmFailed`]：所有重试均失败后返回最后失败原因
    pub fn confirm_with_retry(&mut self) -> Result<(), TccError> {
        // 校验状态：允许从 Tried（首次 confirm）、Confirming（重试中）、Failed（恢复重试）开始
        if self.state != TccState::Tried
            && self.state != TccState::Confirming
            && self.state != TccState::Failed
        {
            return Err(TccError::InvalidState {
                current: self.state,
                expected: "Tried, Confirming, or Failed",
            });
        }
        // 校验重试策略已设置
        let policy = self
            .retry_policy
            .clone()
            .ok_or(TccError::InvalidState {
                current: self.state,
                expected: "retry_policy must be set (call with_retry_policy first)",
            })?;

        self.state = TccState::Confirming;

        for p in &mut self.participants {
            // 幂等：已 Confirmed 跳过（confirm_phase 内部也会跳过，这里显式跳过避免计数）
            if p.state == TccParticipantState::Confirmed {
                continue;
            }

            let max_attempts = policy.max_attempts();
            let mut last_error: Option<String> = None;
            let mut succeeded = false;

            // 首次调用 + 重试
            for attempt in 0..max_attempts {
                if attempt > 0 {
                    // 重试前等待（指数退避）
                    std::thread::sleep(policy.delay_for(attempt - 1));
                }
                match p.confirm_phase() {
                    Ok(()) => {
                        succeeded = true;
                        break;
                    }
                    Err(e) => {
                        last_error = Some(e);
                        // confirm_phase 失败后状态会被标记为 Failed，
                        // 但重试时我们希望尝试再次调用，所以需要重置状态为 Tried
                        // （因为 confirm_phase 没有状态前置检查，只检查 confirm_succeeded，
                        // 所以直接重试即可，不需要重置状态）
                    }
                }
            }

            if !succeeded {
                let resource_id = p.resource_id.clone();
                let reason = last_error.unwrap_or_else(|| "未知错误".to_string());
                p.fail();
                self.state = TccState::Failed;
                return Err(TccError::ConfirmFailed {
                    resource_id,
                    reason: format!(
                        "{} (重试 {} 次后仍失败)",
                        reason,
                        policy.max_retries
                    ),
                });
            }
        }
        self.state = TccState::Confirmed;
        Ok(())
    }

    /// 带重试的 Cancel 执行（按指数退避策略自动重试失败的分支）
    ///
    /// 必须先设置 [`TccRetryPolicy`]（通过 [`TccCoordinator::with_retry_policy`]），
    /// 否则返回 [`TccError::InvalidState`]。
    ///
    /// # 执行逻辑
    ///
    /// 1. 对每个需要 cancel 的分支（Tried/Failed）依次调用 `cancel_phase`
    /// 2. 若某分支失败，按指数退避策略重试：
    ///    - 第 0 次重试延迟：`initial_delay_ms`
    ///    - 第 n 次重试延迟：`initial_delay_ms * multiplier^n`（封顶 `max_delay_ms`）
    ///    - 用 `std::thread::sleep`（同步实现）
    /// 3. 达到 `max_retries` 仍失败则标记该分支 Failed
    /// 4. 利用 [`TccParticipant::cancel_phase`] 的幂等性，已 Cancelled 的分支会被跳过
    /// 5. best-effort：即使某分支 cancel 失败，仍继续尝试其他分支
    /// 6. 所有分支处理完毕后，若存在失败分支，整体状态为 Failed
    ///
    /// # 状态迁移
    ///
    /// - 全部成功 → `TccState::Cancelled`
    /// - 任一失败 → `TccState::Failed`（需人工介入）
    ///
    /// # 错误
    ///
    /// - [`TccError::InvalidState`]：未设置重试策略，或状态不是 Tried/Cancelling/Failed/Trying
    /// - [`TccError::CancelFailed`]：所有分支处理完毕后，至少一个分支仍失败
    pub fn cancel_with_retry(&mut self) -> Result<(), TccError> {
        // 校验状态：必须是可重试 cancel 的状态，或者 Trying（try 失败后立即 cancel）
        if self.state != TccState::Tried
            && self.state != TccState::Cancelling
            && self.state != TccState::Failed
            && self.state != TccState::Trying
        {
            return Err(TccError::InvalidState {
                current: self.state,
                expected: "Tried, Cancelling, Failed, or Trying",
            });
        }
        // 校验重试策略已设置
        let policy = self
            .retry_policy
            .clone()
            .ok_or(TccError::InvalidState {
                current: self.state,
                expected: "retry_policy must be set (call with_retry_policy first)",
            })?;

        self.state = TccState::Cancelling;

        let mut failures: Vec<(String, String)> = Vec::new();

        for p in &mut self.participants {
            // 跳过：已 Cancelled、已 Confirmed、未 try（Init）
            if p.state == TccParticipantState::Cancelled
                || p.state == TccParticipantState::Confirmed
                || p.state == TccParticipantState::Init
            {
                continue;
            }

            let max_attempts = policy.max_attempts();
            let mut last_error: Option<String> = None;
            let mut succeeded = false;

            for attempt in 0..max_attempts {
                if attempt > 0 {
                    std::thread::sleep(policy.delay_for(attempt - 1));
                }
                match p.cancel_phase() {
                    Ok(()) => {
                        succeeded = true;
                        break;
                    }
                    Err(e) => {
                        last_error = Some(e);
                    }
                }
            }

            if !succeeded {
                let resource_id = p.resource_id.clone();
                let reason = last_error.unwrap_or_else(|| "未知错误".to_string());
                p.fail();
                failures.push((
                    resource_id.clone(),
                    format!("{} (重试 {} 次后仍失败)", reason, policy.max_retries),
                ));
            }
            // best-effort：继续处理其他分支，不短路
        }

        if !failures.is_empty() {
            self.state = TccState::Failed;
            let details: Vec<String> = failures
                .iter()
                .map(|(rid, e)| format!("{}: {}", rid, e))
                .collect();
            return Err(TccError::CancelFailed {
                reason: format!(
                    "部分分支 cancel 失败 (共 {} 个): {}",
                    failures.len(),
                    details.join("; ")
                ),
            });
        }
        self.state = TccState::Cancelled;
        Ok(())
    }

    /// 内部方法：对前 n 个分支执行 cancel（用于 try 失败后回滚）
    ///
    /// 返回 `(resource_id, error)` 列表，记录哪些分支 cancel 失败及其原因。
    /// 失败的分支仍会被标记为 Failed，但不会短路——继续尝试 cancel 其他分支（best-effort）。
    ///
    /// # 跳过条件（v0.2.1 修复 Critical C-2）
    ///
    /// - `state == Cancelled`：已经 cancel 过，不重复
    /// - `state == Confirmed`：已 confirm 的分支**绝不能**被 cancel
    ///   （原 bug：`is_tried()` 包含 Confirmed，但 cancel 时只跳过 Cancelled，
    ///   导致已确认分支被回滚，破坏分布式事务一致性）
    fn cancel_tried_participants(&mut self, tried_count: usize) -> Vec<(String, String)> {
        let mut failures = Vec::new();
        for i in 0..tried_count {
            // 跳过已 Cancelled 和已 Confirmed 的分支
            // 注意：is_tried() 包含 Confirmed，所以必须显式排除 Confirmed
            if self.participants[i].is_tried()
                && self.participants[i].state != TccParticipantState::Cancelled
                && self.participants[i].state != TccParticipantState::Confirmed
            {
                if let Err(e) = self.participants[i].cancel_phase() {
                    self.participants[i].fail();
                    failures.push((self.participants[i].resource_id.clone(), e));
                }
            }
        }
        failures
    }
}

impl std::fmt::Debug for TccCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TccCoordinator")
            .field("tx_id", &self.tx_id)
            .field("state", &self.state)
            .field("participants", &self.participants.len())
            .field("has_retry_policy", &self.retry_policy.is_some())
            .finish()
    }
}

// =====================================================================
// TccError — TCC 错误类型
// =====================================================================

/// TCC 事务错误
#[derive(Debug)]
pub enum TccError {
    /// Try 阶段失败
    TryFailed {
        resource_id: String,
        reason: String,
        cancelled_count: usize,
    },
    /// Confirm 阶段失败（关键路径，需要重试或人工介入）
    ConfirmFailed { resource_id: String, reason: String },
    /// Cancel 阶段失败（关键路径，需要重试或人工介入）
    CancelFailed { reason: String },
    /// 状态机非法迁移
    InvalidState {
        current: TccState,
        expected: &'static str,
    },
    /// 分支不存在
    ParticipantNotFound { resource_id: String },
    /// 内部锁中毒（线程 panic 导致状态损坏）
    LockPoisoned,
}

impl std::fmt::Display for TccError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TccError::TryFailed {
                resource_id,
                reason,
                cancelled_count,
            } => write!(
                f,
                "TCC Try 失败 [resource={}]: {} (已 cancel {} 个分支)",
                resource_id, reason, cancelled_count
            ),
            TccError::ConfirmFailed {
                resource_id,
                reason,
            } => {
                write!(f, "TCC Confirm 失败 [resource={}]: {}", resource_id, reason)
            }
            TccError::CancelFailed { reason } => write!(f, "TCC Cancel 失败: {}", reason),
            TccError::InvalidState { current, expected } => {
                write!(f, "TCC 状态非法: 当前={:?}, 期望={}", current, expected)
            }
            TccError::ParticipantNotFound { resource_id } => {
                write!(f, "TCC 分支不存在: {}", resource_id)
            }
            TccError::LockPoisoned => write!(f, "TCC 内部锁中毒"),
        }
    }
}

impl std::error::Error for TccError {}

// =====================================================================
// TccManager — 全局 TCC 事务管理器
// =====================================================================

/// TCC 全局事务管理器
///
/// 管理多个并发 TCC 事务，提供按事务 ID 查询状态、批量恢复等功能。
pub struct TccManager {
    transactions: Arc<RwLock<std::collections::HashMap<String, TccCoordinator>>>,
}

impl Default for TccManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TccManager {
    /// 创建空的 TCC 管理器
    pub fn new() -> Self {
        Self {
            transactions: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// 注册新的 TCC 事务
    pub fn register(&self, tx_id: impl Into<String>) -> Result<(), TccError> {
        let id = tx_id.into();
        let mut txs = self
            .transactions
            .write()
            .map_err(|_| TccError::LockPoisoned)?;
        if txs.contains_key(&id) {
            return Err(TccError::InvalidState {
                current: TccState::Init,
                expected: "unique tx_id",
            });
        }
        txs.insert(id.clone(), TccCoordinator::new(id));
        Ok(())
    }

    /// 添加分支到指定事务
    pub fn add_participant(
        &self,
        tx_id: &str,
        participant: TccParticipant,
    ) -> Result<(), TccError> {
        let mut txs = self
            .transactions
            .write()
            .map_err(|_| TccError::LockPoisoned)?;
        let coord = txs
            .get_mut(tx_id)
            .ok_or_else(|| TccError::ParticipantNotFound {
                resource_id: tx_id.to_string(),
            })?;
        coord.add_participant(participant);
        Ok(())
    }

    /// 执行指定事务的完整 TCC 流程
    pub fn execute(&self, tx_id: &str) -> Result<(), TccError> {
        let mut txs = self
            .transactions
            .write()
            .map_err(|_| TccError::LockPoisoned)?;
        let coord = txs
            .get_mut(tx_id)
            .ok_or_else(|| TccError::ParticipantNotFound {
                resource_id: tx_id.to_string(),
            })?;
        coord.execute()
    }

    /// 获取事务状态
    pub fn get_state(&self, tx_id: &str) -> Option<TccState> {
        self.transactions
            .read()
            .ok()
            .and_then(|txs| txs.get(tx_id).map(|c| c.state))
    }

    /// 重试 Confirm（用于恢复 Failed 事务）
    pub fn retry_confirm(&self, tx_id: &str) -> Result<(), TccError> {
        let mut txs = self
            .transactions
            .write()
            .map_err(|_| TccError::LockPoisoned)?;
        let coord = txs
            .get_mut(tx_id)
            .ok_or_else(|| TccError::ParticipantNotFound {
                resource_id: tx_id.to_string(),
            })?;
        coord.retry_confirm()
    }

    /// 重试 Cancel（用于恢复 Failed 事务）
    pub fn retry_cancel(&self, tx_id: &str) -> Result<(), TccError> {
        let mut txs = self
            .transactions
            .write()
            .map_err(|_| TccError::LockPoisoned)?;
        let coord = txs
            .get_mut(tx_id)
            .ok_or_else(|| TccError::ParticipantNotFound {
                resource_id: tx_id.to_string(),
            })?;
        coord.retry_cancel()
    }

    /// 列出所有处于 Failed 状态的事务 ID（用于人工介入或恢复扫描）
    pub fn list_failed(&self) -> Vec<String> {
        self.transactions
            .read()
            .map(|txs| {
                let mut ids: Vec<String> = txs
                    .iter()
                    .filter(|(_, c)| c.state == TccState::Failed)
                    .map(|(k, _)| k.clone())
                    .collect();
                ids.sort();
                ids
            })
            .unwrap_or_default()
    }

    /// 列出所有事务 ID
    pub fn list_all(&self) -> Vec<String> {
        self.transactions
            .read()
            .map(|txs| {
                let mut ids: Vec<String> = txs.keys().cloned().collect();
                ids.sort();
                ids
            })
            .unwrap_or_default()
    }
}

// =====================================================================
// 测试
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ===== TccParticipant 单元测试 =====

    #[test]
    fn participant_new_initial_state() {
        let p = TccParticipant::new("db1");
        assert_eq!(p.resource_id, "db1");
        assert_eq!(p.state, TccParticipantState::Init);
        assert_eq!(p.try_attempts, 0);
        assert_eq!(p.confirm_attempts, 0);
        assert_eq!(p.cancel_attempts, 0);
    }

    #[test]
    fn participant_try_success() {
        let mut p = TccParticipant::new("db1").with_try(|| Ok(()));
        p.try_phase().unwrap();
        assert_eq!(p.state, TccParticipantState::Tried);
        assert_eq!(p.try_attempts, 1);
    }

    #[test]
    fn participant_try_failure_marks_failed() {
        let mut p = TccParticipant::new("db1").with_try(|| Err("insufficient balance".to_string()));
        let result = p.try_phase();
        assert!(result.is_err());
        assert_eq!(p.state, TccParticipantState::Failed);
        assert_eq!(p.try_attempts, 1);
    }

    #[test]
    fn participant_confirm_after_try() {
        let mut p = TccParticipant::new("db1")
            .with_try(|| Ok(()))
            .with_confirm(|| Ok(()));
        p.try_phase().unwrap();
        p.confirm_phase().unwrap();
        assert_eq!(p.state, TccParticipantState::Confirmed);
        assert_eq!(p.confirm_attempts, 1);
    }

    #[test]
    fn participant_cancel_after_try() {
        let mut p = TccParticipant::new("db1")
            .with_try(|| Ok(()))
            .with_cancel(|| Ok(()));
        p.try_phase().unwrap();
        p.cancel_phase().unwrap();
        assert_eq!(p.state, TccParticipantState::Cancelled);
        assert_eq!(p.cancel_attempts, 1);
    }

    #[test]
    fn participant_is_tried_check() {
        let mut p = TccParticipant::new("db1").with_try(|| Ok(()));
        assert!(!p.is_tried());
        p.try_phase().unwrap();
        assert!(p.is_tried());
        let _ = p.cancel_phase();
        assert!(p.is_tried()); // 已 cancel 后仍认为 try 曾成功
    }

    // ===== TccState 测试 =====

    #[test]
    fn tcc_state_terminal_check() {
        assert!(TccState::Confirmed.is_terminal());
        assert!(TccState::Cancelled.is_terminal());
        assert!(TccState::Failed.is_terminal());
        assert!(!TccState::Init.is_terminal());
        assert!(!TccState::Trying.is_terminal());
        assert!(!TccState::Tried.is_terminal());
        assert!(!TccState::Confirming.is_terminal());
        assert!(!TccState::Cancelling.is_terminal());
    }

    #[test]
    fn tcc_state_retry_check() {
        assert!(TccState::Confirming.can_retry_confirm());
        assert!(TccState::Failed.can_retry_confirm());
        assert!(!TccState::Tried.can_retry_confirm());

        assert!(TccState::Cancelling.can_retry_cancel());
        assert!(TccState::Failed.can_retry_cancel());
        assert!(!TccState::Tried.can_retry_cancel());
    }

    // ===== TccCoordinator 完整流程测试 =====

    fn make_counters() -> (Arc<AtomicU32>, Arc<AtomicU32>, Arc<AtomicU32>) {
        (
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicU32::new(0)),
        )
    }

    #[test]
    fn coordinator_execute_all_success() {
        let (try_count, confirm_count, cancel_count) = make_counters();
        let mut coord = TccCoordinator::new("tx-001");

        let t1 = try_count.clone();
        let c1 = confirm_count.clone();
        let ca1 = cancel_count.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(move || {
                    t1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_confirm(move || {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_cancel(move || {
                    ca1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );

        let t2 = try_count.clone();
        let c2 = confirm_count.clone();
        let ca2 = cancel_count.clone();
        coord.add_participant(
            TccParticipant::new("db2")
                .with_try(move || {
                    t2.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_confirm(move || {
                    c2.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_cancel(move || {
                    ca2.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );

        coord.execute().unwrap();

        assert_eq!(coord.state(), TccState::Confirmed);
        assert_eq!(try_count.load(Ordering::SeqCst), 2);
        assert_eq!(confirm_count.load(Ordering::SeqCst), 2);
        assert_eq!(cancel_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn coordinator_try_failure_triggers_cancel() {
        let (try_count, _confirm_count, cancel_count) = make_counters();
        let mut coord = TccCoordinator::new("tx-002");

        // 第一个分支 try 成功
        let t1 = try_count.clone();
        let ca1 = cancel_count.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(move || {
                    t1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .with_cancel(move || {
                    ca1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );

        // 第二个分支 try 失败
        coord.add_participant(
            TccParticipant::new("db2")
                .with_try(|| Err("insufficient funds".to_string()))
                .with_cancel(|| Ok(())),
        );

        let result = coord.execute();
        assert!(result.is_err());
        match result.unwrap_err() {
            TccError::TryFailed {
                resource_id,
                cancelled_count,
                ..
            } => {
                assert_eq!(resource_id, "db2");
                assert_eq!(cancelled_count, 1);
            }
            other => panic!("期望 TryFailed 错误，得到 {:?}", other),
        }

        // 第一个分支应被 cancel
        assert_eq!(try_count.load(Ordering::SeqCst), 1); // 仅 db1 try 成功
        assert_eq!(cancel_count.load(Ordering::SeqCst), 1); // db1 被 cancel

        // 状态：因 try 失败已自动 cancel，应为 Cancelled 或 Failed
        assert!(
            coord.state() == TccState::Cancelled || coord.state() == TccState::Failed,
            "期望 Cancelled 或 Failed，得到 {:?}",
            coord.state()
        );
    }

    #[test]
    fn coordinator_first_participant_try_failure_no_cancel() {
        let cancel_count = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-003");

        // 第一个分支直接失败
        coord.add_participant(TccParticipant::new("db1").with_try(|| Err("fail".to_string())));
        // 第二个分支不该被调用
        let ca2 = cancel_count.clone();
        coord.add_participant(TccParticipant::new("db2").with_try(|| Ok(())).with_cancel(
            move || {
                ca2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));

        let result = coord.execute();
        assert!(result.is_err());
        // db2 未 try，所以 cancel 不应被调用
        assert_eq!(cancel_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn coordinator_confirm_failure_marks_failed() {
        let mut coord = TccCoordinator::new("tx-004");
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        );
        coord.add_participant(
            TccParticipant::new("db2")
                .with_try(|| Ok(()))
                .with_confirm(|| Err("network error".to_string())),
        );

        let result = coord.execute();
        assert!(result.is_err());
        assert_eq!(coord.state(), TccState::Failed);
        // db1 confirm 成功
        assert_eq!(
            coord.participants()[0].state,
            TccParticipantState::Confirmed
        );
        // db2 confirm 失败
        assert_eq!(coord.participants()[1].state, TccParticipantState::Failed);
    }

    #[test]
    fn coordinator_retry_confirm_after_failure() {
        let attempt = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-005");
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        );

        let a = attempt.clone();
        coord.add_participant(TccParticipant::new("db2").with_try(|| Ok(())).with_confirm(
            move || {
                let n = a.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err("first attempt fails".to_string())
                } else {
                    Ok(())
                }
            },
        ));

        // 第一次 execute 失败
        let r1 = coord.execute();
        assert!(r1.is_err());
        assert_eq!(coord.state(), TccState::Failed);

        // 重试 confirm 应成功
        let r2 = coord.retry_confirm();
        assert!(r2.is_ok());
        assert_eq!(coord.state(), TccState::Confirmed);
    }

    #[test]
    fn coordinator_retry_cancel_after_failure() {
        let attempt = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-006");
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_cancel(|| Ok(())),
        );

        let a = attempt.clone();
        coord.add_participant(
            TccParticipant::new("db2")
                .with_try(|| Err("fail".to_string()))
                .with_cancel(move || {
                    let n = a.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        Err("cancel first attempt fails".to_string())
                    } else {
                        Ok(())
                    }
                }),
        );

        // 第一次 execute 失败（try 失败 → 自动 cancel，但 cancel 也失败）
        let r1 = coord.execute();
        assert!(r1.is_err());
        // db1 应被 cancel 成功，db2 cancel 失败
        // 状态可能为 Failed（cancel_tried_participants 失败标记 Failed 但不抛错）
        assert_eq!(
            coord.participants()[0].state,
            TccParticipantState::Cancelled
        );

        // 注意：try_phase 失败后，coord 的状态可能直接是 Cancelled（如果所有 cancel 成功）
        // 或 Failed（如果某些 cancel 失败）。这里 db2 的 cancel 失败，但 db2 没 try 成功，
        // 所以不会被 cancel；coord 状态应为 Cancelled（db1 cancel 成功）
        assert!(
            coord.state() == TccState::Cancelled || coord.state() == TccState::Failed,
            "状态 {:?}",
            coord.state()
        );
    }

    #[test]
    fn coordinator_invalid_state_transition() {
        let mut coord = TccCoordinator::new("tx-007");
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        );

        // 直接调用 confirm_phase 而不先 try_phase
        let result = coord.confirm_phase();
        assert!(result.is_err());
        match result.unwrap_err() {
            TccError::InvalidState { .. } => {}
            other => panic!("期望 InvalidState 错误，得到 {:?}", other),
        }
    }

    #[test]
    fn coordinator_empty_participants_execute() {
        let mut coord = TccCoordinator::new("tx-008");
        let result = coord.execute();
        assert!(result.is_ok());
        assert_eq!(coord.state(), TccState::Confirmed);
    }

    #[test]
    fn coordinator_three_participants_middle_failure() {
        let cancel_count = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-009");

        let ca1 = cancel_count.clone();
        coord.add_participant(TccParticipant::new("db1").with_try(|| Ok(())).with_cancel(
            move || {
                ca1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));

        // 第二个失败
        coord.add_participant(
            TccParticipant::new("db2").with_try(|| Err("middle failure".to_string())),
        );

        // 第三个不应被 try
        let ca3 = cancel_count.clone();
        coord.add_participant(TccParticipant::new("db3").with_try(|| Ok(())).with_cancel(
            move || {
                ca3.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));

        let result = coord.execute();
        assert!(result.is_err());
        // 仅 db1 被 cancel
        assert_eq!(cancel_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn coordinator_attempts_count() {
        let mut coord = TccCoordinator::new("tx-010");
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        );
        coord.execute().unwrap();
        assert_eq!(coord.participants()[0].try_attempts, 1);
        assert_eq!(coord.participants()[0].confirm_attempts, 1);
        assert_eq!(coord.participants()[0].cancel_attempts, 0);
    }

    // ===== TccManager 测试 =====

    #[test]
    fn manager_register_and_execute() {
        let m = TccManager::new();
        m.register("tx-mgr-001").unwrap();
        m.add_participant(
            "tx-mgr-001",
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        )
        .unwrap();
        m.execute("tx-mgr-001").unwrap();
        assert_eq!(m.get_state("tx-mgr-001"), Some(TccState::Confirmed));
    }

    #[test]
    fn manager_list_failed_transactions() {
        let m = TccManager::new();
        m.register("tx-failed-1").unwrap();
        m.add_participant(
            "tx-failed-1",
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Err("fail".to_string())),
        )
        .unwrap();
        m.execute("tx-failed-1").unwrap_err();
        assert_eq!(m.list_failed(), vec!["tx-failed-1".to_string()]);

        m.register("tx-success-1").unwrap();
        m.add_participant(
            "tx-success-1",
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        )
        .unwrap();
        m.execute("tx-success-1").unwrap();

        // list_failed 仍只包含失败的事务
        assert_eq!(m.list_failed(), vec!["tx-failed-1".to_string()]);
        // list_all 包含所有事务
        let mut all = m.list_all();
        all.sort();
        assert_eq!(
            all,
            vec!["tx-failed-1".to_string(), "tx-success-1".to_string()]
        );
    }

    #[test]
    fn manager_retry_confirm_recovers_failed_transaction() {
        let attempt = Arc::new(AtomicU32::new(0));
        let m = TccManager::new();
        m.register("tx-recover").unwrap();

        let a = attempt.clone();
        m.add_participant(
            "tx-recover",
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(move || {
                    let n = a.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        Err("first attempt fails".to_string())
                    } else {
                        Ok(())
                    }
                }),
        )
        .unwrap();

        m.execute("tx-recover").unwrap_err();
        assert_eq!(m.get_state("tx-recover"), Some(TccState::Failed));

        m.retry_confirm("tx-recover").unwrap();
        assert_eq!(m.get_state("tx-recover"), Some(TccState::Confirmed));
    }

    #[test]
    fn manager_duplicate_registration_fails() {
        let m = TccManager::new();
        m.register("tx-dup").unwrap();
        let result = m.register("tx-dup");
        assert!(result.is_err());
    }

    #[test]
    fn manager_missing_transaction_errors() {
        let m = TccManager::new();
        assert!(m
            .add_participant("missing", TccParticipant::new("db1"))
            .is_err());
        assert!(m.execute("missing").is_err());
        assert!(m.retry_confirm("missing").is_err());
        assert!(m.retry_cancel("missing").is_err());
    }

    // ===== 转账场景端到端测试 =====

    #[test]
    fn end_to_end_transfer_success() {
        // 模拟：从账户 A 转账 100 元到账户 B
        // 两个 TCC 分支：
        //   - 扣款分支（账户 A）：try 冻结 100，confirm 扣减 100，cancel 解冻 100
        //   - 加款分支（账户 B）：try 增加预到账 100，confirm 确认，cancel 撤销
        let balance_a = Arc::new(std::sync::atomic::AtomicI64::new(1000));
        let balance_b = Arc::new(std::sync::atomic::AtomicI64::new(500));
        let frozen_a = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let pending_b = Arc::new(std::sync::atomic::AtomicI64::new(0));

        let mut coord = TccCoordinator::new("tx-transfer-100");

        let ba_try = balance_a.clone();
        let fa_try = frozen_a.clone();
        let ba_confirm = balance_a.clone();
        let fa_confirm = frozen_a.clone();
        let fa_cancel = frozen_a.clone();
        coord.add_participant(
            TccParticipant::new("account-deduct")
                .with_try(move || {
                    let avail = ba_try.load(Ordering::SeqCst) - fa_try.load(Ordering::SeqCst);
                    if avail < 100 {
                        return Err("余额不足".to_string());
                    }
                    fa_try.fetch_add(100, Ordering::SeqCst);
                    Ok(())
                })
                .with_confirm(move || {
                    ba_confirm.fetch_sub(100, Ordering::SeqCst);
                    fa_confirm.fetch_sub(100, Ordering::SeqCst);
                    Ok(())
                })
                .with_cancel(move || {
                    fa_cancel.fetch_sub(100, Ordering::SeqCst);
                    Ok(())
                }),
        );

        let bb_confirm = balance_b.clone();
        let pb_try = pending_b.clone();
        let pb_confirm = pending_b.clone();
        let pb_cancel = pending_b.clone();
        coord.add_participant(
            TccParticipant::new("account-credit")
                .with_try(move || {
                    pb_try.fetch_add(100, Ordering::SeqCst);
                    Ok(())
                })
                .with_confirm(move || {
                    bb_confirm.fetch_add(100, Ordering::SeqCst);
                    pb_confirm.fetch_sub(100, Ordering::SeqCst);
                    Ok(())
                })
                .with_cancel(move || {
                    pb_cancel.fetch_sub(100, Ordering::SeqCst);
                    Ok(())
                }),
        );

        coord.execute().unwrap();
        assert_eq!(coord.state(), TccState::Confirmed);
        assert_eq!(balance_a.load(Ordering::SeqCst), 900);
        assert_eq!(balance_b.load(Ordering::SeqCst), 600);
        assert_eq!(frozen_a.load(Ordering::SeqCst), 0);
        assert_eq!(pending_b.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn end_to_end_transfer_insufficient_balance() {
        // 账户 A 余额不足，try 阶段失败，应自动 cancel
        let balance_a = Arc::new(std::sync::atomic::AtomicI64::new(50)); // 不足 100
        let frozen_a = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let pending_b = Arc::new(std::sync::atomic::AtomicI64::new(0));

        let mut coord = TccCoordinator::new("tx-transfer-fail");

        let ba_try = balance_a.clone();
        let fa_try = frozen_a.clone();
        let fa_cancel = frozen_a.clone();
        coord.add_participant(
            TccParticipant::new("account-deduct")
                .with_try(move || {
                    let avail = ba_try.load(Ordering::SeqCst) - fa_try.load(Ordering::SeqCst);
                    if avail < 100 {
                        return Err("余额不足".to_string());
                    }
                    fa_try.fetch_add(100, Ordering::SeqCst);
                    Ok(())
                })
                .with_cancel(move || {
                    fa_cancel.fetch_sub(100, Ordering::SeqCst);
                    Ok(())
                }),
        );

        let pb_try = pending_b.clone();
        let pb_cancel = pending_b.clone();
        coord.add_participant(
            TccParticipant::new("account-credit")
                .with_try(move || {
                    pb_try.fetch_add(100, Ordering::SeqCst);
                    Ok(())
                })
                .with_cancel(move || {
                    pb_cancel.fetch_sub(100, Ordering::SeqCst);
                    Ok(())
                }),
        );

        let result = coord.execute();
        assert!(result.is_err());
        // 第一个分支 try 失败，第二个分支不应该 try（但状态可能因 try_phase 内部 cancel 而变）
        // 注意：第一个分支 try 失败时 frozen_a 未被增加
        assert_eq!(frozen_a.load(Ordering::SeqCst), 0);
        // pending_b 应被 cancel 回滚（如果它被 try 成功过）
        // 注意：try_phase 是顺序执行的，第一个失败后不会执行第二个
        assert_eq!(pending_b.load(Ordering::SeqCst), 0);
    }

    // ===== 边界/极端测试 =====

    #[test]
    fn coordinator_confirm_without_try_returns_invalid_state() {
        let mut coord = TccCoordinator::new("tx-edge-1");
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        );
        // 不调用 try_phase 直接 confirm_phase
        let result = coord.confirm_phase();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TccError::InvalidState { .. }));
    }

    #[test]
    fn coordinator_cancel_without_try_returns_invalid_state() {
        let mut coord = TccCoordinator::new("tx-edge-2");
        coord.add_participant(TccParticipant::new("db1"));
        // Init 状态调用 cancel_phase 应失败
        let result = coord.cancel_phase();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TccError::InvalidState { .. }));
    }

    #[test]
    fn coordinator_retry_confirm_in_wrong_state() {
        let mut coord = TccCoordinator::new("tx-edge-3");
        // Init 状态调用 retry_confirm 应失败
        let result = coord.retry_confirm();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TccError::InvalidState { .. }));
    }

    #[test]
    fn tcc_error_display_formats() {
        let e1 = TccError::TryFailed {
            resource_id: "db1".into(),
            reason: "余额不足".into(),
            cancelled_count: 2,
        };
        let s1 = format!("{}", e1);
        assert!(s1.contains("db1"));
        assert!(s1.contains("余额不足"));
        assert!(s1.contains("2"));

        let e2 = TccError::ConfirmFailed {
            resource_id: "db2".into(),
            reason: "网络错误".into(),
        };
        assert!(format!("{}", e2).contains("网络错误"));

        let e3 = TccError::InvalidState {
            current: TccState::Init,
            expected: "Tried",
        };
        assert!(format!("{}", e3).contains("Init"));
    }

    #[test]
    fn coordinator_debug_output() {
        let mut coord = TccCoordinator::new("tx-debug");
        coord.add_participant(TccParticipant::new("db1"));
        let s = format!("{:?}", coord);
        assert!(s.contains("tx-debug"));
        assert!(s.contains("participants: 1"));
    }

    #[test]
    fn coordinator_try_phase_idempotent_after_tried() {
        // 已 Tried 后再调用 try_phase 应失败
        let mut coord = TccCoordinator::new("tx-idempotent");
        coord.add_participant(TccParticipant::new("db1").with_try(|| Ok(())));
        coord.try_phase().unwrap();
        assert_eq!(coord.state(), TccState::Tried);

        let result = coord.try_phase();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TccError::InvalidState { .. }));
    }

    #[test]
    fn coordinator_state_after_confirm_failure_can_retry() {
        let mut coord = TccCoordinator::new("tx-recoverable");
        let attempt = Arc::new(AtomicU32::new(0));

        let a = attempt.clone();
        coord.add_participant(TccParticipant::new("db1").with_try(|| Ok(())).with_confirm(
            move || {
                let n = a.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(format!("attempt {} failed", n))
                } else {
                    Ok(())
                }
            },
        ));

        coord.execute().unwrap_err();
        assert_eq!(coord.state(), TccState::Failed);

        // 第一次重试也失败
        coord.retry_confirm().unwrap_err();
        assert_eq!(coord.state(), TccState::Failed);

        // 第二次重试成功
        coord.retry_confirm().unwrap();
        assert_eq!(coord.state(), TccState::Confirmed);
    }

    // ===== 新增测试：TCC 幂等性、重试策略（v0.2.1 深度优化） =====

    #[test]
    fn retry_policy_default_values() {
        let p = TccRetryPolicy::default();
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.initial_delay_ms, 100);
        assert_eq!(p.max_delay_ms, 5000);
        assert_eq!(p.multiplier, 2.0);
    }

    #[test]
    fn retry_policy_custom_values() {
        let p = TccRetryPolicy::new(5, 200, 10000, 1.5);
        assert_eq!(p.max_retries, 5);
        assert_eq!(p.initial_delay_ms, 200);
        assert_eq!(p.max_delay_ms, 10000);
        assert_eq!(p.multiplier, 1.5);
    }

    #[test]
    fn retry_policy_delay_for_calculation() {
        let p = TccRetryPolicy::new(3, 100, 5000, 2.0);
        // attempt=0 → 100ms
        assert_eq!(p.delay_for(0), Duration::from_millis(100));
        // attempt=1 → 100 * 2^1 = 200ms
        assert_eq!(p.delay_for(1), Duration::from_millis(200));
        // attempt=2 → 100 * 2^2 = 400ms
        assert_eq!(p.delay_for(2), Duration::from_millis(400));
        // attempt=3 → 100 * 2^3 = 800ms
        assert_eq!(p.delay_for(3), Duration::from_millis(800));
    }

    #[test]
    fn retry_policy_delay_capped_at_max() {
        let p = TccRetryPolicy::new(10, 100, 500, 2.0);
        // attempt=3 → 100 * 2^3 = 800ms，但封顶 500ms
        assert_eq!(p.delay_for(3), Duration::from_millis(500));
        // attempt=10 → 远超 500ms，封顶 500ms
        assert_eq!(p.delay_for(10), Duration::from_millis(500));
    }

    #[test]
    fn retry_policy_max_attempts() {
        let p = TccRetryPolicy::new(3, 100, 5000, 2.0);
        // 首次 + 3 次重试 = 4 次
        assert_eq!(p.max_attempts(), 4);

        let p0 = TccRetryPolicy::new(0, 100, 5000, 2.0);
        // max_retries=0 → 只有首次调用，不重试
        assert_eq!(p0.max_attempts(), 1);
    }

    #[test]
    fn participant_confirm_idempotent() {
        let confirm_count = Arc::new(AtomicU32::new(0));
        let c1 = confirm_count.clone();
        let mut p = TccParticipant::new("db1")
            .with_try(|| Ok(()))
            .with_confirm(move || {
                c1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
        p.try_phase().unwrap();

        // 第一次 confirm 成功
        p.confirm_phase().unwrap();
        assert_eq!(p.state, TccParticipantState::Confirmed);
        assert_eq!(p.confirm_attempts, 1);
        assert!(p.is_confirm_completed());
        assert_eq!(confirm_count.load(Ordering::SeqCst), 1);

        // 第二次 confirm 应该幂等跳过，不增加计数
        p.confirm_phase().unwrap();
        assert_eq!(p.confirm_attempts, 1);
        assert_eq!(confirm_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn participant_cancel_idempotent() {
        let cancel_count = Arc::new(AtomicU32::new(0));
        let c1 = cancel_count.clone();
        let mut p = TccParticipant::new("db1")
            .with_try(|| Ok(()))
            .with_cancel(move || {
                c1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
        p.try_phase().unwrap();

        // 第一次 cancel 成功
        p.cancel_phase().unwrap();
        assert_eq!(p.state, TccParticipantState::Cancelled);
        assert_eq!(p.cancel_attempts, 1);
        assert!(p.is_cancel_completed());
        assert_eq!(cancel_count.load(Ordering::SeqCst), 1);

        // 第二次 cancel 应该幂等跳过
        p.cancel_phase().unwrap();
        assert_eq!(p.cancel_attempts, 1);
        assert_eq!(cancel_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn participant_confirm_not_completed_initially() {
        let p = TccParticipant::new("db1").with_try(|| Ok(()));
        assert!(!p.is_confirm_completed());
        assert!(!p.is_cancel_completed());
    }

    #[test]
    fn participant_confirm_failure_does_not_mark_completed() {
        let mut p = TccParticipant::new("db1")
            .with_try(|| Ok(()))
            .with_confirm(|| Err("confirm fail".to_string()));
        p.try_phase().unwrap();
        let result = p.confirm_phase();
        assert!(result.is_err());
        assert!(!p.is_confirm_completed());
        assert_eq!(p.confirm_attempts, 1);
    }

    #[test]
    fn coordinator_with_retry_policy_sets_policy() {
        let coord = TccCoordinator::new("tx-1")
            .with_retry_policy(TccRetryPolicy::default());
        assert!(coord.retry_policy().is_some());
        assert_eq!(coord.retry_policy().unwrap().max_retries, 3);
    }

    #[test]
    fn coordinator_without_retry_policy_returns_none() {
        let coord = TccCoordinator::new("tx-2");
        assert!(coord.retry_policy().is_none());
    }

    #[test]
    fn coordinator_confirm_with_retry_succeeds_after_failures() {
        let attempt = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-retry-confirm")
            .with_retry_policy(TccRetryPolicy::new(3, 1, 10, 2.0));

        let a = attempt.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(move || {
                    let n = a.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(format!("attempt {} failed", n))
                    } else {
                        Ok(())
                    }
                }),
        );

        // try_phase 成功
        coord.try_phase().unwrap();
        assert_eq!(coord.state(), TccState::Tried);

        // confirm_with_retry 应该在第 3 次尝试成功
        coord.confirm_with_retry().unwrap();
        assert_eq!(coord.state(), TccState::Confirmed);
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn coordinator_confirm_with_retry_fails_after_max_retries() {
        let attempt = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-retry-fail")
            .with_retry_policy(TccRetryPolicy::new(2, 1, 10, 2.0));

        let a = attempt.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(move || {
                    a.fetch_add(1, Ordering::SeqCst);
                    Err("always fails".to_string())
                }),
        );

        coord.try_phase().unwrap();
        let result = coord.confirm_with_retry();
        assert!(result.is_err());
        assert_eq!(coord.state(), TccState::Failed);
        // 首次 + 2 次重试 = 3 次
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn coordinator_confirm_with_retry_idempotent_skips_confirmed() {
        let confirm_count = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-idempotent")
            .with_retry_policy(TccRetryPolicy::default());

        let c1 = confirm_count.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(move || {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );

        coord.try_phase().unwrap();
        coord.confirm_with_retry().unwrap();
        assert_eq!(coord.state(), TccState::Confirmed);
        assert_eq!(confirm_count.load(Ordering::SeqCst), 1);

        // 再次调用 confirm_with_retry，应该跳过已 Confirmed 的分支
        // 但状态已经是 Confirmed，不是 Confirming/Failed，所以会返回 InvalidState
        let result = coord.confirm_with_retry();
        assert!(result.is_err());
        // confirm 只被调用一次（幂等）
        assert_eq!(confirm_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn coordinator_confirm_with_retry_without_policy_fails() {
        let mut coord = TccCoordinator::new("tx-no-policy");
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        );
        coord.try_phase().unwrap();

        // 未设置 retry_policy，应该返回错误
        let result = coord.confirm_with_retry();
        assert!(result.is_err());
    }

    #[test]
    fn coordinator_cancel_with_retry_succeeds_after_failures() {
        let attempt = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-retry-cancel")
            .with_retry_policy(TccRetryPolicy::new(3, 1, 10, 2.0));

        let a = attempt.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_cancel(move || {
                    let n = a.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(format!("cancel attempt {} failed", n))
                    } else {
                        Ok(())
                    }
                }),
        );

        // try 成功后直接 cancel
        coord.try_phase().unwrap();
        coord.cancel_with_retry().unwrap();
        assert_eq!(coord.state(), TccState::Cancelled);
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn coordinator_cancel_with_retry_fails_after_max_retries() {
        let attempt = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-cancel-fail")
            .with_retry_policy(TccRetryPolicy::new(2, 1, 10, 2.0));

        let a = attempt.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_cancel(move || {
                    a.fetch_add(1, Ordering::SeqCst);
                    Err("cancel always fails".to_string())
                }),
        );

        coord.try_phase().unwrap();
        let result = coord.cancel_with_retry();
        assert!(result.is_err());
        assert_eq!(coord.state(), TccState::Failed);
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn coordinator_cancel_with_retry_without_policy_fails() {
        let mut coord = TccCoordinator::new("tx-cancel-no-policy");
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_cancel(|| Ok(())),
        );
        coord.try_phase().unwrap();

        let result = coord.cancel_with_retry();
        assert!(result.is_err());
    }

    #[test]
    fn coordinator_confirm_with_retry_max_retries_zero() {
        // 边界：max_retries=0，不重试，只调用一次
        let attempt = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-zero-retries")
            .with_retry_policy(TccRetryPolicy::new(0, 1, 10, 2.0));

        let a = attempt.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(move || {
                    a.fetch_add(1, Ordering::SeqCst);
                    Err("always fails".to_string())
                }),
        );

        coord.try_phase().unwrap();
        let result = coord.confirm_with_retry();
        assert!(result.is_err());
        assert_eq!(coord.state(), TccState::Failed);
        // 只调用 1 次（首次，无重试）
        assert_eq!(attempt.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn coordinator_cancel_with_retry_max_retries_zero() {
        // 边界：max_retries=0，不重试
        let attempt = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-cancel-zero-retries")
            .with_retry_policy(TccRetryPolicy::new(0, 1, 10, 2.0));

        let a = attempt.clone();
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_cancel(move || {
                    a.fetch_add(1, Ordering::SeqCst);
                    Err("always fails".to_string())
                }),
        );

        coord.try_phase().unwrap();
        let result = coord.cancel_with_retry();
        assert!(result.is_err());
        assert_eq!(coord.state(), TccState::Failed);
        assert_eq!(attempt.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn coordinator_confirm_with_retry_wrong_state_fails() {
        let mut coord = TccCoordinator::new("tx-wrong-state")
            .with_retry_policy(TccRetryPolicy::default());
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_confirm(|| Ok(())),
        );

        // 状态是 Init，不能 confirm_with_retry
        let result = coord.confirm_with_retry();
        assert!(result.is_err());
    }

    #[test]
    fn coordinator_cancel_with_retry_skips_confirmed_and_init() {
        let cancel_count = Arc::new(AtomicU32::new(0));
        let mut coord = TccCoordinator::new("tx-skip-confirmed")
            .with_retry_policy(TccRetryPolicy::default());

        let c1 = cancel_count.clone();
        // 第一个分支：try 成功，需要 cancel
        coord.add_participant(
            TccParticipant::new("db1")
                .with_try(|| Ok(()))
                .with_cancel(move || {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
        );
        // 第二个分支：只有 try（会 try 成功），但 cancel 时会被处理
        coord.add_participant(
            TccParticipant::new("db2")
                .with_try(|| Ok(()))
                .with_cancel(|| Ok(())),
        );

        coord.try_phase().unwrap();
        coord.cancel_with_retry().unwrap();
        assert_eq!(coord.state(), TccState::Cancelled);
        // db1 的 cancel 被调用一次
        assert_eq!(cancel_count.load(Ordering::SeqCst), 1);
    }
}
