//! # 高级备份功能
//!
//! 提供多算法压缩（Gzip/Zstd）、备份加密（AES-GCM）和
//! RPO/RTO 配置等高级备份恢复能力。

use crate::error::BkError;
use serde::{Deserialize, Serialize};
use std::time::Duration;
// 引入 Crypter trait 以便调用 AesGcmCrypter 的 encrypt/decrypt 方法
use sz_orm_crypto::Crypter;

// ====================================================================
// 多算法压缩：支持 Gzip 和 Zstd
// ====================================================================

/// 压缩算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// 不压缩
    None,
    /// Gzip 压缩（flate2，默认）
    Gzip,
    /// Zstd 压缩（zstd-rs，更高压缩比与速度）
    Zstd,
}

impl CompressionAlgorithm {
    /// 返回算法的魔法字节（用于自动检测压缩格式）
    pub fn magic_bytes(&self) -> &'static [u8] {
        match self {
            CompressionAlgorithm::None => &[],
            CompressionAlgorithm::Gzip => &[0x1f, 0x8b],
            // Zstd 魔法帧头：0x28 0xB5 0x2F 0xFD
            CompressionAlgorithm::Zstd => &[0x28, 0xB5, 0x2F, 0xFD],
        }
    }

    /// 根据数据前缀自动检测压缩算法
    pub fn detect(bytes: &[u8]) -> CompressionAlgorithm {
        if bytes.len() >= 4
            && bytes[0] == 0x28
            && bytes[1] == 0xB5
            && bytes[2] == 0x2F
            && bytes[3] == 0xFD
        {
            CompressionAlgorithm::Zstd
        } else if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
            CompressionAlgorithm::Gzip
        } else {
            CompressionAlgorithm::None
        }
    }
}

/// 压缩配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// 使用的压缩算法
    pub algorithm: CompressionAlgorithm,
    /// 压缩级别（0-19 for Zstd, 0-9 for Gzip）
    pub level: u32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Gzip,
            level: 6,
        }
    }
}

impl CompressionConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_algorithm(mut self, algorithm: CompressionAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    pub fn with_level(mut self, level: u32) -> Self {
        self.level = level;
        self
    }
}

/// 压缩数据：根据指定算法压缩输入
pub fn compress(input: &[u8], config: &CompressionConfig) -> Result<Vec<u8>, BkError> {
    match config.algorithm {
        CompressionAlgorithm::None => Ok(input.to_vec()),
        CompressionAlgorithm::Gzip => {
            let level = config.level.clamp(0, 9);
            gzip_encode_internal(input, Some(level))
        }
        CompressionAlgorithm::Zstd => {
            let level = config.level.clamp(0, 19) as i32;
            zstd::encode_all(input, level)
                .map_err(|e| BkError::Compression(format!("zstd compress failed: {}", e)))
        }
    }
}

/// 解压数据：自动检测压缩格式并解压
pub fn decompress(input: &[u8]) -> Result<Vec<u8>, BkError> {
    match CompressionAlgorithm::detect(input) {
        CompressionAlgorithm::None => Ok(input.to_vec()),
        CompressionAlgorithm::Gzip => {
            use std::io::Read;
            let mut decoder = flate2::read::GzDecoder::new(input);
            let mut out = Vec::with_capacity(input.len() * 4);
            decoder
                .read_to_end(&mut out)
                .map_err(|e| BkError::Compression(format!("gzip decompress failed: {}", e)))?;
            Ok(out)
        }
        CompressionAlgorithm::Zstd => {
            zstd::decode_all(input)
                .map_err(|e| BkError::Compression(format!("zstd decompress failed: {}", e)))
        }
    }
}

/// Gzip 编码内部实现（复用 backup.rs 中的逻辑，避免循环依赖）
fn gzip_encode_internal(input: &[u8], level: Option<u32>) -> Result<Vec<u8>, BkError> {
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

// ====================================================================
// 备份加密：基于 AES-GCM
// ====================================================================

/// 加密算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    /// 不加密
    None,
    /// AES-256-GCM（复用 sz-orm-crypto）
    Aes256Gcm,
}

/// 加密配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// 加密算法
    pub algorithm: EncryptionAlgorithm,
    /// 32 字节密钥的十六进制字符串（64 个 hex 字符）
    pub key_hex: String,
}

impl EncryptionConfig {
    /// 创建新的加密配置
    pub fn new(algorithm: EncryptionAlgorithm, key_hex: impl Into<String>) -> Self {
        Self {
            algorithm,
            key_hex: key_hex.into(),
        }
    }

    /// 创建不加密的配置
    pub fn none() -> Self {
        Self {
            algorithm: EncryptionAlgorithm::None,
            key_hex: String::new(),
        }
    }

    /// 验证密钥格式是否正确
    pub fn validate_key(&self) -> Result<(), BkError> {
        if self.algorithm == EncryptionAlgorithm::None {
            return Ok(());
        }
        if self.key_hex.len() != 64 {
            return Err(BkError::Encryption(format!(
                "AES-256 key must be 64 hex chars, got {}",
                self.key_hex.len()
            )));
        }
        if !self.key_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(BkError::Encryption(
                "key_hex must contain only hex digits".to_string(),
            ));
        }
        Ok(())
    }
}

/// 加密数据：使用 AES-256-GCM
pub fn encrypt(plaintext: &[u8], config: &EncryptionConfig) -> Result<Vec<u8>, BkError> {
    match config.algorithm {
        EncryptionAlgorithm::None => Ok(plaintext.to_vec()),
        EncryptionAlgorithm::Aes256Gcm => {
            config.validate_key()?;
            let crypter = sz_orm_crypto::AesGcmCrypter::from_key_str(&config.key_hex);
            crypter
                .encrypt(plaintext)
                .map_err(|e| BkError::Encryption(format!("AES-GCM encrypt failed: {}", e)))
        }
    }
}

/// 解密数据：使用 AES-256-GCM
pub fn decrypt(ciphertext: &[u8], config: &EncryptionConfig) -> Result<Vec<u8>, BkError> {
    match config.algorithm {
        EncryptionAlgorithm::None => Ok(ciphertext.to_vec()),
        EncryptionAlgorithm::Aes256Gcm => {
            config.validate_key()?;
            let crypter = sz_orm_crypto::AesGcmCrypter::from_key_str(&config.key_hex);
            crypter
                .decrypt(ciphertext)
                .map_err(|e| BkError::Encryption(format!("AES-GCM decrypt failed: {}", e)))
        }
    }
}

// ====================================================================
// 备份编码格式：加密头标记
// ====================================================================

/// 备份文件的编码头（4 字节魔法标记）
///
/// 用于在恢复时自动检测文件是否经过加密。
/// 格式：`SZBE`（SZ-orm Back Encrypted）
pub const ENCRYPTED_MAGIC: &[u8; 4] = b"SZBE";

/// 判断数据是否以加密魔法头开头
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[0..4] == ENCRYPTED_MAGIC
}

/// 带加密头标记地编码数据：先加密，再添加魔法头前缀
///
/// 当 `enc_config.algorithm == EncryptionAlgorithm::None` 时，直接返回原始数据
/// 而不添加魔法头，以保持向后兼容。
pub fn encode_with_encryption(
    plaintext: &[u8],
    enc_config: &EncryptionConfig,
) -> Result<Vec<u8>, BkError> {
    if enc_config.algorithm == EncryptionAlgorithm::None {
        return Ok(plaintext.to_vec());
    }
    let encrypted = encrypt(plaintext, enc_config)?;
    let mut output = Vec::with_capacity(4 + encrypted.len());
    output.extend_from_slice(ENCRYPTED_MAGIC);
    output.extend_from_slice(&encrypted);
    Ok(output)
}

/// 解码带加密头标记的数据：先去除魔法头，再解密
pub fn decode_with_encryption(
    input: &[u8],
    enc_config: &EncryptionConfig,
) -> Result<Vec<u8>, BkError> {
    if !is_encrypted(input) {
        // 未加密，直接返回原始数据
        return Ok(input.to_vec());
    }
    let ciphertext = &input[4..];
    decrypt(ciphertext, enc_config)
}

// ====================================================================
// RPO / RTO 配置
// ====================================================================

/// 恢复点目标（RPO）：可容忍的最大数据丢失时间窗口
///
/// RPO 定义了系统在故障发生后可容忍丢失的最大数据量（以时间衡量）。
/// 例如 RPO = 15 分钟表示最多丢失最近 15 分钟的数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpoConfig {
    /// RPO 时间窗口（秒）
    pub rpo_seconds: u64,
    /// 增量备份间隔（秒），应 <= rpo_seconds
    pub incremental_interval_seconds: u64,
}

impl Default for RpoConfig {
    fn default() -> Self {
        Self {
            rpo_seconds: 900,         // 15 分钟
            incremental_interval_seconds: 300, // 5 分钟
        }
    }
}

impl RpoConfig {
    pub fn new(rpo_seconds: u64) -> Self {
        let incremental_interval = rpo_seconds / 3;
        Self {
            rpo_seconds,
            incremental_interval_seconds: incremental_interval.max(60),
        }
    }

    /// 设置增量备份间隔
    pub fn with_incremental_interval(mut self, seconds: u64) -> Self {
        self.incremental_interval_seconds = seconds;
        self
    }

    /// 返回 RPO 时间窗口
    pub fn rpo_duration(&self) -> Duration {
        Duration::from_secs(self.rpo_seconds)
    }

    /// 返回增量备份间隔
    pub fn incremental_interval(&self) -> Duration {
        Duration::from_secs(self.incremental_interval_seconds)
    }

    /// 验证配置是否合理
    pub fn validate(&self) -> Result<(), BkError> {
        if self.rpo_seconds == 0 {
            return Err(BkError::Backup("RPO must be > 0".to_string()));
        }
        if self.incremental_interval_seconds == 0 {
            return Err(BkError::Backup(
                "incremental interval must be > 0".to_string(),
            ));
        }
        if self.incremental_interval_seconds > self.rpo_seconds {
            return Err(BkError::Backup(format!(
                "incremental interval {}s must be <= RPO {}s",
                self.incremental_interval_seconds, self.rpo_seconds
            )));
        }
        Ok(())
    }

    /// 根据上次备份时间计算下次备份时间
    pub fn next_backup_time(&self, last_backup_secs: u64) -> u64 {
        last_backup_secs + self.incremental_interval_seconds
    }

    /// 判断当前时间是否超过 RPO 窗口（即数据丢失风险）
    pub fn is_rpo_breached(&self, last_backup_secs: u64, now_secs: u64) -> bool {
        now_secs.saturating_sub(last_backup_secs) > self.rpo_seconds
    }
}

/// 恢复时间目标（RTO）：可容忍的最大恢复时间
///
/// RTO 定义了系统在故障发生后可容忍的最长恢复时间。
/// 例如 RTO = 30 分钟表示系统必须在 30 分钟内恢复运行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtoConfig {
    /// RTO 目标时间（秒）
    pub rto_seconds: u64,
    /// 最大允许并行恢复任务数
    pub max_parallel_tasks: u32,
    /// 是否启用快速恢复模式（跳过校验）
    pub fast_mode: bool,
}

impl Default for RtoConfig {
    fn default() -> Self {
        Self {
            rto_seconds: 1800, // 30 分钟
            max_parallel_tasks: 4,
            fast_mode: false,
        }
    }
}

impl RtoConfig {
    pub fn new(rto_seconds: u64) -> Self {
        Self {
            rto_seconds,
            max_parallel_tasks: 4,
            fast_mode: rto_seconds <= 300, // RTO <= 5分钟时自动启用快速模式
        }
    }

    pub fn with_max_parallel_tasks(mut self, tasks: u32) -> Self {
        self.max_parallel_tasks = tasks;
        self
    }

    pub fn with_fast_mode(mut self, fast: bool) -> Self {
        self.fast_mode = fast;
        self
    }

    /// 返回 RTO 时间窗口
    pub fn rto_duration(&self) -> Duration {
        Duration::from_secs(self.rto_seconds)
    }

    /// 验证配置是否合理
    pub fn validate(&self) -> Result<(), BkError> {
        if self.rto_seconds == 0 {
            return Err(BkError::Backup("RTO must be > 0".to_string()));
        }
        if self.max_parallel_tasks == 0 {
            return Err(BkError::Backup(
                "max parallel tasks must be > 0".to_string(),
            ));
        }
        Ok(())
    }

    /// 判断恢复是否超过 RTO 目标
    pub fn is_rto_breached(&self, recovery_duration_ms: u64) -> bool {
        recovery_duration_ms > self.rto_seconds * 1000
    }
}

/// 完整的备份策略配置：结合 RPO、RTO、压缩和加密
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStrategy {
    /// RPO 配置
    pub rpo: RpoConfig,
    /// RTO 配置
    pub rto: RtoConfig,
    /// 压缩配置
    pub compression: CompressionConfig,
    /// 加密配置
    pub encryption: EncryptionConfig,
}

impl Default for BackupStrategy {
    fn default() -> Self {
        Self {
            rpo: RpoConfig::default(),
            rto: RtoConfig::default(),
            compression: CompressionConfig::default(),
            encryption: EncryptionConfig::none(),
        }
    }
}

impl BackupStrategy {
    pub fn new() -> Self {
        Self::default()
    }

    /// 验证整个备份策略是否合理
    pub fn validate(&self) -> Result<(), BkError> {
        self.rpo.validate()?;
        self.rto.validate()?;
        self.encryption.validate_key()?;
        Ok(())
    }

    /// 返回全量备份策略描述
    pub fn describe(&self) -> String {
        format!(
            "BackupStrategy{{ rpo={}s, rto={}s, compression={:?}, encryption={:?}, fast_mode={} }}",
            self.rpo.rpo_seconds,
            self.rto.rto_seconds,
            self.compression.algorithm,
            self.encryption.algorithm,
            self.rto.fast_mode,
        )
    }
}

// ====================================================================
// 备份计划调度
// ====================================================================

/// 备份类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupType {
    /// 全量备份
    Full,
    /// 增量备份
    Incremental,
}

/// 备份计划：根据 RPO/RTO 配置生成备份时间表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    /// RPO 配置
    pub rpo: RpoConfig,
    /// 全量备份间隔（秒）
    pub full_backup_interval_seconds: u64,
    /// 计划开始时间（Unix 秒）
    pub start_time_secs: u64,
}

impl BackupSchedule {
    pub fn new(rpo: RpoConfig, full_backup_interval_seconds: u64, start_time_secs: u64) -> Self {
        Self {
            rpo,
            full_backup_interval_seconds,
            start_time_secs,
        }
    }

    /// 生成从开始时间到结束时间内的所有备份计划项
    pub fn generate_schedule(&self, end_time_secs: u64) -> Vec<ScheduleEntry> {
        let mut entries = Vec::new();
        let mut current = self.start_time_secs;
        let mut last_full = current;

        // 第一次：全量备份
        entries.push(ScheduleEntry {
            time_secs: current,
            backup_type: BackupType::Full,
        });
        current += self.rpo.incremental_interval_seconds;

        while current <= end_time_secs {
            // 如果距离上次全量备份超过全量备份间隔，则做全量备份
            if current - last_full >= self.full_backup_interval_seconds {
                entries.push(ScheduleEntry {
                    time_secs: current,
                    backup_type: BackupType::Full,
                });
                last_full = current;
            } else {
                // 否则做增量备份
                entries.push(ScheduleEntry {
                    time_secs: current,
                    backup_type: BackupType::Incremental,
                });
            }
            current += self.rpo.incremental_interval_seconds;
        }

        entries
    }

    /// 统计计划中的全量和增量备份数量
    pub fn count_by_type(&self, end_time_secs: u64) -> (usize, usize) {
        let schedule = self.generate_schedule(end_time_secs);
        let full_count = schedule
            .iter()
            .filter(|e| e.backup_type == BackupType::Full)
            .count();
        let incr_count = schedule
            .iter()
            .filter(|e| e.backup_type == BackupType::Incremental)
            .count();
        (full_count, incr_count)
    }
}

/// 计划项：单次备份的时间和类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleEntry {
    /// 备份执行时间（Unix 秒）
    pub time_secs: u64,
    /// 备份类型
    pub backup_type: BackupType,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ====================================================================
    // CompressionAlgorithm 测试
    // ====================================================================

    #[test]
    fn test_compression_algorithm_magic_bytes() {
        assert_eq!(CompressionAlgorithm::None.magic_bytes(), &[] as &[u8]);
        assert_eq!(
            CompressionAlgorithm::Gzip.magic_bytes(),
            &[0x1f, 0x8b]
        );
        assert_eq!(
            CompressionAlgorithm::Zstd.magic_bytes(),
            &[0x28, 0xB5, 0x2F, 0xFD]
        );
    }

    #[test]
    fn test_compression_algorithm_detect_gzip() {
        let data = [0x1f, 0x8b, 0x00, 0x00];
        assert_eq!(CompressionAlgorithm::detect(&data), CompressionAlgorithm::Gzip);
    }

    #[test]
    fn test_compression_algorithm_detect_zstd() {
        let data = [0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x00];
        assert_eq!(CompressionAlgorithm::detect(&data), CompressionAlgorithm::Zstd);
    }

    #[test]
    fn test_compression_algorithm_detect_none() {
        let data = [0x00, 0x00, 0x00, 0x00];
        assert_eq!(CompressionAlgorithm::detect(&data), CompressionAlgorithm::None);
    }

    #[test]
    fn test_compression_algorithm_detect_short_data() {
        let data = [0x1f];
        assert_eq!(CompressionAlgorithm::detect(&data), CompressionAlgorithm::None);
    }

    // ====================================================================
    // CompressionConfig 测试
    // ====================================================================

    #[test]
    fn test_compression_config_default() {
        let config = CompressionConfig::default();
        assert_eq!(config.algorithm, CompressionAlgorithm::Gzip);
        assert_eq!(config.level, 6);
    }

    #[test]
    fn test_compression_config_builder() {
        let config = CompressionConfig::new()
            .with_algorithm(CompressionAlgorithm::Zstd)
            .with_level(19);
        assert_eq!(config.algorithm, CompressionAlgorithm::Zstd);
        assert_eq!(config.level, 19);
    }

    // ====================================================================
    // 压缩/解压往返测试
    // ====================================================================

    #[test]
    fn test_compress_decompress_none_roundtrip() {
        let data = b"hello world, this is test data";
        let config = CompressionConfig::new().with_algorithm(CompressionAlgorithm::None);
        let compressed = compress(data, &config).unwrap();
        assert_eq!(compressed, data);
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_compress_decompress_gzip_roundtrip() {
        let data = b"hello world, this is test data that should compress well aaaaaaaa";
        let config = CompressionConfig::new()
            .with_algorithm(CompressionAlgorithm::Gzip)
            .with_level(6);
        let compressed = compress(data, &config).unwrap();
        assert_ne!(compressed, data);
        assert_eq!(compressed[0], 0x1f);
        assert_eq!(compressed[1], 0x8b);
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_compress_decompress_zstd_roundtrip() {
        let data = b"hello world, this is test data that should compress well aaaaaaaa";
        let config = CompressionConfig::new()
            .with_algorithm(CompressionAlgorithm::Zstd)
            .with_level(3);
        let compressed = compress(data, &config).unwrap();
        assert_ne!(compressed, data);
        assert_eq!(compressed[0], 0x28);
        assert_eq!(compressed[1], 0xB5);
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_compress_zstd_smaller_for_repetitive_data() {
        // 高度可压缩数据：zstd 应该比原始数据小
        let data: Vec<u8> = vec![0xAA; 10000];
        let config = CompressionConfig::new()
            .with_algorithm(CompressionAlgorithm::Zstd)
            .with_level(9);
        let compressed = compress(&data, &config).unwrap();
        assert!(
            compressed.len() < data.len(),
            "zstd compressed {} should be smaller than original {}",
            compressed.len(),
            data.len()
        );
    }

    #[test]
    fn test_compress_gzip_clamps_level() {
        // level 100 应该被 clamp 到 9，不报错
        let data = b"test data";
        let config = CompressionConfig::new()
            .with_algorithm(CompressionAlgorithm::Gzip)
            .with_level(100);
        let compressed = compress(data, &config).unwrap();
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_compress_zstd_clamps_level() {
        // level 100 应该被 clamp 到 19，不报错
        let data = b"test data";
        let config = CompressionConfig::new()
            .with_algorithm(CompressionAlgorithm::Zstd)
            .with_level(100);
        let compressed = compress(data, &config).unwrap();
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_decompress_auto_detects_gzip() {
        let data = b"auto-detect compression test data aaaa";
        let gzip_config = CompressionConfig::new()
            .with_algorithm(CompressionAlgorithm::Gzip)
            .with_level(6);
        let compressed = compress(data, &gzip_config).unwrap();
        // decompress 应该自动检测为 gzip 并解压
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_decompress_auto_detects_zstd() {
        let data = b"auto-detect compression test data aaaa";
        let zstd_config = CompressionConfig::new()
            .with_algorithm(CompressionAlgorithm::Zstd)
            .with_level(3);
        let compressed = compress(data, &zstd_config).unwrap();
        // decompress 应该自动检测为 zstd 并解压
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_decompress_auto_detects_none() {
        let data = b"uncompressed data";
        let decompressed = decompress(data).unwrap();
        assert_eq!(decompressed, data);
    }

    // ====================================================================
    // 加密配置测试
    // ====================================================================

    #[test]
    fn test_encryption_config_none() {
        let config = EncryptionConfig::none();
        assert_eq!(config.algorithm, EncryptionAlgorithm::None);
        assert!(config.validate_key().is_ok());
    }

    #[test]
    fn test_encryption_config_aes_valid_key() {
        // 64 个 hex 字符 = 32 字节密钥
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key);
        assert!(config.validate_key().is_ok());
    }

    #[test]
    fn test_encryption_config_aes_short_key_fails() {
        let key = "0123456789abcdef"; // 太短
        let config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key);
        assert!(config.validate_key().is_err());
    }

    #[test]
    fn test_encryption_config_aes_invalid_chars_fails() {
        let key = "g".repeat(64); // 非 hex 字符
        let config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key);
        assert!(config.validate_key().is_err());
    }

    // ====================================================================
    // 加密/解密往返测试
    // ====================================================================

    #[test]
    fn test_encrypt_decrypt_none_roundtrip() {
        let data = b"sensitive data";
        let config = EncryptionConfig::none();
        let encrypted = encrypt(data, &config).unwrap();
        assert_eq!(encrypted, data);
        let decrypted = decrypt(&encrypted, &config).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_encrypt_decrypt_aes_roundtrip() {
        let data = b"sensitive backup data that needs encryption protection";
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key);
        let encrypted = encrypt(data, &config).unwrap();
        assert_ne!(encrypted, data);
        assert!(encrypted.len() > data.len()); // AES-GCM 添加 nonce + tag
        let decrypted = decrypt(&encrypted, &config).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_encrypt_decrypt_wrong_key_fails() {
        let data = b"sensitive data";
        let key1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let key2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
        let enc_config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key1);
        let dec_config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key2);

        let encrypted = encrypt(data, &enc_config).unwrap();
        // 用错误的密钥解密应该失败
        assert!(decrypt(&encrypted, &dec_config).is_err());
    }

    // ====================================================================
    // 加密头标记测试
    // ====================================================================

    #[test]
    fn test_encrypted_magic_bytes() {
        assert_eq!(ENCRYPTED_MAGIC, b"SZBE");
    }

    #[test]
    fn test_is_encrypted_detects_magic() {
        let mut data = Vec::new();
        data.extend_from_slice(ENCRYPTED_MAGIC);
        data.extend_from_slice(b"encrypted payload");
        assert!(is_encrypted(&data));
    }

    #[test]
    fn test_is_encrypted_rejects_non_magic() {
        let data = b"plain data without magic header";
        assert!(!is_encrypted(data));
    }

    #[test]
    fn test_is_encrypted_rejects_short_data() {
        let data = b"SZ";
        assert!(!is_encrypted(data));
    }

    #[test]
    fn test_encode_with_encryption_adds_magic_header() {
        let data = b"backup data";
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key);
        let encoded = encode_with_encryption(data, &config).unwrap();
        assert!(is_encrypted(&encoded));
        assert_eq!(&encoded[0..4], ENCRYPTED_MAGIC);
    }

    #[test]
    fn test_encode_decode_with_encryption_roundtrip() {
        let data = b"backup data that needs full encryption protection";
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key);
        let encoded = encode_with_encryption(data, &config).unwrap();
        assert!(is_encrypted(&encoded));
        let decoded = decode_with_encryption(&encoded, &config).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_with_encryption_handles_unencrypted() {
        let data = b"unencrypted backup data";
        let config = EncryptionConfig::none();
        let decoded = decode_with_encryption(data, &config).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_encode_with_encryption_none_no_magic_header() {
        let data = b"backup data";
        let config = EncryptionConfig::none();
        let encoded = encode_with_encryption(data, &config).unwrap();
        // 不加密时不应该添加魔法头
        assert!(!is_encrypted(&encoded));
        assert_eq!(encoded, data);
    }

    // ====================================================================
    // RPO 配置测试
    // ====================================================================

    #[test]
    fn test_rpo_config_default() {
        let rpo = RpoConfig::default();
        assert_eq!(rpo.rpo_seconds, 900);
        assert_eq!(rpo.incremental_interval_seconds, 300);
    }

    #[test]
    fn test_rpo_config_new() {
        let rpo = RpoConfig::new(1800);
        assert_eq!(rpo.rpo_seconds, 1800);
        assert_eq!(rpo.incremental_interval_seconds, 600); // 1800/3
    }

    #[test]
    fn test_rpo_config_new_clamps_interval() {
        let rpo = RpoConfig::new(60);
        assert_eq!(rpo.rpo_seconds, 60);
        // 60/3 = 20，但会被 clamp 到 60
        assert_eq!(rpo.incremental_interval_seconds, 60);
    }

    #[test]
    fn test_rpo_config_with_incremental_interval() {
        let rpo = RpoConfig::new(900).with_incremental_interval(120);
        assert_eq!(rpo.incremental_interval_seconds, 120);
    }

    #[test]
    fn test_rpo_config_validate_ok() {
        let rpo = RpoConfig::new(900);
        assert!(rpo.validate().is_ok());
    }

    #[test]
    fn test_rpo_config_validate_zero_rpo_fails() {
        let rpo = RpoConfig {
            rpo_seconds: 0,
            incremental_interval_seconds: 60,
        };
        assert!(rpo.validate().is_err());
    }

    #[test]
    fn test_rpo_config_validate_zero_interval_fails() {
        let rpo = RpoConfig {
            rpo_seconds: 900,
            incremental_interval_seconds: 0,
        };
        assert!(rpo.validate().is_err());
    }

    #[test]
    fn test_rpo_config_validate_interval_greater_than_rpo_fails() {
        let rpo = RpoConfig {
            rpo_seconds: 100,
            incremental_interval_seconds: 200,
        };
        assert!(rpo.validate().is_err());
    }

    #[test]
    fn test_rpo_config_next_backup_time() {
        let rpo = RpoConfig::new(900).with_incremental_interval(300);
        assert_eq!(rpo.next_backup_time(1000), 1300);
    }

    #[test]
    fn test_rpo_config_is_rpo_breached() {
        let rpo = RpoConfig::new(900);
        // 上次备份在 1000，当前 1200，差距 200 < 900，未超
        assert!(!rpo.is_rpo_breached(1000, 1200));
        // 当前 2000，差距 1000 > 900，已超
        assert!(rpo.is_rpo_breached(1000, 2000));
    }

    #[test]
    fn test_rpo_config_is_rpo_breached_saturating_sub() {
        let rpo = RpoConfig::new(900);
        // 当前时间早于上次备份时间，应返回 false
        assert!(!rpo.is_rpo_breached(2000, 1000));
    }

    // ====================================================================
    // RTO 配置测试
    // ====================================================================

    #[test]
    fn test_rto_config_default() {
        let rto = RtoConfig::default();
        assert_eq!(rto.rto_seconds, 1800);
        assert_eq!(rto.max_parallel_tasks, 4);
        assert!(!rto.fast_mode);
    }

    #[test]
    fn test_rto_config_new_long_rto() {
        let rto = RtoConfig::new(1800);
        assert_eq!(rto.rto_seconds, 1800);
        assert!(!rto.fast_mode);
    }

    #[test]
    fn test_rto_config_new_short_rto_enables_fast_mode() {
        let rto = RtoConfig::new(300);
        assert_eq!(rto.rto_seconds, 300);
        assert!(rto.fast_mode);
    }

    #[test]
    fn test_rto_config_with_max_parallel_tasks() {
        let rto = RtoConfig::new(1800).with_max_parallel_tasks(8);
        assert_eq!(rto.max_parallel_tasks, 8);
    }

    #[test]
    fn test_rto_config_with_fast_mode() {
        let rto = RtoConfig::new(1800).with_fast_mode(true);
        assert!(rto.fast_mode);
    }

    #[test]
    fn test_rto_config_validate_ok() {
        let rto = RtoConfig::new(1800);
        assert!(rto.validate().is_ok());
    }

    #[test]
    fn test_rto_config_validate_zero_rto_fails() {
        let rto = RtoConfig {
            rto_seconds: 0,
            max_parallel_tasks: 4,
            fast_mode: false,
        };
        assert!(rto.validate().is_err());
    }

    #[test]
    fn test_rto_config_validate_zero_tasks_fails() {
        let rto = RtoConfig {
            rto_seconds: 1800,
            max_parallel_tasks: 0,
            fast_mode: false,
        };
        assert!(rto.validate().is_err());
    }

    #[test]
    fn test_rto_config_is_rto_breached() {
        let rto = RtoConfig::new(60); // RTO = 60秒
        // 恢复耗时 30秒 = 30000ms < 60000ms，未超
        assert!(!rto.is_rto_breached(30000));
        // 恢复耗时 90秒 = 90000ms > 60000ms，已超
        assert!(rto.is_rto_breached(90000));
    }

    // ====================================================================
    // BackupStrategy 测试
    // ====================================================================

    #[test]
    fn test_backup_strategy_default() {
        let strategy = BackupStrategy::default();
        assert!(strategy.validate().is_ok());
    }

    #[test]
    fn test_backup_strategy_with_encryption() {
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let strategy = BackupStrategy {
            rpo: RpoConfig::new(900),
            rto: RtoConfig::new(1800),
            compression: CompressionConfig::new().with_algorithm(CompressionAlgorithm::Zstd),
            encryption: EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key),
        };
        assert!(strategy.validate().is_ok());
    }

    #[test]
    fn test_backup_strategy_invalid_encryption_fails() {
        let strategy = BackupStrategy {
            rpo: RpoConfig::new(900),
            rto: RtoConfig::new(1800),
            compression: CompressionConfig::default(),
            encryption: EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, "short"),
        };
        assert!(strategy.validate().is_err());
    }

    #[test]
    fn test_backup_strategy_invalid_rpo_fails() {
        let strategy = BackupStrategy {
            rpo: RpoConfig {
                rpo_seconds: 0,
                incremental_interval_seconds: 60,
            },
            rto: RtoConfig::default(),
            compression: CompressionConfig::default(),
            encryption: EncryptionConfig::none(),
        };
        assert!(strategy.validate().is_err());
    }

    #[test]
    fn test_backup_strategy_describe() {
        let strategy = BackupStrategy::default();
        let desc = strategy.describe();
        assert!(desc.contains("rpo=900s"));
        assert!(desc.contains("rto=1800s"));
        assert!(desc.contains("Gzip"));
    }

    // ====================================================================
    // BackupSchedule 测试
    // ====================================================================

    #[test]
    fn test_backup_schedule_first_backup_is_full() {
        let rpo = RpoConfig::new(900).with_incremental_interval(300);
        let schedule = BackupSchedule::new(rpo, 3600, 1000);
        let entries = schedule.generate_schedule(1000);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].backup_type, BackupType::Full);
    }

    #[test]
    fn test_backup_schedule_generates_incremental_between_full() {
        let rpo = RpoConfig::new(900).with_incremental_interval(300);
        // 全量备份间隔 900 秒，增量间隔 300 秒
        let schedule = BackupSchedule::new(rpo, 900, 0);
        let entries = schedule.generate_schedule(900);
        // 时间点：0(full), 300(incr), 600(incr), 900(full)
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].backup_type, BackupType::Full);
        assert_eq!(entries[0].time_secs, 0);
        assert_eq!(entries[1].backup_type, BackupType::Incremental);
        assert_eq!(entries[1].time_secs, 300);
        assert_eq!(entries[2].backup_type, BackupType::Incremental);
        assert_eq!(entries[2].time_secs, 600);
        assert_eq!(entries[3].backup_type, BackupType::Full);
        assert_eq!(entries[3].time_secs, 900);
    }

    #[test]
    fn test_backup_schedule_count_by_type() {
        let rpo = RpoConfig::new(900).with_incremental_interval(300);
        let schedule = BackupSchedule::new(rpo, 900, 0);
        let (full, incr) = schedule.count_by_type(900);
        assert_eq!(full, 2); // 0 和 900
        assert_eq!(incr, 2); // 300 和 600
    }

    #[test]
    fn test_backup_schedule_all_full_when_interval_equals_rpo() {
        let rpo = RpoConfig::new(900).with_incremental_interval(300);
        // 全量备份间隔 = 增量间隔 = 300
        let schedule = BackupSchedule::new(rpo, 300, 0);
        let entries = schedule.generate_schedule(900);
        // 每次都应该是全量备份
        for entry in &entries {
            assert_eq!(entry.backup_type, BackupType::Full);
        }
    }

    #[test]
    fn test_backup_schedule_empty_when_end_before_start() {
        let rpo = RpoConfig::new(900).with_incremental_interval(300);
        let schedule = BackupSchedule::new(rpo, 3600, 1000);
        let entries = schedule.generate_schedule(500);
        // 结束时间 < 开始时间，只返回第一个全量备份
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_backup_type_equality() {
        assert_eq!(BackupType::Full, BackupType::Full);
        assert_ne!(BackupType::Full, BackupType::Incremental);
    }

    #[test]
    fn test_schedule_entry_equality() {
        let a = ScheduleEntry {
            time_secs: 100,
            backup_type: BackupType::Full,
        };
        let b = ScheduleEntry {
            time_secs: 100,
            backup_type: BackupType::Full,
        };
        assert_eq!(a, b);
    }

    // ====================================================================
    // 完整流程测试：压缩 + 加密 + 编码
    // ====================================================================

    #[test]
    fn test_full_pipeline_compress_then_encrypt_roundtrip() {
        let original = b"{\"tables\":[{\"name\":\"users\",\"rows\":[{\"id\":1}]}]}";
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let comp_config = CompressionConfig::new().with_algorithm(CompressionAlgorithm::Zstd);
        let enc_config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key);

        // 1. 压缩
        let compressed = compress(original, &comp_config).unwrap();
        assert_ne!(compressed, original);
        // 2. 加密
        let encoded = encode_with_encryption(&compressed, &enc_config).unwrap();
        assert!(is_encrypted(&encoded));

        // 3. 解密
        let decrypted = decode_with_encryption(&encoded, &enc_config).unwrap();
        assert_eq!(decrypted, compressed);
        // 4. 解压
        let restored = decompress(&decrypted).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn test_full_pipeline_gzip_then_aes_roundtrip() {
        let original = b"backup payload with repetitive data: aaaaaaaaaaaaaaaaaaaa";
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let comp_config = CompressionConfig::new().with_algorithm(CompressionAlgorithm::Gzip);
        let enc_config = EncryptionConfig::new(EncryptionAlgorithm::Aes256Gcm, key);

        let compressed = compress(original, &comp_config).unwrap();
        let encoded = encode_with_encryption(&compressed, &enc_config).unwrap();

        let decrypted = decode_with_encryption(&encoded, &enc_config).unwrap();
        let restored = decompress(&decrypted).unwrap();
        assert_eq!(restored, original);
    }
}
