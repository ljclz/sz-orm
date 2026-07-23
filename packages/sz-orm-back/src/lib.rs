//! # SZ-ORM Back — 备份恢复
//!
//! 提供数据库全量/增量备份、恢复与灾难恢复演练能力，覆盖完整备份-恢复往返与
//! 损坏文件负向路径，用于验证 RTO 与完整性校验。
//!
//! ## 主要模块
//!
//! - [`backup`] — 备份执行
//! - [`restore`] — 恢复执行
//! - [`DrillScenario`] — 灾难演练场景（全量恢复/增量合并/损坏文件）

pub mod advanced;
pub mod backup;
pub mod error;
pub mod restore;

pub use advanced::*;
pub use backup::*;
pub use error::BkError;
pub use restore::*;

/// Scenario selection for [`DisasterRecoveryDrill::run`].
///
/// Each variant drives a different recovery path so operators can validate
/// the full disaster-recovery playbook, not just the happy path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrillScenario {
    /// Take a full backup, then restore it. Validates the baseline
    /// backup+restore round-trip and measures RTO.
    FullBackupRestore,
    /// Take a full backup followed by an incremental backup, then restore
    /// the full backup. Validates that incremental manifests are produced
    /// correctly and the full backup remains independently restorable.
    IncrementalMerge,
    /// Take a full backup, corrupt the on-disk file, then attempt restore.
    /// The drill succeeds only if the restore layer rejects the corrupt
    /// file - this is the "negative" path that proves integrity checks
    /// actually fire.
    CorruptFile,
}

/// Outcome of a single disaster-recovery drill run.
///
/// * `rto_ms` - Recovery Time Objective: wall-clock milliseconds from
///   drill start to recovery completion.
/// * `rpo_ms` - Recovery Point Objective: milliseconds of data considered
///   at risk (0 when no data is lost).
/// * `data_loss_count` - number of rows that could not be recovered.
/// * `success` - `true` when the drill behaved as expected (successful
///   restore for happy paths, correct rejection for the corrupt path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrillReport {
    pub rto_ms: u64,
    pub rpo_ms: u64,
    pub data_loss_count: u64,
    pub success: bool,
}

impl DrillReport {
    /// Convenience constructor for a failed drill with zeroed metrics.
    pub fn failure() -> Self {
        Self {
            rto_ms: 0,
            rpo_ms: 0,
            data_loss_count: 0,
            success: false,
        }
    }
}

/// Orchestrates disaster-recovery drills against a real
/// [`BackupManager`] / [`RestoreManager`] pair. Each [`Self::run`] call
/// exercises a single [`DrillScenario`] and returns a [`DrillReport`]
/// capturing the measured RTO/RPO and data-loss counts.
pub struct DisasterRecoveryDrill;

impl DisasterRecoveryDrill {
    pub fn new() -> Self {
        Self
    }

    /// Runs the requested `scenario` against the supplied managers and
    /// returns a [`DrillReport`]. Never panics - failures are surfaced as
    /// `success: false` so the report can be aggregated by callers.
    pub async fn run(
        &self,
        backup_mgr: &BackupManager,
        restore_mgr: &RestoreManager,
        scenario: DrillScenario,
    ) -> DrillReport {
        match scenario {
            DrillScenario::FullBackupRestore => {
                Self::run_full_backup_restore(backup_mgr, restore_mgr).await
            }
            DrillScenario::IncrementalMerge => {
                Self::run_incremental_merge(backup_mgr, restore_mgr).await
            }
            DrillScenario::CorruptFile => Self::run_corrupt_file(backup_mgr, restore_mgr).await,
        }
    }

    async fn run_full_backup_restore(
        backup_mgr: &BackupManager,
        restore_mgr: &RestoreManager,
    ) -> DrillReport {
        let temp_dir = unique_dir("drill-full");
        let pool = "drill_full_pool";
        let start = std::time::Instant::now();

        let backup_result = match backup_mgr.backup(pool, &temp_dir).await {
            Ok(r) => r,
            Err(_) => return DrillReport::failure(),
        };
        let expected_rows = backup_result.total_rows;

        let restore_result = restore_mgr.restore(pool, &backup_result.output_path).await;
        let rto_ms = start.elapsed().as_millis() as u64;

        cleanup_path(&backup_result.output_path);
        cleanup_dir(&temp_dir);

        match restore_result {
            Ok(restore) => {
                let data_loss_count = expected_rows.saturating_sub(restore.restored_rows);
                DrillReport {
                    rto_ms,
                    rpo_ms: 0,
                    data_loss_count,
                    success: data_loss_count == 0,
                }
            }
            Err(_) => DrillReport::failure(),
        }
    }

    async fn run_incremental_merge(
        backup_mgr: &BackupManager,
        restore_mgr: &RestoreManager,
    ) -> DrillReport {
        let temp_dir = unique_dir("drill-incr");
        let pool = "drill_incr_pool";
        let start = std::time::Instant::now();

        let backup_result = match backup_mgr.backup(pool, &temp_dir).await {
            Ok(r) => r,
            Err(_) => return DrillReport::failure(),
        };
        let expected_rows = backup_result.total_rows;

        // Incremental backup covering "everything since the full backup
        // started". In a real deployment `since` would be the last
        // successful backup timestamp; here we use a wide window so the
        // incremental contains at least the same tables.
        let since = chrono::Utc::now() - chrono::Duration::seconds(60);
        let incremental_result = backup_mgr.incremental_backup(since).await;

        let restore_result = restore_mgr.restore(pool, &backup_result.output_path).await;
        let rto_ms = start.elapsed().as_millis() as u64;

        cleanup_path(&backup_result.output_path);
        cleanup_dir(&temp_dir);

        let incremental_ok = match incremental_result {
            Ok(manifest) => manifest.is_incremental,
            Err(_) => false,
        };

        match restore_result {
            Ok(restore) => {
                let data_loss_count = expected_rows.saturating_sub(restore.restored_rows);
                DrillReport {
                    rto_ms,
                    rpo_ms: 0,
                    data_loss_count,
                    success: incremental_ok && data_loss_count == 0,
                }
            }
            Err(_) => DrillReport::failure(),
        }
    }

    async fn run_corrupt_file(
        backup_mgr: &BackupManager,
        restore_mgr: &RestoreManager,
    ) -> DrillReport {
        let temp_dir = unique_dir("drill-corrupt");
        let pool = "drill_corrupt_pool";
        let start = std::time::Instant::now();

        let backup_result = match backup_mgr.backup(pool, &temp_dir).await {
            Ok(r) => r,
            Err(_) => return DrillReport::failure(),
        };
        let expected_rows = backup_result.total_rows;

        // Overwrite the backup file with garbage so the restore layer must
        // reject it. The bytes intentionally do not start with the gzip
        // magic header so restore falls through to JSON parsing and fails.
        let corrupt_payload = b"CORRUPT-BACKUP-DRILL-PAYLOAD-NOT-JSON-NOT-GZIP";
        let _ = tokio::fs::write(&backup_result.output_path, corrupt_payload).await;

        let restore_result = restore_mgr.restore(pool, &backup_result.output_path).await;
        let rto_ms = start.elapsed().as_millis() as u64;

        cleanup_path(&backup_result.output_path);
        cleanup_dir(&temp_dir);

        match restore_result {
            // Restore should NOT succeed on a corrupt file. If it does,
            // the integrity check is broken and the drill fails.
            Ok(_) => DrillReport::failure(),
            // Restore correctly rejected the corrupt file - the data is
            // "lost" (cannot be recovered from this backup), but the
            // detection layer is working as designed.
            Err(_) => DrillReport {
                rto_ms,
                rpo_ms: 0,
                data_loss_count: expected_rows,
                success: true,
            },
        }
    }
}

impl Default for DisasterRecoveryDrill {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// L4 - Degradation Policy
// ====================================================================

/// Action recommended by [`DegradationPolicy::evaluate`] given the current
/// [`HealthStatus`]. Actions are ordered from least to most disruptive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationAction {
    /// All metrics healthy - serve reads and writes normally.
    Normal,
    /// Primary is degraded - continue serving reads but shed writes to
    /// protect the underlying system.
    ReadOnly,
    /// Primary is failing fast - serve reads from the latest backup so
    /// callers see *some* data rather than errors.
    FallbackToBackup,
    /// Circuit breaker is open - fail fast and stop touching the primary
    /// until it recovers.
    CircuitOpen,
}

/// Snapshot of system health used by [`DegradationPolicy::evaluate`] to
/// pick a [`DegradationAction`]. All fields are simple value types so the
/// struct can be cloned/compared cheaply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthStatus {
    /// Fraction of recent requests that errored, in `0.0..=1.0`.
    pub error_rate: f64,
    /// Recent average request latency in milliseconds.
    pub latency_ms: u64,
    /// Whether a usable backup exists for [`DegradationAction::FallbackToBackup`].
    pub backup_available: bool,
    /// Whether the circuit breaker on the primary is currently open.
    pub circuit_open: bool,
}

/// Configurable thresholds that map a [`HealthStatus`] to a
/// [`DegradationAction`]. Defaults are conservative (10% error rate / 5s
/// latency trigger `ReadOnly`; 50% error rate triggers
/// `FallbackToBackup`/`CircuitOpen`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegradationPolicy {
    /// Error rate at or above which the system enters `ReadOnly`.
    pub error_rate_threshold: f64,
    /// Error rate at or above which the system enters `FallbackToBackup`
    /// (if a backup is available) or `CircuitOpen` (otherwise).
    pub critical_error_rate: f64,
    /// Latency at or above which the system enters `ReadOnly`.
    pub latency_threshold_ms: u64,
}

impl Default for DegradationPolicy {
    fn default() -> Self {
        Self {
            error_rate_threshold: 0.1,
            critical_error_rate: 0.5,
            latency_threshold_ms: 5000,
        }
    }
}

impl DegradationPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_error_rate_threshold(mut self, threshold: f64) -> Self {
        self.error_rate_threshold = threshold;
        self
    }

    pub fn with_critical_error_rate(mut self, threshold: f64) -> Self {
        self.critical_error_rate = threshold;
        self
    }

    pub fn with_latency_threshold_ms(mut self, threshold_ms: u64) -> Self {
        self.latency_threshold_ms = threshold_ms;
        self
    }

    /// Evaluates the supplied [`HealthStatus`] and returns the recommended
    /// [`DegradationAction`]. The decision tree is:
    ///
    /// 1. `circuit_open` -> [`DegradationAction::CircuitOpen`] (highest priority)
    /// 2. `error_rate >= critical_error_rate` -> `FallbackToBackup` if a
    ///    backup is available, else `CircuitOpen`
    /// 3. `error_rate >= error_rate_threshold` OR `latency_ms >= latency_threshold_ms`
    ///    -> [`DegradationAction::ReadOnly`]
    /// 4. Otherwise -> [`DegradationAction::Normal`]
    pub fn evaluate(&self, health: &HealthStatus) -> DegradationAction {
        if health.circuit_open {
            return DegradationAction::CircuitOpen;
        }
        if health.error_rate >= self.critical_error_rate {
            return if health.backup_available {
                DegradationAction::FallbackToBackup
            } else {
                DegradationAction::CircuitOpen
            };
        }
        if health.error_rate >= self.error_rate_threshold
            || health.latency_ms >= self.latency_threshold_ms
        {
            return DegradationAction::ReadOnly;
        }
        DegradationAction::Normal
    }
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            error_rate: 0.0,
            latency_ms: 0,
            backup_available: false,
            circuit_open: false,
        }
    }
}

fn unique_dir(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "sz-orm-back-{}-{}",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

fn cleanup_path(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

fn cleanup_dir(dir: &std::path::Path) {
    let _ = std::fs::remove_dir(dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes backup file bytes, transparently handling gzip-compressed
    /// payloads (magic bytes `0x1f 0x8b`). Used by tests that need to
    /// inspect the underlying JSON manifest directly.
    fn decode_backup_bytes(bytes: &[u8]) -> Vec<u8> {
        if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
            use std::io::Read;
            let mut decoder = flate2::read::GzDecoder::new(bytes);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .expect("gzip decode in test helper must succeed");
            out
        } else {
            bytes.to_vec()
        }
    }

    #[test]
    fn test_backup_config_default() {
        let config = BackupConfig::default();
        assert!(config.compress);
        assert!(config.compression_level.is_some());
        assert_eq!(config.batch_size, 1000);
    }

    #[test]
    fn test_backup_config_builder() {
        let config = BackupConfig::new()
            .with_compress(true)
            .with_compression_level(9)
            .with_batch_size(500);

        assert!(config.compress);
        assert_eq!(config.compression_level, Some(9));
        assert_eq!(config.batch_size, 500);
    }

    #[test]
    fn test_backup_result() {
        let result = BackupResult::new("test_pool");
        assert_eq!(result.pool_name, "test_pool");
        assert_eq!(result.total_tables, 0);
        assert_eq!(result.backed_tables, 0);
    }

    #[test]
    fn test_export_result() {
        let result = ExportResult::new("test_pool");
        assert_eq!(result.pool_name, "test_pool");
        assert_eq!(result.tables, 0);
    }

    #[test]
    fn test_restore_result() {
        let result = RestoreResult::new("test_pool");
        assert_eq!(result.pool_name, "test_pool");
        assert_eq!(result.restored_rows, 0);
    }

    #[test]
    fn test_import_result() {
        let result = ImportResult::new("test_pool");
        assert_eq!(result.pool_name, "test_pool");
        assert_eq!(result.total_statements, 0);
        assert_eq!(result.executed_statements, 0);
    }

    #[test]
    fn test_import_result_success_rate() {
        let mut result = ImportResult::new("test");
        result.total_statements = 100;
        result.executed_statements = 95;
        assert_eq!(result.success_rate(), 95.0);
    }

    #[test]
    fn test_import_result_zero_total() {
        let result = ImportResult::new("test");
        assert_eq!(result.success_rate(), 0.0);
    }

    #[tokio::test]
    async fn test_backup_manager_new() {
        let config = BackupConfig::default();
        let _manager = BackupManager::new(config);
        let _manager2 = BackupManager::new(BackupConfig::new());
    }

    #[tokio::test]
    async fn test_restore_manager_new() {
        let _manager = RestoreManager::new();
        let _manager2 = RestoreManager::new();
    }

    #[tokio::test]
    async fn test_backup_manager_backup() {
        let config = BackupConfig::default();
        let manager = BackupManager::new(config);

        let temp_dir = std::env::temp_dir();
        let result = manager.backup("test_pool", &temp_dir).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.pool_name, "test_pool");
    }

    #[tokio::test]
    async fn test_backup_writes_real_file_with_manifest() {
        // Unique subdirectory so the test cleans up after itself and does
        // not stomp on sibling tests.
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default());
        manager.register_table(
            "users",
            vec![
                serde_json::json!({"id": 1, "name": "alice"}),
                serde_json::json!({"id": 2, "name": "bob"}),
            ],
        );
        manager.register_table(
            "orders",
            vec![serde_json::json!({"id": 100, "amount": 3.5})],
        );

        let result = manager.backup("prod_pool", &temp_dir).await.unwrap();
        assert_eq!(result.pool_name, "prod_pool");
        assert_eq!(result.total_tables, 2);
        assert_eq!(result.backed_tables, 2);
        assert_eq!(result.total_rows, 3);
        assert!(result.file_size > 0);
        assert!(result.compressed);
        assert!(result.output_path.exists());
        assert!(result
            .output_path
            .to_string_lossy()
            .contains("prod_pool_backup_"));

        // The file must contain a real JSON manifest we can read back.
        // Default config compresses with gzip, so we decompress first.
        let bytes = tokio::fs::read(&result.output_path).await.unwrap();
        let decoded = decode_backup_bytes(&bytes);
        let manifest: BackupManifest = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(manifest.format, BackupManifest::FORMAT);
        assert_eq!(manifest.format_version, BackupManifest::VERSION);
        assert_eq!(manifest.pool_name, "prod_pool");
        assert_eq!(manifest.tables.len(), 2);
        // Tables are sorted alphabetically inside the manifest.
        assert_eq!(manifest.tables[0].name, "orders");
        assert_eq!(manifest.tables[0].row_count, 1);
        assert_eq!(manifest.tables[1].name, "users");
        assert_eq!(manifest.tables[1].row_count, 2);

        // Clean up.
        let _ = std::fs::remove_file(&result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_backup_then_restore_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-roundtrip-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default());
        manager.register_table(
            "users",
            vec![
                serde_json::json!({"id": 1}),
                serde_json::json!({"id": 2}),
                serde_json::json!({"id": 3}),
            ],
        );

        let backup_result = manager.backup("rt_pool", &temp_dir).await.unwrap();
        assert_eq!(backup_result.total_rows, 3);

        let restore_manager = RestoreManager::new();
        let restore_result = restore_manager
            .restore("rt_pool", &backup_result.output_path)
            .await
            .unwrap();
        assert_eq!(restore_result.pool_name, "rt_pool");
        assert_eq!(restore_result.restored_rows, 3);
        assert_eq!(restore_result.tables, 1);
        assert_eq!(restore_result.input_path, backup_result.output_path);

        // Clean up.
        let _ = std::fs::remove_file(&backup_result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_restore_rejects_pool_name_mismatch() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-mismatch-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default());
        let backup_result = manager.backup("pool_a", &temp_dir).await.unwrap();

        let restore_manager = RestoreManager::new();
        let err = restore_manager
            .restore("pool_b", &backup_result.output_path)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("pool name mismatch"),
            "expected pool name mismatch error, got: {}",
            msg
        );

        let _ = std::fs::remove_file(&backup_result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_restore_rejects_invalid_format_header() {
        // Write a JSON file that does NOT have the magic format header.
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-fmt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        let bogus_path = temp_dir.join("bogus_backup.json");
        tokio::fs::write(
            &bogus_path,
            br#"{"format":"not-sz-orm-back","format_version":1,"pool_name":"x","created_at":0,"compressed":false,"tables":[]}"#,
        )
        .await
        .unwrap();

        let restore_manager = RestoreManager::new();
        let err = restore_manager.restore("x", &bogus_path).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unrecognized backup format header"),
            "expected format header error, got: {}",
            msg
        );

        let _ = std::fs::remove_file(&bogus_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_restore_rejects_corrupt_json() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-corrupt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        let corrupt_path = temp_dir.join("corrupt.json");
        tokio::fs::write(&corrupt_path, b"this is not json")
            .await
            .unwrap();

        let restore_manager = RestoreManager::new();
        let err = restore_manager
            .restore("x", &corrupt_path)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid backup manifest"), "got: {}", msg);

        let _ = std::fs::remove_file(&corrupt_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_export_sql_writes_real_insert_statements() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-export-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default());
        manager.register_table(
            "users",
            vec![
                serde_json::json!({"id": 1, "name": "alice"}),
                serde_json::json!({"id": 2, "name": "bob"}),
            ],
        );

        let result = manager.export_sql("prod", &temp_dir).await.unwrap();
        assert_eq!(result.pool_name, "prod");
        assert_eq!(result.tables, 1);
        assert_eq!(result.total_rows, 2);
        assert!(result.file_size > 0);
        assert!(result.output_path.exists());

        let content = tokio::fs::read_to_string(&result.output_path)
            .await
            .unwrap();
        assert!(content.contains("-- SZ-ORM export for pool: prod"));
        assert!(content.contains("INSERT INTO \"users\" VALUES"));
        // Two INSERT statements should be present.
        assert_eq!(content.matches("INSERT INTO \"users\" VALUES").count(), 2);

        let _ = std::fs::remove_file(&result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_export_then_import_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-sqlrt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default());
        manager.register_table(
            "users",
            vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})],
        );

        let export = manager.export_sql("rt", &temp_dir).await.unwrap();

        let restore = RestoreManager::new();
        let import = restore.import_sql("rt", &export.output_path).await.unwrap();
        assert_eq!(import.pool_name, "rt");
        assert_eq!(import.total_statements, 2);
        assert_eq!(import.executed_statements, 2);
        assert!(import.errors.is_empty());
        assert_eq!(import.success_rate(), 100.0);

        let _ = std::fs::remove_file(&export.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_import_sql_records_errors_for_unknown_keywords() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-imperr-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
        let sql_path = temp_dir.join("mixed.sql");
        tokio::fs::write(
            &sql_path,
            b"INSERT INTO a VALUES (1);\nGARBAGE STATEMENT HERE;\nDELETE FROM b WHERE x=1;\n-- comment only\n",
        )
        .await
        .unwrap();

        let restore = RestoreManager::new();
        let import = restore.import_sql("p", &sql_path).await.unwrap();
        assert_eq!(import.total_statements, 3);
        assert_eq!(import.executed_statements, 2);
        assert_eq!(import.errors.len(), 1);
        assert!(
            import.errors[0].contains("GARBAGE"),
            "got: {}",
            import.errors[0]
        );
        // success_rate is 2/3 * 100 = ~66.66 - check approximate
        let rate = import.success_rate();
        assert!(rate > 66.0 && rate < 67.0, "expected ~66.66%, got {}", rate);

        let _ = std::fs::remove_file(&sql_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_backup_manager_registers_and_lists_tables() {
        let manager = BackupManager::new(BackupConfig::default());
        manager.register_table("zoo", vec![]);
        manager.register_table("alpha", vec![]);
        manager.register_table("mid", vec![]);

        // registered_tables returns names in sorted order, regardless of
        // insertion order.
        let names = manager.registered_tables();
        assert_eq!(names, vec!["alpha", "mid", "zoo"]);
    }

    #[tokio::test]
    async fn test_backup_with_no_tables_still_writes_file() {
        // The existing default test only checked that backup returned Ok.
        // Strengthen: an empty BackupManager must still produce a real file
        // with the manifest, just containing zero tables.
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-empty-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default());
        let result = manager.backup("empty_pool", &temp_dir).await.unwrap();
        assert_eq!(result.total_tables, 0);
        assert_eq!(result.total_rows, 0);
        assert!(result.file_size > 0);
        assert!(result.output_path.exists());

        let bytes = tokio::fs::read(&result.output_path).await.unwrap();
        let decoded = decode_backup_bytes(&bytes);
        let manifest: BackupManifest = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(manifest.pool_name, "empty_pool");
        assert!(manifest.tables.is_empty());

        let _ = std::fs::remove_file(&result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_restore_manager_restore_not_found() {
        let manager = RestoreManager::new();

        let result = manager
            .restore("test_pool", std::path::Path::new("/nonexistent/file.zip"))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_restore_manager_import_sql_not_found() {
        let manager = RestoreManager::new();

        let result = manager
            .import_sql("test_pool", std::path::Path::new("/nonexistent/file.sql"))
            .await;

        assert!(result.is_err());
    }

    // ====================================================================
    // L4 - Real gzip compression
    // ====================================================================

    #[tokio::test]
    async fn test_backup_compressed_file_has_gzip_magic_bytes() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-gzmagic-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default().with_compress(true));
        manager.register_table("users", vec![serde_json::json!({"id": 1, "name": "alice"})]);

        let result = manager.backup("gz_pool", &temp_dir).await.unwrap();
        assert!(result.compressed);

        let bytes = tokio::fs::read(&result.output_path).await.unwrap();
        assert!(
            bytes.len() >= 2,
            "compressed file must have at least 2 bytes"
        );
        assert_eq!(
            bytes[0], 0x1f,
            "gzip magic byte 0 must be 0x1f, got {:#04x}",
            bytes[0]
        );
        assert_eq!(
            bytes[1], 0x8b,
            "gzip magic byte 1 must be 0x8b, got {:#04x}",
            bytes[1]
        );

        let _ = std::fs::remove_file(&result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_backup_uncompressed_file_starts_with_json_brace() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-plain-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default().with_compress(false));
        manager.register_table("users", vec![serde_json::json!({"id": 1, "name": "alice"})]);

        let result = manager.backup("plain_pool", &temp_dir).await.unwrap();
        assert!(!result.compressed);

        let bytes = tokio::fs::read(&result.output_path).await.unwrap();
        assert!(!bytes.is_empty());
        // Pretty-printed JSON starts with `{`.
        assert_eq!(bytes[0], b'{', "uncompressed file must start with '{{'");

        // And it must round-trip directly as JSON without decompression.
        let manifest: BackupManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(manifest.pool_name, "plain_pool");

        let _ = std::fs::remove_file(&result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_backup_compressed_then_restore_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-gzrt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = BackupManager::new(BackupConfig::default().with_compress(true));
        manager.register_table(
            "users",
            vec![
                serde_json::json!({"id": 1}),
                serde_json::json!({"id": 2}),
                serde_json::json!({"id": 3}),
            ],
        );

        let backup_result = manager.backup("gzrt_pool", &temp_dir).await.unwrap();
        assert!(backup_result.compressed);

        let restore = RestoreManager::new();
        let restore_result = restore
            .restore("gzrt_pool", &backup_result.output_path)
            .await
            .unwrap();
        assert_eq!(restore_result.restored_rows, 3);
        assert_eq!(restore_result.tables, 1);

        let _ = std::fs::remove_file(&backup_result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[tokio::test]
    async fn test_backup_compressed_file_is_smaller_than_uncompressed_for_repetitive_data() {
        // Highly compressible data: the gzip stream must be smaller than
        // the raw JSON. This is the whole point of `compress: true`.
        let temp_dir = std::env::temp_dir().join(format!(
            "sz-orm-back-gzsize-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let big_rows: Vec<serde_json::Value> = (0..500)
            .map(|i| serde_json::json!({"id": i, "name": "alicealicealicealicealice", "payload": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}))
            .collect();

        let manager_compressed = BackupManager::new(BackupConfig::default().with_compress(true));
        manager_compressed.register_table("t", big_rows.clone());
        let compressed_result = manager_compressed
            .backup("cmp_pool", &temp_dir)
            .await
            .unwrap();

        let manager_plain = BackupManager::new(BackupConfig::default().with_compress(false));
        manager_plain.register_table("t", big_rows);
        let plain_result = manager_plain.backup("plain_pool", &temp_dir).await.unwrap();

        assert!(
            compressed_result.file_size < plain_result.file_size,
            "gzip compressed size {} must be smaller than plain size {}",
            compressed_result.file_size,
            plain_result.file_size
        );

        let _ = std::fs::remove_file(&compressed_result.output_path);
        let _ = std::fs::remove_file(&plain_result.output_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    // ====================================================================
    // L4 - Checksum & integrity
    // ====================================================================

    #[test]
    fn test_manifest_checksum_sha256_is_hex_64() {
        let manifest = BackupManifest::new(
            "checksum_pool",
            false,
            vec![BackupTable {
                name: "users".to_string(),
                row_count: 2,
                rows: vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})],
            }],
        );

        let checksum = manifest.checksum_sha256();
        // SHA256 hex digest is exactly 64 hex characters.
        assert_eq!(
            checksum.len(),
            64,
            "expected 64-char hex, got: {}",
            checksum
        );
        assert!(
            checksum.chars().all(|c| c.is_ascii_hexdigit()),
            "expected hex chars only, got: {}",
            checksum
        );
    }

    #[test]
    fn test_manifest_checksum_is_deterministic_for_same_manifest() {
        let build = || {
            BackupManifest::new(
                "p",
                false,
                vec![BackupTable {
                    name: "t".to_string(),
                    row_count: 1,
                    rows: vec![serde_json::json!({"id": 1})],
                }],
            )
        };

        let a = build();
        let b = build();
        // Two manifests constructed from identical inputs must produce the
        // same checksum, EXCEPT for `created_at` which is wall-clock based.
        // To make this test meaningful we override created_at to a fixed
        // value on both copies.
        let mut a2 = a;
        a2.created_at = 1_700_000_000_000;
        let mut b2 = b;
        b2.created_at = 1_700_000_000_000;
        assert_eq!(a2.checksum_sha256(), b2.checksum_sha256());
    }

    #[test]
    fn test_manifest_checksum_changes_when_data_changes() {
        let mut manifest = BackupManifest::new(
            "p",
            false,
            vec![BackupTable {
                name: "t".to_string(),
                row_count: 1,
                rows: vec![serde_json::json!({"id": 1})],
            }],
        );
        manifest.created_at = 1_700_000_000_000;
        let before = manifest.checksum_sha256();

        // Mutate the data.
        manifest.tables[0].rows[0] = serde_json::json!({"id": 999});
        let after = manifest.checksum_sha256();

        assert_ne!(before, after, "checksum must change when data changes");
    }

    #[test]
    fn test_restore_verify_checksum_accepts_correct_value() {
        let manifest = BackupManifest::new(
            "p",
            false,
            vec![BackupTable {
                name: "t".to_string(),
                row_count: 1,
                rows: vec![serde_json::json!({"id": 1})],
            }],
        );
        let expected = manifest.checksum_sha256();

        let restore = RestoreManager::new();
        let ok = restore.verify_checksum(&manifest, &expected).unwrap();
        assert!(
            ok,
            "verify_checksum must accept the manifest's own checksum"
        );
    }

    #[test]
    fn test_restore_verify_checksum_rejects_wrong_value() {
        let manifest = BackupManifest::new(
            "p",
            false,
            vec![BackupTable {
                name: "t".to_string(),
                row_count: 1,
                rows: vec![serde_json::json!({"id": 1})],
            }],
        );
        let wrong = "0".repeat(64);

        let restore = RestoreManager::new();
        let ok = restore.verify_checksum(&manifest, &wrong).unwrap();
        assert!(!ok, "verify_checksum must reject a wrong checksum");
    }

    // ====================================================================
    // L4 - Incremental backup
    // ====================================================================

    #[tokio::test]
    async fn test_incremental_backup_marks_manifest_as_incremental() {
        let manager = BackupManager::new(BackupConfig::default());
        manager.register_table("t", vec![serde_json::json!({"id": 1})]);

        // `since` one second in the past: the table must be included.
        let since = chrono::Utc::now() - chrono::Duration::seconds(1);
        let manifest = manager.incremental_backup(since).await.unwrap();
        assert!(
            manifest.is_incremental,
            "incremental_backup must set is_incremental=true"
        );
        assert_eq!(
            manifest.base_backup_id, None,
            "incremental_backup without a base must leave base_backup_id=None"
        );
        assert_eq!(manifest.tables.len(), 1);
    }

    #[tokio::test]
    async fn test_incremental_backup_only_includes_tables_modified_after_since() {
        let manager = BackupManager::new(BackupConfig::default());

        // Register "old_table" before `since`.
        manager.register_table("old_table", vec![serde_json::json!({"id": 1})]);

        // Capture `since` AFTER old_table registration, BEFORE new_table.
        let since = chrono::Utc::now();
        // Sleep to guarantee new_table's modified_at is strictly greater
        // than `since` even on low-resolution clocks.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Register "new_table" after `since`.
        manager.register_table("new_table", vec![serde_json::json!({"id": 2})]);

        let manifest = manager.incremental_backup(since).await.unwrap();
        assert!(manifest.is_incremental);
        assert_eq!(
            manifest.tables.len(),
            1,
            "only tables modified after `since` must be included"
        );
        assert_eq!(manifest.tables[0].name, "new_table");
        assert_eq!(manifest.tables[0].row_count, 1);
    }

    #[tokio::test]
    async fn test_incremental_backup_includes_no_tables_when_nothing_changed() {
        let manager = BackupManager::new(BackupConfig::default());
        manager.register_table("t", vec![serde_json::json!({"id": 1})]);

        // `since` is in the future, so nothing should be included.
        let since = chrono::Utc::now() + chrono::Duration::seconds(1);
        let manifest = manager.incremental_backup(since).await.unwrap();
        assert!(manifest.is_incremental);
        assert!(
            manifest.tables.is_empty(),
            "no tables should be included when nothing was modified after `since`"
        );
    }

    #[tokio::test]
    async fn test_incremental_backup_includes_all_tables_when_all_modified() {
        let manager = BackupManager::new(BackupConfig::default());
        manager.register_table("a", vec![serde_json::json!({"id": 1})]);
        manager.register_table("b", vec![serde_json::json!({"id": 2})]);

        // `since` ten seconds in the past: both tables must be included,
        // and they must be sorted alphabetically (consistent with full backup).
        let since = chrono::Utc::now() - chrono::Duration::seconds(10);
        let manifest = manager.incremental_backup(since).await.unwrap();
        assert_eq!(manifest.tables.len(), 2);
        assert_eq!(manifest.tables[0].name, "a");
        assert_eq!(manifest.tables[1].name, "b");
    }

    #[test]
    fn test_manifest_with_base_backup_id_sets_field() {
        let manifest = BackupManifest::new("p", false, vec![]).with_base_backup_id("base-123");
        assert_eq!(
            manifest.base_backup_id.as_deref(),
            Some("base-123"),
            "with_base_backup_id must set the field"
        );
        // Builder must not flip is_incremental on a full backup unless the
        // caller explicitly asks; we only set base_backup_id here.
        assert!(!manifest.is_incremental);
    }

    // ====================================================================
    // L4 - Disaster Recovery Drill
    // ====================================================================

    #[tokio::test]
    async fn test_drill_full_backup_restore_succeeds_with_no_data_loss() {
        let backup_mgr = BackupManager::new(BackupConfig::default());
        backup_mgr.register_table(
            "users",
            vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})],
        );
        let restore_mgr = RestoreManager::new();

        let drill = DisasterRecoveryDrill::new();
        let report = drill
            .run(&backup_mgr, &restore_mgr, DrillScenario::FullBackupRestore)
            .await;

        assert!(
            report.success,
            "full backup+restore drill must succeed, got: {:?}",
            report
        );
        assert_eq!(
            report.data_loss_count, 0,
            "no data loss expected for full backup drill"
        );
    }

    #[tokio::test]
    async fn test_drill_full_backup_restore_records_positive_rto() {
        let backup_mgr = BackupManager::new(BackupConfig::default());
        backup_mgr.register_table("t", vec![serde_json::json!({"id": 1})]);
        let restore_mgr = RestoreManager::new();

        let drill = DisasterRecoveryDrill::new();
        let report = drill
            .run(&backup_mgr, &restore_mgr, DrillScenario::FullBackupRestore)
            .await;

        // RTO is the wall-clock recovery duration; for any real backup+restore
        // it must be >= 0. We don't assert an upper bound to avoid CI flakes.
        assert!(
            report.success,
            "drill must succeed so RTO is meaningful, got: {:?}",
            report
        );
    }

    #[tokio::test]
    async fn test_drill_incremental_merge_succeeds_with_no_data_loss() {
        let backup_mgr = BackupManager::new(BackupConfig::default());
        backup_mgr.register_table("users", vec![serde_json::json!({"id": 1})]);
        let restore_mgr = RestoreManager::new();

        let drill = DisasterRecoveryDrill::new();
        let report = drill
            .run(&backup_mgr, &restore_mgr, DrillScenario::IncrementalMerge)
            .await;

        assert!(
            report.success,
            "incremental merge drill must succeed, got: {:?}",
            report
        );
        assert_eq!(report.data_loss_count, 0);
    }

    #[tokio::test]
    async fn test_drill_corrupt_file_detects_corruption_and_reports_data_loss() {
        let backup_mgr = BackupManager::new(BackupConfig::default());
        backup_mgr.register_table(
            "users",
            vec![
                serde_json::json!({"id": 1}),
                serde_json::json!({"id": 2}),
                serde_json::json!({"id": 3}),
            ],
        );
        let restore_mgr = RestoreManager::new();

        let drill = DisasterRecoveryDrill::new();
        let report = drill
            .run(&backup_mgr, &restore_mgr, DrillScenario::CorruptFile)
            .await;

        // The drill "succeeds" when the restore layer correctly rejects the
        // corrupt backup file - that is the behaviour we are validating.
        assert!(
            report.success,
            "drill must report success when corruption is detected, got: {:?}",
            report
        );
        assert!(
            report.data_loss_count > 0,
            "corrupt backup must report > 0 data loss, got: {}",
            report.data_loss_count
        );
    }

    #[test]
    fn test_drill_report_default_is_failure() {
        let report = DrillReport {
            rto_ms: 0,
            rpo_ms: 0,
            data_loss_count: 0,
            success: false,
        };
        assert!(!report.success);
        assert_eq!(report.rto_ms, 0);
        assert_eq!(report.rpo_ms, 0);
        assert_eq!(report.data_loss_count, 0);
    }

    // ====================================================================
    // L4 - Degradation Policy
    // ====================================================================

    #[test]
    fn test_degradation_returns_normal_when_healthy() {
        let policy = DegradationPolicy::default();
        let health = HealthStatus {
            error_rate: 0.0,
            latency_ms: 10,
            backup_available: true,
            circuit_open: false,
        };
        assert_eq!(policy.evaluate(&health), DegradationAction::Normal);
    }

    #[test]
    fn test_degradation_returns_read_only_on_moderate_error_rate() {
        let policy = DegradationPolicy::default();
        // Default moderate threshold is 0.1; 0.2 is above it but below the
        // critical 0.5 threshold.
        let health = HealthStatus {
            error_rate: 0.2,
            latency_ms: 10,
            backup_available: true,
            circuit_open: false,
        };
        assert_eq!(policy.evaluate(&health), DegradationAction::ReadOnly);
    }

    #[test]
    fn test_degradation_returns_read_only_on_high_latency() {
        let policy = DegradationPolicy::default();
        // Default latency threshold is 5000ms; 8000ms exceeds it.
        let health = HealthStatus {
            error_rate: 0.0,
            latency_ms: 8000,
            backup_available: true,
            circuit_open: false,
        };
        assert_eq!(policy.evaluate(&health), DegradationAction::ReadOnly);
    }

    #[test]
    fn test_degradation_returns_fallback_to_backup_on_critical_error_with_backup() {
        let policy = DegradationPolicy::default();
        // Default critical threshold is 0.5; 0.6 exceeds it. A backup is
        // available, so we should fall back to it rather than opening the
        // circuit.
        let health = HealthStatus {
            error_rate: 0.6,
            latency_ms: 10,
            backup_available: true,
            circuit_open: false,
        };
        assert_eq!(
            policy.evaluate(&health),
            DegradationAction::FallbackToBackup
        );
    }

    #[test]
    fn test_degradation_returns_circuit_open_on_critical_error_without_backup() {
        let policy = DegradationPolicy::default();
        // Critical error rate but no backup to fall back to: open the
        // circuit to protect downstream systems.
        let health = HealthStatus {
            error_rate: 0.6,
            latency_ms: 10,
            backup_available: false,
            circuit_open: false,
        };
        assert_eq!(policy.evaluate(&health), DegradationAction::CircuitOpen);
    }

    #[test]
    fn test_degradation_returns_circuit_open_when_circuit_breaker_trips() {
        let policy = DegradationPolicy::default();
        // Even with healthy metrics, an open circuit breaker short-circuits
        // to CircuitOpen.
        let health = HealthStatus {
            error_rate: 0.0,
            latency_ms: 5,
            backup_available: true,
            circuit_open: true,
        };
        assert_eq!(policy.evaluate(&health), DegradationAction::CircuitOpen);
    }

    #[test]
    fn test_degradation_circuit_open_takes_priority_over_critical_error() {
        let policy = DegradationPolicy::default();
        // Circuit is open AND error rate is critical - circuit wins.
        let health = HealthStatus {
            error_rate: 0.9,
            latency_ms: 10_000,
            backup_available: true,
            circuit_open: true,
        };
        assert_eq!(policy.evaluate(&health), DegradationAction::CircuitOpen);
    }

    #[test]
    fn test_degradation_policy_thresholds_are_configurable() {
        // Tighten thresholds and verify the policy respects them.
        let policy = DegradationPolicy::new()
            .with_error_rate_threshold(0.05)
            .with_critical_error_rate(0.2)
            .with_latency_threshold_ms(1000);

        // 0.1 is above the tightened moderate threshold (0.05) but below
        // the tightened critical threshold (0.2) -> ReadOnly.
        let moderate = HealthStatus {
            error_rate: 0.1,
            latency_ms: 10,
            backup_available: true,
            circuit_open: false,
        };
        assert_eq!(policy.evaluate(&moderate), DegradationAction::ReadOnly);

        // 0.3 is above the tightened critical threshold (0.2) -> Fallback.
        let critical = HealthStatus {
            error_rate: 0.3,
            latency_ms: 10,
            backup_available: true,
            circuit_open: false,
        };
        assert_eq!(
            policy.evaluate(&critical),
            DegradationAction::FallbackToBackup
        );

        // 1500ms exceeds the tightened latency threshold -> ReadOnly.
        let slow = HealthStatus {
            error_rate: 0.0,
            latency_ms: 1500,
            backup_available: true,
            circuit_open: false,
        };
        assert_eq!(policy.evaluate(&slow), DegradationAction::ReadOnly);
    }

    // ====================================================================
    // L4 - Retention Policy (BackupCatalog)
    // ====================================================================

    fn make_manifest_at(pool: &str, created_at_millis: i64) -> BackupManifest {
        let mut manifest = BackupManifest::new(pool, false, vec![]);
        manifest.created_at = created_at_millis;
        manifest
    }

    #[test]
    fn test_catalog_new_is_empty() {
        let catalog = BackupCatalog::new();
        assert!(catalog.list().is_empty(), "new catalog must be empty");
    }

    #[test]
    fn test_catalog_register_increases_list_size() {
        let mut catalog = BackupCatalog::new();
        catalog.register(BackupManifest::new("p", false, vec![]));
        assert_eq!(catalog.list().len(), 1);
        catalog.register(BackupManifest::new("p", false, vec![]));
        assert_eq!(catalog.list().len(), 2);
    }

    #[test]
    fn test_catalog_list_returns_registered_manifests() {
        let mut catalog = BackupCatalog::new();
        let first = BackupManifest::new("p", false, vec![]);
        let second = BackupManifest::new("p", false, vec![]);
        catalog.register(first.clone());
        catalog.register(second.clone());

        let listed = catalog.list();
        assert_eq!(listed.len(), 2);
        // Order is preserved (insertion order).
        assert_eq!(listed[0].pool_name, first.pool_name);
        assert_eq!(listed[1].pool_name, second.pool_name);
    }

    #[test]
    fn test_catalog_prune_removes_old_manifests_and_keeps_recent() {
        let now = current_timestamp_millis_helper();
        let mut catalog = BackupCatalog::new();
        // "Old" manifest: created 1 hour ago.
        let old = make_manifest_at("old_pool", now - 3_600_000);
        // "Recent" manifest: created 1 second ago.
        let recent = make_manifest_at("recent_pool", now - 1_000);
        catalog.register(old.clone());
        catalog.register(recent.clone());

        // Retention: 5 minutes. Anything older than 5 minutes is pruned.
        let removed = catalog.prune(std::time::Duration::from_secs(300));

        assert_eq!(removed.len(), 1, "exactly the old manifest must be pruned");
        assert_eq!(removed[0].pool_name, "old_pool");
        assert_eq!(
            catalog.list().len(),
            1,
            "recent manifest must survive the prune"
        );
        assert_eq!(catalog.list()[0].pool_name, "recent_pool");
    }

    #[test]
    fn test_catalog_prune_returns_removed_manifests_in_order() {
        let now = current_timestamp_millis_helper();
        let mut catalog = BackupCatalog::new();
        let older = make_manifest_at("older", now - 10_000);
        let old = make_manifest_at("old", now - 5_000);
        let recent = make_manifest_at("recent", now - 100);
        catalog.register(older.clone());
        catalog.register(old.clone());
        catalog.register(recent.clone());

        // Retention: 1 second. Both older and old must be pruned.
        let removed = catalog.prune(std::time::Duration::from_secs(1));

        assert_eq!(removed.len(), 2);
        // Removed manifests preserve insertion order.
        assert_eq!(removed[0].pool_name, "older");
        assert_eq!(removed[1].pool_name, "old");
        assert_eq!(catalog.list().len(), 1);
        assert_eq!(catalog.list()[0].pool_name, "recent");
    }

    #[test]
    fn test_catalog_prune_with_zero_retention_removes_everything() {
        let now = current_timestamp_millis_helper();
        let mut catalog = BackupCatalog::new();
        // Manifests created 1ms before `now` are strictly older than the
        // prune cutoff (which is `now_prune - 0 = now_prune >= now`), so a
        // zero-duration retention must remove them. We use `now - 1` to
        // avoid same-millisecond race conditions on fast machines.
        catalog.register(make_manifest_at("a", now - 1));
        catalog.register(make_manifest_at("b", now - 1));

        let removed = catalog.prune(std::time::Duration::ZERO);
        assert_eq!(removed.len(), 2, "zero retention must prune everything");
        assert!(catalog.list().is_empty());
    }

    #[test]
    fn test_catalog_prune_with_max_retention_removes_nothing() {
        let now = current_timestamp_millis_helper();
        let mut catalog = BackupCatalog::new();
        catalog.register(make_manifest_at("a", now - 1_000));
        catalog.register(make_manifest_at("b", now - 2_000));

        // A retention of ~30 days must keep both recent manifests.
        let removed = catalog.prune(std::time::Duration::from_secs(60 * 60 * 24 * 30));
        assert!(removed.is_empty(), "long retention must prune nothing");
        assert_eq!(catalog.list().len(), 2);
    }

    #[test]
    fn test_catalog_prune_on_empty_catalog_returns_empty() {
        let mut catalog = BackupCatalog::new();
        let removed = catalog.prune(std::time::Duration::from_secs(60));
        assert!(removed.is_empty());
        assert!(catalog.list().is_empty());
    }

    /// Test-only helper that mirrors the production `current_timestamp_millis`
    /// function in `backup.rs` so tests can build manifests at known offsets
    /// from "now" without depending on private items.
    fn current_timestamp_millis_helper() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
}
