//! 备份验证自动化模块（v4.1.0，`backup-verify` feature gate）
//!
//! 提供备份完整性校验、恢复演练、校验报告生成能力。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// 校验项类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckType {
    /// 文件存在性
    FileExists,
    /// SHA-256 完整性
    Integrity,
    /// 文件大小
    FileSize,
    /// 恢复演练
    RestoreDrill,
}

/// 校验结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    /// 通过
    Passed,
    /// 失败
    Failed,
    /// 跳过
    Skipped,
}

/// 单项校验结果
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// 校验项类型
    pub check_type: CheckType,
    /// 校验目标
    pub target: String,
    /// 状态
    pub status: CheckStatus,
    /// 耗时（毫秒）
    pub duration_ms: u64,
    /// 详情
    pub detail: String,
}

/// 备份校验报告
#[derive(Debug, Clone)]
pub struct VerifyReport {
    /// 备份标识
    pub backup_id: String,
    /// 校验时间戳
    pub timestamp: u64,
    /// 各项校验结果
    pub results: Vec<CheckResult>,
    /// 总校验项数
    pub total_checks: usize,
    /// 通过数
    pub passed: usize,
    /// 失败数
    pub failed: usize,
    /// 跳过数
    pub skipped: usize,
}

impl VerifyReport {
    /// 是否全部通过
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// 通过率
    pub fn pass_rate(&self) -> f64 {
        if self.total_checks == 0 {
            1.0
        } else {
            self.passed as f64 / self.total_checks as f64
        }
    }
}

/// 备份元数据
#[derive(Debug, Clone)]
pub struct BackupMetadata {
    /// 备份标识
    pub backup_id: String,
    /// 文件列表（文件名 → 期望大小）
    pub files: HashMap<String, u64>,
    /// SHA-256 校验和列表（文件名 → 期望哈希）
    pub checksums: HashMap<String, String>,
    /// 创建时间
    pub created_at: u64,
}

/// 备份验证器
pub struct BackupVerifier;

impl BackupVerifier {
    /// 验证备份完整性
    pub fn verify(
        metadata: &BackupMetadata,
        actual_files: &HashMap<String, (u64, String)>,
    ) -> VerifyReport {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let mut results = Vec::new();

        for (filename, expected_size) in &metadata.files {
            let start = SystemTime::now();

            let (actual_size, actual_hash) = match actual_files.get(filename) {
                Some(data) => (data.0, data.1.clone()),
                None => {
                    let duration = start.elapsed().unwrap_or_default().as_millis() as u64;
                    results.push(CheckResult {
                        check_type: CheckType::FileExists,
                        target: filename.clone(),
                        status: CheckStatus::Failed,
                        duration_ms: duration,
                        detail: "文件不存在".to_string(),
                    });
                    continue;
                }
            };

            let duration = start.elapsed().unwrap_or_default().as_millis() as u64;

            results.push(CheckResult {
                check_type: CheckType::FileExists,
                target: filename.clone(),
                status: CheckStatus::Passed,
                duration_ms: duration,
                detail: "文件存在".to_string(),
            });

            results.push(CheckResult {
                check_type: CheckType::FileSize,
                target: filename.clone(),
                status: if actual_size == *expected_size {
                    CheckStatus::Passed
                } else {
                    CheckStatus::Failed
                },
                duration_ms: 0,
                detail: format!("期望 {} 字节，实际 {} 字节", expected_size, actual_size),
            });

            if let Some(expected_hash) = metadata.checksums.get(filename) {
                results.push(CheckResult {
                    check_type: CheckType::Integrity,
                    target: filename.clone(),
                    status: if &actual_hash == expected_hash {
                        CheckStatus::Passed
                    } else {
                        CheckStatus::Failed
                    },
                    duration_ms: 0,
                    detail: format!("期望 {}，实际 {}", expected_hash, actual_hash),
                });
            }
        }

        let total_checks = results.len();
        let passed = results
            .iter()
            .filter(|r| r.status == CheckStatus::Passed)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == CheckStatus::Failed)
            .count();
        let skipped = results
            .iter()
            .filter(|r| r.status == CheckStatus::Skipped)
            .count();

        VerifyReport {
            backup_id: metadata.backup_id.clone(),
            timestamp,
            results,
            total_checks,
            passed,
            failed,
            skipped,
        }
    }

    /// 执行恢复演练（模拟）
    pub fn restore_drill(metadata: &BackupMetadata) -> CheckResult {
        CheckResult {
            check_type: CheckType::RestoreDrill,
            target: metadata.backup_id.clone(),
            status: if metadata.files.is_empty() {
                CheckStatus::Failed
            } else {
                CheckStatus::Passed
            },
            duration_ms: 0,
            detail: format!("恢复演练完成，{} 个文件", metadata.files.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metadata() -> BackupMetadata {
        let mut files = HashMap::new();
        files.insert("backup.sql".to_string(), 1000);
        files.insert("schema.sql".to_string(), 500);

        let mut checksums = HashMap::new();
        checksums.insert("backup.sql".to_string(), "abc123".to_string());
        checksums.insert("schema.sql".to_string(), "def456".to_string());

        BackupMetadata {
            backup_id: "backup-001".to_string(),
            files,
            checksums,
            created_at: 0,
        }
    }

    #[test]
    fn test_verify_all_passed() {
        let metadata = make_metadata();
        let mut actual = HashMap::new();
        actual.insert("backup.sql".to_string(), (1000, "abc123".to_string()));
        actual.insert("schema.sql".to_string(), (500, "def456".to_string()));

        let report = BackupVerifier::verify(&metadata, &actual);
        assert!(report.all_passed());
        assert_eq!(report.failed, 0);
        assert_eq!(report.pass_rate(), 1.0);
    }

    #[test]
    fn test_verify_missing_file() {
        let metadata = make_metadata();
        let mut actual = HashMap::new();
        actual.insert("backup.sql".to_string(), (1000, "abc123".to_string()));

        let report = BackupVerifier::verify(&metadata, &actual);
        assert!(!report.all_passed());
        assert!(report.failed > 0);
    }

    #[test]
    fn test_verify_size_mismatch() {
        let metadata = make_metadata();
        let mut actual = HashMap::new();
        actual.insert("backup.sql".to_string(), (999, "abc123".to_string()));
        actual.insert("schema.sql".to_string(), (500, "def456".to_string()));

        let report = BackupVerifier::verify(&metadata, &actual);
        assert!(!report.all_passed());
        assert!(report
            .results
            .iter()
            .any(|r| { r.check_type == CheckType::FileSize && r.status == CheckStatus::Failed }));
    }

    #[test]
    fn test_verify_checksum_mismatch() {
        let metadata = make_metadata();
        let mut actual = HashMap::new();
        actual.insert("backup.sql".to_string(), (1000, "wrong".to_string()));
        actual.insert("schema.sql".to_string(), (500, "def456".to_string()));

        let report = BackupVerifier::verify(&metadata, &actual);
        assert!(!report.all_passed());
        assert!(report
            .results
            .iter()
            .any(|r| { r.check_type == CheckType::Integrity && r.status == CheckStatus::Failed }));
    }

    #[test]
    fn test_verify_empty_backup() {
        let metadata = BackupMetadata {
            backup_id: "empty".to_string(),
            files: HashMap::new(),
            checksums: HashMap::new(),
            created_at: 0,
        };
        let actual = HashMap::new();
        let report = BackupVerifier::verify(&metadata, &actual);
        assert!(report.all_passed());
        assert_eq!(report.total_checks, 0);
    }

    #[test]
    fn test_restore_drill_success() {
        let metadata = make_metadata();
        let result = BackupVerifier::restore_drill(&metadata);
        assert_eq!(result.status, CheckStatus::Passed);
        assert!(result.detail.contains("2 个文件"));
    }

    #[test]
    fn test_restore_drill_empty() {
        let metadata = BackupMetadata {
            backup_id: "empty".to_string(),
            files: HashMap::new(),
            checksums: HashMap::new(),
            created_at: 0,
        };
        let result = BackupVerifier::restore_drill(&metadata);
        assert_eq!(result.status, CheckStatus::Failed);
    }

    #[test]
    fn test_report_pass_rate() {
        let report = VerifyReport {
            backup_id: "test".to_string(),
            timestamp: 0,
            results: Vec::new(),
            total_checks: 10,
            passed: 8,
            failed: 2,
            skipped: 0,
        };
        assert_eq!(report.pass_rate(), 0.8);
    }
}
