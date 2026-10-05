//! v8.0.0 任务 2.4：零拷贝反序列化
//!
//! 直接借用底层字节缓冲反序列化，消除 `Vec<u8>` 拷贝。
//! 复用既有 `ZeroCopyPipeline`（`zero_copy_pipeline.rs:314`）和
//! `BorrowedValue`（`value_borrowed.rs:17`）。
//! 类型不支持零拷贝 → 回退拷贝路径（标记未命中）。

use crate::perf_metrics::PerfMetrics;
use crate::value_borrowed::BorrowedValue;
use crate::zero_copy_pipeline::{ZeroCopyPipeline, ZeroCopyTypeRegistry};

/// 零拷贝反序列化配置
#[derive(Debug, Clone)]
pub struct ZeroCopyDeserConfig {
    /// 命中率目标（0.0 ~ 1.0）
    pub hit_rate_target: f64,
}

impl Default for ZeroCopyDeserConfig {
    fn default() -> Self {
        Self {
            hit_rate_target: 0.9,
        }
    }
}

/// 零拷贝反序列化器
///
/// 注入 `ZeroCopyTypeRegistry` + `ZeroCopyDeserConfig`，
/// 提供 `deserialize_borrowed<'a>` 方法，生命周期绑定输入 `buf`。
pub struct ZeroCopyDeserializer {
    registry: ZeroCopyTypeRegistry,
    pipeline: ZeroCopyPipeline,
    config: ZeroCopyDeserConfig,
    metrics: &'static PerfMetrics,
}

impl ZeroCopyDeserializer {
    /// 创建零拷贝反序列化器（内置类型注册表）
    pub fn new(config: ZeroCopyDeserConfig) -> Self {
        Self {
            registry: ZeroCopyTypeRegistry::with_builtins(),
            pipeline: ZeroCopyPipeline::new(),
            config,
            metrics: PerfMetrics::global(),
        }
    }

    /// 使用指定注册表构造
    pub fn with_registry(registry: ZeroCopyTypeRegistry, config: ZeroCopyDeserConfig) -> Self {
        Self {
            registry,
            pipeline: ZeroCopyPipeline::new(),
            config,
            metrics: PerfMetrics::global(),
        }
    }

    /// 获取配置引用
    pub fn config(&self) -> &ZeroCopyDeserConfig {
        &self.config
    }

    /// 零拷贝反序列化单行字节缓冲
    ///
    /// 直接借用 `buf` 的生命周期，返回 `BorrowedValue<'a>`。
    /// 类型支持零拷贝 → 命中（`BytesRef`/`String` 借用）；否则回退拷贝路径。
    pub fn deserialize_borrowed<'a>(&self, buf: &'a [u8]) -> BorrowedValue<'a> {
        if buf.is_empty() {
            self.metrics.record_zero_copy_miss();
            return BorrowedValue::Null;
        }

        // 简单协议：首字节为类型标记
        let type_tag = buf[0];
        let payload = &buf[1..];

        match type_tag {
            0x01 if self
                .registry
                .is_supported(crate::zero_copy_pipeline::ZeroCopyTypeId::Bool) =>
            {
                self.metrics.record_zero_copy_hit();
                BorrowedValue::Bool(payload.first().copied().unwrap_or(0) != 0)
            }
            0x02 if self
                .registry
                .is_supported(crate::zero_copy_pipeline::ZeroCopyTypeId::I64) =>
            {
                self.metrics.record_zero_copy_hit();
                let v = payload
                    .iter()
                    .take(8)
                    .fold(0i64, |acc, &b| (acc << 8) | b as i64);
                BorrowedValue::I64(v)
            }
            0x03 if self
                .registry
                .is_supported(crate::zero_copy_pipeline::ZeroCopyTypeId::F64) =>
            {
                self.metrics.record_zero_copy_hit();
                let v = payload
                    .iter()
                    .take(8)
                    .fold(0u64, |acc, &b| (acc << 8) | b as u64);
                BorrowedValue::F64(f64::from_bits(v))
            }
            0x04 if self
                .registry
                .is_supported(crate::zero_copy_pipeline::ZeroCopyTypeId::String) =>
            {
                self.metrics.record_zero_copy_hit();
                // 零拷贝：直接借用 buf 切片
                let s = std::str::from_utf8(payload).unwrap_or("");
                BorrowedValue::String(std::borrow::Cow::Borrowed(s))
            }
            0x05 if self
                .registry
                .is_supported(crate::zero_copy_pipeline::ZeroCopyTypeId::Bytes) =>
            {
                self.metrics.record_zero_copy_hit();
                BorrowedValue::BytesRef(payload)
            }
            _ => {
                self.metrics.record_zero_copy_miss();
                BorrowedValue::Bytes(std::borrow::Cow::Borrowed(buf))
            }
        }
    }

    /// 批量反序列化多行
    ///
    /// 输入为多行字节缓冲切片，返回每行对应的 `BorrowedValue`。
    pub fn deserialize_batch_borrowed<'a>(&self, rows: &'a [&'a [u8]]) -> Vec<BorrowedValue<'a>> {
        rows.iter().map(|r| self.deserialize_borrowed(r)).collect()
    }

    /// 获取零拷贝管线引用（用于统计）
    pub fn pipeline(&self) -> &ZeroCopyPipeline {
        &self.pipeline
    }

    /// 获取类型注册表引用
    pub fn registry(&self) -> &ZeroCopyTypeRegistry {
        &self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_borrowed::BorrowedValue;
    use crate::zero_copy_pipeline::ZeroCopyTypeId;

    #[test]
    fn deserialize_empty_buf_returns_null() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let val = deser.deserialize_borrowed(&[]);
        assert!(matches!(val, BorrowedValue::Null));
    }

    #[test]
    fn deserialize_bool_hit() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let buf = [0x01, 0x01];
        let val = deser.deserialize_borrowed(&buf);
        assert!(matches!(val, BorrowedValue::Bool(true)));
    }

    #[test]
    fn deserialize_string_borrowed_zero_copy() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let buf = [0x04, b'h', b'i'];
        let val = deser.deserialize_borrowed(&buf);
        match val {
            BorrowedValue::String(s) => assert_eq!(s, "hi"),
            _ => panic!("应为 String"),
        }
    }

    #[test]
    fn deserialize_bytes_ref_zero_copy() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let buf = [0x05, 0xAA, 0xBB, 0xCC];
        let val = deser.deserialize_borrowed(&buf);
        match val {
            BorrowedValue::BytesRef(b) => assert_eq!(b, &[0xAA, 0xBB, 0xCC]),
            _ => panic!("应为 BytesRef"),
        }
    }

    #[test]
    fn unsupported_type_falls_back_to_copy() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let buf = [0xFF, 0x01, 0x02];
        let val = deser.deserialize_borrowed(&buf);
        assert!(matches!(val, BorrowedValue::Bytes(_)));
    }

    #[test]
    fn batch_deserialize_1000_rows() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let rows: Vec<Vec<u8>> = (0..1000).map(|i| vec![0x04, b'v', i as u8]).collect();
        let row_refs: Vec<&[u8]> = rows.iter().map(|r| r.as_slice()).collect();
        let vals = deser.deserialize_batch_borrowed(&row_refs);
        assert_eq!(vals.len(), 1000);
        for v in &vals {
            assert!(matches!(v, BorrowedValue::String(_)));
        }
    }

    #[test]
    fn empty_registry_all_miss() {
        let deser = ZeroCopyDeserializer::with_registry(
            ZeroCopyTypeRegistry::empty(),
            ZeroCopyDeserConfig::default(),
        );
        let buf = [0x04, b'x'];
        let val = deser.deserialize_borrowed(&buf);
        assert!(matches!(val, BorrowedValue::Bytes(_)));
    }

    #[test]
    fn deserialize_i64_hit() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let mut buf = vec![0x02];
        buf.extend_from_slice(&42i64.to_be_bytes());
        let val = deser.deserialize_borrowed(&buf);
        assert!(matches!(val, BorrowedValue::I64(42)));
    }

    #[test]
    fn deserialize_f64_hit() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let mut buf = vec![0x03];
        buf.extend_from_slice(&3.25f64.to_be_bytes());
        let val = deser.deserialize_borrowed(&buf);
        match val {
            BorrowedValue::F64(v) => assert!((v - 3.25).abs() < 1e-9),
            _ => panic!("应为 F64"),
        }
    }

    #[test]
    fn deserialize_i64_partial_payload() {
        // payload 不足 8 字节时 take(8) 只取到末尾，高位为 0
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let buf = [0x02, 0x01];
        let val = deser.deserialize_borrowed(&buf);
        assert!(matches!(val, BorrowedValue::I64(1)));
    }

    #[test]
    fn deserialize_f64_partial_payload() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let buf = [0x03, 0x00];
        let val = deser.deserialize_borrowed(&buf);
        assert!(matches!(val, BorrowedValue::F64(0.0)));
    }

    #[test]
    fn deserialize_invalid_utf8_string_returns_empty() {
        // 非法 UTF-8 时 from_utf8 失败 → unwrap_or("") 返回空字符串借用
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let buf = [0x04, 0xFF, 0xFE];
        let val = deser.deserialize_borrowed(&buf);
        assert!(matches!(val, BorrowedValue::String(s) if s.is_empty()));
    }

    #[test]
    fn config_method_returns_reference() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        assert_eq!(deser.config().hit_rate_target, 0.9);
    }

    #[test]
    fn pipeline_method_returns_reference() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        // 多次调用返回同一实例引用
        assert!(std::ptr::eq(deser.pipeline(), deser.pipeline()));
    }

    #[test]
    fn registry_method_returns_reference() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        assert!(deser.registry().is_supported(ZeroCopyTypeId::Bool));
        assert!(deser.registry().is_supported(ZeroCopyTypeId::Bytes));
    }

    /// G22 覆盖率：空注册表下所有类型标记（Bool/I64/F64/String/Bytes）均应
    /// 回退拷贝路径（`_` 兜底分支），此前仅 String（0x04）被 `empty_registry_all_miss`
    /// 覆盖，其余 guard 的 `is_supported == false` 分支未执行。
    #[test]
    fn empty_registry_all_types_miss() {
        let deser = ZeroCopyDeserializer::with_registry(
            ZeroCopyTypeRegistry::empty(),
            ZeroCopyDeserConfig::default(),
        );
        for tag in [0x01u8, 0x02, 0x03, 0x04, 0x05] {
            let buf = [tag, 0x01];
            let val = deser.deserialize_borrowed(&buf);
            assert!(
                matches!(val, BorrowedValue::Bytes(_)),
                "tag {tag:#04x} 在空注册表下应回退拷贝路径"
            );
        }
    }

    /// G22 覆盖率：Bool 类型 payload 为空时走 `unwrap_or(0)` 兜底分支返回 false，
    /// 此前仅测试了 `[0x01, 0x01]`（payload 非空 → true）。
    #[test]
    fn deserialize_bool_empty_payload_returns_false() {
        let deser = ZeroCopyDeserializer::new(ZeroCopyDeserConfig::default());
        let buf = [0x01]; // 类型标记后无 payload
        let val = deser.deserialize_borrowed(&buf);
        assert!(matches!(val, BorrowedValue::Bool(false)));
    }
}
