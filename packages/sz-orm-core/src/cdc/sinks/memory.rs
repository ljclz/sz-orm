//! 内存 sink — 测试和缓冲用

use crate::cdc::checkpoint::CdcError;
use crate::cdc::dispatcher::CdcSink;
use crate::cdc::event::ChangeEvent;
use async_trait::async_trait;
use parking_lot::RwLock;

/// 统计 sink — 按事件类型计数
pub struct CountingSink {
    inserts: std::sync::atomic::AtomicU64,
    updates: std::sync::atomic::AtomicU64,
    deletes: std::sync::atomic::AtomicU64,
}

impl CountingSink {
    /// 创建空统计 sink
    pub fn new() -> Self {
        Self {
            inserts: std::sync::atomic::AtomicU64::new(0),
            updates: std::sync::atomic::AtomicU64::new(0),
            deletes: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// INSERT 计数
    pub fn inserts(&self) -> u64 {
        self.inserts.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// UPDATE 计数
    pub fn updates(&self) -> u64 {
        self.updates.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// DELETE 计数
    pub fn deletes(&self) -> u64 {
        self.deletes.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 总数
    pub fn total(&self) -> u64 {
        self.inserts() + self.updates() + self.deletes()
    }
}

impl Default for CountingSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CdcSink for CountingSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        use crate::cdc::event::ChangeEventType;
        use std::sync::atomic::Ordering;
        match event.event_type {
            ChangeEventType::Insert => {
                self.inserts.fetch_add(1, Ordering::Relaxed);
            }
            ChangeEventType::Update => {
                self.updates.fetch_add(1, Ordering::Relaxed);
            }
            ChangeEventType::Delete => {
                self.deletes.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(())
    }
}

/// 日志 sink — 记录事件到内存日志
pub struct LogSink {
    logs: RwLock<Vec<String>>,
}

impl LogSink {
    /// 创建空日志 sink
    pub fn new() -> Self {
        Self {
            logs: RwLock::new(Vec::new()),
        }
    }

    /// 获取日志条目
    pub fn logs(&self) -> Vec<String> {
        self.logs.read().clone()
    }
}

impl Default for LogSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CdcSink for LogSink {
    async fn handle(&self, event: &ChangeEvent) -> Result<(), CdcError> {
        let log = format!(
            "[{}] {} {}.{} (pos: {:?})",
            event.timestamp_ms,
            match event.event_type {
                crate::cdc::event::ChangeEventType::Insert => "INSERT",
                crate::cdc::event::ChangeEventType::Update => "UPDATE",
                crate::cdc::event::ChangeEventType::Delete => "DELETE",
            },
            event.source_db,
            event.source_table,
            event.position
        );
        self.logs.write().push(log);
        Ok(())
    }
}
