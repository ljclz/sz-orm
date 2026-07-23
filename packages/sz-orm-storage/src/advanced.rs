//! # 高级存储功能
//!
//! 提供分片上传（Multipart Upload）、断点续传、存储桶生命周期管理、
//! CDN 刷新等高级对象存储能力。所有实现均为纯内存模型，可在不依赖真实云服务的情况下进行单元测试。

use crate::error::StorageError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 全局上传 ID 计数器，确保即使在同一纳秒内发起的多次上传也有唯一 ID
static UPLOAD_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 全局刷新请求 ID 计数器
static REFRESH_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

// ====================================================================
// 分片上传（Multipart Upload）
// ====================================================================

/// 单个分片的状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    /// 分片编号（从 1 开始）
    pub number: u32,
    /// 分片数据
    pub data: Vec<u8>,
    /// 分片大小（字节）
    pub size: usize,
    /// 分片 ETag（简单实现用分片编号的哈希）
    pub etag: String,
}

impl Part {
    pub fn new(number: u32, data: Vec<u8>) -> Self {
        let size = data.len();
        let etag = format!("etag-{:x}-{:x}", number, size);
        Self {
            number,
            data,
            size,
            etag,
        }
    }
}

/// 分片上传状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadStatus {
    /// 已初始化，等待上传分片
    Initiated,
    /// 正在上传分片
    InProgress,
    /// 已完成
    Completed,
    /// 已中止
    Aborted,
}

/// 分片上传会话
#[derive(Debug, Serialize, Deserialize)]
pub struct MultipartUpload {
    /// 上传 ID
    pub upload_id: String,
    /// 目标对象 key
    pub key: String,
    /// Content-Type
    pub content_type: String,
    /// 已上传的分片列表
    pub parts: Vec<Part>,
    /// 预期的总分片数
    pub expected_parts: u32,
    /// 每个分片的大小阈值（字节）
    pub part_size: usize,
    /// 上传状态
    pub status: UploadStatus,
    /// 创建时间戳（秒）
    pub created_at: u64,
}

impl MultipartUpload {
    /// 创建新的分片上传会话
    pub fn new(key: impl Into<String>, content_type: impl Into<String>, part_size: usize) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let counter = UPLOAD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            upload_id: format!("upload-{}-{}", now, counter),
            key: key.into(),
            content_type: content_type.into(),
            parts: Vec::new(),
            expected_parts: 0,
            part_size,
            status: UploadStatus::Initiated,
            created_at: now,
        }
    }

    /// 上传一个分片
    pub fn upload_part(&mut self, number: u32, data: Vec<u8>) -> Result<String, StorageError> {
        if self.status == UploadStatus::Completed || self.status == UploadStatus::Aborted {
            return Err(StorageError::Put(format!(
                "upload {} already {:?}",
                self.upload_id, self.status
            )));
        }
        // 分片编号必须连续且递增
        let next_number = self.parts.last().map(|p| p.number + 1).unwrap_or(1);
        if number != next_number {
            return Err(StorageError::Put(format!(
                "expected part {}, got {}",
                next_number, number
            )));
        }
        // 分片大小检查（除最后一个分片外必须等于 part_size）
        if number < self.expected_parts && data.len() != self.part_size {
            return Err(StorageError::Put(format!(
                "part {} size {} != expected {}",
                number,
                data.len(),
                self.part_size
            )));
        }
        let part = Part::new(number, data);
        let etag = part.etag.clone();
        self.parts.push(part);
        self.status = UploadStatus::InProgress;
        Ok(etag)
    }

    /// 完成分片上传，返回合并后的完整数据
    pub fn complete(&mut self) -> Result<Vec<u8>, StorageError> {
        if self.status == UploadStatus::Completed {
            return Err(StorageError::Put(format!(
                "upload {} already completed",
                self.upload_id
            )));
        }
        if self.status == UploadStatus::Aborted {
            return Err(StorageError::Put(format!(
                "upload {} was aborted",
                self.upload_id
            )));
        }
        if self.parts.is_empty() {
            return Err(StorageError::Put(format!(
                "upload {} has no parts",
                self.upload_id
            )));
        }
        // 验证分片编号连续
        for (idx, part) in self.parts.iter().enumerate() {
            if part.number as usize != idx + 1 {
                return Err(StorageError::Put(format!(
                    "part numbering gap: expected {}, got {}",
                    idx + 1,
                    part.number
                )));
            }
        }
        let mut combined = Vec::new();
        for part in &self.parts {
            combined.extend_from_slice(&part.data);
        }
        self.status = UploadStatus::Completed;
        Ok(combined)
    }

    /// 中止分片上传
    pub fn abort(&mut self) -> Result<(), StorageError> {
        if self.status == UploadStatus::Completed {
            return Err(StorageError::Put(format!(
                "upload {} already completed, cannot abort",
                self.upload_id
            )));
        }
        self.status = UploadStatus::Aborted;
        self.parts.clear();
        Ok(())
    }

    /// 返回已上传分片数量
    pub fn uploaded_part_count(&self) -> usize {
        self.parts.len()
    }

    /// 返回已上传字节数
    pub fn uploaded_bytes(&self) -> usize {
        self.parts.iter().map(|p| p.size).sum()
    }

    /// 返回上传进度百分比（0-100）
    pub fn progress_percent(&self) -> u8 {
        if self.expected_parts == 0 {
            return if self.status == UploadStatus::Completed {
                100
            } else {
                0
            };
        }
        ((self.parts.len() as f64 / self.expected_parts as f64) * 100.0) as u8
    }

    /// 判断上传是否已完成
    pub fn is_completed(&self) -> bool {
        self.status == UploadStatus::Completed
    }
}

// ====================================================================
// 断点续传（Resumable Upload）
// ====================================================================

/// 断点续传管理器：持久化分片上传状态，支持中断后恢复
pub struct ResumableUploadManager {
    /// 所有活跃的上传会话（upload_id -> MultipartUpload）
    sessions: Mutex<HashMap<String, MultipartUpload>>,
    /// 已完成上传的合并数据缓存（key -> data）
    completed: Mutex<HashMap<String, Vec<u8>>>,
}

impl ResumableUploadManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            completed: Mutex::new(HashMap::new()),
        }
    }

    /// 发起分片上传
    pub fn initiate(
        &self,
        key: &str,
        content_type: &str,
        total_size: usize,
        part_size: usize,
    ) -> Result<String, StorageError> {
        let mut upload = MultipartUpload::new(key, content_type, part_size);
        upload.expected_parts = total_size.div_ceil(part_size) as u32;
        let upload_id = upload.upload_id.clone();
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| StorageError::Connection(format!("lock error: {}", e)))?;
        sessions.insert(upload_id.clone(), upload);
        Ok(upload_id)
    }

    /// 上传单个分片
    pub fn upload_part(
        &self,
        upload_id: &str,
        number: u32,
        data: Vec<u8>,
    ) -> Result<String, StorageError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| StorageError::Connection(format!("lock error: {}", e)))?;
        let upload = sessions
            .get_mut(upload_id)
            .ok_or_else(|| StorageError::NotFound(format!("upload {}", upload_id)))?;
        upload.upload_part(number, data)
    }

    /// 完成上传
    pub fn complete(&self, upload_id: &str) -> Result<Vec<u8>, StorageError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| StorageError::Connection(format!("lock error: {}", e)))?;
        let upload = sessions
            .get_mut(upload_id)
            .ok_or_else(|| StorageError::NotFound(format!("upload {}", upload_id)))?;
        let combined = upload.complete()?;
        let key = upload.key.clone();
        // 缓存完成的数据
        if let Ok(mut completed) = self.completed.lock() {
            completed.insert(key, combined.clone());
        }
        Ok(combined)
    }

    /// 中止上传
    pub fn abort(&self, upload_id: &str) -> Result<(), StorageError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| StorageError::Connection(format!("lock error: {}", e)))?;
        let upload = sessions
            .get_mut(upload_id)
            .ok_or_else(|| StorageError::NotFound(format!("upload {}", upload_id)))?;
        upload.abort()
    }

    /// 获取上传会话状态（用于断点续传查询已上传分片）
    pub fn get_session(&self, upload_id: &str) -> Option<MultipartUploadSnapshot> {
        let sessions = self.sessions.lock().ok()?;
        let upload = sessions.get(upload_id)?;
        Some(MultipartUploadSnapshot {
            upload_id: upload.upload_id.clone(),
            key: upload.key.clone(),
            content_type: upload.content_type.clone(),
            uploaded_part_numbers: upload.parts.iter().map(|p| p.number).collect(),
            expected_parts: upload.expected_parts,
            part_size: upload.part_size,
            status: upload.status,
            uploaded_bytes: upload.uploaded_bytes(),
            progress_percent: upload.progress_percent(),
        })
    }

    /// 列出所有活跃的上传会话 ID（不包含已完成或已中止的）
    pub fn list_uploads(&self) -> Vec<String> {
        self.sessions
            .lock()
            .map(|s| {
                s.iter()
                    .filter(|(_, u)| {
                        u.status != UploadStatus::Completed && u.status != UploadStatus::Aborted
                    })
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取已完成上传的数据
    pub fn get_completed_data(&self, key: &str) -> Option<Vec<u8>> {
        self.completed
            .lock()
            .ok()
            .and_then(|c| c.get(key).cloned())
    }

    /// 清理已完成或已中止的会话
    pub fn cleanup(&self) -> usize {
        let mut sessions = match self.sessions.lock() {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let before = sessions.len();
        sessions.retain(|_, u| u.status == UploadStatus::Initiated || u.status == UploadStatus::InProgress);
        before - sessions.len()
    }
}

impl Default for ResumableUploadManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 分片上传状态快照（用于序列化和断点续传恢复）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUploadSnapshot {
    pub upload_id: String,
    pub key: String,
    pub content_type: String,
    /// 已上传的分片编号列表
    pub uploaded_part_numbers: Vec<u32>,
    pub expected_parts: u32,
    pub part_size: usize,
    pub status: UploadStatus,
    pub uploaded_bytes: usize,
    pub progress_percent: u8,
}

// ====================================================================
// 存储桶生命周期管理（Bucket Lifecycle）
// ====================================================================

/// 生命周期动作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LifecycleAction {
    /// 转换存储类别（如 Standard -> IA -> Archive）
    Transition {
        /// 转换到的存储类别
        storage_class: String,
        /// 对象创建后多少天执行
        days: u32,
    },
    /// 过期删除
    Expiration {
        /// 对象创建后多少天执行
        days: u32,
    },
    /// 删除未完成的分片上传
    AbortIncompleteMultipartUpload {
        /// 发起后多少天执行
        days: u32,
    },
}

/// 生命周期规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRule {
    /// 规则 ID
    pub id: String,
    /// 规则是否启用
    pub enabled: bool,
    /// 规则匹配的前缀（空表示匹配所有）
    pub prefix: String,
    /// 规则动作
    pub action: LifecycleAction,
}

impl LifecycleRule {
    pub fn new(id: impl Into<String>, action: LifecycleAction) -> Self {
        Self {
            id: id.into(),
            enabled: true,
            prefix: String::new(),
            action,
        }
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 判断给定 key 是否匹配此规则的前缀
    pub fn matches(&self, key: &str) -> bool {
        self.enabled && (self.prefix.is_empty() || key.starts_with(&self.prefix))
    }
}

/// 存储桶生命周期管理器
pub struct BucketLifecycle {
    /// 规则列表
    rules: Vec<LifecycleRule>,
}

impl BucketLifecycle {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 添加生命周期规则
    pub fn add_rule(&mut self, rule: LifecycleRule) {
        self.rules.push(rule);
    }

    /// 移除指定 ID 的规则
    pub fn remove_rule(&mut self, id: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != id);
        self.rules.len() < before
    }

    /// 返回所有规则
    pub fn rules(&self) -> &[LifecycleRule] {
        &self.rules
    }

    /// 返回匹配指定 key 的所有规则
    pub fn matching_rules(&self, key: &str) -> Vec<&LifecycleRule> {
        self.rules.iter().filter(|r| r.matches(key)).collect()
    }

    /// 根据对象创建时间和当前时间，计算需要执行的动作
    pub fn evaluate(
        &self,
        key: &str,
        object_age_days: u32,
        has_incomplete_upload: bool,
    ) -> Vec<LifecycleEvaluationResult> {
        let mut results = Vec::new();
        for rule in self.matching_rules(key) {
            match &rule.action {
                LifecycleAction::Transition {
                    storage_class,
                    days,
                } => {
                    if object_age_days >= *days {
                        results.push(LifecycleEvaluationResult {
                            rule_id: rule.id.clone(),
                            key: key.to_string(),
                            action: LifecycleAction::Transition {
                                storage_class: storage_class.clone(),
                                days: *days,
                            },
                        });
                    }
                }
                LifecycleAction::Expiration { days } => {
                    if object_age_days >= *days {
                        results.push(LifecycleEvaluationResult {
                            rule_id: rule.id.clone(),
                            key: key.to_string(),
                            action: LifecycleAction::Expiration { days: *days },
                        });
                    }
                }
                LifecycleAction::AbortIncompleteMultipartUpload { days } => {
                    if has_incomplete_upload {
                        results.push(LifecycleEvaluationResult {
                            rule_id: rule.id.clone(),
                            key: key.to_string(),
                            action: LifecycleAction::AbortIncompleteMultipartUpload { days: *days },
                        });
                    }
                }
            }
        }
        results
    }

    /// 返回规则数量
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 启用指定规则
    pub fn enable_rule(&mut self, id: &str) -> bool {
        let mut found = false;
        for rule in &mut self.rules {
            if rule.id == id {
                rule.enabled = true;
                found = true;
            }
        }
        found
    }

    /// 禁用指定规则
    pub fn disable_rule(&mut self, id: &str) -> bool {
        let mut found = false;
        for rule in &mut self.rules {
            if rule.id == id {
                rule.enabled = false;
                found = true;
            }
        }
        found
    }
}

impl Default for BucketLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

/// 生命周期评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvaluationResult {
    pub rule_id: String,
    pub key: String,
    pub action: LifecycleAction,
}

// ====================================================================
// CDN 刷新（CDN Refresh / Purge）
// ====================================================================

/// CDN 刷新请求类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefreshType {
    /// 刷新单个 URL
    Url,
    /// 刷新整个目录
    Directory,
}

/// CDN 刷新请求状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefreshStatus {
    /// 已提交，处理中
    Pending,
    /// 已完成
    Done,
    /// 失败
    Failed,
}

/// CDN 刷新请求记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshRequest {
    /// 请求 ID
    pub request_id: String,
    /// 刷新类型
    pub refresh_type: RefreshType,
    /// 刷新目标列表
    pub targets: Vec<String>,
    /// 提交时间戳（秒）
    pub submitted_at: u64,
    /// 状态
    pub status: RefreshStatus,
    /// 失败原因（如果有）
    pub error: Option<String>,
}

impl RefreshRequest {
    pub fn new(refresh_type: RefreshType, targets: Vec<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let counter = REFRESH_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            request_id: format!("refresh-{}-{}", now, counter),
            refresh_type,
            targets,
            submitted_at: now,
            status: RefreshStatus::Pending,
            error: None,
        }
    }
}

/// CDN 刷新器：提交并跟踪 CDN 缓存刷新请求
pub struct CdnRefresher {
    /// 刷新历史记录
    history: Mutex<Vec<RefreshRequest>>,
    /// 刷新请求速率限制：最近窗口内的请求数
    rate_limit: Mutex<Vec<u64>>,
    /// 每分钟最大刷新请求数
    pub max_requests_per_minute: u32,
    /// 每次请求最大 URL/目录数量
    pub max_targets_per_request: u32,
}

impl CdnRefresher {
    pub fn new() -> Self {
        Self {
            history: Mutex::new(Vec::new()),
            rate_limit: Mutex::new(Vec::new()),
            max_requests_per_minute: 100,
            max_targets_per_request: 1000,
        }
    }

    /// 自定义速率限制
    pub fn with_rate_limit(mut self, max_per_minute: u32) -> Self {
        self.max_requests_per_minute = max_per_minute;
        self
    }

    /// 提交 URL 刷新请求
    pub fn refresh_urls(&self, urls: Vec<String>) -> Result<String, StorageError> {
        self.submit(RefreshType::Url, urls)
    }

    /// 提交目录刷新请求
    pub fn refresh_dirs(&self, dirs: Vec<String>) -> Result<String, StorageError> {
        self.submit(RefreshType::Directory, dirs)
    }

    /// 内部提交方法
    fn submit(
        &self,
        refresh_type: RefreshType,
        targets: Vec<String>,
    ) -> Result<String, StorageError> {
        if targets.is_empty() {
            return Err(StorageError::InvalidConfig(
                "refresh targets cannot be empty".to_string(),
            ));
        }
        if targets.len() as u32 > self.max_targets_per_request {
            return Err(StorageError::InvalidConfig(format!(
                "too many targets: {} > {}",
                targets.len(),
                self.max_targets_per_request
            )));
        }
        // 速率限制检查
        self.check_rate_limit()?;
        let mut request = RefreshRequest::new(refresh_type, targets);
        // 模拟异步处理：立即标记为 Done
        request.status = RefreshStatus::Done;
        let request_id = request.request_id.clone();
        let mut history = self
            .history
            .lock()
            .map_err(|e| StorageError::Connection(format!("lock error: {}", e)))?;
        history.push(request);
        Ok(request_id)
    }

    /// 速率限制检查
    fn check_rate_limit(&self) -> Result<(), StorageError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut rate = self
            .rate_limit
            .lock()
            .map_err(|e| StorageError::Connection(format!("lock error: {}", e)))?;
        // 移除 60 秒前的记录
        rate.retain(|&t| now - t < 60);
        if rate.len() as u32 >= self.max_requests_per_minute {
            return Err(StorageError::InvalidConfig(format!(
                "rate limit exceeded: {} requests in last minute",
                rate.len()
            )));
        }
        rate.push(now);
        Ok(())
    }

    /// 查询刷新请求状态
    pub fn get_status(&self, request_id: &str) -> Option<RefreshStatus> {
        let history = self.history.lock().ok()?;
        history
            .iter()
            .find(|r| r.request_id == request_id)
            .map(|r| r.status)
    }

    /// 获取刷新请求详情
    pub fn get_request(&self, request_id: &str) -> Option<RefreshRequest> {
        let history = self.history.lock().ok()?;
        history
            .iter()
            .find(|r| r.request_id == request_id)
            .cloned()
    }

    /// 返回所有刷新历史
    pub fn history(&self) -> Vec<RefreshRequest> {
        self.history
            .lock()
            .map(|h| h.clone())
            .unwrap_or_default()
    }

    /// 返回刷新请求总数
    pub fn total_requests(&self) -> usize {
        self.history
            .lock()
            .map(|h| h.len())
            .unwrap_or(0)
    }

    /// 返回最近 N 秒内的刷新请求数
    pub fn requests_in_last(&self, seconds: u64) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.history
            .lock()
            .map(|h| {
                h.iter()
                    .filter(|r| now - r.submitted_at < seconds)
                    .count()
            })
            .unwrap_or(0)
    }

    /// 预热 URL（将内容推送到 CDN 节点）
    pub fn prefetch(&self, urls: Vec<String>) -> Result<String, StorageError> {
        if urls.is_empty() {
            return Err(StorageError::InvalidConfig(
                "prefetch urls cannot be empty".to_string(),
            ));
        }
        self.check_rate_limit()?;
        let mut request = RefreshRequest::new(RefreshType::Url, urls);
        request.status = RefreshStatus::Done;
        let request_id = request.request_id.clone();
        let mut history = self
            .history
            .lock()
            .map_err(|e| StorageError::Connection(format!("lock error: {}", e)))?;
        history.push(request);
        Ok(request_id)
    }
}

impl Default for CdnRefresher {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// 辅助函数
// ====================================================================

/// 计算对象年龄（天）
pub fn object_age_days(created_at: SystemTime) -> u32 {
    let now = SystemTime::now();
    match now.duration_since(created_at) {
        Ok(d) => (d.as_secs() / 86400) as u32,
        Err(_) => 0,
    }
}

/// 将字节大小格式化为人类可读字符串
pub fn format_size(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    format!("{:.2} {}", size, UNITS[unit_idx])
}

/// 计算上传预估剩余时间（秒），基于已上传字节数和耗时
pub fn estimate_remaining_seconds(
    uploaded_bytes: usize,
    total_bytes: usize,
    elapsed: Duration,
) -> Option<u64> {
    if uploaded_bytes == 0 || total_bytes == 0 {
        return None;
    }
    let elapsed_secs = elapsed.as_secs();
    if elapsed_secs == 0 {
        return None;
    }
    let speed = uploaded_bytes as f64 / elapsed_secs as f64;
    if speed < 1.0 {
        return None;
    }
    let remaining = (total_bytes - uploaded_bytes) as f64 / speed;
    Some(remaining as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ====================================================================
    // MultipartUpload 测试
    // ====================================================================

    #[test]
    fn test_multipart_upload_init() {
        let upload = MultipartUpload::new("test.txt", "text/plain", 1024);
        assert!(!upload.upload_id.is_empty());
        assert_eq!(upload.key, "test.txt");
        assert_eq!(upload.content_type, "text/plain");
        assert_eq!(upload.part_size, 1024);
        assert_eq!(upload.status, UploadStatus::Initiated);
        assert_eq!(upload.uploaded_part_count(), 0);
        assert_eq!(upload.uploaded_bytes(), 0);
    }

    #[test]
    fn test_multipart_upload_single_part() {
        let mut upload = MultipartUpload::new("file.txt", "text/plain", 100);
        upload.expected_parts = 1;
        let etag = upload.upload_part(1, b"hello".to_vec()).unwrap();
        assert!(!etag.is_empty());
        assert_eq!(upload.uploaded_part_count(), 1);
        assert_eq!(upload.uploaded_bytes(), 5);
        assert_eq!(upload.status, UploadStatus::InProgress);

        let combined = upload.complete().unwrap();
        assert_eq!(combined, b"hello");
        assert_eq!(upload.status, UploadStatus::Completed);
        assert!(upload.is_completed());
    }

    #[test]
    fn test_multipart_upload_multiple_parts() {
        let mut upload = MultipartUpload::new("big.bin", "application/octet-stream", 10);
        upload.expected_parts = 3;
        upload.upload_part(1, vec![0u8; 10]).unwrap();
        upload.upload_part(2, vec![1u8; 10]).unwrap();
        upload.upload_part(3, vec![2u8; 5]).unwrap(); // 最后一个分片可以小于 part_size
        assert_eq!(upload.uploaded_part_count(), 3);
        assert_eq!(upload.uploaded_bytes(), 25);

        let combined = upload.complete().unwrap();
        assert_eq!(combined.len(), 25);
        assert_eq!(upload.progress_percent(), 100);
    }

    #[test]
    fn test_multipart_upload_wrong_part_number() {
        let mut upload = MultipartUpload::new("f", "text/plain", 10);
        upload.expected_parts = 2;
        upload.upload_part(1, vec![0u8; 10]).unwrap();
        // 尝试上传编号 3 而不是 2
        let result = upload.upload_part(3, vec![0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_multipart_upload_wrong_part_size() {
        let mut upload = MultipartUpload::new("f", "text/plain", 10);
        upload.expected_parts = 2;
        // 第一个分片大小不对（不是最后一个分片）
        let result = upload.upload_part(1, vec![0u8; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_multipart_upload_complete_after_complete() {
        let mut upload = MultipartUpload::new("f", "text/plain", 10);
        upload.expected_parts = 1;
        upload.upload_part(1, b"data".to_vec()).unwrap();
        upload.complete().unwrap();
        let result = upload.complete();
        assert!(result.is_err());
    }

    #[test]
    fn test_multipart_upload_abort() {
        let mut upload = MultipartUpload::new("f", "text/plain", 10);
        upload.expected_parts = 2;
        upload.upload_part(1, vec![0u8; 10]).unwrap();
        upload.abort().unwrap();
        assert_eq!(upload.status, UploadStatus::Aborted);
        assert_eq!(upload.uploaded_part_count(), 0);
        // 中止后不能再上传
        let result = upload.upload_part(2, vec![0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_multipart_upload_complete_empty() {
        let mut upload = MultipartUpload::new("f", "text/plain", 10);
        let result = upload.complete();
        assert!(result.is_err());
    }

    #[test]
    fn test_multipart_upload_progress() {
        let mut upload = MultipartUpload::new("f", "text/plain", 10);
        upload.expected_parts = 4;
        assert_eq!(upload.progress_percent(), 0);
        upload.upload_part(1, vec![0u8; 10]).unwrap();
        assert_eq!(upload.progress_percent(), 25);
        upload.upload_part(2, vec![0u8; 10]).unwrap();
        assert_eq!(upload.progress_percent(), 50);
        upload.upload_part(3, vec![0u8; 10]).unwrap();
        assert_eq!(upload.progress_percent(), 75);
    }

    #[test]
    fn test_multipart_upload_abort_after_complete_fails() {
        let mut upload = MultipartUpload::new("f", "text/plain", 10);
        upload.expected_parts = 1;
        upload.upload_part(1, b"data".to_vec()).unwrap();
        upload.complete().unwrap();
        let result = upload.abort();
        assert!(result.is_err());
    }

    // ====================================================================
    // ResumableUploadManager 测试
    // ====================================================================

    #[test]
    fn test_resumable_initiate() {
        let mgr = ResumableUploadManager::new();
        let upload_id = mgr
            .initiate("file.bin", "application/octet-stream", 1000, 100)
            .unwrap();
        assert!(!upload_id.is_empty());
        let session = mgr.get_session(&upload_id).unwrap();
        assert_eq!(session.key, "file.bin");
        assert_eq!(session.expected_parts, 10);
        assert_eq!(session.part_size, 100);
        assert_eq!(session.uploaded_part_numbers.len(), 0);
    }

    #[test]
    fn test_resumable_upload_and_complete() {
        let mgr = ResumableUploadManager::new();
        let upload_id = mgr
            .initiate("file.bin", "application/octet-stream", 200, 100)
            .unwrap();
        mgr.upload_part(&upload_id, 1, vec![0u8; 100]).unwrap();
        mgr.upload_part(&upload_id, 2, vec![1u8; 100]).unwrap();
        let combined = mgr.complete(&upload_id).unwrap();
        assert_eq!(combined.len(), 200);
        let data = mgr.get_completed_data("file.bin").unwrap();
        assert_eq!(data.len(), 200);
    }

    #[test]
    fn test_resumable_abort() {
        let mgr = ResumableUploadManager::new();
        let upload_id = mgr
            .initiate("file.bin", "application/octet-stream", 200, 100)
            .unwrap();
        mgr.upload_part(&upload_id, 1, vec![0u8; 100]).unwrap();
        mgr.abort(&upload_id).unwrap();
        let session = mgr.get_session(&upload_id).unwrap();
        assert_eq!(session.status, UploadStatus::Aborted);
    }

    #[test]
    fn test_resumable_list_uploads() {
        let mgr = ResumableUploadManager::new();
        let id1 = mgr.initiate("a", "text/plain", 100, 50).unwrap();
        let _id2 = mgr.initiate("b", "text/plain", 100, 50).unwrap();
        assert_eq!(mgr.list_uploads().len(), 2);
        // 上传分片后才能完成
        mgr.upload_part(&id1, 1, vec![0u8; 50]).unwrap();
        mgr.upload_part(&id1, 2, vec![0u8; 50]).unwrap();
        mgr.complete(&id1).unwrap();
        // 完成后不再出现在活跃列表
        assert_eq!(mgr.list_uploads().len(), 1);
    }

    #[test]
    fn test_resumable_get_session_not_found() {
        let mgr = ResumableUploadManager::new();
        assert!(mgr.get_session("nonexistent").is_none());
    }

    #[test]
    fn test_resumable_upload_part_invalid_session() {
        let mgr = ResumableUploadManager::new();
        let result = mgr.upload_part("nonexistent", 1, vec![0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_resumable_complete_invalid_session() {
        let mgr = ResumableUploadManager::new();
        let result = mgr.complete("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resumable_cleanup() {
        let mgr = ResumableUploadManager::new();
        let id1 = mgr.initiate("a", "text/plain", 100, 50).unwrap();
        let id2 = mgr.initiate("b", "text/plain", 100, 50).unwrap();
        // 上传分片后才能完成
        mgr.upload_part(&id1, 1, vec![0u8; 50]).unwrap();
        mgr.upload_part(&id1, 2, vec![0u8; 50]).unwrap();
        mgr.complete(&id1).unwrap();
        mgr.abort(&id2).unwrap();
        let cleaned = mgr.cleanup();
        assert_eq!(cleaned, 2);
    }

    #[test]
    fn test_resumable_snapshot_has_uploaded_parts() {
        let mgr = ResumableUploadManager::new();
        let upload_id = mgr.initiate("f", "text/plain", 300, 100).unwrap();
        mgr.upload_part(&upload_id, 1, vec![0u8; 100]).unwrap();
        mgr.upload_part(&upload_id, 2, vec![0u8; 100]).unwrap();
        let snapshot = mgr.get_session(&upload_id).unwrap();
        assert_eq!(snapshot.uploaded_part_numbers, vec![1, 2]);
        assert_eq!(snapshot.uploaded_bytes, 200);
        assert_eq!(snapshot.progress_percent, 66);
    }

    // ====================================================================
    // BucketLifecycle 测试
    // ====================================================================

    #[test]
    fn test_lifecycle_add_and_remove_rule() {
        let mut lc = BucketLifecycle::new();
        lc.add_rule(LifecycleRule::new(
            "expire-30d",
            LifecycleAction::Expiration { days: 30 },
        ));
        assert_eq!(lc.rule_count(), 1);
        assert!(lc.remove_rule("expire-30d"));
        assert_eq!(lc.rule_count(), 0);
        assert!(!lc.remove_rule("nonexistent"));
    }

    #[test]
    fn test_lifecycle_rule_matches_prefix() {
        let rule = LifecycleRule::new(
            "logs",
            LifecycleAction::Transition {
                storage_class: "IA".to_string(),
                days: 30,
            },
        )
        .with_prefix("logs/");
        assert!(rule.matches("logs/app.log"));
        assert!(!rule.matches("images/photo.jpg"));
    }

    #[test]
    fn test_lifecycle_rule_empty_prefix_matches_all() {
        let rule = LifecycleRule::new("all", LifecycleAction::Expiration { days: 365 });
        assert!(rule.matches("anything"));
        assert!(rule.matches("path/to/file"));
    }

    #[test]
    fn test_lifecycle_rule_disabled_does_not_match() {
        let rule = LifecycleRule::new("r", LifecycleAction::Expiration { days: 30 }).disabled();
        assert!(!rule.matches("any"));
    }

    #[test]
    fn test_lifecycle_evaluate_expiration() {
        let mut lc = BucketLifecycle::new();
        lc.add_rule(LifecycleRule::new(
            "expire-30d",
            LifecycleAction::Expiration { days: 30 },
        ));
        // 20 天 -> 不应过期
        let results = lc.evaluate("file.txt", 20, false);
        assert!(results.is_empty());
        // 35 天 -> 应过期
        let results = lc.evaluate("file.txt", 35, false);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].action, LifecycleAction::Expiration { .. }));
    }

    #[test]
    fn test_lifecycle_evaluate_transition() {
        let mut lc = BucketLifecycle::new();
        lc.add_rule(LifecycleRule::new(
            "to-ia",
            LifecycleAction::Transition {
                storage_class: "IA".to_string(),
                days: 30,
            },
        ));
        let results = lc.evaluate("data.bin", 40, false);
        assert_eq!(results.len(), 1);
        if let LifecycleAction::Transition { storage_class, .. } = &results[0].action {
            assert_eq!(storage_class, "IA");
        } else {
            panic!("expected Transition action");
        }
    }

    #[test]
    fn test_lifecycle_evaluate_abort_incomplete() {
        let mut lc = BucketLifecycle::new();
        lc.add_rule(LifecycleRule::new(
            "abort-multipart",
            LifecycleAction::AbortIncompleteMultipartUpload { days: 7 },
        ));
        // 没有未完成上传 -> 不触发
        let results = lc.evaluate("file", 0, false);
        assert!(results.is_empty());
        // 有未完成上传 -> 触发
        let results = lc.evaluate("file", 0, true);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_lifecycle_enable_disable_rule() {
        let mut lc = BucketLifecycle::new();
        lc.add_rule(LifecycleRule::new("r", LifecycleAction::Expiration { days: 30 }));
        assert!(lc.disable_rule("r"));
        assert!(!lc.matching_rules("file").iter().any(|r| r.id == "r"));
        assert!(lc.enable_rule("r"));
        assert!(lc.matching_rules("file").iter().any(|r| r.id == "r"));
    }

    #[test]
    fn test_lifecycle_matching_rules_filtered_by_prefix() {
        let mut lc = BucketLifecycle::new();
        lc.add_rule(
            LifecycleRule::new("logs", LifecycleAction::Expiration { days: 30 })
                .with_prefix("logs/"),
        );
        lc.add_rule(
            LifecycleRule::new("imgs", LifecycleAction::Expiration { days: 60 })
                .with_prefix("images/"),
        );
        let matches = lc.matching_rules("logs/app.log");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "logs");
    }

    #[test]
    fn test_lifecycle_multiple_rules_match_same_key() {
        let mut lc = BucketLifecycle::new();
        lc.add_rule(LifecycleRule::new(
            "to-ia",
            LifecycleAction::Transition {
                storage_class: "IA".to_string(),
                days: 30,
            },
        ));
        lc.add_rule(LifecycleRule::new(
            "expire",
            LifecycleAction::Expiration { days: 365 },
        ));
        let results = lc.evaluate("file", 400, false);
        assert_eq!(results.len(), 2);
    }

    // ====================================================================
    // CdnRefresher 测试
    // ====================================================================

    #[test]
    fn test_cdn_refresh_urls() {
        let refresher = CdnRefresher::new();
        let id = refresher
            .refresh_urls(vec![
                "https://cdn.example.com/a.js".to_string(),
                "https://cdn.example.com/b.css".to_string(),
            ])
            .unwrap();
        assert!(!id.is_empty());
        assert_eq!(refresher.total_requests(), 1);
        let status = refresher.get_status(&id).unwrap();
        assert_eq!(status, RefreshStatus::Done);
    }

    #[test]
    fn test_cdn_refresh_dirs() {
        let refresher = CdnRefresher::new();
        let id = refresher
            .refresh_dirs(vec!["https://cdn.example.com/static/".to_string()])
            .unwrap();
        let request = refresher.get_request(&id).unwrap();
        assert_eq!(request.refresh_type, RefreshType::Directory);
        assert_eq!(request.targets.len(), 1);
    }

    #[test]
    fn test_cdn_refresh_empty_targets() {
        let refresher = CdnRefresher::new();
        let result = refresher.refresh_urls(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cdn_refresh_too_many_targets() {
        let refresher = CdnRefresher::new().with_rate_limit(100);
        let urls: Vec<String> = (0..2000).map(|i| format!("https://cdn.example.com/{}.js", i)).collect();
        let result = refresher.refresh_urls(urls);
        assert!(result.is_err());
    }

    #[test]
    fn test_cdn_refresh_history() {
        let refresher = CdnRefresher::new();
        refresher.refresh_urls(vec!["https://cdn.example.com/a".to_string()]).unwrap();
        refresher.refresh_urls(vec!["https://cdn.example.com/b".to_string()]).unwrap();
        refresher.refresh_dirs(vec!["https://cdn.example.com/static/".to_string()]).unwrap();
        let history = refresher.history();
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_cdn_get_status_not_found() {
        let refresher = CdnRefresher::new();
        assert!(refresher.get_status("nonexistent").is_none());
    }

    #[test]
    fn test_cdn_get_request_not_found() {
        let refresher = CdnRefresher::new();
        assert!(refresher.get_request("nonexistent").is_none());
    }

    #[test]
    fn test_cdn_prefetch() {
        let refresher = CdnRefresher::new();
        let id = refresher
            .prefetch(vec!["https://cdn.example.com/big-file.zip".to_string()])
            .unwrap();
        assert_eq!(refresher.get_status(&id).unwrap(), RefreshStatus::Done);
    }

    #[test]
    fn test_cdn_prefetch_empty() {
        let refresher = CdnRefresher::new();
        let result = refresher.prefetch(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cdn_rate_limit() {
        let refresher = CdnRefresher::new().with_rate_limit(3);
        refresher.refresh_urls(vec!["https://a.com".to_string()]).unwrap();
        refresher.refresh_urls(vec!["https://b.com".to_string()]).unwrap();
        refresher.refresh_urls(vec!["https://c.com".to_string()]).unwrap();
        // 第 4 次应被速率限制拒绝
        let result = refresher.refresh_urls(vec!["https://d.com".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cdn_requests_in_last() {
        let refresher = CdnRefresher::new();
        refresher.refresh_urls(vec!["https://a.com".to_string()]).unwrap();
        refresher.refresh_urls(vec!["https://b.com".to_string()]).unwrap();
        assert_eq!(refresher.requests_in_last(60), 2);
    }

    // ====================================================================
    // 辅助函数测试
    // ====================================================================

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0.00 B");
        assert_eq!(format_size(512), "512.00 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_estimate_remaining_seconds() {
        // 上传 100 字节用了 10 秒，还剩 100 字节 -> 预计还需 10 秒
        let remaining = estimate_remaining_seconds(100, 200, Duration::from_secs(10));
        assert_eq!(remaining, Some(10));
    }

    #[test]
    fn test_estimate_remaining_seconds_zero_uploaded() {
        let remaining = estimate_remaining_seconds(0, 100, Duration::from_secs(5));
        assert_eq!(remaining, None);
    }

    #[test]
    fn test_estimate_remaining_seconds_zero_elapsed() {
        let remaining = estimate_remaining_seconds(50, 100, Duration::from_secs(0));
        assert_eq!(remaining, None);
    }

    #[test]
    fn test_object_age_days() {
        let one_day_ago = SystemTime::now() - Duration::from_secs(86400);
        let age = object_age_days(one_day_ago);
        assert!(age >= 1);
    }

    #[test]
    fn test_part_etag_unique() {
        let p1 = Part::new(1, vec![0u8; 10]);
        let p2 = Part::new(2, vec![0u8; 10]);
        let p3 = Part::new(1, vec![0u8; 20]);
        assert_ne!(p1.etag, p2.etag);
        assert_ne!(p1.etag, p3.etag);
    }
}
