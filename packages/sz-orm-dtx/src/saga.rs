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

/// Saga 步骤的动作回调类型
pub type SagaAction = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// Saga 步骤的补偿回调类型
pub type SagaCompensation = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

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

    /// 添加步骤（必须在新建状态下添加）
    pub fn add_step(&mut self, step: SagaStep) -> Result<(), String> {
        if self.state != SagaState::New {
            return Err(format!(
                "Cannot add step to Saga in state {:?} (only New allowed)",
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

    /// 执行 Saga
    ///
    /// 按顺序执行所有步骤的 action；任一步骤失败时，按反向顺序对已成功步骤执行 compensation。
    pub fn execute(&mut self) -> Result<SagaResult, String> {
        if self.state != SagaState::New {
            return Err(format!(
                "Cannot execute Saga in state {:?} (only New allowed)",
                self.state
            ));
        }
        if self.steps.is_empty() {
            self.state = SagaState::Completed;
            self.last_result = Some(SagaResult::Success);
            return Ok(SagaResult::Success);
        }

        self.state = SagaState::Running;
        for i in 0..self.steps.len() {
            let step_name = self.steps[i].name.clone();
            match self.steps[i].execute_action() {
                Ok(()) => {
                    self.completed_count = i + 1;
                }
                Err(e) => {
                    // 步骤失败，开始补偿
                    let failed_step = step_name.clone();
                    let failure_reason = e.clone();
                    self.state = SagaState::Compensating;
                    let comp_result = self.compensate();
                    return match comp_result {
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
                    };
                }
            }
        }

        // 所有步骤成功
        self.state = SagaState::Completed;
        self.last_result = Some(SagaResult::Success);
        Ok(SagaResult::Success)
    }

    /// 按反向顺序补偿已成功执行的步骤
    ///
    /// 返回 Ok(()) 表示所有补偿成功；
    /// 返回 Err((step_name, reason)) 表示某个补偿失败，此时后续步骤不再补偿。
    fn compensate(&mut self) -> Result<(), (String, String)> {
        // 按反向顺序遍历已成功的步骤
        for i in (0..self.completed_count).rev() {
            let step_name = self.steps[i].name.clone();
            if self.steps[i].needs_compensation() {
                if let Err(e) = self.steps[i].execute_compensation() {
                    return Err((step_name, e));
                }
            }
        }
        Ok(())
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
}
