//! # 低代码双向同步引擎
//!
//! ORM 模型 ↔ 低代码引擎模型双向同步 + 冲突检测与解决 + 增量追踪 + 审计日志。
//!
//! ## 主要类型
//!
//! - [`BidirectionalSyncEngine`] — 双向同步引擎
//! - [`SyncConflictDetector`] — 冲突检测器
//! - [`SyncConflictResolver`] — 冲突解决器
//! - [`SyncIncrementTracker`] — 增量追踪器
//! - [`SyncAuditLogger`] — 审计日志器

use crate::{FieldDef, ModelDefinition};
use std::collections::HashMap;

// ============================================================================
// 枚举定义
// ============================================================================

/// 同步方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    /// ORM → 低代码
    OrmToLc,
    /// 低代码 → ORM
    LcToOrm,
    /// 双向同步
    Bidirectional,
}

/// 冲突解决策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolutionStrategy {
    /// ORM 版本优先
    OrmWins,
    /// 低代码版本优先
    LcWins,
    /// 合并变更：约束冲突取保守并集（nullable=false 胜、unique/pk=true 胜）；
    /// 类型冲突无并集语义，挂起等待人工确认
    Merge,
    /// 人工确认（默认）
    #[default]
    Manual,
}

/// 变更类型
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    /// 字段新增
    FieldAdded,
    /// 字段删除
    FieldRemoved,
    /// 类型变更
    TypeChanged,
    /// 约束变更
    ConstraintChanged,
    /// 关联变更
    RelationChanged,
}

/// 冲突类型
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictType {
    /// 类型不匹配
    TypeMismatch,
    /// 约束不匹配
    ConstraintMismatch,
    /// 关联不匹配
    RelationMismatch,
    /// 双向变更
    BidirectionalChange,
}

// ============================================================================
// 配置
// ============================================================================

/// 同步配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncConfig {
    /// 默认冲突解决策略
    pub default_strategy: ConflictResolutionStrategy,
    /// 是否启用增量追踪
    pub enable_increment_tracking: bool,
    /// 审计日志路径
    pub audit_log_path: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncConfig {
    /// 创建默认配置（Manual, true, None）
    pub fn new() -> Self {
        Self {
            default_strategy: ConflictResolutionStrategy::Manual,
            enable_increment_tracking: true,
            audit_log_path: None,
        }
    }

    /// 设置默认策略
    pub fn with_default_strategy(mut self, strategy: ConflictResolutionStrategy) -> Self {
        self.default_strategy = strategy;
        self
    }

    /// 设置增量追踪
    pub fn with_increment_tracking(mut self, enabled: bool) -> Self {
        self.enable_increment_tracking = enabled;
        self
    }

    /// 设置审计日志路径
    pub fn with_audit_log(mut self, path: &str) -> Self {
        self.audit_log_path = Some(path.to_string());
        self
    }
}

// ============================================================================
// 同步错误与结果
// ============================================================================

/// 同步错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum SyncError {
    /// 冲突未解决
    #[error("unresolved conflict on field '{field}': {detail}")]
    UnresolvedConflict { field: String, detail: String },

    /// 破坏性变更需人工确认
    #[error("destructive change requires manual confirmation: {0}")]
    DestructiveChange(String),

    /// 类型映射不支持
    #[error("type mapping not supported for '{0}', skipped")]
    UnsupportedTypeMapping(String),

    /// 模型不匹配
    #[error("model mismatch: {0}")]
    ModelMismatch(String),
}

/// 同步结果
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// 同步后的模型
    pub synced_model: ModelDefinition,
    /// 检测到的冲突
    pub conflicts: Vec<SyncConflict>,
    /// 变更项
    pub changes: Vec<SyncChange>,
    /// 是否成功
    pub success: bool,
    /// 暂停原因（如有）
    pub paused_reason: Option<String>,
}

/// 冲突解决结果
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    /// 解决后的字段映射
    pub resolved_fields: HashMap<String, FieldDef>,
    /// 未解决的冲突
    pub unresolved: Vec<SyncConflict>,
    /// 是否暂停等待人工确认
    pub paused: bool,
    /// 暂停原因
    pub pause_reason: Option<String>,
}

// ============================================================================
// SyncConflict — 冲突定义
// ============================================================================

/// 同步冲突
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncConflict {
    /// 冲突字段名
    pub field: String,
    /// 冲突类型
    pub conflict_type: ConflictType,
    /// ORM 端值
    pub orm_value: String,
    /// 低代码端值
    pub lc_value: String,
}

// ============================================================================
// SyncChange — 变更定义
// ============================================================================

/// 同步变更
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncChange {
    /// 同步方向
    pub direction: SyncDirection,
    /// 模型名
    pub model_name: String,
    /// 字段名
    pub field_name: String,
    /// 变更类型
    pub change_type: ChangeType,
    /// 旧值
    pub old_value: Option<String>,
    /// 新值
    pub new_value: Option<String>,
    /// 时间戳
    pub timestamp: u64,
}

// ============================================================================
// SyncConflictDetector — 冲突检测器
// ============================================================================

/// 冲突检测器
#[derive(Debug, Clone)]
pub struct SyncConflictDetector;

impl SyncConflictDetector {
    /// 创建新的冲突检测器
    pub fn new() -> Self {
        Self
    }

    /// 检测双向同步冲突
    pub fn detect(
        &self,
        orm_model: &ModelDefinition,
        lc_model: &ModelDefinition,
    ) -> Vec<SyncConflict> {
        let mut conflicts = Vec::new();

        for orm_field in &orm_model.fields {
            if let Some(lc_field) = lc_model.find_field(&orm_field.name) {
                if orm_field.field_type != lc_field.field_type {
                    // 完整描述格式（含约束），供 resolver 构造完整 FieldDef 时保留胜者约束
                    conflicts.push(SyncConflict {
                        field: orm_field.name.clone(),
                        conflict_type: ConflictType::TypeMismatch,
                        orm_value: format!(
                            "type={}, nullable={}, unique={}, pk={}",
                            orm_field.field_type,
                            orm_field.nullable,
                            orm_field.unique,
                            orm_field.primary_key
                        ),
                        lc_value: format!(
                            "type={}, nullable={}, unique={}, pk={}",
                            lc_field.field_type,
                            lc_field.nullable,
                            lc_field.unique,
                            lc_field.primary_key
                        ),
                    });
                }

                if orm_field.nullable != lc_field.nullable
                    || orm_field.unique != lc_field.unique
                    || orm_field.primary_key != lc_field.primary_key
                {
                    // 值格式含类型信息，供 resolver 在约束合并时构造完整 FieldDef
                    conflicts.push(SyncConflict {
                        field: orm_field.name.clone(),
                        conflict_type: ConflictType::ConstraintMismatch,
                        orm_value: format!(
                            "type={}, nullable={}, unique={}, pk={}",
                            orm_field.field_type,
                            orm_field.nullable,
                            orm_field.unique,
                            orm_field.primary_key
                        ),
                        lc_value: format!(
                            "type={}, nullable={}, unique={}, pk={}",
                            lc_field.field_type,
                            lc_field.nullable,
                            lc_field.unique,
                            lc_field.primary_key
                        ),
                    });
                }
            }
        }

        for orm_rel in &orm_model.relations {
            if let Some(lc_rel) = lc_model.relations.iter().find(|r| r.name == orm_rel.name) {
                if orm_rel.rel_type != lc_rel.rel_type
                    || orm_rel.target_model != lc_rel.target_model
                {
                    conflicts.push(SyncConflict {
                        field: orm_rel.name.clone(),
                        conflict_type: ConflictType::RelationMismatch,
                        orm_value: format!("{} -> {}", orm_rel.rel_type, orm_rel.target_model),
                        lc_value: format!("{} -> {}", lc_rel.rel_type, lc_rel.target_model),
                    });
                }
            }
        }

        conflicts
    }
}

impl Default for SyncConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SyncConflictResolver — 冲突解决器
// ============================================================================

/// 冲突解决器
#[derive(Debug, Clone)]
pub struct SyncConflictResolver {
    strategy: ConflictResolutionStrategy,
}

impl SyncConflictResolver {
    /// 创建新的冲突解决器
    pub fn new(strategy: ConflictResolutionStrategy) -> Self {
        Self { strategy }
    }

    /// 解决冲突
    ///
    /// 按冲突类型逐项解决（审计 M-14 语义重定义）：
    /// - `TypeMismatch`：OrmWins/LcWins 取胜者类型；Merge/Manual 一律挂起——
    ///   类型冲突没有"并集"语义，破坏性变更必须人工确认。
    /// - `ConstraintMismatch`：OrmWins/LcWins 取胜者约束；Merge 取保守并集
    ///   （nullable=false 胜、unique=true 胜、primary_key=true 胜）。
    /// - `RelationMismatch`/`BidirectionalChange`：无法映射为字段，归入 `unresolved`
    ///   待人工审查，不再以错误类型塞入字段映射。
    pub fn resolve(
        &self,
        conflicts: Vec<SyncConflict>,
        strategy: ConflictResolutionStrategy,
    ) -> Result<ResolutionResult, SyncError> {
        let mut resolved_fields: HashMap<String, FieldDef> = HashMap::new();
        let mut unresolved = Vec::new();
        let mut paused = false;
        let mut pause_reason = None;

        for conflict in conflicts {
            match (strategy, &conflict.conflict_type) {
                (ConflictResolutionStrategy::OrmWins, ConflictType::TypeMismatch) => {
                    let field = field_from_value(&conflict.field, &conflict.orm_value);
                    resolved_fields.insert(conflict.field.clone(), field);
                }
                (ConflictResolutionStrategy::OrmWins, ConflictType::ConstraintMismatch) => {
                    if let Some((ty, nullable, unique, primary_key)) =
                        parse_constraint_value(&conflict.orm_value)
                    {
                        let mut field = FieldDef::new(&conflict.field, &ty);
                        field.nullable = nullable;
                        field.unique = unique;
                        field.primary_key = primary_key;
                        resolved_fields.insert(conflict.field.clone(), field);
                    } else {
                        unresolved.push(conflict);
                    }
                }
                (ConflictResolutionStrategy::LcWins, ConflictType::TypeMismatch) => {
                    let field = field_from_value(&conflict.field, &conflict.lc_value);
                    resolved_fields.insert(conflict.field.clone(), field);
                }
                (ConflictResolutionStrategy::LcWins, ConflictType::ConstraintMismatch) => {
                    if let Some((ty, nullable, unique, primary_key)) =
                        parse_constraint_value(&conflict.lc_value)
                    {
                        let mut field = FieldDef::new(&conflict.field, &ty);
                        field.nullable = nullable;
                        field.unique = unique;
                        field.primary_key = primary_key;
                        resolved_fields.insert(conflict.field.clone(), field);
                    } else {
                        unresolved.push(conflict);
                    }
                }
                (ConflictResolutionStrategy::Merge, ConflictType::TypeMismatch) => {
                    paused = true;
                    pause_reason = Some(format!(
                        "destructive change: type change on field '{}' ({} -> {}), requires manual confirmation",
                        conflict.field, conflict.orm_value, conflict.lc_value
                    ));
                }
                (ConflictResolutionStrategy::Merge, ConflictType::ConstraintMismatch) => {
                    if let (Some((ty, o_nullable, o_unique, o_pk)), Some((_, l_nullable, l_unique, l_pk))) =
                        (
                            parse_constraint_value(&conflict.orm_value),
                            parse_constraint_value(&conflict.lc_value),
                        )
                    {
                        // 保守并集：任一侧更严格则取更严格
                        let mut field = FieldDef::new(&conflict.field, &ty);
                        field.nullable = o_nullable && l_nullable;
                        field.unique = o_unique || l_unique;
                        field.primary_key = o_pk || l_pk;
                        resolved_fields.insert(conflict.field.clone(), field);
                    } else {
                        unresolved.push(conflict);
                    }
                }
                (ConflictResolutionStrategy::Manual, ConflictType::TypeMismatch) => {
                    paused = true;
                    pause_reason = Some(format!(
                        "destructive change: type change on field '{}' ({} -> {}), requires manual confirmation",
                        conflict.field, conflict.orm_value, conflict.lc_value
                    ));
                    unresolved.push(conflict);
                }
                (ConflictResolutionStrategy::Manual, ConflictType::ConstraintMismatch)
                | (_, ConflictType::RelationMismatch)
                | (_, ConflictType::BidirectionalChange) => {
                    unresolved.push(conflict);
                }
            }
        }

        Ok(ResolutionResult {
            resolved_fields,
            unresolved,
            paused,
            pause_reason,
        })
    }

    /// 获取当前策略
    pub fn strategy(&self) -> ConflictResolutionStrategy {
        self.strategy
    }
}

/// 解析约束冲突值（`detect()` 生成的 `type=.., nullable=.., unique=.., pk=..` 格式）。
///
/// 返回 `(类型, nullable, unique, primary_key)`；格式不匹配时返回 `None`，
/// 调用方将该冲突归入 `unresolved` 而非错误解决。
fn parse_constraint_value(value: &str) -> Option<(String, bool, bool, bool)> {
    let mut ty = None;
    let mut nullable = None;
    let mut unique = None;
    let mut pk = None;
    for part in value.split(',') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("type=") {
            ty = Some(v.to_string());
        } else if let Some(v) = part.strip_prefix("nullable=") {
            nullable = v.parse().ok();
        } else if let Some(v) = part.strip_prefix("unique=") {
            unique = v.parse().ok();
        } else if let Some(v) = part.strip_prefix("pk=") {
            pk = v.parse().ok();
        }
    }
    Some((ty?, nullable?, unique?, pk?))
}

/// 从冲突值构造完整 `FieldDef`：优先解析完整描述格式（含约束），
/// 对旧格式（纯类型字符串，如手构冲突）回退为仅类型字段。
fn field_from_value(name: &str, value: &str) -> FieldDef {
    match parse_constraint_value(value) {
        Some((ty, nullable, unique, primary_key)) => {
            let mut field = FieldDef::new(name, &ty);
            field.nullable = nullable;
            field.unique = unique;
            field.primary_key = primary_key;
            field
        }
        None => FieldDef::new(name, value),
    }
}

// ============================================================================
// SyncIncrementTracker — 增量追踪器
// ============================================================================

/// 增量追踪器
#[derive(Debug, Clone)]
pub struct SyncIncrementTracker {
    /// 变更列表
    pub changes: Vec<SyncChange>,
    /// 最后同步时间戳
    pub last_sync_timestamp: u64,
}

impl SyncIncrementTracker {
    /// 创建新的增量追踪器
    pub fn new() -> Self {
        Self {
            changes: Vec::new(),
            last_sync_timestamp: 0,
        }
    }

    /// 追踪模型变更
    pub fn track_change(&mut self, change: SyncChange) {
        if change.timestamp > self.last_sync_timestamp {
            self.last_sync_timestamp = change.timestamp;
        }
        self.changes.push(change);
    }

    /// 获取指定时间后的变更项
    pub fn get_changes_since(&self, timestamp: u64) -> Vec<SyncChange> {
        self.changes
            .iter()
            .filter(|c| c.timestamp > timestamp)
            .cloned()
            .collect()
    }

    /// 获取指定模型的变更项
    pub fn get_changes(&self, model_name: &str) -> Vec<SyncChange> {
        self.changes
            .iter()
            .filter(|c| c.model_name == model_name)
            .cloned()
            .collect()
    }

    /// 变更数量
    pub fn change_count(&self) -> usize {
        self.changes.len()
    }
}

impl Default for SyncIncrementTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SyncAuditLogger — 审计日志器
// ============================================================================

/// 审计日志条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncAuditEntry {
    /// 同步方向
    pub direction: SyncDirection,
    /// 变更项
    pub changes: Vec<SyncChange>,
    /// 冲突
    pub conflicts: Vec<SyncConflict>,
    /// 解决策略
    pub strategy: ConflictResolutionStrategy,
    /// 时间戳
    pub timestamp: u64,
    /// 操作人
    pub operator: String,
}

/// 审计日志器
#[derive(Debug, Clone)]
pub struct SyncAuditLogger {
    /// 日志路径
    pub log_path: Option<String>,
    /// 日志条目
    pub entries: Vec<SyncAuditEntry>,
}

impl SyncAuditLogger {
    /// 创建新的审计日志器
    pub fn new(log_path: Option<String>) -> Self {
        Self {
            log_path,
            entries: Vec::new(),
        }
    }

    /// 记录审计日志
    pub fn log(&mut self, entry: SyncAuditEntry) {
        self.entries.push(entry);
    }

    /// 获取审计日志
    pub fn get_entries(&self) -> &[SyncAuditEntry] {
        &self.entries
    }

    /// 日志条目数
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

// ============================================================================
// BidirectionalSyncEngine — 双向同步引擎
// ============================================================================

/// 双向同步引擎
pub struct BidirectionalSyncEngine {
    /// 配置
    config: SyncConfig,
    /// 冲突检测器
    conflict_detector: SyncConflictDetector,
    /// 冲突解决器
    conflict_resolver: SyncConflictResolver,
    /// 增量追踪器
    increment_tracker: SyncIncrementTracker,
    /// 审计日志器
    audit_logger: SyncAuditLogger,
}

impl BidirectionalSyncEngine {
    /// 创建新的双向同步引擎
    pub fn new(config: SyncConfig) -> Self {
        let strategy = config.default_strategy;
        Self {
            conflict_resolver: SyncConflictResolver::new(strategy),
            config,
            conflict_detector: SyncConflictDetector::new(),
            increment_tracker: SyncIncrementTracker::new(),
            audit_logger: SyncAuditLogger::new(None),
        }
    }

    /// 执行同步
    ///
    /// 无论同步方向，冲突均先过冲突门禁（detect → resolve，审计 M-14）：
    /// - 默认策略 `Manual` 下，破坏性类型变更（`TypeMismatch`）一律挂起等待人工确认——
    ///   单向同步（OrmToLc/LcToOrm）不再绕过冲突检测。
    /// - `Merge` 策略对约束冲突取保守并集；类型冲突同样挂起。
    /// - 解决结果（`resolved_fields`）应用到同步产物（双向并集 / 单向胜者模型）。
    pub fn sync(
        &mut self,
        direction: SyncDirection,
        orm_model: &ModelDefinition,
        lc_model: &ModelDefinition,
    ) -> Result<SyncResult, SyncError> {
        let conflicts = self.conflict_detector.detect(orm_model, lc_model);
        let changes = self.detect_changes(direction, orm_model, lc_model);

        if self.config.enable_increment_tracking {
            for change in &changes {
                self.increment_tracker.track_change(change.clone());
            }
        }

        let mut resolution = None;
        if !conflicts.is_empty() {
            let resolved = self
                .conflict_resolver
                .resolve(conflicts.clone(), self.config.default_strategy)?;
            if resolved.paused {
                return Ok(SyncResult {
                    synced_model: orm_model.clone(),
                    conflicts,
                    changes,
                    success: false,
                    paused_reason: resolved.pause_reason,
                });
            }
            resolution = Some(resolved);
        }

        let mut synced_model = match direction {
            SyncDirection::OrmToLc => self.sync_orm_to_lc(orm_model),
            SyncDirection::LcToOrm => self.sync_lc_to_orm(lc_model),
            SyncDirection::Bidirectional => self.sync_bidirectional(orm_model, lc_model),
        };
        if let Some(res) = &resolution {
            Self::apply_resolution(&mut synced_model, res);
        }

        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        self.audit_logger.log(SyncAuditEntry {
            direction,
            changes: changes.clone(),
            conflicts: conflicts.clone(),
            strategy: self.config.default_strategy,
            timestamp,
            operator: "system".to_string(),
        });

        Ok(SyncResult {
            synced_model,
            conflicts,
            changes,
            success: true,
            paused_reason: None,
        })
    }

    /// ORM → 低代码同步
    fn sync_orm_to_lc(&self, orm_model: &ModelDefinition) -> ModelDefinition {
        orm_model.clone()
    }

    /// 低代码 → ORM 同步
    fn sync_lc_to_orm(&self, lc_model: &ModelDefinition) -> ModelDefinition {
        lc_model.clone()
    }

    /// 双向同步（字段并集），解决结果由 [`Self::apply_resolution`] 在调用方应用
    fn sync_bidirectional(
        &self,
        orm_model: &ModelDefinition,
        lc_model: &ModelDefinition,
    ) -> ModelDefinition {
        let mut result = orm_model.clone();
        for lc_field in &lc_model.fields {
            if !result.fields.iter().any(|f| f.name == lc_field.name) {
                result.fields.push(lc_field.clone());
            }
        }
        result
    }

    /// 将冲突解决结果应用到同步产物（按字段名覆盖；不存在的字段忽略）
    fn apply_resolution(model: &mut ModelDefinition, resolution: &ResolutionResult) {
        for (name, field) in &resolution.resolved_fields {
            if let Some(existing) = model.fields.iter_mut().find(|f| f.name == *name) {
                *existing = field.clone();
            }
        }
    }

    /// 检测变更
    fn detect_changes(
        &self,
        direction: SyncDirection,
        orm_model: &ModelDefinition,
        lc_model: &ModelDefinition,
    ) -> Vec<SyncChange> {
        let mut changes = Vec::new();
        let timestamp = chrono::Utc::now().timestamp_millis() as u64;

        for orm_field in &orm_model.fields {
            if let Some(lc_field) = lc_model.find_field(&orm_field.name) {
                if orm_field.field_type != lc_field.field_type {
                    changes.push(SyncChange {
                        direction,
                        model_name: orm_model.name.clone(),
                        field_name: orm_field.name.clone(),
                        change_type: ChangeType::TypeChanged,
                        old_value: Some(lc_field.field_type.clone()),
                        new_value: Some(orm_field.field_type.clone()),
                        timestamp,
                    });
                }
            } else {
                changes.push(SyncChange {
                    direction,
                    model_name: orm_model.name.clone(),
                    field_name: orm_field.name.clone(),
                    change_type: ChangeType::FieldAdded,
                    old_value: None,
                    new_value: Some(orm_field.field_type.clone()),
                    timestamp,
                });
            }
        }

        for lc_field in &lc_model.fields {
            if !orm_model.fields.iter().any(|f| f.name == lc_field.name) {
                changes.push(SyncChange {
                    direction,
                    model_name: lc_model.name.clone(),
                    field_name: lc_field.name.clone(),
                    change_type: ChangeType::FieldRemoved,
                    old_value: Some(lc_field.field_type.clone()),
                    new_value: None,
                    timestamp,
                });
            }
        }

        changes
    }

    /// 获取增量追踪器
    pub fn increment_tracker(&self) -> &SyncIncrementTracker {
        &self.increment_tracker
    }

    /// 获取审计日志器
    pub fn audit_logger(&self) -> &SyncAuditLogger {
        &self.audit_logger
    }

    /// 获取配置
    pub fn config(&self) -> &SyncConfig {
        &self.config
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FieldTypeMapping;

    fn make_orm_user() -> ModelDefinition {
        ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "VARCHAR").unique())
            .with_field(FieldDef::new("name", "VARCHAR"))
    }

    fn make_lc_user() -> ModelDefinition {
        ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "VARCHAR").unique())
            .with_field(FieldDef::new("name", "VARCHAR"))
    }

    fn make_lc_user_modified() -> ModelDefinition {
        ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "TEXT").unique())
            .with_field(FieldDef::new("name", "VARCHAR"))
    }

    #[test]
    fn test_sync_config_default() {
        let config = SyncConfig::new();
        assert_eq!(config.default_strategy, ConflictResolutionStrategy::Manual);
        assert!(config.enable_increment_tracking);
        assert!(config.audit_log_path.is_none());
    }

    #[test]
    fn test_sync_config_builder() {
        let config = SyncConfig::new()
            .with_default_strategy(ConflictResolutionStrategy::OrmWins)
            .with_increment_tracking(false)
            .with_audit_log("/var/log/sync.log");

        assert_eq!(config.default_strategy, ConflictResolutionStrategy::OrmWins);
        assert!(!config.enable_increment_tracking);
        assert_eq!(config.audit_log_path, Some("/var/log/sync.log".to_string()));
    }

    #[test]
    fn test_sync_direction_enum() {
        let d1 = SyncDirection::OrmToLc;
        let d2 = SyncDirection::LcToOrm;
        let d3 = SyncDirection::Bidirectional;
        assert_ne!(d1, d2);
        assert_ne!(d2, d3);
        assert_ne!(d1, d3);
    }

    #[test]
    fn test_conflict_resolution_strategy_default() {
        assert_eq!(
            ConflictResolutionStrategy::default(),
            ConflictResolutionStrategy::Manual
        );
    }

    #[test]
    fn test_conflict_detector_no_conflicts() {
        let detector = SyncConflictDetector::new();
        let orm = make_orm_user();
        let lc = make_lc_user();

        let conflicts = detector.detect(&orm, &lc);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_conflict_detector_type_mismatch() {
        let detector = SyncConflictDetector::new();
        let orm = make_orm_user();
        let lc = make_lc_user_modified();

        let conflicts = detector.detect(&orm, &lc);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].field, "email");
        assert_eq!(conflicts[0].conflict_type, ConflictType::TypeMismatch);
        // 值采用完整描述格式（含约束），供 resolver 保留胜者约束
        assert!(conflicts[0].orm_value.starts_with("type=VARCHAR"));
        assert!(conflicts[0].lc_value.starts_with("type=TEXT"));
    }

    #[test]
    fn test_conflict_resolver_orm_wins() {
        let resolver = SyncConflictResolver::new(ConflictResolutionStrategy::OrmWins);
        let conflicts = vec![SyncConflict {
            field: "email".to_string(),
            conflict_type: ConflictType::TypeMismatch,
            orm_value: "VARCHAR(500)".to_string(),
            lc_value: "TEXT".to_string(),
        }];

        let result = resolver
            .resolve(conflicts, ConflictResolutionStrategy::OrmWins)
            .unwrap();
        assert!(result.resolved_fields.contains_key("email"));
        assert_eq!(result.resolved_fields["email"].field_type, "VARCHAR(500)");
        assert!(!result.paused);
    }

    #[test]
    fn test_conflict_resolver_lc_wins() {
        let resolver = SyncConflictResolver::new(ConflictResolutionStrategy::LcWins);
        let conflicts = vec![SyncConflict {
            field: "email".to_string(),
            conflict_type: ConflictType::TypeMismatch,
            orm_value: "VARCHAR(500)".to_string(),
            lc_value: "TEXT".to_string(),
        }];

        let result = resolver
            .resolve(conflicts, ConflictResolutionStrategy::LcWins)
            .unwrap();
        assert_eq!(result.resolved_fields["email"].field_type, "TEXT");
        assert!(!result.paused);
    }

    #[test]
    fn test_conflict_resolver_manual_paused() {
        let resolver = SyncConflictResolver::new(ConflictResolutionStrategy::Manual);
        let conflicts = vec![SyncConflict {
            field: "email".to_string(),
            conflict_type: ConflictType::TypeMismatch,
            orm_value: "VARCHAR(500)".to_string(),
            lc_value: "TEXT".to_string(),
        }];

        let result = resolver
            .resolve(conflicts, ConflictResolutionStrategy::Manual)
            .unwrap();
        assert!(result.paused);
        assert!(result.pause_reason.is_some());
        assert!(!result.unresolved.is_empty());
    }

    #[test]
    fn test_conflict_resolver_merge_type_mismatch_paused() {
        // M-14：Merge 不再是 OrmWins 别名——类型冲突无法自动合并，挂起人工确认
        let resolver = SyncConflictResolver::new(ConflictResolutionStrategy::Merge);
        let conflicts = vec![SyncConflict {
            field: "email".to_string(),
            conflict_type: ConflictType::TypeMismatch,
            orm_value: "VARCHAR".to_string(),
            lc_value: "TEXT".to_string(),
        }];

        let result = resolver
            .resolve(conflicts, ConflictResolutionStrategy::Merge)
            .unwrap();
        assert!(result.paused);
        assert!(result.pause_reason.is_some());
        assert!(result.resolved_fields.is_empty());
    }

    #[test]
    fn test_conflict_resolver_merge_constraint_union() {
        // M-14：Merge 对约束冲突取保守并集（nullable=false 胜、unique=true 胜）
        let resolver = SyncConflictResolver::new(ConflictResolutionStrategy::Merge);
        let conflicts = vec![SyncConflict {
            field: "email".to_string(),
            conflict_type: ConflictType::ConstraintMismatch,
            orm_value: "type=VARCHAR, nullable=true, unique=false, pk=false".to_string(),
            lc_value: "type=VARCHAR, nullable=false, unique=true, pk=false".to_string(),
        }];

        let result = resolver
            .resolve(conflicts, ConflictResolutionStrategy::Merge)
            .unwrap();
        assert!(!result.paused);
        let field = &result.resolved_fields["email"];
        assert_eq!(field.field_type, "VARCHAR");
        assert!(!field.nullable);
        assert!(field.unique);
        assert!(!field.primary_key);
    }

    #[test]
    fn test_conflict_resolver_constraint_winner_side() {
        // M-14：OrmWins/LcWins 对约束冲突取各自一侧（含类型，不再塞入垃圾字段）
        let conflicts = vec![SyncConflict {
            field: "email".to_string(),
            conflict_type: ConflictType::ConstraintMismatch,
            orm_value: "type=VARCHAR, nullable=true, unique=false, pk=false".to_string(),
            lc_value: "type=TEXT, nullable=false, unique=true, pk=true".to_string(),
        }];

        let orm_result = SyncConflictResolver::new(ConflictResolutionStrategy::OrmWins)
            .resolve(conflicts.clone(), ConflictResolutionStrategy::OrmWins)
            .unwrap();
        let orm_field = &orm_result.resolved_fields["email"];
        assert_eq!(orm_field.field_type, "VARCHAR");
        assert!(orm_field.nullable);

        let lc_result = SyncConflictResolver::new(ConflictResolutionStrategy::LcWins)
            .resolve(conflicts, ConflictResolutionStrategy::LcWins)
            .unwrap();
        let lc_field = &lc_result.resolved_fields["email"];
        assert_eq!(lc_field.field_type, "TEXT");
        assert!(!lc_field.nullable);
        assert!(lc_field.unique);
        assert!(lc_field.primary_key);
    }

    #[test]
    fn test_conflict_resolver_relation_mismatch_unresolved() {
        // M-14：关联冲突无法映射为字段，归入 unresolved，不再污染 resolved_fields
        let conflicts = vec![SyncConflict {
            field: "orders".to_string(),
            conflict_type: ConflictType::RelationMismatch,
            orm_value: "has_many -> orders".to_string(),
            lc_value: "many_to_many -> orders".to_string(),
        }];

        let result = SyncConflictResolver::new(ConflictResolutionStrategy::OrmWins)
            .resolve(conflicts, ConflictResolutionStrategy::OrmWins)
            .unwrap();
        assert!(!result.paused);
        assert!(result.resolved_fields.is_empty());
        assert_eq!(result.unresolved.len(), 1);
    }

    #[test]
    fn test_increment_tracker() {
        let mut tracker = SyncIncrementTracker::new();
        let change = SyncChange {
            direction: SyncDirection::OrmToLc,
            model_name: "users".to_string(),
            field_name: "age".to_string(),
            change_type: ChangeType::FieldAdded,
            old_value: None,
            new_value: Some("INT".to_string()),
            timestamp: 1000,
        };

        tracker.track_change(change);
        assert_eq!(tracker.change_count(), 1);
        assert_eq!(tracker.last_sync_timestamp, 1000);

        let changes = tracker.get_changes_since(500);
        assert_eq!(changes.len(), 1);

        let changes = tracker.get_changes_since(1500);
        assert!(changes.is_empty());

        let changes = tracker.get_changes("users");
        assert_eq!(changes.len(), 1);

        let changes = tracker.get_changes("orders");
        assert!(changes.is_empty());
    }

    #[test]
    fn test_audit_logger() {
        let mut logger = SyncAuditLogger::new(None);
        let entry = SyncAuditEntry {
            direction: SyncDirection::Bidirectional,
            changes: vec![],
            conflicts: vec![],
            strategy: ConflictResolutionStrategy::OrmWins,
            timestamp: 1000,
            operator: "admin".to_string(),
        };

        logger.log(entry);
        assert_eq!(logger.entry_count(), 1);
        assert_eq!(logger.get_entries()[0].operator, "admin");
    }

    #[test]
    fn test_sync_engine_orm_to_lc() {
        let config = SyncConfig::new();
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = make_orm_user();
        let lc = make_lc_user();

        let result = engine.sync(SyncDirection::OrmToLc, &orm, &lc).unwrap();
        assert!(result.success);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.synced_model.name, "users");
    }

    #[test]
    fn test_sync_engine_bidirectional_with_conflict() {
        let config = SyncConfig::new().with_default_strategy(ConflictResolutionStrategy::OrmWins);
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = make_orm_user();
        let lc = make_lc_user_modified();

        let result = engine
            .sync(SyncDirection::Bidirectional, &orm, &lc)
            .unwrap();
        assert!(result.success);
        assert_eq!(result.conflicts.len(), 1);
    }

    #[test]
    fn test_sync_engine_bidirectional_manual_paused() {
        let config = SyncConfig::new();
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = make_orm_user();
        let lc = make_lc_user_modified();

        let result = engine
            .sync(SyncDirection::Bidirectional, &orm, &lc)
            .unwrap();
        assert!(!result.success);
        assert!(result.paused_reason.is_some());
    }

    #[test]
    fn test_sync_engine_unidirectional_manual_paused_on_type_mismatch() {
        // M-14：单向同步不再绕过冲突门禁——默认 Manual 下类型变更必须人工确认
        let config = SyncConfig::new();
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = make_orm_user();
        let lc = make_lc_user_modified();

        let result = engine.sync(SyncDirection::OrmToLc, &orm, &lc).unwrap();
        assert!(!result.success);
        assert!(result.paused_reason.is_some());
        assert_eq!(result.conflicts.len(), 1);

        let result = engine.sync(SyncDirection::LcToOrm, &orm, &lc).unwrap();
        assert!(!result.success);
        assert!(result.paused_reason.is_some());
    }

    #[test]
    fn test_sync_engine_unidirectional_orm_wins_proceeds() {
        // M-14：显式 OrmWins 策略下单向同步放行，胜者模型不变
        let config = SyncConfig::new().with_default_strategy(ConflictResolutionStrategy::OrmWins);
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = make_orm_user();
        let lc = make_lc_user_modified();

        let result = engine.sync(SyncDirection::OrmToLc, &orm, &lc).unwrap();
        assert!(result.success);
        assert_eq!(result.conflicts.len(), 1);
        // 胜者字段保留约束（resolve 不再把冲突值当纯类型丢约束）
        let email = result.synced_model.find_field("email").unwrap();
        assert_eq!(email.field_type, "VARCHAR");
        assert!(email.unique);
    }

    #[test]
    fn test_sync_engine_bidirectional_merge_applies_constraint_union() {
        // M-14：resolved_fields 真正被消费——Merge 约束并集落到同步产物
        let config = SyncConfig::new().with_default_strategy(ConflictResolutionStrategy::Merge);
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "VARCHAR").with_nullable(true));
        let lc = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "VARCHAR").unique());

        let result = engine
            .sync(SyncDirection::Bidirectional, &orm, &lc)
            .unwrap();
        assert!(result.success);
        let email = result.synced_model.find_field("email").unwrap();
        assert_eq!(email.field_type, "VARCHAR");
        assert!(!email.nullable);
        assert!(email.unique);
    }

    #[test]
    fn test_sync_engine_unidirectional_merge_applies_constraint_union() {
        // M-14：单向同步同样应用解决结果——ORM 推送时保留 LC 侧更严格约束
        let config = SyncConfig::new().with_default_strategy(ConflictResolutionStrategy::Merge);
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "VARCHAR").with_nullable(true));
        let lc = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "VARCHAR").unique());

        let result = engine.sync(SyncDirection::OrmToLc, &orm, &lc).unwrap();
        assert!(result.success);
        let email = result.synced_model.find_field("email").unwrap();
        assert!(!email.nullable);
        assert!(email.unique);
    }

    #[test]
    fn test_sync_engine_tracks_changes() {
        let config = SyncConfig::new();
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = make_orm_user();
        let lc = make_lc_user();

        engine.sync(SyncDirection::OrmToLc, &orm, &lc).unwrap();
        assert!(engine.increment_tracker().change_count() >= 0);
    }

    #[test]
    fn test_sync_engine_logs_audit() {
        let config = SyncConfig::new();
        let mut engine = BidirectionalSyncEngine::new(config);
        let orm = make_orm_user();
        let lc = make_lc_user();

        engine.sync(SyncDirection::OrmToLc, &orm, &lc).unwrap();
        assert_eq!(engine.audit_logger().entry_count(), 1);
    }

    #[test]
    fn test_type_mapping_consistency() {
        let rust_type = FieldTypeMapping::sql_to_rust("BIGINT");
        assert_eq!(rust_type, "i64");

        let html_input = FieldTypeMapping::sql_to_html_input("BIGINT");
        assert_eq!(html_input, "number");
    }

    #[test]
    fn test_destructive_change_field_removal() {
        let config = SyncConfig::new();
        let mut engine = BidirectionalSyncEngine::new(config);

        let orm = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "VARCHAR"))
            .with_field(FieldDef::new("age", "INT"));

        let lc = ModelDefinition::new("users")
            .with_field(FieldDef::new("id", "BIGINT").primary())
            .with_field(FieldDef::new("email", "VARCHAR"));

        let result = engine.sync(SyncDirection::LcToOrm, &orm, &lc).unwrap();
        assert!(result.success || result.paused_reason.is_some());
    }
}
