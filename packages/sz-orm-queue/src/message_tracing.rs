//! 消息轨迹追踪模块（v4.1.0，`message-tracing` feature gate）
//!
//! 提供消息轨迹追踪能力：采样率控制、消息内容脱敏、端到端关联。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 追踪上下文
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// 追踪 ID（端到端关联）
    pub trace_id: String,
    /// 跨度 ID
    pub span_id: String,
    /// 父跨度 ID
    pub parent_span_id: Option<String>,
    /// 时间戳（Unix 毫秒）
    pub timestamp: u64,
    /// 属性
    pub attributes: HashMap<String, String>,
}

impl TraceContext {
    /// 创建新的追踪上下文
    pub fn new(trace_id: String, span_id: String) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            attributes: HashMap::new(),
        }
    }

    /// 设置父跨度
    pub fn with_parent(mut self, parent: String) -> Self {
        self.parent_span_id = Some(parent);
        self
    }

    /// 添加属性
    pub fn with_attribute(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }
}

/// 采样策略
#[derive(Debug, Clone)]
pub struct SamplingStrategy {
    /// 采样率（0.0~1.0）
    pub rate: f64,
    /// 已采样计数
    sampled_count: Arc<AtomicU64>,
    /// 总计数
    total_count: Arc<AtomicU64>,
}

impl SamplingStrategy {
    /// 创建采样策略
    pub fn new(rate: f64) -> Self {
        Self {
            rate: rate.clamp(0.0, 1.0),
            sampled_count: Arc::new(AtomicU64::new(0)),
            total_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 判断是否采样
    pub fn should_sample(&self) -> bool {
        let total = self.total_count.fetch_add(1, Ordering::Relaxed) + 1;
        let threshold = (total as f64 * self.rate) as u64;
        let sampled = self.sampled_count.load(Ordering::Relaxed);
        if sampled < threshold {
            self.sampled_count.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// 获取采样统计
    pub fn stats(&self) -> (u64, u64) {
        (
            self.sampled_count.load(Ordering::Relaxed),
            self.total_count.load(Ordering::Relaxed),
        )
    }
}

/// 脱敏规则
#[derive(Debug, Clone)]
pub struct DesensitizeRule {
    /// 敏感字段名（大小写不敏感）
    pub field_name: String,
    /// 脱敏方式
    pub mode: DesensitizeMode,
}

/// 脱敏方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesensitizeMode {
    /// 完全遮蔽（****）
    FullMask,
    /// 部分遮蔽（保留首尾字符）
    PartialMask,
    /// 哈希脱敏
    Hash,
}

impl DesensitizeRule {
    /// 应用脱敏
    pub fn apply(&self, value: &str) -> String {
        match self.mode {
            DesensitizeMode::FullMask => "****".to_string(),
            DesensitizeMode::PartialMask => {
                if value.len() <= 2 {
                    "****".to_string()
                } else {
                    let chars: Vec<char> = value.chars().collect();
                    let first = chars.first().unwrap();
                    let last = chars.last().unwrap();
                    format!("{}****{}", first, last)
                }
            }
            DesensitizeMode::Hash => {
                let hash = value
                    .bytes()
                    .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
                format!("{:016x}", hash)
            }
        }
    }
}

/// 消息轨迹追踪器
pub struct MessageTracingInterceptor {
    /// 采样策略
    sampling: SamplingStrategy,
    /// 脱敏规则
    desensitize_rules: Vec<DesensitizeRule>,
    /// 追踪记录
    traces: std::sync::RwLock<Vec<TraceContext>>,
}

impl MessageTracingInterceptor {
    /// 创建追踪器
    pub fn new(sampling: SamplingStrategy, desensitize_rules: Vec<DesensitizeRule>) -> Self {
        Self {
            sampling,
            desensitize_rules,
            traces: std::sync::RwLock::new(Vec::new()),
        }
    }

    /// 记录消息轨迹
    pub fn record(&self, mut ctx: TraceContext, message: &HashMap<String, String>) -> bool {
        if !self.sampling.should_sample() {
            return false;
        }
        for rule in &self.desensitize_rules {
            if let Some(value) = message.get(&rule.field_name) {
                let masked = rule.apply(value);
                ctx.attributes.insert(rule.field_name.clone(), masked);
            }
        }
        self.traces.write().unwrap().push(ctx);
        true
    }

    /// 获取追踪记录数
    pub fn trace_count(&self) -> usize {
        self.traces.read().unwrap().len()
    }

    /// 按 trace_id 查询追踪
    pub fn find_by_trace_id(&self, trace_id: &str) -> Vec<TraceContext> {
        self.traces
            .read()
            .unwrap()
            .iter()
            .filter(|t| t.trace_id == trace_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_creation() {
        let ctx = TraceContext::new("trace-1".to_string(), "span-1".to_string());
        assert_eq!(ctx.trace_id, "trace-1");
        assert_eq!(ctx.span_id, "span-1");
        assert!(ctx.parent_span_id.is_none());
        assert!(ctx.timestamp > 0);
    }

    #[test]
    fn test_trace_context_with_parent() {
        let ctx = TraceContext::new("trace-1".to_string(), "span-2".to_string())
            .with_parent("span-1".to_string());
        assert_eq!(ctx.parent_span_id, Some("span-1".to_string()));
    }

    #[test]
    fn test_sampling_full() {
        let strategy = SamplingStrategy::new(1.0);
        for _ in 0..10 {
            assert!(strategy.should_sample());
        }
        let (sampled, total) = strategy.stats();
        assert_eq!(sampled, 10);
        assert_eq!(total, 10);
    }

    #[test]
    fn test_sampling_zero() {
        let strategy = SamplingStrategy::new(0.0);
        for _ in 0..10 {
            assert!(!strategy.should_sample());
        }
    }

    #[test]
    fn test_sampling_partial() {
        let strategy = SamplingStrategy::new(0.5);
        let mut sampled = 0;
        for _ in 0..100 {
            if strategy.should_sample() {
                sampled += 1;
            }
        }
        assert!(sampled > 30 && sampled < 70, "sampled: {}", sampled);
    }

    #[test]
    fn test_desensitize_full_mask() {
        let rule = DesensitizeRule {
            field_name: "password".to_string(),
            mode: DesensitizeMode::FullMask,
        };
        assert_eq!(rule.apply("secret123"), "****");
    }

    #[test]
    fn test_desensitize_partial_mask() {
        let rule = DesensitizeRule {
            field_name: "email".to_string(),
            mode: DesensitizeMode::PartialMask,
        };
        let masked = rule.apply("user@example.com");
        assert!(masked.starts_with('u'));
        assert!(masked.ends_with('m'));
        assert!(masked.contains("****"));
    }

    #[test]
    fn test_desensitize_hash() {
        let rule = DesensitizeRule {
            field_name: "token".to_string(),
            mode: DesensitizeMode::Hash,
        };
        let h1 = rule.apply("abc");
        let h2 = rule.apply("abc");
        let h3 = rule.apply("xyz");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn test_interceptor_record() {
        let interceptor = MessageTracingInterceptor::new(
            SamplingStrategy::new(1.0),
            vec![DesensitizeRule {
                field_name: "password".to_string(),
                mode: DesensitizeMode::FullMask,
            }],
        );

        let ctx = TraceContext::new("trace-1".to_string(), "span-1".to_string());
        let mut msg = HashMap::new();
        msg.insert("password".to_string(), "secret".to_string());
        msg.insert("user".to_string(), "admin".to_string());

        let recorded = interceptor.record(ctx, &msg);
        assert!(recorded);
        assert_eq!(interceptor.trace_count(), 1);
    }

    #[test]
    fn test_interceptor_find_by_trace_id() {
        let interceptor = MessageTracingInterceptor::new(SamplingStrategy::new(1.0), vec![]);

        let ctx1 = TraceContext::new("trace-1".to_string(), "span-1".to_string());
        let ctx2 = TraceContext::new("trace-1".to_string(), "span-2".to_string());
        let ctx3 = TraceContext::new("trace-2".to_string(), "span-3".to_string());
        let msg = HashMap::new();

        interceptor.record(ctx1, &msg);
        interceptor.record(ctx2, &msg);
        interceptor.record(ctx3, &msg);

        assert_eq!(interceptor.find_by_trace_id("trace-1").len(), 2);
        assert_eq!(interceptor.find_by_trace_id("trace-2").len(), 1);
        assert_eq!(interceptor.find_by_trace_id("trace-3").len(), 0);
    }

    #[test]
    fn test_interceptor_sampling_skip() {
        let interceptor = MessageTracingInterceptor::new(SamplingStrategy::new(0.0), vec![]);

        let ctx = TraceContext::new("trace-1".to_string(), "span-1".to_string());
        let msg = HashMap::new();
        let recorded = interceptor.record(ctx, &msg);
        assert!(!recorded);
        assert_eq!(interceptor.trace_count(), 0);
    }
}
