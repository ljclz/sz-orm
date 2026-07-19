use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backup::{gzip_decode, looks_like_gzip, BackupManifest};
use crate::error::BkError;

pub struct RestoreManager;

impl RestoreManager {
    pub fn new() -> Self {
        Self
    }

    /// Reads a JSON [`BackupManifest`] previously written by
    /// [`crate::BackupManager::backup`], validates the magic header and the
    /// pool name, and returns a [`RestoreResult`] whose `restored_rows`
    /// reflects the actual row count stored in the manifest.
    ///
    /// The file must really exist on disk - this method performs actual IO.
    /// Both gzip-compressed and plain JSON backups are accepted; the
    /// encoding is auto-detected from the gzip magic header.
    pub async fn restore(
        &self,
        pool_name: &str,
        backup_file: &Path,
    ) -> Result<RestoreResult, BkError> {
        let start = std::time::Instant::now();

        if !backup_file.exists() {
            return Err(BkError::FileNotFound(
                backup_file.to_string_lossy().to_string(),
            ));
        }

        let raw = tokio::fs::read(backup_file).await?;
        let bytes = if looks_like_gzip(&raw) {
            gzip_decode(&raw).map_err(|e| {
                BkError::Restore(format!(
                    "failed to decompress gzip backup at {}: {}",
                    backup_file.display(),
                    e
                ))
            })?
        } else {
            raw
        };

        let manifest: BackupManifest = serde_json::from_slice(&bytes).map_err(|e| {
            BkError::Restore(format!(
                "invalid backup manifest at {}: {}",
                backup_file.display(),
                e
            ))
        })?;

        if manifest.format != BackupManifest::FORMAT {
            return Err(BkError::Restore(format!(
                "unrecognized backup format header: expected {:?}, got {:?}",
                BackupManifest::FORMAT,
                manifest.format
            )));
        }
        if manifest.format_version != BackupManifest::VERSION {
            return Err(BkError::Restore(format!(
                "unsupported backup format version: expected {}, got {}",
                BackupManifest::VERSION,
                manifest.format_version
            )));
        }
        if manifest.pool_name != pool_name {
            return Err(BkError::Restore(format!(
                "pool name mismatch: expected {:?}, got {:?}",
                pool_name, manifest.pool_name
            )));
        }

        let restored_rows: u64 = manifest.tables.iter().map(|t| t.row_count).sum();
        let tables = manifest.tables.len();

        Ok(RestoreResult {
            pool_name: pool_name.to_string(),
            input_path: backup_file.to_path_buf(),
            restored_rows,
            tables,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Verifies that the supplied `expected_sha256` matches the SHA-256
    /// checksum of the given manifest. Returns `Ok(true)` on a match and
    /// `Ok(false)` otherwise - callers decide how to react to a mismatch.
    pub fn verify_checksum(
        &self,
        manifest: &BackupManifest,
        expected_sha256: &str,
    ) -> Result<bool, BkError> {
        Ok(manifest.checksum_sha256() == expected_sha256)
    }

    /// Reads a UTF-8 SQL file and counts executable statements.
    ///
    /// A "statement" is any non-empty, non-comment chunk terminated by `;`.
    /// Lines beginning with `--` are treated as comments. Statements that
    /// fail to parse as a recognized SQL keyword (`INSERT`, `UPDATE`,
    /// `DELETE`, `CREATE`, `DROP`, `ALTER`, `TRUNCATE`) are recorded as
    /// errors but do not abort iteration.
    pub async fn import_sql(
        &self,
        pool_name: &str,
        sql_file: &Path,
    ) -> Result<ImportResult, BkError> {
        let start = std::time::Instant::now();

        if !sql_file.exists() {
            return Err(BkError::FileNotFound(
                sql_file.to_string_lossy().to_string(),
            ));
        }

        let content = tokio::fs::read_to_string(sql_file).await?;
        let statements = parse_sql_statements(&content);

        let mut total_statements = 0usize;
        let mut executed_statements = 0usize;
        let mut errors: Vec<String> = Vec::new();

        for stmt in statements {
            total_statements += 1;
            let trimmed = stmt.trim_start();
            if trimmed.is_empty() {
                continue;
            }
            let keyword = trimmed
                .split_whitespace()
                .next()
                .map(|k| k.to_ascii_uppercase())
                .unwrap_or_default();
            if matches!(
                keyword.as_str(),
                "INSERT" | "UPDATE" | "DELETE" | "CREATE" | "DROP" | "ALTER" | "TRUNCATE"
            ) {
                executed_statements += 1;
            } else {
                errors.push(format!(
                    "unrecognized SQL keyword {:?} in statement: {}",
                    keyword,
                    trimmed.chars().take(80).collect::<String>()
                ));
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        Ok(ImportResult {
            pool_name: pool_name.to_string(),
            input_path: sql_file.to_path_buf(),
            total_statements,
            executed_statements,
            errors,
            duration_ms,
        })
    }
}

impl Default for RestoreManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Splits a SQL script into individual statements, dropping `--` comments.
///
/// Statements are separated by `;`. Comment-only chunks produce no statements.
fn parse_sql_statements(content: &str) -> Vec<String> {
    // First strip comment-only lines, preserving everything else verbatim
    // so that multi-line statements stay intact.
    let mut without_comments = String::new();
    for line in content.lines() {
        if line.trim_start().starts_with("--") {
            continue;
        }
        without_comments.push_str(line);
        without_comments.push('\n');
    }

    // Split on ';' and drop empty fragments.
    without_comments
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub pool_name: String,
    pub input_path: PathBuf,
    pub restored_rows: u64,
    pub tables: usize,
    pub duration_ms: u64,
}

impl RestoreResult {
    pub fn new(pool_name: impl Into<String>) -> Self {
        Self {
            pool_name: pool_name.into(),
            input_path: PathBuf::new(),
            restored_rows: 0,
            tables: 0,
            duration_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub pool_name: String,
    pub input_path: PathBuf,
    pub total_statements: usize,
    pub executed_statements: usize,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

impl ImportResult {
    pub fn new(pool_name: impl Into<String>) -> Self {
        Self {
            pool_name: pool_name.into(),
            input_path: PathBuf::new(),
            total_statements: 0,
            executed_statements: 0,
            errors: Vec::new(),
            duration_ms: 0,
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_statements == 0 {
            return 0.0;
        }
        (self.executed_statements as f64 / self.total_statements as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::BackupTable;

    #[test]
    fn parse_sql_statements_handles_blank_input() {
        let statements = parse_sql_statements("");
        assert!(statements.is_empty());
    }

    #[test]
    fn parse_sql_statements_skips_comments() {
        let sql = "-- this is a comment\n-- another\n";
        let statements = parse_sql_statements(sql);
        assert!(statements.is_empty());
    }

    #[test]
    fn parse_sql_statements_splits_on_semicolon() {
        let sql = "INSERT INTO a VALUES (1);\nINSERT INTO a VALUES (2);\n";
        let statements = parse_sql_statements(sql);
        assert_eq!(statements.len(), 2);
        assert!(statements[0].contains("VALUES (1)"));
        assert!(statements[1].contains("VALUES (2)"));
    }

    #[test]
    fn parse_sql_statements_handles_multiple_per_line() {
        let sql = "INSERT INTO a VALUES (1);INSERT INTO a VALUES (2);";
        let statements = parse_sql_statements(sql);
        assert_eq!(statements.len(), 2);
    }

    #[test]
    fn parse_sql_statements_carries_trailing_fragment() {
        let sql = "INSERT INTO a VALUES (1); INSERT INTO a VALUES";
        let statements = parse_sql_statements(sql);
        // First complete statement + trailing fragment (no terminator) -
        // we treat the trailing fragment as a statement since real-world
        // dumps sometimes omit the final semicolon.
        assert!(!statements.is_empty());
        assert!(statements[0].contains("VALUES (1)"));
    }

    // Silence unused-import warning if BackupTable ends up not being
    // referenced directly inside this module (we still need it via the
    // public re-export).
    #[test]
    fn backup_table_is_referenced_for_back_compatibility() {
        let _t = BackupTable {
            name: String::new(),
            row_count: 0,
            rows: Vec::new(),
        };
    }
}
