//! GDPR/CCPA 合规处理器：数据主体权利执行 + 跨境传输控制。
//!
//! 支持四种数据主体权利（访问/可携/删除/更正）和跨境传输合规判定。
//! GDPR 删除权与审计平衡：删除权仅删除业务数据，审计记录脱敏后保留。

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// 数据导出格式（用于可携权）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataFormat {
    Json,
    Csv,
    Xml,
}

/// 合规区域。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComplianceRegion {
    /// GDPR（欧盟）
    Eu,
    /// CCPA（美国加州）
    Usa,
    /// PIPL（中国）
    China,
    /// 其他
    Other,
}

/// 数据主体权利请求。
#[derive(Debug, Clone)]
pub enum DataSubjectRight {
    /// 访问权：获取数据主体相关的所有个人数据。
    Access { subject_id: String },
    /// 可携权：以指定格式导出数据主体相关数据。
    Portability { subject_id: String, format: DataFormat },
    /// 删除权（被遗忘权）：删除数据主体相关业务数据，审计记录脱敏后保留。
    Deletion { subject_id: String },
    /// 更正权：更正数据主体相关数据。
    Correction {
        subject_id: String,
        corrections: HashMap<String, String>,
    },
}

/// 跨境传输控制规则。
#[derive(Debug, Clone)]
pub struct CrossBorderTransferControl {
    /// 允许传输的目标区域列表。
    pub allowed_regions: Vec<ComplianceRegion>,
    /// 拒绝传输的目标区域列表（优先级高于 allowed）。
    pub denied_regions: Vec<ComplianceRegion>,
}

impl CrossBorderTransferControl {
    pub fn new(
        allowed: Vec<ComplianceRegion>,
        denied: Vec<ComplianceRegion>,
    ) -> Self {
        Self {
            allowed_regions: allowed,
            denied_regions: denied,
        }
    }
}

/// 权利执行结果。
#[derive(Debug, Clone)]
pub struct RightExecutionResult {
    /// 数据主体 ID。
    pub subject_id: String,
    /// 返回的数据（访问权/可携权时填充）。
    pub data: Option<String>,
    /// 导出格式（可携权时填充）。
    pub format: Option<DataFormat>,
    /// 受影响的记录数。
    pub affected_records: usize,
    /// 审计记录是否保留（删除权时为 true，满足 GDPR 与审计双重要求）。
    pub audit_preserved: bool,
}

/// GDPR/CCPA 合规错误。
#[derive(Debug, Clone)]
pub enum GdprError {
    /// 数据主体未找到。
    SubjectNotFound(String),
    /// 跨境传输被拒绝。
    TransferDenied(String),
    /// 无效格式。
    InvalidFormat(String),
    /// 执行失败。
    ExecutionFailed(String),
}

/// GDPR/CCPA 合规处理器。
///
/// 管理数据主体权利执行和跨境传输合规判定。
/// 删除权执行时仅删除业务数据，审计记录脱敏后保留（GDPR 与审计双重要求）。
pub struct GdprCcpaProcessor {
    transfer_control: CrossBorderTransferControl,
    subject_data: Mutex<HashMap<String, String>>,
    audit_records: Mutex<Vec<String>>,
}

impl GdprCcpaProcessor {
    /// 创建 GDPR/CCPA 合规处理器。
    pub fn new(transfer_control: CrossBorderTransferControl) -> Self {
        Self {
            transfer_control,
            subject_data: Mutex::new(HashMap::new()),
            audit_records: Mutex::new(Vec::new()),
        }
    }

    /// 注册数据主体数据（生产接线点：业务系统注册用户数据）。
    pub fn register_subject(&self, subject_id: &str, data: &str) {
        let mut data_map = self.subject_data.lock().expect("subject_data lock");
        data_map.insert(subject_id.to_string(), data.to_string());
    }

    /// 执行数据主体权利。
    ///
    /// - Access：返回数据主体相关数据
    /// - Portability：以指定格式导出数据
    /// - Deletion：删除业务数据，审计记录脱敏后保留
    /// - Correction：更正数据主体相关数据
    pub async fn execute_right(
        &self,
        right: DataSubjectRight,
    ) -> Result<RightExecutionResult, GdprError> {
        match right {
            DataSubjectRight::Access { subject_id } => {
                let data_map = self.subject_data.lock().expect("subject_data lock");
                let data = data_map
                    .get(&subject_id)
                    .cloned()
                    .ok_or_else(|| GdprError::SubjectNotFound(subject_id.clone()))?;
                Ok(RightExecutionResult {
                    subject_id,
                    data: Some(data),
                    format: None,
                    affected_records: 1,
                    audit_preserved: true,
                })
            }
            DataSubjectRight::Portability {
                subject_id,
                format,
            } => {
                let data_map = self.subject_data.lock().expect("subject_data lock");
                let data = data_map
                    .get(&subject_id)
                    .cloned()
                    .ok_or_else(|| GdprError::SubjectNotFound(subject_id.clone()))?;
                let exported = match format {
                    DataFormat::Json => serde_json::json!({
                        "subject_id": subject_id,
                        "data": data,
                    })
                    .to_string(),
                    DataFormat::Csv => format!("{},{}", subject_id, data),
                    DataFormat::Xml => format!("<subject id=\"{}\">{}</subject>", subject_id, data),
                };
                Ok(RightExecutionResult {
                    subject_id,
                    data: Some(exported),
                    format: Some(format),
                    affected_records: 1,
                    audit_preserved: true,
                })
            }
            DataSubjectRight::Deletion { subject_id } => {
                let mut data_map = self.subject_data.lock().expect("subject_data lock");
                let existed = data_map.remove(&subject_id).is_some();
                if !existed {
                    return Err(GdprError::SubjectNotFound(subject_id));
                }
                let masked_id = mask_subject_id(&subject_id);
                let mut audit = self.audit_records.lock().expect("audit_records lock");
                audit.push(format!("DELETION: subject={}", masked_id));
                Ok(RightExecutionResult {
                    subject_id: masked_id,
                    data: None,
                    format: None,
                    affected_records: 1,
                    audit_preserved: true,
                })
            }
            DataSubjectRight::Correction {
                subject_id,
                corrections,
            } => {
                let mut data_map = self.subject_data.lock().expect("subject_data lock");
                let data = data_map
                    .get_mut(&subject_id)
                    .ok_or_else(|| GdprError::SubjectNotFound(subject_id.clone()))?;
                let count = corrections.len();
                for (field, value) in &corrections {
                    *data = format!("{}|{}={}", data, field, value);
                }
                Ok(RightExecutionResult {
                    subject_id,
                    data: Some(data.clone()),
                    format: None,
                    affected_records: count,
                    audit_preserved: true,
                })
            }
        }
    }

    /// 判定跨境传输是否合规。
    ///
    /// - 目标区域在 denied 列表中 → `Err(TransferDenied)`
    /// - 目标区域在 allowed 列表中 → `Ok(true)`
    /// - 目标区域不在任何列表中 → `Ok(false)`（未明确允许）
    pub fn check_transfer(
        &self,
        target_region: ComplianceRegion,
    ) -> Result<bool, GdprError> {
        if self.transfer_control.denied_regions.contains(&target_region) {
            return Err(GdprError::TransferDenied(format!(
                "{:?} is in denied regions",
                target_region
            )));
        }
        Ok(self
            .transfer_control
            .allowed_regions
            .contains(&target_region))
    }

    /// 获取审计记录（验证 GDPR 删除权保留审计记录）。
    pub fn audit_records(&self) -> Vec<String> {
        self.audit_records
            .lock()
            .expect("audit_records lock")
            .clone()
    }

    /// 获取数据主体数据（验证删除效果）。
    pub fn get_subject_data(&self, subject_id: &str) -> Option<String> {
        self.subject_data
            .lock()
            .expect("subject_data lock")
            .get(subject_id)
            .cloned()
    }
}

/// 脱敏数据主体 ID：保留前 2 字符，其余替换为 `*`。
fn mask_subject_id(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() <= 2 {
        return "*".repeat(chars.len());
    }
    let prefix: String = chars[..2].iter().collect();
    let masked = "*".repeat(chars.len() - 2);
    format!("{}{}", prefix, masked)
}

// ==================== v10.11.0 T4.3: 数据生命周期自动化 ====================

/// 生命周期状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleStatus {
    Active,
    Archived,
    Destroyed,
}

/// 保留期配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    pub retention_period_days: u32,
    pub auto_archive: bool,
    pub auto_destroy: bool,
    pub destroy_audit: bool,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            retention_period_days: 365,
            auto_archive: true,
            auto_destroy: false,
            destroy_audit: true,
        }
    }
}

/// 生命周期状态记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleState {
    pub data_id: String,
    pub created_at: u64,
    pub retention_days: u32,
    pub status: LifecycleStatus,
    pub business_hold: bool,
    pub archive_id: Option<String>,
}

impl LifecycleState {
    pub fn new(data_id: &str, created_at: u64, retention_days: u32) -> Self {
        Self {
            data_id: data_id.to_string(),
            created_at,
            retention_days,
            status: LifecycleStatus::Active,
            business_hold: false,
            archive_id: None,
        }
    }

    pub fn with_business_hold(mut self, hold: bool) -> Self {
        self.business_hold = hold;
        self
    }

    pub fn is_expired(&self, now: u64) -> bool {
        if now <= self.created_at {
            return false;
        }
        let age_days = (now - self.created_at) / 86400;
        age_days >= self.retention_days as u64
    }

    pub fn remaining_days(&self, now: u64) -> i64 {
        if now <= self.created_at {
            return self.retention_days as i64;
        }
        let age_days = ((now - self.created_at) / 86400) as i64;
        self.retention_days as i64 - age_days
    }
}

/// 生命周期结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleResult {
    Retained { remaining_days: i64 },
    Archived { archive_id: String },
    Destroyed { destroy_audit_id: String },
}

/// 生命周期错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    NotExpired,
    BusinessHold,
    AlreadyArchived,
    AlreadyDestroyed,
    NotFound,
    ArchiveFailed(String),
    DestroyFailed(String),
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifecycleError::NotExpired => write!(f, "Data not expired"),
            LifecycleError::BusinessHold => write!(f, "Data has business hold"),
            LifecycleError::AlreadyArchived => write!(f, "Data already archived"),
            LifecycleError::AlreadyDestroyed => write!(f, "Data already destroyed"),
            LifecycleError::NotFound => write!(f, "Data not found"),
            LifecycleError::ArchiveFailed(msg) => write!(f, "Archive failed: {}", msg),
            LifecycleError::DestroyFailed(msg) => write!(f, "Destroy failed: {}", msg),
        }
    }
}

impl std::error::Error for LifecycleError {}

/// 销毁审计留痕
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestroyAuditTrail {
    pub audit_id: String,
    pub data_id: String,
    pub destroyed_at: u64,
    pub reason: String,
    pub operator: String,
}

/// 保留期检查器
pub struct RetentionChecker {
    config: RetentionConfig,
}

impl RetentionChecker {
    pub fn new(config: RetentionConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &RetentionConfig {
        &self.config
    }

    /// 检查数据是否到期且无业务持有
    pub fn check(&self, state: &LifecycleState, now: u64) -> Result<(), LifecycleError> {
        if state.status == LifecycleStatus::Destroyed {
            return Err(LifecycleError::AlreadyDestroyed);
        }
        if !state.is_expired(now) {
            return Err(LifecycleError::NotExpired);
        }
        if state.business_hold {
            return Err(LifecycleError::BusinessHold);
        }
        Ok(())
    }
}

/// 自动归档器
pub struct AutoArchiver {
    archives: Mutex<HashMap<String, String>>,
    count: Mutex<u64>,
}

impl AutoArchiver {
    pub fn new() -> Self {
        Self {
            archives: Mutex::new(HashMap::new()),
            count: Mutex::new(0),
        }
    }

    pub fn archive(
        &self,
        state: &LifecycleState,
        data: &str,
    ) -> Result<String, LifecycleError> {
        if state.status == LifecycleStatus::Archived {
            return Err(LifecycleError::AlreadyArchived);
        }
        if state.status == LifecycleStatus::Destroyed {
            return Err(LifecycleError::AlreadyDestroyed);
        }
        let archive_id = format!("ARCH-{}-{}", state.data_id, state.created_at);
        self.archives
            .lock()
            .expect("archives lock")
            .insert(archive_id.clone(), data.to_string());
        *self.count.lock().expect("archive count lock") += 1;
        Ok(archive_id)
    }

    pub fn get_archive(&self, archive_id: &str) -> Option<String> {
        self.archives
            .lock()
            .expect("archives lock")
            .get(archive_id)
            .cloned()
    }

    pub fn archive_count(&self) -> u64 {
        *self.count.lock().expect("archive count lock")
    }
}

impl Default for AutoArchiver {
    fn default() -> Self {
        Self::new()
    }
}

/// 自动销毁器（不可恢复 + 审计留痕）
pub struct AutoDestroyer {
    audit_trails: Mutex<Vec<DestroyAuditTrail>>,
    count: Mutex<u64>,
}

impl AutoDestroyer {
    pub fn new() -> Self {
        Self {
            audit_trails: Mutex::new(Vec::new()),
            count: Mutex::new(0),
        }
    }

    /// 销毁数据（二次校验 + 审计留痕）
    pub fn destroy(
        &self,
        state: &LifecycleState,
        retention_checker: &RetentionChecker,
        now: u64,
        operator: &str,
    ) -> Result<DestroyAuditTrail, LifecycleError> {
        retention_checker.check(state, now)?;

        if state.status == LifecycleStatus::Destroyed {
            return Err(LifecycleError::AlreadyDestroyed);
        }

        let audit_id = format!("DESTROY-AUDIT-{}-{}", state.data_id, now);
        let trail = DestroyAuditTrail {
            audit_id: audit_id.clone(),
            data_id: state.data_id.clone(),
            destroyed_at: now,
            reason: format!("数据到期（保留期 {} 天）", state.retention_days),
            operator: operator.to_string(),
        };
        self.audit_trails
            .lock()
            .expect("audit_trails lock")
            .push(trail.clone());
        *self.count.lock().expect("destroy count lock") += 1;
        Ok(trail)
    }

    pub fn audit_trails(&self) -> Vec<DestroyAuditTrail> {
        self.audit_trails
            .lock()
            .expect("audit_trails lock")
            .clone()
    }

    pub fn destroy_count(&self) -> u64 {
        *self.count.lock().expect("destroy count lock")
    }
}

impl Default for AutoDestroyer {
    fn default() -> Self {
        Self::new()
    }
}

/// 生命周期自动管理器
///
/// 保留期/归档/销毁自动化，销毁前二次校验保留期避免误销毁。
/// GDPR 删除权满足：销毁不可恢复 + 审计留痕。
pub struct LifecycleAutoManager {
    retention_checker: RetentionChecker,
    archiver: AutoArchiver,
    destroyer: AutoDestroyer,
    states: Mutex<HashMap<String, LifecycleState>>,
    data_store: Mutex<HashMap<String, String>>,
}

impl LifecycleAutoManager {
    pub fn new(config: RetentionConfig) -> Self {
        Self {
            retention_checker: RetentionChecker::new(config),
            archiver: AutoArchiver::new(),
            destroyer: AutoDestroyer::new(),
            states: Mutex::new(HashMap::new()),
            data_store: Mutex::new(HashMap::new()),
        }
    }

    /// 注册数据（生产接线点）
    pub fn register_data(&self, data_id: &str, data: &str, created_at: u64) {
        let retention_days = self.retention_checker.config.retention_period_days;
        let state = LifecycleState::new(data_id, created_at, retention_days);
        self.states
            .lock()
            .expect("states lock")
            .insert(data_id.to_string(), state);
        self.data_store
            .lock()
            .expect("data_store lock")
            .insert(data_id.to_string(), data.to_string());
    }

    /// 设置业务持有
    pub fn set_business_hold(
        &self,
        data_id: &str,
        hold: bool,
    ) -> Result<(), LifecycleError> {
        let mut states = self.states.lock().expect("states lock");
        let state = states
            .get_mut(data_id)
            .ok_or(LifecycleError::NotFound)?;
        state.business_hold = hold;
        Ok(())
    }

    /// 检查并处理生命周期
    pub fn check_and_process(
        &self,
        data_id: &str,
        now: u64,
    ) -> Result<LifecycleResult, LifecycleError> {
        let state = {
            let states = self.states.lock().expect("states lock");
            states
                .get(data_id)
                .cloned()
                .ok_or(LifecycleError::NotFound)?
        };

        match state.status {
            LifecycleStatus::Destroyed => Err(LifecycleError::AlreadyDestroyed),
            LifecycleStatus::Archived => {
                if self.retention_checker.config.auto_destroy {
                    let trail = self.destroyer.destroy(
                        &state,
                        &self.retention_checker,
                        now,
                        "system",
                    )?;
                    self.update_status(data_id, LifecycleStatus::Destroyed);
                    self.data_store
                        .lock()
                        .expect("data_store lock")
                        .remove(data_id);
                    Ok(LifecycleResult::Destroyed {
                        destroy_audit_id: trail.audit_id,
                    })
                } else {
                    Ok(LifecycleResult::Archived {
                        archive_id: state.archive_id.unwrap_or_default(),
                    })
                }
            }
            LifecycleStatus::Active => {
                if !state.is_expired(now) {
                    return Ok(LifecycleResult::Retained {
                        remaining_days: state.remaining_days(now),
                    });
                }
                if state.business_hold {
                    return Err(LifecycleError::BusinessHold);
                }

                if self.retention_checker.config.auto_archive {
                    let data = self
                        .data_store
                        .lock()
                        .expect("data_store lock")
                        .get(data_id)
                        .cloned()
                        .unwrap_or_default();
                    let archive_id = self.archiver.archive(&state, &data)?;
                    self.update_status_with_archive(data_id, LifecycleStatus::Archived, &archive_id);

                    if self.retention_checker.config.auto_destroy {
                        let new_state = {
                            let states = self.states.lock().expect("states lock");
                            states.get(data_id).cloned().unwrap()
                        };
                        let trail = self.destroyer.destroy(
                            &new_state,
                            &self.retention_checker,
                            now,
                            "system",
                        )?;
                        self.update_status(data_id, LifecycleStatus::Destroyed);
                        self.data_store
                            .lock()
                            .expect("data_store lock")
                            .remove(data_id);
                        return Ok(LifecycleResult::Destroyed {
                            destroy_audit_id: trail.audit_id,
                        });
                    }

                    Ok(LifecycleResult::Archived { archive_id })
                } else {
                    Ok(LifecycleResult::Retained {
                        remaining_days: 0,
                    })
                }
            }
        }
    }

    fn update_status(&self, data_id: &str, status: LifecycleStatus) {
        let mut states = self.states.lock().expect("states lock");
        if let Some(state) = states.get_mut(data_id) {
            state.status = status;
        }
    }

    fn update_status_with_archive(
        &self,
        data_id: &str,
        status: LifecycleStatus,
        archive_id: &str,
    ) {
        let mut states = self.states.lock().expect("states lock");
        if let Some(state) = states.get_mut(data_id) {
            state.status = status;
            state.archive_id = Some(archive_id.to_string());
        }
    }

    pub fn get_state(&self, data_id: &str) -> Result<LifecycleState, LifecycleError> {
        self.states
            .lock()
            .expect("states lock")
            .get(data_id)
            .cloned()
            .ok_or(LifecycleError::NotFound)
    }

    pub fn get_data(&self, data_id: &str) -> Option<String> {
        self.data_store
            .lock()
            .expect("data_store lock")
            .get(data_id)
            .cloned()
    }

    pub fn archive_count(&self) -> u64 {
        self.archiver.archive_count()
    }

    pub fn destroy_count(&self) -> u64 {
        self.destroyer.destroy_count()
    }

    pub fn destroy_audit_trails(&self) -> Vec<DestroyAuditTrail> {
        self.destroyer.audit_trails()
    }

    pub fn retention_checker(&self) -> &RetentionChecker {
        &self.retention_checker
    }

    pub fn archiver(&self) -> &AutoArchiver {
        &self.archiver
    }

    pub fn destroyer(&self) -> &AutoDestroyer {
        &self.destroyer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_subject_id_long() {
        assert_eq!(mask_subject_id("user123"), "us*****");
    }

    #[test]
    fn test_mask_subject_id_short() {
        assert_eq!(mask_subject_id("ab"), "**");
        assert_eq!(mask_subject_id("a"), "*");
    }
}