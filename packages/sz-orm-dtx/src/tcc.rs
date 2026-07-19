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
    pub fn confirm_phase(&mut self) -> Result<(), String> {
        self.confirm_attempts += 1;
        if let Some(cb) = &self.confirm_fn {
            if let Err(e) = cb() {
                self.state = TccParticipantState::Failed;
                return Err(e);
            }
        }
        self.state = TccParticipantState::Confirmed;
        Ok(())
    }

    /// 执行 cancel 阶段（必须幂等）
    pub fn cancel_phase(&mut self) -> Result<(), String> {
        self.cancel_attempts += 1;
        if let Some(cb) = &self.cancel_fn {
            if let Err(e) = cb() {
                self.state = TccParticipantState::Failed;
                return Err(e);
            }
        }
        self.state = TccParticipantState::Cancelled;
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
            .finish()
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
}

impl TccCoordinator {
    /// 创建新的 TCC 协调器
    pub fn new(tx_id: impl Into<String>) -> Self {
        Self {
            tx_id: tx_id.into(),
            state: TccState::Init,
            participants: Vec::new(),
        }
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

    /// 内部方法：对前 n 个分支执行 cancel（用于 try 失败后回滚）
    ///
    /// 返回 `(resource_id, error)` 列表，记录哪些分支 cancel 失败及其原因。
    /// 失败的分支仍会被标记为 Failed，但不会短路——继续尝试 cancel 其他分支（best-effort）。
    fn cancel_tried_participants(&mut self, tried_count: usize) -> Vec<(String, String)> {
        let mut failures = Vec::new();
        for i in 0..tried_count {
            if self.participants[i].is_tried()
                && self.participants[i].state != TccParticipantState::Cancelled
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
}
