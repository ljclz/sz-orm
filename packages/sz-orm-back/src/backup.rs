use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::error::BkError;

/// A serialized backup manifest written to disk by [`BackupManager::backup`]
/// and read back by [`crate::RestoreManager::restore`].
///
/// The format is JSON so that restore can validate the pool name and inspect
/// row counts without depending on any external tooling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Magic header so restore can fail early on unrelated files.
    pub format: String,
    pub format_version: u32,
    pub pool_name: String,
    pub created_at: i64,
    pub compressed: bool,
    pub tables: Vec<BackupTable>,
    /// `true` for incremental backups produced by
    /// [`BackupManager::incremental_backup`]. Full backups keep this `false`
    /// so older readers can deserialize the manifest unchanged.
    #[serde(default)]
    pub is_incremental: bool,
    /// Identifier of the full backup an incremental backup is built on top
    /// of. `None` for full backups or when the incremental has no base.
    #[serde(default)]
    pub base_backup_id: Option<String>,
}

impl BackupManifest {
    pub const FORMAT: &'static str = "sz-orm-back";
    pub const VERSION: u32 = 1;

    pub fn new(pool_name: impl Into<String>, compressed: bool, tables: Vec<BackupTable>) -> Self {
        Self {
            format: Self::FORMAT.to_string(),
            format_version: Self::VERSION,
            pool_name: pool_name.into(),
            created_at: current_timestamp_millis(),
            compressed,
            tables,
            is_incremental: false,
            base_backup_id: None,
        }
    }

    /// Computes the SHA-256 hex digest of the manifest's canonical JSON
    /// serialization. The digest is deterministic for byte-identical
    /// manifests, so it can be used to detect tampering or corruption in
    /// transit.
    pub fn checksum_sha256(&self) -> String {
        use sha2::{Digest, Sha256};
        // serde_json serializes struct fields in declaration order and
        // serde_json::Map uses BTreeMap by default (no `preserve_order`
        // feature), so the byte output is stable across processes.
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher.finalize();
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest.iter() {
            out.push_str(&format!("{:02x}", byte));
        }
        out
    }

    /// Builder-style setter for `base_backup_id`. Useful for associating an
    /// incremental backup manifest with the full backup it was built on top
    /// of. Does not mutate any other field.
    pub fn with_base_backup_id(mut self, base_backup_id: impl Into<String>) -> Self {
        self.base_backup_id = Some(base_backup_id.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTable {
    pub name: String,
    pub row_count: u64,
    pub rows: Vec<serde_json::Value>,
}

pub struct BackupManager {
    config: BackupConfig,
    tables: RwLock<HashMap<String, RegisteredTable>>,
}

/// Internal record tracking a registered table's rows and the wall-clock
/// time of its last mutation. The modification timestamp powers
/// [`BackupManager::incremental_backup`].
struct RegisteredTable {
    rows: Vec<serde_json::Value>,
    modified_at: chrono::DateTime<chrono::Utc>,
}

impl BackupManager {
    pub fn new(config: BackupConfig) -> Self {
        Self {
            config,
            tables: RwLock::new(HashMap::new()),
        }
    }

    /// Registers (or replaces) the rows for a virtual table that will be
    /// included in the next [`Self::backup`] / [`Self::export_sql`] call.
    /// The table's modification timestamp is updated to `Utc::now()` so
    /// subsequent [`Self::incremental_backup`] calls can pick it up.
    pub fn register_table(&self, name: impl Into<String>, rows: Vec<serde_json::Value>) {
        // lock poisoned 时降级为 no-op，避免级联 panic 拖垮整个进程。
        // lock poisoned 意味着另一线程已 panic，该数据结构状态不可信。
        if let Ok(mut tables) = self.tables.write() {
            tables.insert(
                name.into(),
                RegisteredTable {
                    rows,
                    modified_at: chrono::Utc::now(),
                },
            );
        }
    }

    /// Returns a snapshot of the table names currently registered, in
    /// deterministic (sorted) order.
    pub fn registered_tables(&self) -> Vec<String> {
        // lock poisoned 时返回空 Vec，避免级联 panic。
        let names = match self.tables.read() {
            Ok(tables) => {
                let mut names: Vec<String> = tables.keys().cloned().collect();
                names.sort();
                names
            }
            Err(_) => Vec::new(),
        };
        names
    }

    /// Writes a JSON [`BackupManifest`] containing every registered table to
    /// `output_dir`. The on-disk file is real - subsequent calls to
    /// [`crate::RestoreManager::restore`] can read it back.
    ///
    /// When `config.compress == true` the JSON payload is gzip-compressed
    /// (magic bytes `0x1f 0x8b`) before being written to disk. Restore
    /// auto-detects the encoding, so callers do not need to track it.
    pub async fn backup(
        &self,
        pool_name: &str,
        output_dir: &Path,
    ) -> Result<BackupResult, BkError> {
        let start = std::time::Instant::now();

        if !output_dir.exists() {
            tokio::fs::create_dir_all(output_dir).await?;
        }

        let filename = format!("{}_backup_{}.json", pool_name, timestamp_secs());
        let output_path = output_dir.join(filename);

        let tables_snapshot = self.snapshot_all_tables()?;

        let manifest =
            BackupManifest::new(pool_name, self.config.compress, tables_snapshot.clone());

        let json_bytes = if self.config.compress {
            serde_json::to_vec(&manifest).map_err(|e| BkError::Backup(e.to_string()))?
        } else {
            serde_json::to_vec_pretty(&manifest).map_err(|e| BkError::Backup(e.to_string()))?
        };

        let bytes = if self.config.compress {
            gzip_encode(&json_bytes, self.config.compression_level)?
        } else {
            json_bytes
        };

        let file_size = bytes.len() as u64;
        tokio::fs::write(&output_path, &bytes).await?;

        let total_tables = tables_snapshot.len();
        let total_rows: u64 = tables_snapshot.iter().map(|t| t.row_count).sum();

        Ok(BackupResult {
            pool_name: pool_name.to_string(),
            output_path,
            total_tables,
            backed_tables: total_tables,
            total_rows,
            duration_ms: start.elapsed().as_millis() as u64,
            compressed: self.config.compress,
            file_size,
        })
    }

    /// Produces an incremental [`BackupManifest`] containing only tables
    /// whose `modified_at >= since`. The returned manifest has
    /// `is_incremental = true` and `base_backup_id = None`; callers can
    /// chain [`BackupManifest::with_base_backup_id`] to associate it with
    /// a full backup.
    ///
    /// No file is written - the manifest is returned in-memory so callers
    /// can decide where (and whether) to persist it.
    pub async fn incremental_backup(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<BackupManifest, BkError> {
        let mut snapshot = self.snapshot_tables_modified_since(since)?;
        // Deterministic ordering, matching full backup semantics.
        snapshot.sort_by(|a, b| a.name.cmp(&b.name));

        let mut manifest = BackupManifest::new("", self.config.compress, snapshot);
        manifest.is_incremental = true;
        manifest.base_backup_id = None;
        Ok(manifest)
    }

    /// Snapshots every registered table into a sorted [`BackupTable`] vec.
    fn snapshot_all_tables(&self) -> Result<Vec<BackupTable>, BkError> {
        let tables = self
            .tables
            .read()
            .map_err(|e| BkError::Backup(e.to_string()))?;
        let mut entries: Vec<(&String, &RegisteredTable)> = tables.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        Ok(entries
            .into_iter()
            .map(|(name, registered)| BackupTable {
                name: name.clone(),
                row_count: registered.rows.len() as u64,
                rows: registered.rows.clone(),
            })
            .collect())
    }

    /// Snapshots only tables whose `modified_at >= since` into an unsorted
    /// [`BackupTable`] vec. Callers that need deterministic ordering should
    /// sort the result.
    fn snapshot_tables_modified_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<BackupTable>, BkError> {
        let tables = self
            .tables
            .read()
            .map_err(|e| BkError::Backup(e.to_string()))?;
        Ok(tables
            .iter()
            .filter(|(_, registered)| registered.modified_at >= since)
            .map(|(name, registered)| BackupTable {
                name: name.clone(),
                row_count: registered.rows.len() as u64,
                rows: registered.rows.clone(),
            })
            .collect())
    }

    /// Writes a UTF-8 SQL file containing one `INSERT` statement per registered
    /// row. The output is real - it can be re-read by
    /// [`crate::RestoreManager::import_sql`].
    pub async fn export_sql(
        &self,
        pool_name: &str,
        output_dir: &Path,
    ) -> Result<ExportResult, BkError> {
        let start = std::time::Instant::now();

        if !output_dir.exists() {
            tokio::fs::create_dir_all(output_dir).await?;
        }

        let output_path = output_dir.join(format!("{}_export.sql", pool_name));

        let mut sql = String::new();
        sql.push_str(&format!("-- SZ-ORM export for pool: {}\n", pool_name));
        sql.push_str(&format!(
            "-- Generated at: {}\n",
            current_timestamp_millis()
        ));
        sql.push_str("-- Format: sz-orm-back/sql-export v1\n\n");

        let (total_tables, total_rows) = {
            let tables = self
                .tables
                .read()
                .map_err(|e| BkError::Export(e.to_string()))?;
            let mut entries: Vec<(&String, &RegisteredTable)> = tables.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut tables_count = 0usize;
            let mut rows_count = 0u64;
            for (name, registered) in entries {
                tables_count += 1;
                sql.push_str(&format!("-- Table: {}\n", name));
                for row in &registered.rows {
                    let json_str =
                        serde_json::to_string(row).map_err(|e| BkError::Export(e.to_string()))?;
                    sql.push_str(&format!(
                        "INSERT INTO \"{}\" VALUES ({});\n",
                        name, json_str
                    ));
                    rows_count += 1;
                }
                sql.push('\n');
            }
            (tables_count, rows_count)
        };

        let file_size = sql.len() as u64;
        tokio::fs::write(&output_path, sql.as_bytes()).await?;

        Ok(ExportResult {
            pool_name: pool_name.to_string(),
            output_path,
            file_size,
            tables: total_tables,
            total_rows,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub compress: bool,
    pub compression_level: Option<u32>,
    pub include_schema: bool,
    pub batch_size: usize,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            compress: true,
            compression_level: Some(6),
            include_schema: true,
            batch_size: 1000,
        }
    }
}

impl BackupConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }

    pub fn with_compression_level(mut self, level: u32) -> Self {
        self.compression_level = Some(level);
        self
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult {
    pub pool_name: String,
    pub output_path: PathBuf,
    pub total_tables: usize,
    pub backed_tables: usize,
    pub total_rows: u64,
    pub duration_ms: u64,
    pub compressed: bool,
    pub file_size: u64,
}

impl BackupResult {
    pub fn new(pool_name: impl Into<String>) -> Self {
        Self {
            pool_name: pool_name.into(),
            output_path: PathBuf::new(),
            total_tables: 0,
            backed_tables: 0,
            total_rows: 0,
            duration_ms: 0,
            compressed: false,
            file_size: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub pool_name: String,
    pub output_path: PathBuf,
    pub file_size: u64,
    pub tables: usize,
    pub total_rows: u64,
    pub duration_ms: u64,
}

impl ExportResult {
    pub fn new(pool_name: impl Into<String>) -> Self {
        Self {
            pool_name: pool_name.into(),
            output_path: PathBuf::new(),
            file_size: 0,
            tables: 0,
            total_rows: 0,
            duration_ms: 0,
        }
    }
}

// ====================================================================
// L4 - Retention Policy (BackupCatalog)
// ====================================================================

/// In-memory registry of [`BackupManifest`]s used to enforce retention
/// policies. The catalog keeps manifests in insertion order so that
/// [`Self::prune`] can return the evicted manifests in the same order they
/// were registered.
pub struct BackupCatalog {
    manifests: Vec<BackupManifest>,
}

impl BackupCatalog {
    pub fn new() -> Self {
        Self {
            manifests: Vec::new(),
        }
    }

    /// Appends a manifest to the catalog. Manifests are stored in
    /// insertion order; later calls to [`Self::list`] and
    /// [`Self::prune`] preserve that order.
    pub fn register(&mut self, manifest: BackupManifest) {
        self.manifests.push(manifest);
    }

    /// Returns a snapshot of all registered manifests in insertion order.
    pub fn list(&self) -> Vec<&BackupManifest> {
        self.manifests.iter().collect()
    }

    /// Removes every manifest whose `created_at` is older than
    /// `now - retention` and returns the evicted manifests in insertion
    /// order. Manifests exactly at the cutoff boundary are kept (the
    /// comparison is strict `<`).
    pub fn prune(&mut self, retention: std::time::Duration) -> Vec<BackupManifest> {
        let now_millis = current_timestamp_millis();
        let retention_millis = i64::try_from(retention.as_millis()).unwrap_or(i64::MAX);
        let cutoff = now_millis.saturating_sub(retention_millis);

        let all = std::mem::take(&mut self.manifests);
        let (removed, kept) = all
            .into_iter()
            .partition(|manifest: &BackupManifest| manifest.created_at < cutoff);
        self.manifests = kept;
        removed
    }
}

impl Default for BackupCatalog {
    fn default() -> Self {
        Self::new()
    }
}

fn timestamp_secs() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", now)
}

fn current_timestamp_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Gzip-encodes `input` using the requested compression level. `level` of
/// `None` falls back to flate2's default (currently 6). Levels outside the
/// valid range (0..=9) are clamped.
fn gzip_encode(input: &[u8], level: Option<u32>) -> Result<Vec<u8>, BkError> {
    use flate2::write::GzEncoder;
    use std::io::Write;
    let level = level.unwrap_or(6).clamp(0, 9);
    let compression = flate2::Compression::new(level);
    let encoder = GzEncoder::new(Vec::with_capacity(input.len() / 4 + 32), compression);
    let mut writer = encoder;
    writer
        .write_all(input)
        .map_err(|e| BkError::Compression(e.to_string()))?;
    writer
        .finish()
        .map_err(|e| BkError::Compression(e.to_string()))
}

/// Returns `true` if `bytes` begins with the gzip magic header `0x1f 0x8b`.
pub(crate) fn looks_like_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

/// Gzip-decodes `input`. Caller is expected to have already verified the
/// magic header via [`looks_like_gzip`].
pub(crate) fn gzip_decode(input: &[u8]) -> Result<Vec<u8>, BkError> {
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(input);
    let mut out = Vec::with_capacity(input.len() * 4);
    decoder
        .read_to_end(&mut out)
        .map_err(|e| BkError::Compression(e.to_string()))?;
    Ok(out)
}
