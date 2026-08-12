//! POC → 转正 API 迁移指引（`db-fusion-v2` feature）
//!
//! v4.3.0 POC API → v4.4.0 转正 API 迁移步骤。
//!
//! | POC API (v4.3.0) | 转正 API (v4.4.0) | 迁移说明 |
//! |-------------------|-------------------|----------|
//! | `MemoryFusionCache` | `TtlFusionCache` | 替换缓存实现，TTL 自动过期 |
//! | `FusionConfig` | `FusionConfig`（不变） | 配置结构保留，仅标注 `#[deprecated]` 迁移指引 |
//! | `FusionExecutor` | `FusionExecutor` + `with_invalidation_bus` | 添加失效广播 |
//! | 搜索下推仅记录数据源 | `VectorPushdownExecutor` | 真实向量检索下推 |
//! | 无 CDC 同步 | `CdcSyncCoordinator` | CDC 增量同步缓存/搜索索引 |

/// 迁移步骤
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStep {
    /// 步骤编号
    pub step: u8,
    /// POC API
    pub poc_api: String,
    /// 转正 API
    pub v2_api: String,
    /// 迁移说明
    pub description: String,
}

/// 返回完整迁移步骤列表
pub fn migration_steps() -> Vec<MigrationStep> {
    vec![
        MigrationStep {
            step: 1,
            poc_api: "MemoryFusionCache".into(),
            v2_api: "TtlFusionCache".into(),
            description: "替换缓存实现：MemoryFusionCache → TtlFusionCache，TTL 自动过期".into(),
        },
        MigrationStep {
            step: 2,
            poc_api: "FusionExecutor::new".into(),
            v2_api: "FusionExecutor::new + with_invalidation_bus".into(),
            description: "添加失效广播：with_invalidation_bus 跨实例缓存失效".into(),
        },
        MigrationStep {
            step: 3,
            poc_api: "搜索下推仅记录数据源 (executor.rs:146)".into(),
            v2_api: "VectorPushdownExecutor".into(),
            description: "真实向量检索下推：调用 HybridSearcher::search 执行三源并行查询".into(),
        },
        MigrationStep {
            step: 4,
            poc_api: "无 CDC 同步".into(),
            v2_api: "CdcSyncCoordinator".into(),
            description: "CDC 增量同步：主库变更自动同步到缓存/搜索索引下游".into(),
        },
    ]
}

/// 生成迁移指引文本
pub fn migration_guide() -> String {
    let steps = migration_steps();
    let mut guide = String::from("db-fusion POC → v2 迁移指引\n\n");
    for s in &steps {
        guide.push_str(&format!(
            "步骤 {}: {} → {}\n  {}\n\n",
            s.step, s.poc_api, s.v2_api, s.description
        ));
    }
    guide
}

/// 返回 POC API 废弃标注 note
pub fn deprecated_note() -> &'static str {
    "use TtlFusionCache + CdcSyncCoordinator + VectorPushdownExecutor with db-fusion-v2 feature instead"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_steps_complete() {
        let steps = migration_steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].step, 1);
        assert_eq!(steps[3].step, 4);
    }

    #[test]
    fn migration_guide_contains_all_steps() {
        let guide = migration_guide();
        assert!(guide.contains("MemoryFusionCache"));
        assert!(guide.contains("TtlFusionCache"));
        assert!(guide.contains("VectorPushdownExecutor"));
        assert!(guide.contains("CdcSyncCoordinator"));
        assert!(guide.contains("with_invalidation_bus"));
    }

    #[test]
    fn deprecated_note_mentions_all_v2_apis() {
        let note = deprecated_note();
        assert!(note.contains("TtlFusionCache"));
        assert!(note.contains("CdcSyncCoordinator"));
        assert!(note.contains("VectorPushdownExecutor"));
        assert!(note.contains("db-fusion-v2"));
    }

    #[test]
    fn migration_step_ordering() {
        let steps = migration_steps();
        for i in 0..steps.len() - 1 {
            assert!(steps[i].step < steps[i + 1].step);
        }
    }
}
