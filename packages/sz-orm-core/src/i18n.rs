//! # 国际化（i18n）支持
//!
//! 提供错误消息和日志消息的多语言框架。
//!
//! ## 设计目标
//!
//! - **向后兼容**：默认中文消息，不破坏现有 API
//! - **可选启用**：使用方可注册自定义语言包
//! - **零开销**：未注册语言包时直接返回默认消息
//! - **线程安全**：使用 `RwLock` 保护语言目录
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use sz_orm_core::i18n::{MessageCatalog, MessageKey, set_catalog, translate};
//!
//! // 1. 注册英文语言包
//! let mut catalog = MessageCatalog::new();
//! catalog.insert(MessageKey::ConnectionFailed, "Connection failed: {0}");
//! set_catalog(catalog);
//!
//! // 2. 翻译消息（无注册时返回默认中文）
//! let msg = translate(MessageKey::ConnectionFailed, &["timeout"]);
//! ```
//!
//! ## 当前状态
//!
//! - 提供 `MessageKey` 枚举覆盖核心错误类型
//! - 默认中文消息硬编码在 `MessageKey::default_msg()` 中
//! - 使用方可通过 `set_catalog()` 注册自定义翻译
//! - 后续版本将逐步迁移现有中文硬编码消息到 `MessageKey`

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// 消息键枚举
///
/// 覆盖 sz-orm-core 的核心错误类型。
/// 后续版本将逐步扩展。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageKey {
    /// 连接失败
    ConnectionFailed,
    /// 连接超时
    ConnectionTimeout,
    /// 查询错误
    QueryError,
    /// 未找到
    NotFound,
    /// 约束违反
    ConstraintViolation,
    /// 连接池耗尽
    PoolExhausted,
    /// 连接池超时
    PoolTimeout,
    /// 事务未启动
    TxNotStarted,
    /// 事务提交失败
    TxCommitFailed,
    /// 事务回滚失败
    TxRollbackFailed,
    /// 缓存未命中
    CacheMiss,
    /// 缓存写入失败
    CacheWriteFailed,
    /// SQL 注入检测
    SqlInjectionDetected,
    /// 参数绑定缺失
    MissingParameter,
    /// 类型转换失败
    TypeMismatch,
    /// 自定义消息（向后兼容）
    Custom,
}

impl MessageKey {
    /// 获取默认中文消息
    pub fn default_msg(self) -> &'static str {
        match self {
            MessageKey::ConnectionFailed => "连接失败",
            MessageKey::ConnectionTimeout => "连接超时",
            MessageKey::QueryError => "查询错误",
            MessageKey::NotFound => "未找到",
            MessageKey::ConstraintViolation => "约束违反",
            MessageKey::PoolExhausted => "连接池耗尽",
            MessageKey::PoolTimeout => "连接池超时",
            MessageKey::TxNotStarted => "事务未启动",
            MessageKey::TxCommitFailed => "事务提交失败",
            MessageKey::TxRollbackFailed => "事务回滚失败",
            MessageKey::CacheMiss => "缓存未命中",
            MessageKey::CacheWriteFailed => "缓存写入失败",
            MessageKey::SqlInjectionDetected => "检测到 SQL 注入",
            MessageKey::MissingParameter => "参数绑定缺失",
            MessageKey::TypeMismatch => "类型转换失败",
            MessageKey::Custom => "",
        }
    }
}

/// 消息目录（语言包）
///
/// 存储 `MessageKey` 到翻译消息的映射。
/// 翻译消息可包含 `{0}`、`{1}` 等位置占位符。
pub type MessageCatalog = HashMap<MessageKey, String>;

/// 全局消息目录（OnceLock + RwLock）
static CATALOG: OnceLock<RwLock<MessageCatalog>> = OnceLock::new();

/// 获取全局消息目录的只读锁
fn catalog() -> &'static RwLock<MessageCatalog> {
    CATALOG.get_or_init(|| RwLock::new(MessageCatalog::new()))
}

/// 设置全局消息目录
///
/// 覆盖现有目录。通常在应用启动时调用一次。
pub fn set_catalog(new_catalog: MessageCatalog) {
    let mut guard = catalog().write().expect("i18n catalog poisoned");
    *guard = new_catalog;
}

/// 注册单条翻译
///
/// 向现有目录添加或覆盖单条翻译。
pub fn register(key: MessageKey, msg: impl Into<String>) {
    let mut guard = catalog().write().expect("i18n catalog poisoned");
    guard.insert(key, msg.into());
}

/// 清空全局消息目录
///
/// 恢复默认中文消息。
pub fn clear() {
    let mut guard = catalog().write().expect("i18n catalog poisoned");
    guard.clear();
}

/// 翻译消息
///
/// 若目录中存在翻译，则使用翻译并用 `args` 替换 `{0}`、`{1}` 等占位符；
/// 否则返回 `key.default_msg()`。
pub fn translate(key: MessageKey, args: &[&str]) -> String {
    let guard = catalog().read().expect("i18n catalog poisoned");
    if let Some(template) = guard.get(&key) {
        format_args(template, args)
    } else {
        key.default_msg().to_string()
    }
}

/// 格式化占位符
///
/// 将 `{0}`、`{1}` 等替换为 `args` 中对应索引的字符串。
/// 越界索引保留原占位符。
fn format_args(template: &str, args: &[&str]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut idx_str = String::new();
            while let Some(&next) = chars.peek() {
                if next == '}' {
                    chars.next();
                    break;
                }
                idx_str.push(next);
                chars.next();
            }
            if let Ok(idx) = idx_str.parse::<usize>() {
                if let Some(arg) = args.get(idx) {
                    result.push_str(arg);
                } else {
                    result.push('{');
                    result.push_str(&idx_str);
                    result.push('}');
                }
            } else {
                result.push('{');
                result.push_str(&idx_str);
                result.push('}');
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_message() {
        assert_eq!(MessageKey::ConnectionFailed.default_msg(), "连接失败");
        assert_eq!(MessageKey::QueryError.default_msg(), "查询错误");
    }

    #[test]
    fn test_translate_default() {
        clear();
        let msg = translate(MessageKey::ConnectionFailed, &[]);
        assert_eq!(msg, "连接失败");
    }

    #[test]
    fn test_translate_with_catalog() {
        clear();
        let mut catalog = MessageCatalog::new();
        catalog.insert(
            MessageKey::ConnectionFailed,
            "Connection failed: {0}".to_string(),
        );
        set_catalog(catalog);
        let msg = translate(MessageKey::ConnectionFailed, &["timeout"]);
        assert_eq!(msg, "Connection failed: timeout");
        clear();
    }

    #[test]
    fn test_register_single() {
        clear();
        register(MessageKey::NotFound, "Not found");
        let msg = translate(MessageKey::NotFound, &[]);
        assert_eq!(msg, "Not found");
        clear();
    }

    #[test]
    fn test_format_args_out_of_bounds() {
        let result = format_args("Hello {0} {1}", &["world"]);
        assert_eq!(result, "Hello world {1}");
    }

    #[test]
    fn test_format_args_no_placeholders() {
        let result = format_args("Hello world", &[]);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_format_args_invalid_index() {
        let result = format_args("Hello {abc}", &[]);
        assert_eq!(result, "Hello {abc}");
    }

    #[test]
    fn test_clear() {
        register(MessageKey::QueryError, "Query error");
        assert_eq!(translate(MessageKey::QueryError, &[]), "Query error");
        clear();
        assert_eq!(translate(MessageKey::QueryError, &[]), "查询错误");
    }
}
