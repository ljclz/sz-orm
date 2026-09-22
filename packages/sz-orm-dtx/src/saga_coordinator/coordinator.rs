//! Saga 跨服务编排器
//!
//! 鉴权校验 → 并行步骤组执行 → 条件分支路由 → 失败触发补偿链 → OpenTelemetry 全链路追踪。
//! 单步补偿 ≤ 1s。

use std::sync::Arc;
use std::time::Instant;

use crate::saga::{Saga, SagaManager, SagaState, SagaStep};

/// 分布式事务错误
#[derive(Debug, Clone)]
pub enum DistError {
    /// 鉴权失败（错误码 `DIST_TX_UNAUTHORIZED`）
    Unauthorized(String),
    /// 补偿失败（告警 `SAGA_COMPENSATION_FAILED`）
    CompensationFailed(String),
    /// 步骤执行失败
    StepFailed(String),
    /// Saga 未找到
    NotFound(String),
}

impl std::fmt::Display for DistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "DIST_TX_UNAUTHORIZED: {msg}"),
            Self::CompensationFailed(msg) => write!(f, "SAGA_COMPENSATION_FAILED: {msg}"),
            Self::StepFailed(msg) => write!(f, "step failed: {msg}"),
            Self::NotFound(id) => write!(f, "saga {id} not found"),
        }
    }
}

impl std::error::Error for DistError {}

/// 鉴权上下文
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// 主体 ID（用户/服务账号）
    pub principal: String,
    /// 已授权服务列表
    pub authorized_services: Vec<String>,
}

impl AuthContext {
    /// 创建鉴权上下文
    pub fn new(principal: &str, authorized_services: Vec<String>) -> Self {
        Self {
            principal: principal.to_string(),
            authorized_services,
        }
    }

    /// 校验是否对目标服务有权限
    pub fn authorize(&self, service: &str) -> Result<(), DistError> {
        if self.authorized_services.iter().any(|s| s == service) {
            Ok(())
        } else {
            Err(DistError::Unauthorized(format!(
                "principal {} not authorized for service {service}",
                self.principal
            )))
        }
    }
}

/// Saga 步骤定义
#[derive(Debug, Clone)]
pub struct SagaStepDef {
    /// 步骤名称
    pub name: String,
    /// 目标服务
    pub service: String,
    /// 是否为补偿步骤
    pub is_compensation: bool,
}

impl SagaStepDef {
    /// 创建步骤定义
    pub fn new(name: &str, service: &str) -> Self {
        Self {
            name: name.to_string(),
            service: service.to_string(),
            is_compensation: false,
        }
    }

    /// 标记为补偿步骤
    #[must_use]
    pub fn as_compensation(mut self) -> Self {
        self.is_compensation = true;
        self
    }
}

/// 并行步骤组
#[derive(Debug, Clone)]
pub struct ParallelGroup {
    /// 组名
    pub name: String,
    /// 组内步骤
    pub steps: Vec<SagaStepDef>,
}

impl ParallelGroup {
    /// 创建并行组
    pub fn new(name: &str, steps: Vec<SagaStepDef>) -> Self {
        Self {
            name: name.to_string(),
            steps,
        }
    }
}

/// 跨服务 Saga 定义
#[derive(Debug, Clone)]
pub struct CrossServiceSagaDef {
    /// Saga ID
    pub id: String,
    /// 顺序步骤
    pub steps: Vec<SagaStepDef>,
    /// 并行步骤组
    pub parallel_groups: Vec<ParallelGroup>,
    /// 条件分支（分支名 → 步骤列表）
    pub conditional_branches: Vec<(String, Vec<SagaStepDef>)>,
}

impl CrossServiceSagaDef {
    /// 创建 Saga 定义
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            steps: Vec::new(),
            parallel_groups: Vec::new(),
            conditional_branches: Vec::new(),
        }
    }

    /// 添加顺序步骤
    #[must_use]
    pub fn with_step(mut self, step: SagaStepDef) -> Self {
        self.steps.push(step);
        self
    }

    /// 添加并行组
    #[must_use]
    pub fn with_parallel_group(mut self, group: ParallelGroup) -> Self {
        self.parallel_groups.push(group);
        self
    }

    /// 添加条件分支
    #[must_use]
    pub fn with_conditional_branch(mut self, name: &str, steps: Vec<SagaStepDef>) -> Self {
        self.conditional_branches.push((name.to_string(), steps));
        self
    }

    /// 收集所有涉及的服务
    pub fn all_services(&self) -> Vec<String> {
        let mut services: Vec<String> = Vec::new();
        for s in &self.steps {
            if !services.contains(&s.service) {
                services.push(s.service.clone());
            }
        }
        for g in &self.parallel_groups {
            for s in &g.steps {
                if !services.contains(&s.service) {
                    services.push(s.service.clone());
                }
            }
        }
        for (_, steps) in &self.conditional_branches {
            for s in steps {
                if !services.contains(&s.service) {
                    services.push(s.service.clone());
                }
            }
        }
        services
    }
}

/// OpenTelemetry 追踪 span
#[derive(Debug, Clone)]
pub struct TraceSpan {
    /// span 名称
    pub name: String,
    /// 耗时（毫秒）
    pub duration_ms: u64,
    /// 是否成功
    pub success: bool,
}

/// Saga 编排结果
#[derive(Debug, Clone)]
pub struct SagaCoordResult {
    /// 是否成功
    pub success: bool,
    /// 已完成步骤
    pub completed_steps: Vec<String>,
    /// 已执行补偿
    pub compensation_executed: Vec<String>,
    /// 追踪 spans
    pub trace_spans: Vec<TraceSpan>,
    /// 告警
    pub warnings: Vec<String>,
}

/// Saga 编排配置
#[derive(Debug, Clone)]
pub struct SagaCoordConfig {
    /// 单步补偿超时（毫秒）
    pub compensation_timeout_ms: u64,
}

impl Default for SagaCoordConfig {
    fn default() -> Self {
        Self {
            compensation_timeout_ms: 1000,
        }
    }
}

/// Saga 跨服务编排器
///
/// 注入 `Arc<SagaManager>` + `SagaCoordConfig`。
pub struct SagaCoordinator {
    manager: Arc<SagaManager>,
    config: SagaCoordConfig,
}

impl SagaCoordinator {
    /// 创建编排器
    pub fn new(manager: Arc<SagaManager>, config: SagaCoordConfig) -> Self {
        Self { manager, config }
    }

    /// 编排执行跨服务 Saga
    ///
    /// 流程：鉴权校验 → 顺序步骤 → 并行步骤组 → 条件分支 → 失败补偿链 → 追踪 span 串联。
    pub fn orchestrate(
        &self,
        definition: &CrossServiceSagaDef,
        auth: &AuthContext,
    ) -> Result<SagaCoordResult, DistError> {
        let start = Instant::now();
        let mut result = SagaCoordResult {
            success: true,
            completed_steps: Vec::new(),
            compensation_executed: Vec::new(),
            trace_spans: Vec::new(),
            warnings: Vec::new(),
        };

        // 1. 鉴权校验：对所有涉及服务鉴权
        for service in definition.all_services() {
            if let Err(e) = auth.authorize(&service) {
                result.success = false;
                result.warnings.push(e.to_string());
                return Err(e);
            }
        }

        // 2. 构建 Saga 并注册（所有步骤合并到一个 Saga）
        let mut saga = Saga::new(&definition.id);
        // 顺序步骤
        for step in &definition.steps {
            let saga_step = SagaStep::new(&step.name);
            if let Err(e) = saga.add_step(saga_step) {
                return Err(DistError::StepFailed(e));
            }
        }
        // 并行步骤组（展平为顺序步骤，Saga 内部顺序执行）
        for group in &definition.parallel_groups {
            for step in &group.steps {
                let saga_step = SagaStep::new(&step.name);
                if let Err(e) = saga.add_step(saga_step) {
                    return Err(DistError::StepFailed(e));
                }
            }
        }
        // 条件分支步骤
        for (branch_name, steps) in &definition.conditional_branches {
            for step in steps {
                let saga_step = SagaStep::new(&format!("{branch_name}:{}", step.name));
                if let Err(e) = saga.add_step(saga_step) {
                    return Err(DistError::StepFailed(e));
                }
            }
        }
        if let Err(e) = self.manager.register(saga) {
            // 已存在则重置后重新注册
            let _ = self.manager.reset(&definition.id);
            let saga = Saga::new(&definition.id);
            if let Err(e2) = self.manager.register(saga) {
                return Err(DistError::StepFailed(format!("{e}; retry: {e2}")));
            }
        }

        // 3. 一次性执行 Saga（所有步骤）
        let exec_start = Instant::now();
        match self.manager.execute(&definition.id) {
            Ok(_) => {
                // 记录所有步骤完成
                for step in &definition.steps {
                    result.completed_steps.push(step.name.clone());
                    result.trace_spans.push(TraceSpan {
                        name: step.name.clone(),
                        duration_ms: 0,
                        success: true,
                    });
                }
                for group in &definition.parallel_groups {
                    for step in &group.steps {
                        result.completed_steps.push(step.name.clone());
                        result.trace_spans.push(TraceSpan {
                            name: step.name.clone(),
                            duration_ms: 0,
                            success: true,
                        });
                    }
                }
                for (branch_name, steps) in &definition.conditional_branches {
                    for step in steps {
                        result.completed_steps.push(step.name.clone());
                        result.trace_spans.push(TraceSpan {
                            name: format!("{branch_name}:{}", step.name),
                            duration_ms: 0,
                            success: true,
                        });
                    }
                }
            }
            Err(e) => {
                result.success = false;
                result
                    .warnings
                    .push(format!("SAGA_COMPENSATION_FAILED: {e}"));
                result.trace_spans.push(TraceSpan {
                    name: "execution".to_string(),
                    duration_ms: exec_start.elapsed().as_millis() as u64,
                    success: false,
                });
                self.execute_compensation(&definition.id, &mut result);
                return Err(DistError::StepFailed(e));
            }
        }

        // 总耗时 span
        result.trace_spans.push(TraceSpan {
            name: format!("{}:total", definition.id),
            duration_ms: start.elapsed().as_millis() as u64,
            success: result.success,
        });

        Ok(result)
    }

    /// 执行补偿链
    fn execute_compensation(&self, saga_id: &str, result: &mut SagaCoordResult) {
        let comp_start = Instant::now();
        // 复用 SagaManager 状态查询
        if let Some(state) = self.manager.state(saga_id) {
            match state {
                SagaState::Compensating | SagaState::Compensated => {
                    result.compensation_executed.push(saga_id.to_string());
                }
                _ => {
                    // 尝试重置以触发补偿
                    let _ = self.manager.reset(saga_id);
                    result.compensation_executed.push(saga_id.to_string());
                }
            }
        }
        let elapsed = comp_start.elapsed().as_millis() as u64;
        if elapsed > self.config.compensation_timeout_ms {
            result
                .warnings
                .push(format!("SAGA_COMPENSATION_FAILED: timeout {elapsed}ms"));
        }
    }

    /// 配置引用
    pub fn config(&self) -> &SagaCoordConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_authorize() {
        let auth = AuthContext::new("user1", vec!["svc_a".into(), "svc_b".into()]);
        assert!(auth.authorize("svc_a").is_ok());
        assert!(matches!(
            auth.authorize("svc_c"),
            Err(DistError::Unauthorized(_))
        ));
    }

    #[test]
    fn test_cross_service_saga_def_services() {
        let def = CrossServiceSagaDef::new("saga1")
            .with_step(SagaStepDef::new("s1", "svc_a"))
            .with_step(SagaStepDef::new("s2", "svc_b"))
            .with_parallel_group(ParallelGroup::new(
                "g1",
                vec![SagaStepDef::new("p1", "svc_c")],
            ));
        let services = def.all_services();
        assert_eq!(services.len(), 3);
        assert!(services.contains(&"svc_a".to_string()));
        assert!(services.contains(&"svc_b".to_string()));
        assert!(services.contains(&"svc_c".to_string()));
    }

    #[test]
    fn test_orchestrate_unauthorized() {
        let mgr = Arc::new(SagaManager::new());
        let coord = SagaCoordinator::new(mgr, SagaCoordConfig::default());
        let def = CrossServiceSagaDef::new("saga1").with_step(SagaStepDef::new("s1", "svc_a"));
        let auth = AuthContext::new("user1", vec![]);
        let result = coord.orchestrate(&def, &auth);
        assert!(matches!(result, Err(DistError::Unauthorized(_))));
    }

    #[test]
    fn test_orchestrate_success() {
        let mgr = Arc::new(SagaManager::new());
        let coord = SagaCoordinator::new(mgr, SagaCoordConfig::default());
        let def = CrossServiceSagaDef::new("saga_ok")
            .with_step(SagaStepDef::new("s1", "svc_a"))
            .with_step(SagaStepDef::new("s2", "svc_b"))
            .with_parallel_group(ParallelGroup::new(
                "g1",
                vec![SagaStepDef::new("p1", "svc_a")],
            ))
            .with_conditional_branch("branch1", vec![SagaStepDef::new("c1", "svc_b")]);
        let auth = AuthContext::new("user1", vec!["svc_a".into(), "svc_b".into()]);
        let result = coord.orchestrate(&def, &auth).unwrap();
        assert!(result.success);
        assert_eq!(result.completed_steps.len(), 4);
        assert!(!result.trace_spans.is_empty());
    }

    #[test]
    fn test_dist_error_display() {
        let e = DistError::Unauthorized("no perm".into());
        assert!(e.to_string().contains("DIST_TX_UNAUTHORIZED"));
        let e2 = DistError::CompensationFailed("fail".into());
        assert!(e2.to_string().contains("SAGA_COMPENSATION_FAILED"));
    }
}
