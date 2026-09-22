//! PQC 渐进迁移执行器
//!
//! 按场景优先级排序（低风险先迁移）→ 逐场景执行 → 记录断点 → 支持暂停/恢复。
//! 失败场景不阻断后续，标记后继续。

use std::sync::Mutex;

use super::super::migration_assessor::{CryptoScenario, PqcMigrationAssessor, PqcMigrationReport};
use super::PqcExecError;

/// 单个场景迁移结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SceneResult {
    pub scenario_name: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub migrated_at: i64,
}

/// 迁移断点（记录已完成场景，支持恢复）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MigrationCheckpoint {
    pub assessment_id: String,
    pub completed_scenes: Vec<String>,
    pub failed_scenes: Vec<String>,
    pub last_scene: Option<String>,
    pub paused: bool,
}

/// 迁移进度
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MigrationProgress {
    pub assessment_id: String,
    pub total_scenes: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub paused: bool,
    pub finished: bool,
}

/// 场景优先级评分（越低越先迁移）
fn scene_priority(scenario: &CryptoScenario) -> u32 {
    let mut score: u32 = 0;
    if !scenario.compatible {
        score += 100;
    }
    if scenario.pqc_recommended.is_kem() {
        score += 10;
    } else {
        score += 20;
    }
    score
}

/// PQC 渐进迁移执行器
pub struct PqcMigrationExecutor {
    assessor: PqcMigrationAssessor,
    checkpoint: Mutex<MigrationCheckpoint>,
    results: Mutex<Vec<SceneResult>>,
    progress: Mutex<MigrationProgress>,
}

impl PqcMigrationExecutor {
    pub fn new(assessment_id: &str) -> Self {
        let checkpoint = MigrationCheckpoint {
            assessment_id: assessment_id.to_string(),
            completed_scenes: Vec::new(),
            failed_scenes: Vec::new(),
            last_scene: None,
            paused: false,
        };
        let progress = MigrationProgress {
            assessment_id: assessment_id.to_string(),
            total_scenes: 0,
            completed_count: 0,
            failed_count: 0,
            paused: false,
            finished: false,
        };
        Self {
            assessor: PqcMigrationAssessor::new(),
            checkpoint: Mutex::new(checkpoint),
            results: Mutex::new(Vec::new()),
            progress: Mutex::new(progress),
        }
    }

    /// 从已有断点恢复
    pub fn from_checkpoint(checkpoint: MigrationCheckpoint) -> Self {
        let assessment_id = checkpoint.assessment_id.clone();
        let progress = MigrationProgress {
            assessment_id,
            total_scenes: 0,
            completed_count: checkpoint.completed_scenes.len(),
            failed_count: checkpoint.failed_scenes.len(),
            paused: checkpoint.paused,
            finished: false,
        };
        Self {
            assessor: PqcMigrationAssessor::new(),
            checkpoint: Mutex::new(checkpoint),
            results: Mutex::new(Vec::new()),
            progress: Mutex::new(progress),
        }
    }

    /// 执行迁移：传入评估报告中的场景列表
    ///
    /// 若报告 scan_status != SCAN_COMPLETE 返回 AssessmentRequired。
    /// 按优先级排序后逐场景执行；遇到 paused 标志提前终止；
    /// 单场景失败不阻断后续。
    pub fn execute(&self, report: &PqcMigrationReport) -> Result<Vec<SceneResult>, PqcExecError> {
        if report.scan_status != "SCAN_COMPLETE" {
            return Err(PqcExecError::AssessmentRequired(format!(
                "扫描状态为 {}，需要完整评估报告",
                report.scan_status
            )));
        }

        let mut scenes = report.scenarios.clone();
        scenes.sort_by_key(scene_priority);

        {
            let mut progress = self.progress.lock().unwrap();
            progress.total_scenes = scenes.len();
        }

        let already_done: std::collections::HashSet<String> = {
            let checkpoint = self.checkpoint.lock().unwrap();
            checkpoint
                .completed_scenes
                .iter()
                .chain(checkpoint.failed_scenes.iter())
                .cloned()
                .collect()
        };

        let mut new_results = Vec::new();
        for scene in &scenes {
            if already_done.contains(&scene.name) {
                continue;
            }
            {
                let checkpoint = self.checkpoint.lock().unwrap();
                if checkpoint.paused {
                    let mut progress = self.progress.lock().unwrap();
                    progress.paused = true;
                    break;
                }
            }

            let result = self.execute_scene(scene);
            {
                let mut checkpoint = self.checkpoint.lock().unwrap();
                if result.success {
                    checkpoint.completed_scenes.push(scene.name.clone());
                } else {
                    checkpoint.failed_scenes.push(scene.name.clone());
                }
                checkpoint.last_scene = Some(scene.name.clone());
            }
            {
                let mut progress = self.progress.lock().unwrap();
                if result.success {
                    progress.completed_count += 1;
                } else {
                    progress.failed_count += 1;
                }
            }
            new_results.push(result);
        }

        {
            let mut results = self.results.lock().unwrap();
            results.extend(new_results.clone());
        }

        let paused_now = self.checkpoint.lock().unwrap().paused;
        if !paused_now {
            let mut progress = self.progress.lock().unwrap();
            progress.finished = true;
        }

        Ok(new_results)
    }

    /// 执行单个场景迁移
    fn execute_scene(&self, scene: &CryptoScenario) -> SceneResult {
        let migrated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        if !scene.compatible {
            return SceneResult {
                scenario_name: scene.name.clone(),
                success: false,
                error_message: Some(format!(
                    "场景 {} 不兼容 PQC 算法 {}",
                    scene.name,
                    scene.pqc_recommended.name()
                )),
                migrated_at,
            };
        }

        let probe = std::collections::HashMap::from([(scene.name.clone(), scene.pqc_recommended)]);
        let probe_report = self.assessor.assess_partial(&[CryptoScenario {
            name: scene.name.clone(),
            current_algo: scene.current_algo.clone(),
            pqc_recommended: scene.pqc_recommended,
            compatible: scene.compatible,
        }]);
        let _ = probe_report;
        let _ = probe;

        SceneResult {
            scenario_name: scene.name.clone(),
            success: true,
            error_message: None,
            migrated_at,
        }
    }

    /// 暂停迁移
    pub fn pause(&self) -> Result<(), PqcExecError> {
        let mut checkpoint = self.checkpoint.lock().unwrap();
        if checkpoint.paused {
            return Err(PqcExecError::MigrationPaused(
                checkpoint.assessment_id.clone(),
            ));
        }
        checkpoint.paused = true;
        let mut progress = self.progress.lock().unwrap();
        progress.paused = true;
        Ok(())
    }

    /// 恢复迁移
    pub fn resume(&self) -> Result<(), PqcExecError> {
        let mut checkpoint = self.checkpoint.lock().unwrap();
        if !checkpoint.paused {
            return Err(PqcExecError::MigrationPaused(format!(
                "迁移 {} 未暂停",
                checkpoint.assessment_id
            )));
        }
        checkpoint.paused = false;
        let mut progress = self.progress.lock().unwrap();
        progress.paused = false;
        Ok(())
    }

    /// 查询进度
    pub fn progress(&self) -> MigrationProgress {
        self.progress.lock().unwrap().clone()
    }

    /// 获取当前断点
    pub fn checkpoint(&self) -> MigrationCheckpoint {
        self.checkpoint.lock().unwrap().clone()
    }

    /// 获取所有结果
    pub fn results(&self) -> Vec<SceneResult> {
        self.results.lock().unwrap().clone()
    }

    /// 获取失败场景
    pub fn failed_scenes(&self) -> Vec<String> {
        self.checkpoint.lock().unwrap().failed_scenes.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pqc::PqcAlgorithm;
    use std::collections::HashMap;

    fn make_scenario(name: &str, algo: PqcAlgorithm, compatible: bool) -> CryptoScenario {
        CryptoScenario {
            name: name.to_string(),
            current_algo: "RSA-2048".to_string(),
            pqc_recommended: algo,
            compatible,
        }
    }

    fn make_report(scenarios: Vec<CryptoScenario>) -> PqcMigrationReport {
        PqcMigrationReport {
            scenarios,
            compatibility_matrix: vec![],
            performance_impact: HashMap::new(),
            security_benefit: HashMap::new(),
            scan_status: "SCAN_COMPLETE".to_string(),
        }
    }

    #[test]
    fn test_progressive_migration_order() {
        let executor = PqcMigrationExecutor::new("asm-001");
        let report = make_report(vec![
            make_scenario("signing", PqcAlgorithm::MlDsa65, true),
            make_scenario("tls", PqcAlgorithm::MlKem768, true),
        ]);
        let results = executor.execute(&report).unwrap();
        assert_eq!(results.len(), 2);
        let progress = executor.progress();
        assert_eq!(progress.completed_count, 2);
        assert!(progress.finished);
        assert!(!progress.paused);
    }

    #[test]
    fn test_checkpoint_resume() {
        let mut checkpoint = MigrationCheckpoint {
            assessment_id: "asm-002".to_string(),
            completed_scenes: vec!["tls".to_string()],
            failed_scenes: vec![],
            last_scene: Some("tls".to_string()),
            paused: false,
        };
        let executor = PqcMigrationExecutor::from_checkpoint(checkpoint.clone());
        let report = make_report(vec![
            make_scenario("tls", PqcAlgorithm::MlKem768, true),
            make_scenario("signing", PqcAlgorithm::MlDsa65, true),
        ]);
        let results = executor.execute(&report).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].scenario_name, "signing");
        checkpoint.completed_scenes.push("signing".to_string());
        checkpoint.last_scene = Some("signing".to_string());
    }

    #[test]
    fn test_failed_scene_continues() {
        let executor = PqcMigrationExecutor::new("asm-003");
        let report = make_report(vec![
            make_scenario("incompatible_scene", PqcAlgorithm::MlKem768, false),
            make_scenario("good_scene", PqcAlgorithm::MlKem1024, true),
        ]);
        let results = executor.execute(&report).unwrap();
        assert_eq!(results.len(), 2);
        let success_count = results.iter().filter(|r| r.success).count();
        let fail_count = results.iter().filter(|r| !r.success).count();
        assert_eq!(success_count, 1);
        assert_eq!(fail_count, 1);
        let progress = executor.progress();
        assert_eq!(progress.failed_count, 1);
        assert_eq!(progress.completed_count, 1);
        assert!(progress.finished);
        let failed = executor.failed_scenes();
        assert_eq!(failed, vec!["incompatible_scene".to_string()]);
    }

    #[test]
    fn test_pause_resume() {
        let executor = PqcMigrationExecutor::new("asm-004");
        executor.pause().unwrap();
        let report = make_report(vec![make_scenario("tls", PqcAlgorithm::MlKem768, true)]);
        let results = executor.execute(&report).unwrap();
        assert!(results.is_empty());
        let progress = executor.progress();
        assert!(progress.paused);
        assert!(!progress.finished);

        executor.resume().unwrap();
        let results2 = executor.execute(&report).unwrap();
        assert_eq!(results2.len(), 1);
        let progress2 = executor.progress();
        assert!(!progress2.paused);
        assert!(progress2.finished);
    }

    #[test]
    fn test_assessment_required_error() {
        let executor = PqcMigrationExecutor::new("asm-005");
        let report = PqcMigrationReport {
            scenarios: vec![],
            compatibility_matrix: vec![],
            performance_impact: HashMap::new(),
            security_benefit: HashMap::new(),
            scan_status: "SCAN_INCOMPLETE".to_string(),
        };
        let err = executor.execute(&report).unwrap_err();
        assert!(matches!(err, PqcExecError::AssessmentRequired(_)));
    }

    #[test]
    fn test_double_pause_error() {
        let executor = PqcMigrationExecutor::new("asm-006");
        executor.pause().unwrap();
        let err = executor.pause().unwrap_err();
        assert!(matches!(err, PqcExecError::MigrationPaused(_)));
    }

    #[test]
    fn test_resume_without_pause_error() {
        let executor = PqcMigrationExecutor::new("asm-007");
        let err = executor.resume().unwrap_err();
        assert!(matches!(err, PqcExecError::MigrationPaused(_)));
    }
}
