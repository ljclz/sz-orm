//! # 消息压缩（permessage-deflate 模拟）
//!
//! 模拟 RFC 7692 `permessage-deflate` 扩展的协商与压缩/解压缩流程。
//! 本模块为**纯模拟实现**，不依赖 `flate2` 等外部压缩库：
//!
//! - 协商阶段：解析客户端 `Sec-WebSocket-Extensions` 头并匹配服务端支持的参数
//! - 压缩阶段：使用 RLE（Run-Length Encoding）变体对消息进行简单压缩，
//!   用于验证压缩流程的正确性与压缩比统计
//!
//! ## 主要类型
//!
//! - [`CompressionConfig`] — 压缩配置
//! - [`CompressionNegotiator`] — 扩展协商器
//! - [`CompressionStats`] — 压缩统计
//! - [`MessageCompressor`] — 消息压缩器

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 压缩配置，对应 permessage-deflate 扩展参数
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// 服务端窗口大小指数（4-15，对应 2^server_no_context_takeover_bits）
    pub server_max_window_bits: u8,
    /// 客户端窗口大小指数（4-15）
    pub client_max_window_bits: u8,
    /// 服务端不保留上下文（每条消息独立压缩）
    pub server_no_context_takeover: bool,
    /// 客户端不保留上下文
    pub client_no_context_takeover: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            server_max_window_bits: 15,
            client_max_window_bits: 15,
            server_no_context_takeover: false,
            client_no_context_takeover: false,
        }
    }
}

impl CompressionConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置服务端窗口大小
    pub fn with_server_window_bits(mut self, bits: u8) -> Self {
        self.server_max_window_bits = bits.clamp(4, 15);
        self
    }

    /// 设置客户端窗口大小
    pub fn with_client_window_bits(mut self, bits: u8) -> Self {
        self.client_max_window_bits = bits.clamp(4, 15);
        self
    }

    /// 启用服务端无上下文接管
    pub fn with_server_no_context_takeover(mut self) -> Self {
        self.server_no_context_takeover = true;
        self
    }

    /// 启用客户端无上下文接管
    pub fn with_client_no_context_takeover(mut self) -> Self {
        self.client_no_context_takeover = true;
        self
    }

    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), String> {
        if !(4..=15).contains(&self.server_max_window_bits) {
            return Err("server_max_window_bits must be in [4, 15]".to_string());
        }
        if !(4..=15).contains(&self.client_max_window_bits) {
            return Err("client_max_window_bits must be in [4, 15]".to_string());
        }
        Ok(())
    }

    /// 将配置渲染为 permessage-deflate 扩展参数字符串
    pub fn to_extension_params(&self) -> String {
        let mut parts = vec!["permessage-deflate".to_string()];
        parts.push(format!("server_max_window_bits={}", self.server_max_window_bits));
        if self.server_no_context_takeover {
            parts.push("server_no_context_takeover".to_string());
        }
        if self.client_no_context_takeover {
            parts.push("client_no_context_takeover".to_string());
        }
        parts.push(format!("client_max_window_bits={}", self.client_max_window_bits));
        parts.join("; ")
    }
}

/// 客户端提供的扩展参数
#[derive(Debug, Clone, Default)]
pub struct ClientExtensions {
    /// 原始扩展头值
    pub raw: String,
    /// 解析出的参数键值对
    pub params: HashMap<String, Option<String>>,
}

impl ClientExtensions {
    /// 从 `Sec-WebSocket-Extensions` 头值解析
    pub fn parse(header_value: &str) -> Self {
        let mut params = HashMap::new();
        for part in header_value.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((key, value)) = part.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().trim_matches('"').to_string();
                params.insert(key, Some(value));
            } else {
                params.insert(part.to_string(), None);
            }
        }
        Self {
            raw: header_value.to_string(),
            params,
        }
    }

    /// 是否请求了 permessage-deflate
    pub fn requests_deflate(&self) -> bool {
        self.raw.contains("permessage-deflate")
    }

    /// 获取参数值
    pub fn get_param(&self, key: &str) -> Option<&str> {
        self.params.get(key).and_then(|v| v.as_deref())
    }

    /// 是否包含某参数（无论有无值）
    pub fn has_param(&self, key: &str) -> bool {
        self.params.contains_key(key)
    }
}

/// 扩展协商结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NegotiationResult {
    /// 协商成功，返回服务端选定的扩展头值
    Accepted(String),
    /// 客户端未请求压缩，服务端不启用
    NotRequested,
    /// 客户端请求的参数不合法，拒绝压缩
    Rejected(String),
}

/// 扩展协商器
#[derive(Debug, Clone)]
pub struct CompressionNegotiator {
    config: CompressionConfig,
}

impl CompressionNegotiator {
    /// 创建协商器
    pub fn new(config: CompressionConfig) -> Self {
        Self { config }
    }

    /// 获取配置
    pub fn config(&self) -> &CompressionConfig {
        &self.config
    }

    /// 根据客户端请求协商压缩参数
    pub fn negotiate(&self, client_header: &str) -> NegotiationResult {
        if client_header.is_empty() {
            return NegotiationResult::NotRequested;
        }

        let client_ext = ClientExtensions::parse(client_header);
        if !client_ext.requests_deflate() {
            return NegotiationResult::NotRequested;
        }

        // 校验客户端请求的窗口大小是否在合法范围
        if let Some(bits_str) = client_ext.get_param("server_max_window_bits") {
            if let Ok(bits) = bits_str.parse::<u8>() {
                if !(4..=15).contains(&bits) {
                    return NegotiationResult::Rejected(format!(
                        "invalid server_max_window_bits: {}",
                        bits
                    ));
                }
            }
        }

        // 服务端选定最终参数（取客户端与服务端的较小值）
        let mut response_parts = vec!["permessage-deflate".to_string()];

        // server_max_window_bits：若客户端指定了，取 min(客户端, 服务端)
        let final_server_bits = if let Some(bits_str) = client_ext.get_param("server_max_window_bits") {
            if let Ok(client_bits) = bits_str.parse::<u8>() {
                client_bits.min(self.config.server_max_window_bits)
            } else {
                self.config.server_max_window_bits
            }
        } else {
            self.config.server_max_window_bits
        };
        response_parts.push(format!("server_max_window_bits={}", final_server_bits));

        if self.config.server_no_context_takeover
            || client_ext.has_param("server_no_context_takeover")
        {
            response_parts.push("server_no_context_takeover".to_string());
        }

        // client_max_window_bits：仅当客户端提供了值时才回应
        if let Some(bits_str) = client_ext.get_param("client_max_window_bits") {
            if let Ok(client_bits) = bits_str.parse::<u8>() {
                let final_client_bits = client_bits.min(self.config.client_max_window_bits);
                response_parts.push(format!("client_max_window_bits={}", final_client_bits));
            }
        } else if client_ext.has_param("client_max_window_bits") {
            // 客户端声明支持但未指定值
            response_parts.push(format!(
                "client_max_window_bits={}",
                self.config.client_max_window_bits
            ));
        }

        if self.config.client_no_context_takeover
            || client_ext.has_param("client_no_context_takeover")
        {
            response_parts.push("client_no_context_takeover".to_string());
        }

        NegotiationResult::Accepted(response_parts.join("; "))
    }
}

/// 压缩统计
#[derive(Debug, Clone, Default)]
pub struct CompressionStats {
    /// 压缩前的总字节数
    pub total_uncompressed: u64,
    /// 压缩后的总字节数
    pub total_compressed: u64,
    /// 已压缩的消息数
    pub messages_compressed: u64,
    /// 已解压的消息数
    pub messages_decompressed: u64,
}

impl CompressionStats {
    /// 平均压缩比（compressed / uncompressed，越小越好）
    pub fn ratio(&self) -> f64 {
        if self.total_uncompressed == 0 {
            return 1.0;
        }
        self.total_compressed as f64 / self.total_uncompressed as f64
    }

    /// 总节省字节数
    pub fn bytes_saved(&self) -> i64 {
        self.total_uncompressed as i64 - self.total_compressed as i64
    }

    /// 节省百分比（0.0-100.0）
    pub fn saved_percent(&self) -> f64 {
        if self.total_uncompressed == 0 {
            return 0.0;
        }
        let saved = self.total_uncompressed - self.total_compressed;
        (saved as f64 / self.total_uncompressed as f64) * 100.0
    }

    /// 重置统计
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// 使用 RLE 变体对字节序列进行简单压缩。
///
/// 压缩格式：对于连续重复的字节，输出 `[字节, 计数]`（计数最大 255）。
/// 对于不重复的字节，原样输出。为区分压缩与未压缩数据，
/// 压缩结果前缀一个标记字节 `0xFF`（假设原始数据不会以该标记+计数开头）。
///
/// 注意：这是简化实现，仅用于演示压缩流程与统计，不用于生产环境。
fn rle_compress(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return vec![0xFF];
    }
    let mut result = Vec::with_capacity(data.len());
    result.push(0xFF); // 压缩标记

    let mut i = 0;
    while i < data.len() {
        let current = data[i];
        let mut count = 1usize;
        while i + count < data.len()
            && data[i + count] == current
            && count < 255
        {
            count += 1;
        }
        result.push(current);
        result.push(count as u8);
        i += count;
    }
    result
}

/// 解压 RLE 压缩的数据
fn rle_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        return Err("empty compressed data".to_string());
    }
    if data[0] != 0xFF {
        return Err("invalid compression marker".to_string());
    }
    let mut result = Vec::new();
    let mut i = 1;
    while i + 1 < data.len() {
        let byte = data[i];
        let count = data[i + 1] as usize;
        result.resize(result.len() + count, byte);
        i += 2;
    }
    if i != data.len() {
        return Err("truncated compressed data".to_string());
    }
    Ok(result)
}

/// 消息压缩器，跟踪压缩统计
#[derive(Debug)]
pub struct MessageCompressor {
    config: CompressionConfig,
    stats: Arc<RwLock<CompressionStats>>,
}

impl MessageCompressor {
    /// 创建压缩器
    pub fn new(config: CompressionConfig) -> Self {
        Self {
            config,
            stats: Arc::new(RwLock::new(CompressionStats::default())),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &CompressionConfig {
        &self.config
    }

    /// 压缩消息。对于小消息（<32 字节）不压缩直接返回原文。
    pub async fn compress(&self, data: &[u8]) -> Vec<u8> {
        let uncompressed_size = data.len() as u64;

        // 小消息不压缩
        let compressed = if data.len() < 32 {
            data.to_vec()
        } else {
            let rle = rle_compress(data);
            // 仅当压缩后更小才使用压缩结果
            if rle.len() < data.len() {
                rle
            } else {
                data.to_vec()
            }
        };

        let compressed_size = compressed.len() as u64;
        let mut stats = self.stats.write().await;
        stats.total_uncompressed += uncompressed_size;
        stats.total_compressed += compressed_size;
        stats.messages_compressed += 1;

        compressed
    }

    /// 解压消息
    pub async fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let result = if !data.is_empty() && data[0] == 0xFF {
            rle_decompress(data)?
        } else {
            data.to_vec()
        };

        let mut stats = self.stats.write().await;
        stats.messages_decompressed += 1;

        Ok(result)
    }

    /// 获取当前压缩统计快照
    pub async fn stats(&self) -> CompressionStats {
        self.stats.read().await.clone()
    }

    /// 重置统计
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_config_default() {
        let cfg = CompressionConfig::default();
        assert_eq!(cfg.server_max_window_bits, 15);
        assert_eq!(cfg.client_max_window_bits, 15);
        assert!(!cfg.server_no_context_takeover);
        assert!(!cfg.client_no_context_takeover);
    }

    #[test]
    fn test_compression_config_builder() {
        let cfg = CompressionConfig::new()
            .with_server_window_bits(10)
            .with_client_window_bits(12)
            .with_server_no_context_takeover()
            .with_client_no_context_takeover();
        assert_eq!(cfg.server_max_window_bits, 10);
        assert_eq!(cfg.client_max_window_bits, 12);
        assert!(cfg.server_no_context_takeover);
        assert!(cfg.client_no_context_takeover);
    }

    #[test]
    fn test_compression_config_clamp_window_bits() {
        let cfg = CompressionConfig::new()
            .with_server_window_bits(2)
            .with_client_window_bits(20);
        assert_eq!(cfg.server_max_window_bits, 4);
        assert_eq!(cfg.client_max_window_bits, 15);
    }

    #[test]
    fn test_compression_config_validate_ok() {
        let cfg = CompressionConfig::new();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_compression_config_validate_invalid_server_bits() {
        let cfg = CompressionConfig {
            server_max_window_bits: 3,
            client_max_window_bits: 10,
            server_no_context_takeover: false,
            client_no_context_takeover: false,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_compression_config_validate_invalid_client_bits() {
        let cfg = CompressionConfig {
            server_max_window_bits: 10,
            client_max_window_bits: 16,
            server_no_context_takeover: false,
            client_no_context_takeover: false,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_to_extension_params() {
        let cfg = CompressionConfig::new()
            .with_server_window_bits(10)
            .with_server_no_context_takeover();
        let params = cfg.to_extension_params();
        assert!(params.starts_with("permessage-deflate"));
        assert!(params.contains("server_max_window_bits=10"));
        assert!(params.contains("server_no_context_takeover"));
    }

    #[test]
    fn test_client_extensions_parse_empty() {
        let ext = ClientExtensions::parse("");
        assert!(!ext.requests_deflate());
        assert!(ext.params.is_empty());
    }

    #[test]
    fn test_client_extensions_parse_with_values() {
        let ext = ClientExtensions::parse(
            "permessage-deflate; server_max_window_bits=10; client_max_window_bits",
        );
        assert!(ext.requests_deflate());
        assert_eq!(ext.get_param("server_max_window_bits"), Some("10"));
        assert!(ext.has_param("client_max_window_bits"));
        assert_eq!(ext.get_param("client_max_window_bits"), None);
    }

    #[test]
    fn test_client_extensions_parse_quoted_values() {
        let ext = ClientExtensions::parse("permessage-deflate; param=\"value\"");
        assert_eq!(ext.get_param("param"), Some("value"));
    }

    #[test]
    fn test_negotiator_not_requested_when_empty() {
        let neg = CompressionNegotiator::new(CompressionConfig::default());
        let result = neg.negotiate("");
        assert_eq!(result, NegotiationResult::NotRequested);
    }

    #[test]
    fn test_negotiator_not_requested_when_no_deflate() {
        let neg = CompressionNegotiator::new(CompressionConfig::default());
        let result = neg.negotiate("other-extension");
        assert_eq!(result, NegotiationResult::NotRequested);
    }

    #[test]
    fn test_negotiator_accepted_basic() {
        let neg = CompressionNegotiator::new(CompressionConfig::default());
        let result = neg.negotiate("permessage-deflate");
        match result {
            NegotiationResult::Accepted(resp) => {
                assert!(resp.contains("permessage-deflate"));
                assert!(resp.contains("server_max_window_bits=15"));
            }
            _ => panic!("expected Accepted, got {:?}", result),
        }
    }

    #[test]
    fn test_negotiator_accepted_with_client_window_bits() {
        let neg = CompressionNegotiator::new(CompressionConfig::default());
        let result = neg.negotiate("permessage-deflate; server_max_window_bits=10; client_max_window_bits=12");
        match result {
            NegotiationResult::Accepted(resp) => {
                assert!(resp.contains("server_max_window_bits=10"));
                assert!(resp.contains("client_max_window_bits=12"));
            }
            _ => panic!("expected Accepted, got {:?}", result),
        }
    }

    #[test]
    fn test_negotiator_takes_min_window_bits() {
        let neg = CompressionNegotiator::new(CompressionConfig::new().with_server_window_bits(8));
        // 客户端请求 server_max_window_bits=10，服务端配置为 8，应取 min=8
        let result = neg.negotiate("permessage-deflate; server_max_window_bits=10");
        match result {
            NegotiationResult::Accepted(resp) => {
                assert!(resp.contains("server_max_window_bits=8"));
            }
            _ => panic!("expected Accepted, got {:?}", result),
        }
    }

    #[test]
    fn test_negotiator_rejected_invalid_window_bits() {
        let neg = CompressionNegotiator::new(CompressionConfig::default());
        let result = neg.negotiate("permessage-deflate; server_max_window_bits=99");
        assert!(matches!(result, NegotiationResult::Rejected(_)));
    }

    #[test]
    fn test_negotiator_no_context_takeover_propagated() {
        let neg = CompressionNegotiator::new(
            CompressionConfig::new().with_server_no_context_takeover(),
        );
        let result = neg.negotiate("permessage-deflate");
        match result {
            NegotiationResult::Accepted(resp) => {
                assert!(resp.contains("server_no_context_takeover"));
            }
            _ => panic!("expected Accepted, got {:?}", result),
        }
    }

    #[test]
    fn test_negotiator_no_context_takeover_from_client() {
        let neg = CompressionNegotiator::new(CompressionConfig::default());
        let result = neg.negotiate(
            "permessage-deflate; server_no_context_takeover; client_no_context_takeover",
        );
        match result {
            NegotiationResult::Accepted(resp) => {
                assert!(resp.contains("server_no_context_takeover"));
                assert!(resp.contains("client_no_context_takeover"));
            }
            _ => panic!("expected Accepted, got {:?}", result),
        }
    }

    #[test]
    fn test_negotiator_client_window_bits_without_value() {
        let neg = CompressionNegotiator::new(CompressionConfig::default());
        let result = neg.negotiate("permessage-deflate; client_max_window_bits");
        match result {
            NegotiationResult::Accepted(resp) => {
                assert!(resp.contains("client_max_window_bits=15"));
            }
            _ => panic!("expected Accepted, got {:?}", result),
        }
    }

    #[test]
    fn test_compression_stats_default() {
        let stats = CompressionStats::default();
        assert_eq!(stats.total_uncompressed, 0);
        assert_eq!(stats.total_compressed, 0);
        assert_eq!(stats.messages_compressed, 0);
        assert_eq!(stats.messages_decompressed, 0);
        assert_eq!(stats.ratio(), 1.0);
        assert_eq!(stats.bytes_saved(), 0);
        assert_eq!(stats.saved_percent(), 0.0);
    }

    #[test]
    fn test_compression_stats_ratio() {
        let stats = CompressionStats {
            total_uncompressed: 1000,
            total_compressed: 400,
            messages_compressed: 5,
            messages_decompressed: 0,
        };
        assert!((stats.ratio() - 0.4).abs() < 1e-9);
        assert_eq!(stats.bytes_saved(), 600);
        assert!((stats.saved_percent() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn test_compression_stats_zero_uncompressed() {
        let stats = CompressionStats {
            total_uncompressed: 0,
            total_compressed: 100,
            messages_compressed: 1,
            messages_decompressed: 0,
        };
        assert_eq!(stats.ratio(), 1.0);
        assert_eq!(stats.saved_percent(), 0.0);
    }

    #[test]
    fn test_compression_stats_reset() {
        let mut stats = CompressionStats {
            total_uncompressed: 1000,
            total_compressed: 400,
            messages_compressed: 5,
            messages_decompressed: 3,
        };
        stats.reset();
        assert_eq!(stats.total_uncompressed, 0);
        assert_eq!(stats.messages_compressed, 0);
    }

    #[test]
    fn test_rle_compress_empty() {
        let compressed = rle_compress(b"");
        assert_eq!(compressed, vec![0xFF]);
    }

    #[test]
    fn test_rle_compress_repeated_bytes() {
        let data = b"aaaaabbbccc";
        let compressed = rle_compress(data);
        // [0xFF, 'a', 5, 'b', 3, 'c', 3]
        assert_eq!(compressed, vec![0xFF, b'a', 5, b'b', 3, b'c', 3]);
    }

    #[test]
    fn test_rle_compress_unique_bytes() {
        let data = b"abcdef";
        let compressed = rle_compress(data);
        // 每个字节重复 1 次
        assert_eq!(compressed.len(), 1 + data.len() * 2);
        assert_eq!(compressed[0], 0xFF);
    }

    #[test]
    fn test_rle_decompress_basic() {
        let compressed = vec![0xFF, b'a', 5, b'b', 3];
        let decompressed = rle_decompress(&compressed).unwrap();
        assert_eq!(decompressed, b"aaaaabbb");
    }

    #[test]
    fn test_rle_decompress_empty() {
        let result = rle_decompress(b"");
        assert!(result.is_err());
    }

    #[test]
    fn test_rle_decompress_invalid_marker() {
        let result = rle_decompress(b"\x00\x01\x02");
        assert!(result.is_err());
    }

    #[test]
    fn test_rle_decompress_truncated() {
        let compressed = vec![0xFF, b'a']; // 缺少计数
        let result = rle_decompress(&compressed);
        assert!(result.is_err());
    }

    #[test]
    fn test_rle_roundtrip() {
        let original = b"aaaabbbbbcccccccdde";
        let compressed = rle_compress(original);
        let decompressed = rle_decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[tokio::test]
    async fn test_compressor_compress_small_message() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        // 小消息不压缩
        let result = comp.compress(b"hi").await;
        assert_eq!(result, b"hi");

        let stats = comp.stats().await;
        assert_eq!(stats.messages_compressed, 1);
        assert_eq!(stats.total_uncompressed, 2);
        assert_eq!(stats.total_compressed, 2);
    }

    #[tokio::test]
    async fn test_compressor_compress_large_repeated() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        let data = vec![b'a'; 1000]; // 高度重复的数据
        let result = comp.compress(&data).await;
        // 应该被压缩（RLE 非常高效）
        assert!(result.len() < data.len());

        let stats = comp.stats().await;
        assert_eq!(stats.total_uncompressed, 1000);
        assert!(stats.total_compressed < 1000);
        assert!(stats.saved_percent() > 50.0);
    }

    #[tokio::test]
    async fn test_compressor_compress_incompressible() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        // 32 字节的随机不重复数据，RLE 不会更小
        let data: Vec<u8> = (0..32u8).collect();
        let result = comp.compress(&data).await;
        // 应该返回原文（压缩后更大则不压缩）
        assert_eq!(result, data);

        let stats = comp.stats().await;
        assert_eq!(stats.total_uncompressed, 32);
        assert_eq!(stats.total_compressed, 32);
    }

    #[tokio::test]
    async fn test_compressor_decompress_compressed() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        let original = vec![b'x'; 100];
        let compressed = comp.compress(&original).await;
        let decompressed = comp.decompress(&compressed).await.unwrap();
        assert_eq!(decompressed, original);
    }

    #[tokio::test]
    async fn test_compressor_decompress_uncompressed() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        // 小消息不压缩，直接解压
        let data = b"hello";
        let decompressed = comp.decompress(data).await.unwrap();
        assert_eq!(decompressed, data);
    }

    #[tokio::test]
    async fn test_compressor_stats_accumulate() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        let data = vec![b'a'; 100];
        comp.compress(&data).await;
        comp.compress(&data).await;
        comp.compress(&data).await;

        let stats = comp.stats().await;
        assert_eq!(stats.messages_compressed, 3);
        assert_eq!(stats.total_uncompressed, 300);
        assert!(stats.total_compressed < 300);
    }

    #[tokio::test]
    async fn test_compressor_reset_stats() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        let data = vec![b'a'; 100];
        comp.compress(&data).await;
        assert!(comp.stats().await.messages_compressed > 0);

        comp.reset_stats().await;
        let stats = comp.stats().await;
        assert_eq!(stats.messages_compressed, 0);
        assert_eq!(stats.total_uncompressed, 0);
    }

    #[tokio::test]
    async fn test_compressor_decompress_count() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        comp.decompress(b"hi").await.unwrap();
        comp.decompress(b"world").await.unwrap();

        let stats = comp.stats().await;
        assert_eq!(stats.messages_decompressed, 2);
    }

    #[tokio::test]
    async fn test_compressor_decompress_invalid_returns_error() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        // 0xFF 标记字节后跟单个数据字节但缺少计数字节 —— 截断的压缩数据
        let result = comp.decompress(&[0xFF, b'a']).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_compressor_roundtrip_preserves_data() {
        let comp = MessageCompressor::new(CompressionConfig::default());
        let original = vec![b'z'; 500];
        let compressed = comp.compress(&original).await;
        let decompressed = comp.decompress(&compressed).await.unwrap();
        assert_eq!(decompressed, original);

        let stats = comp.stats().await;
        assert_eq!(stats.messages_compressed, 1);
        assert_eq!(stats.messages_decompressed, 1);
    }
}
