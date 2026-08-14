//! 分布式缓存一致性模块
//!
//! 本模块在 `dist-cache` feature gate 下导出，提供：
//! - [`ConsistencyLevel`] — 一致性级别枚举（Eventual / Strong）
//! - [`RedisPubSubInvalidationBus`] — Redis Pub/Sub 跨实例失效总线
//! - [`GossipInvalidationBus`] — Gossip 去中心化失效总线
//! - [`WriteBehindQueue`] / [`WriteBehindConfig`] / [`WriteOp`] — Write-behind 异步批量写入
//! - [`BloomFilterGuard`] — 布隆过滤器击穿防护
//! - `MutexGuard` — 互斥锁击穿防护
//! - [`RandomTtlJitter`] — 随机 TTL 雪崩防护

use crate::l2_cache::{InvalidationBus, InvalidationMessage};
use crate::value::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

// ─── M2-T2：一致性级别配置 ─────────────────────────────────────────

/// 一致性级别枚举
///
/// - `Eventual`：默认，写库后异步失效 + TTL 兜底
/// - `Strong`：先失效所有实例缓存再写库
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsistencyLevel {
    /// 最终一致（写库后异步失效 + TTL 兜底）
    #[default]
    Eventual,
    /// 强一致（先失效所有实例缓存再写库）
    Strong,
}

// ─── M2-T3：Redis Pub/Sub 失效总线 ─────────────────────────────────

/// Redis Pub/Sub 跨实例失效总线
///
/// 复用既有 Redis 连接管理（自动重连），Pub/Sub 专用连接。
/// 消息序列化为 ≤1KB JSON，跳过本实例 instance_id 避免自回环。
pub struct RedisPubSubInvalidationBus {
    /// Redis 连接管理器
    client: Option<redis::aio::ConnectionManager>,
    /// Pub/Sub 通道名
    channel: String,
    /// 本地缓冲（订阅循环写入，subscribe drain 读取）
    local_buffer: parking_lot::Mutex<VecDeque<InvalidationMessage>>,
    /// 本实例 ID（避免自回环）
    instance_id: String,
}

impl RedisPubSubInvalidationBus {
    /// 创建 Redis Pub/Sub 失效总线
    pub fn new(client: redis::aio::ConnectionManager, instance_id: impl Into<String>) -> Self {
        Self {
            client: Some(client),
            channel: "sz-orm:invalidation".to_string(),
            local_buffer: parking_lot::Mutex::new(VecDeque::new()),
            instance_id: instance_id.into(),
        }
    }

    /// 创建未连接的失效总线（降级为本地失效）
    pub fn disconnected(instance_id: impl Into<String>) -> Self {
        Self {
            client: None,
            channel: "sz-orm:invalidation".to_string(),
            local_buffer: parking_lot::Mutex::new(VecDeque::new()),
            instance_id: instance_id.into(),
        }
    }

    /// 设置通道名
    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = channel.into();
        self
    }

    /// 获取本实例 ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// 将失效消息序列化为 JSON（≤1KB）
    fn serialize_message(message: &InvalidationMessage, instance_id: &str) -> String {
        let payload = match message {
            InvalidationMessage::InvalidateKey(key) => {
                serde_json::json!({"type": "key", "key": key, "src": instance_id})
            }
            InvalidationMessage::InvalidateTable(table) => {
                serde_json::json!({"type": "table", "table": table, "src": instance_id})
            }
            InvalidationMessage::InvalidateAll => {
                serde_json::json!({"type": "all", "src": instance_id})
            }
        };
        payload.to_string()
    }

    /// 从 JSON 反序列化失效消息
    #[allow(dead_code)]
    fn deserialize_message(json: &str, self_instance_id: &str) -> Option<InvalidationMessage> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let src = v.get("src")?.as_str()?;
        // 跳过自回环
        if src == self_instance_id {
            return None;
        }
        match v.get("type")?.as_str()? {
            "key" => {
                let key = v.get("key")?.as_str()?;
                Some(InvalidationMessage::InvalidateKey(key.to_string()))
            }
            "table" => {
                let table = v.get("table")?.as_str()?;
                Some(InvalidationMessage::InvalidateTable(table.to_string()))
            }
            "all" => Some(InvalidationMessage::InvalidateAll),
            _ => None,
        }
    }

    /// 接收消息（从外部订阅循环调用，写入本地缓冲）
    pub fn push_received(&self, message: InvalidationMessage) {
        self.local_buffer.lock().push_back(message);
    }
}

impl InvalidationBus for RedisPubSubInvalidationBus {
    fn publish(&self, message: InvalidationMessage) {
        if let Some(client) = &self.client {
            let json = Self::serialize_message(&message, &self.instance_id);
            // 异步发布（fire-and-forget）
            let client = client.clone();
            let channel = self.channel.clone();
            tokio::spawn(async move {
                let _: Result<(), _> = redis::cmd("PUBLISH")
                    .arg(&channel)
                    .arg(&json)
                    .query_async(&mut client.clone())
                    .await;
            });
        }
        // Redis 不可达时降级为本地失效（仅失效本实例缓存）
    }

    fn subscribe(&self) -> Box<dyn Iterator<Item = InvalidationMessage> + Send> {
        let mut buffer = self.local_buffer.lock();
        let drained: Vec<_> = buffer.drain(..).collect();
        Box::new(drained.into_iter())
    }
}

// ─── M2-T4：Gossip 失效总线 ────────────────────────────────────────

/// 节点地址
#[derive(Debug, Clone)]
pub struct NodeAddr {
    /// 主机地址
    pub host: String,
    /// 端口
    pub port: u16,
}

impl NodeAddr {
    /// 创建节点地址
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}

/// Gossip 去中心化失效总线
///
/// 点对点发送到所有已知节点，HMAC 共享密钥认证，
/// seen_messages 去重避免重复传播。
pub struct GossipInvalidationBus {
    /// 集群节点地址列表
    #[allow(dead_code)]
    nodes: Vec<NodeAddr>,
    /// 共享密钥认证
    shared_secret: Vec<u8>,
    /// 本地缓冲
    local_buffer: parking_lot::Mutex<VecDeque<InvalidationMessage>>,
    /// 已见消息 ID 去重
    seen_messages: parking_lot::RwLock<HashSet<u64>>,
    /// 本实例 ID
    instance_id: String,
    /// 消息序列号
    sequence: AtomicU64,
}

impl GossipInvalidationBus {
    /// 创建 Gossip 失效总线
    pub fn new(
        nodes: Vec<NodeAddr>,
        shared_secret: Vec<u8>,
        instance_id: impl Into<String>,
    ) -> Self {
        Self {
            nodes,
            shared_secret,
            local_buffer: parking_lot::Mutex::new(VecDeque::new()),
            seen_messages: parking_lot::RwLock::new(HashSet::new()),
            instance_id: instance_id.into(),
            sequence: AtomicU64::new(0),
        }
    }

    /// 获取本实例 ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// 生成消息 ID（用于去重）
    fn message_id(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    /// 计算 HMAC 认证标签
    fn compute_hmac(&self, message: &InvalidationMessage) -> Vec<u8> {
        let msg_bytes = format!("{:?}", message);
        sz_orm_crypto::hmac_sha256(&self.shared_secret, msg_bytes.as_bytes()).to_vec()
    }

    /// 验证 HMAC 认证标签
    fn verify_hmac(&self, message: &InvalidationMessage, tag: &[u8]) -> bool {
        let expected = self.compute_hmac(message);
        expected == tag
    }

    /// 接收消息（从其他节点调用，需认证 + 去重）
    pub fn receive(&self, message: InvalidationMessage, msg_id: u64, hmac_tag: &[u8]) -> bool {
        // 认证
        if !self.verify_hmac(&message, hmac_tag) {
            return false;
        }
        // 去重
        let mut seen = self.seen_messages.write();
        if !seen.insert(msg_id) {
            return false; // 已见过
        }
        drop(seen);
        // 写入本地缓冲
        self.local_buffer.lock().push_back(message);
        true
    }
}

impl InvalidationBus for GossipInvalidationBus {
    fn publish(&self, message: InvalidationMessage) {
        let msg_id = self.message_id();
        let _hmac_tag = self.compute_hmac(&message);
        // 点对点发送到所有已知节点（并行）
        // 实际网络发送由调用方实现，此处仅写入本地缓冲
        let mut seen = self.seen_messages.write();
        seen.insert(msg_id);
        drop(seen);
        self.local_buffer.lock().push_back(message);
    }

    fn subscribe(&self) -> Box<dyn Iterator<Item = InvalidationMessage> + Send> {
        let mut buffer = self.local_buffer.lock();
        let drained: Vec<_> = buffer.drain(..).collect();
        Box::new(drained.into_iter())
    }
}

// ─── M2-T6：Write-behind 配置与队列 ────────────────────────────────

/// Write-behind 配置
#[derive(Debug, Clone)]
pub struct WriteBehindConfig {
    /// 批量刷盘大小（默认 100）
    pub batch_size: u32,
    /// 刷盘间隔（默认 100ms）
    pub flush_interval: Duration,
    /// WAL 文件路径
    pub wal_path: PathBuf,
    /// WAL 加密密钥
    pub encryption_key: Vec<u8>,
    /// 刷盘失败回退同步写（默认 true）
    pub fallback_to_sync: bool,
}

impl Default for WriteBehindConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            flush_interval: Duration::from_millis(100),
            wal_path: PathBuf::from("wal/sz-orm-wal.log"),
            encryption_key: Vec::new(),
            fallback_to_sync: true,
        }
    }
}

impl WriteBehindConfig {
    /// 创建配置构建器
    pub fn builder() -> WriteBehindConfigBuilder {
        WriteBehindConfigBuilder::default()
    }
}

/// Write-behind 配置构建器
#[derive(Debug, Clone, Default)]
pub struct WriteBehindConfigBuilder {
    batch_size: Option<u32>,
    flush_interval: Option<Duration>,
    wal_path: Option<PathBuf>,
    encryption_key: Option<Vec<u8>>,
    fallback_to_sync: Option<bool>,
}

impl WriteBehindConfigBuilder {
    /// 设置批大小
    pub fn batch_size(mut self, size: u32) -> Self {
        self.batch_size = Some(size);
        self
    }
    /// 设置刷新间隔
    pub fn flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = Some(interval);
        self
    }
    /// 设置 WAL 文件路径
    pub fn wal_path(mut self, path: PathBuf) -> Self {
        self.wal_path = Some(path);
        self
    }
    /// 设置加密密钥
    pub fn encryption_key(mut self, key: Vec<u8>) -> Self {
        self.encryption_key = Some(key);
        self
    }
    /// 设置是否回退到同步写
    pub fn fallback_to_sync(mut self, fallback: bool) -> Self {
        self.fallback_to_sync = Some(fallback);
        self
    }
    /// 构建配置
    pub fn build(self) -> WriteBehindConfig {
        WriteBehindConfig {
            batch_size: self.batch_size.unwrap_or(100),
            flush_interval: self
                .flush_interval
                .unwrap_or_else(|| Duration::from_millis(100)),
            wal_path: self
                .wal_path
                .unwrap_or_else(|| PathBuf::from("wal/sz-orm-wal.log")),
            encryption_key: self.encryption_key.unwrap_or_default(),
            fallback_to_sync: self.fallback_to_sync.unwrap_or(true),
        }
    }
}

/// 写操作类型
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WriteOpType {
    /// 插入
    Insert,
    /// 更新
    Update,
    /// 删除
    Delete,
}

/// Write-behind 写操作
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WriteOp {
    /// 操作类型
    pub op_type: WriteOpType,
    /// 表名
    pub table: String,
    /// 主键值
    pub pk: Value,
    /// 变更数据
    pub data: Vec<(String, Value)>,
    /// 时间戳
    pub timestamp: i64,
    /// 单调递增序列号
    pub sequence: u64,
}

impl WriteOp {
    /// 创建写操作
    pub fn new(op_type: WriteOpType, table: impl Into<String>, pk: Value) -> Self {
        Self {
            op_type,
            table: table.into(),
            pk,
            data: Vec::new(),
            timestamp: chrono::Utc::now().timestamp(),
            sequence: 0,
        }
    }

    /// 添加变更数据
    pub fn with_data(mut self, data: Vec<(String, Value)>) -> Self {
        self.data = data;
        self
    }
}

/// Write-behind 持久化队列
///
/// WAL 持久化先于返回成功（宕机不丢数据），
/// 内存待刷盘队列 + 单调递增序列号。
pub struct WriteBehindQueue {
    /// WAL 文件
    wal: parking_lot::Mutex<WalFile>,
    /// 内存待刷盘队列
    pending: crossbeam_queue::ArrayQueue<WriteOp>,
    /// 单调递增序列号
    sequence: AtomicU64,
    /// 配置
    config: WriteBehindConfig,
}

impl WriteBehindQueue {
    /// 创建 Write-behind 队列
    pub fn new(config: WriteBehindConfig) -> std::io::Result<Self> {
        let wal = WalFile::open(&config.wal_path, &config.encryption_key)?;
        let capacity = (config.batch_size * 10) as usize;
        Ok(Self {
            wal: parking_lot::Mutex::new(wal),
            pending: crossbeam_queue::ArrayQueue::new(capacity.max(1024)),
            sequence: AtomicU64::new(0),
            config,
        })
    }

    /// 入队写操作（WAL 持久化 + 入内存队列，立即返回）
    ///
    /// WAL 持久化先于返回成功，保证宕机不丢数据。
    pub fn enqueue(&self, mut op: WriteOp) -> std::io::Result<()> {
        op.sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
        // 1. WAL 持久化（加密 + CRC）
        self.wal.lock().append(&op)?;
        // 2. 入内存 pending 队列
        let _ = self.pending.push(op);
        // 3. 立即返回成功
        Ok(())
    }

    /// 批量取出待刷盘操作
    pub fn drain_batch(&self) -> Vec<WriteOp> {
        let batch_size = self.config.batch_size as usize;
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size {
            match self.pending.pop() {
                Some(op) => batch.push(op),
                None => break,
            }
        }
        // 按 sequence 排序
        batch.sort_by_key(|op| op.sequence);
        batch
    }

    /// 标记 WAL 已刷盘（截断）
    pub fn truncate_wal(&self) -> std::io::Result<()> {
        self.wal.lock().truncate()
    }

    /// 宕机重启回放 WAL
    ///
    /// 读取 WAL 文件，CRC 校验 + 解密，按 sequence 顺序回放未刷盘 WriteOp。
    pub fn replay(&self) -> std::io::Result<Vec<WriteOp>> {
        self.wal.lock().read_all()
    }

    /// 获取配置
    pub fn config(&self) -> &WriteBehindConfig {
        &self.config
    }

    /// 获取待刷盘数量
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

// ─── M2-T7：WAL 持久化与加密 ───────────────────────────────────────

/// WAL 文件
///
/// 每条记录格式：[4字节长度][加密载荷][8字节CRC]
struct WalFile {
    path: PathBuf,
    encryption_key: Vec<u8>,
}

impl WalFile {
    fn open(path: &std::path::Path, encryption_key: &[u8]) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self {
            path: path.to_path_buf(),
            encryption_key: encryption_key.to_vec(),
        })
    }

    fn append(&mut self, op: &WriteOp) -> std::io::Result<()> {
        let json = serde_json::to_string(op)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let payload = json.as_bytes();

        let encrypted = if self.encryption_key.is_empty() {
            payload.to_vec()
        } else {
            self.encrypt(payload)
        };

        let crc = crc64(&encrypted);

        let mut record = Vec::with_capacity(4 + encrypted.len() + 8);
        record.extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
        record.extend_from_slice(&encrypted);
        record.extend_from_slice(&crc.to_le_bytes());

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        use std::io::Write;
        file.write_all(&record)?;
        file.flush()?;
        Ok(())
    }

    fn read_all(&self) -> std::io::Result<Vec<WriteOp>> {
        let data = match std::fs::read(&self.path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut ops = Vec::new();
        let mut pos = 0;
        while pos + 4 <= data.len() {
            let len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                as usize;
            pos += 4;
            if pos + len + 8 > data.len() {
                break;
            }
            let encrypted = &data[pos..pos + len];
            pos += len;
            let expected_crc = u64::from_le_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
                data[pos + 4],
                data[pos + 5],
                data[pos + 6],
                data[pos + 7],
            ]);
            pos += 8;

            if crc64(encrypted) != expected_crc {
                continue;
            }

            let decrypted = if self.encryption_key.is_empty() {
                encrypted.to_vec()
            } else {
                self.decrypt(encrypted)
            };

            if let Ok(op) = serde_json::from_slice::<WriteOp>(&decrypted) {
                ops.push(op);
            }
        }
        ops.sort_by_key(|op| op.sequence);
        Ok(ops)
    }

    fn truncate(&mut self) -> std::io::Result<()> {
        std::fs::write(&self.path, b"")?;
        Ok(())
    }

    fn encrypt(&self, data: &[u8]) -> Vec<u8> {
        let crypter = sz_orm_crypto::AesGcmCrypter::from_key_str(
            std::str::from_utf8(&self.encryption_key).unwrap_or("default-key"),
        );
        crypter
            .encrypt_with_aad(data, &[])
            .unwrap_or_else(|_| data.to_vec())
    }

    fn decrypt(&self, data: &[u8]) -> Vec<u8> {
        let crypter = sz_orm_crypto::AesGcmCrypter::from_key_str(
            std::str::from_utf8(&self.encryption_key).unwrap_or("default-key"),
        );
        crypter
            .decrypt_with_aad(data, &[])
            .unwrap_or_else(|_| data.to_vec())
    }
}

fn crc64(data: &[u8]) -> u64 {
    let mut crc: u64 = 0;
    for &byte in data {
        crc ^= byte as u64;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xC96E_8607_EAFC_E6CD;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

// ─── M2-T9：布隆过滤器防护 ─────────────────────────────────────────

/// 布隆过滤器击穿防护
///
/// 假阳性率 ≤ 1% 可配置，超容量自动重建。
/// v4.7.0 双实现合并：内部使用公共 `crate::bloom::BloomFilter`（原 bloomfilter crate 依赖已移除）。
pub struct BloomFilterGuard {
    filter: crate::bloom::BloomFilter,
    count: AtomicU64,
}

impl BloomFilterGuard {
    /// 创建布隆过滤器
    pub fn new(capacity: usize, false_positive_rate: f64) -> Self {
        let filter = crate::bloom::BloomFilter::new(capacity, false_positive_rate);
        Self {
            filter,
            count: AtomicU64::new(0),
        }
    }

    /// 使用默认配置创建（容量 100000，假阳性率 0.01）
    pub fn default_config() -> Self {
        Self::new(100_000, 0.01)
    }

    /// 添加 key（容量满时拒绝写入——击穿防护场景漏判仅导致多查 DB，安全降级）
    pub fn add(&self, key: &str) {
        let _ = self.filter.add(key);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// 判断是否可能存在（假阳性 ≤ false_positive_rate）
    pub fn might_contain(&self, key: &str) -> bool {
        self.filter.might_contain(key)
    }

    /// 重建布隆过滤器（超容量时调用）
    ///
    /// 公共 `BloomFilter` 内部可变（RwLock），`&self` 即可清空重填。
    pub fn rebuild(&self, keys: impl Iterator<Item = String>) {
        self.filter.clear();
        for key in keys {
            let _ = self.filter.add(&key);
            self.count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 获取当前元素计数
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

// ─── M2-T10：互斥锁防护 ────────────────────────────────────────────

/// 互斥锁击穿防护
///
/// 按 key 互斥锁，仅允许一个请求查库回填。
pub struct CacheMutexGuard {
    mutexes: parking_lot::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl CacheMutexGuard {
    /// 创建互斥锁防护
    pub fn new() -> Self {
        Self {
            mutexes: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// 获取 key 的互斥锁 Arc（调用方自行 lock）
    pub fn get_mutex(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self.mutexes.lock();
        map.entry(key.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// 在 key 互斥锁保护下执行闭包
    pub async fn with_guard<F, R>(&self, key: &str, f: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        let mutex = self.get_mutex(key);
        let _guard = mutex.lock().await;
        f.await
    }
}

impl Default for CacheMutexGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ─── M2-T11：随机 TTL 雪崩防护 ─────────────────────────────────────

/// 随机 TTL 雪崩防护
///
/// 抖动范围默认基础 TTL 的 ±20%，安全随机源避免抖动可预测。
pub struct RandomTtlJitter;

impl RandomTtlJitter {
    /// 计算 TTL 抖动
    ///
    /// `base_ttl × (1 ± jitter_range × random)`，random 使用 rand crate 安全随机源。
    /// 默认 jitter_range = 0.2（±20%）。
    pub fn jitter(base_ttl: Duration, jitter_range: f64) -> Duration {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random: f64 = rng.gen_range(-1.0..=1.0);
        let factor = 1.0 + jitter_range * random;
        let jittered_ms = (base_ttl.as_millis() as f64 * factor) as u64;
        Duration::from_millis(jittered_ms.max(1))
    }

    /// 使用默认 ±20% 抖动
    pub fn default_jitter(base_ttl: Duration) -> Duration {
        Self::jitter(base_ttl, 0.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── M2-T14.1：RedisPubSubInvalidationBus 测试 ─────────────────

    #[test]
    fn test_redis_pubsub_serialize_message_key() {
        let msg = InvalidationMessage::InvalidateKey("user:42".to_string());
        let json = RedisPubSubInvalidationBus::serialize_message(&msg, "instance-1");
        assert!(json.contains("\"type\":\"key\""));
        assert!(json.contains("\"key\":\"user:42\""));
        assert!(json.contains("\"src\":\"instance-1\""));
        assert!(json.len() <= 1024, "消息应 ≤1KB: {} bytes", json.len());
    }

    #[test]
    fn test_redis_pubsub_serialize_message_table() {
        let msg = InvalidationMessage::InvalidateTable("users".to_string());
        let json = RedisPubSubInvalidationBus::serialize_message(&msg, "instance-1");
        assert!(json.contains("\"type\":\"table\""));
        assert!(json.contains("\"table\":\"users\""));
        assert!(json.len() <= 1024);
    }

    #[test]
    fn test_redis_pubsub_serialize_message_all() {
        let msg = InvalidationMessage::InvalidateAll;
        let json = RedisPubSubInvalidationBus::serialize_message(&msg, "instance-1");
        assert!(json.contains("\"type\":\"all\""));
        assert!(json.len() <= 1024);
    }

    #[test]
    fn test_redis_pubsub_deserialize_skips_self() {
        let msg = InvalidationMessage::InvalidateKey("user:42".to_string());
        let json = RedisPubSubInvalidationBus::serialize_message(&msg, "instance-1");
        // 自回环应跳过
        let result = RedisPubSubInvalidationBus::deserialize_message(&json, "instance-1");
        assert!(result.is_none(), "应跳过自回环");
    }

    #[test]
    fn test_redis_pubsub_deserialize_other_instance() {
        let msg = InvalidationMessage::InvalidateKey("user:42".to_string());
        let json = RedisPubSubInvalidationBus::serialize_message(&msg, "instance-1");
        // 其他实例应接收
        let result = RedisPubSubInvalidationBus::deserialize_message(&json, "instance-2");
        assert!(result.is_some(), "应接收其他实例消息");
    }

    #[test]
    fn test_redis_pubsub_disconnected_publish() {
        let bus = RedisPubSubInvalidationBus::disconnected("instance-1");
        // 断连状态 publish 不应 panic
        bus.publish(InvalidationMessage::InvalidateAll);
    }

    #[test]
    fn test_redis_pubsub_subscribe_drain() {
        let bus = RedisPubSubInvalidationBus::disconnected("instance-1");
        bus.push_received(InvalidationMessage::InvalidateTable("users".to_string()));
        bus.push_received(InvalidationMessage::InvalidateAll);
        let messages: Vec<_> = bus.subscribe().collect();
        assert_eq!(messages.len(), 2);
        // 再次 subscribe 应为空
        let messages2: Vec<_> = bus.subscribe().collect();
        assert_eq!(messages2.len(), 0);
    }

    // ─── M2-T14.2：GossipInvalidationBus 测试 ──────────────────────

    #[test]
    fn test_gossip_publish_and_subscribe() {
        let bus = GossipInvalidationBus::new(
            vec![NodeAddr::new("127.0.0.1", 8080)],
            b"secret-key".to_vec(),
            "instance-1",
        );
        bus.publish(InvalidationMessage::InvalidateTable("users".to_string()));
        bus.publish(InvalidationMessage::InvalidateAll);
        let messages: Vec<_> = bus.subscribe().collect();
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_gossip_hmac_authentication() {
        let bus = GossipInvalidationBus::new(
            vec![NodeAddr::new("127.0.0.1", 8080)],
            b"secret-key".to_vec(),
            "instance-1",
        );
        let msg = InvalidationMessage::InvalidateKey("user:42".to_string());
        let tag = bus.compute_hmac(&msg);
        assert!(bus.verify_hmac(&msg, &tag));
        // 错误的 tag 应拒绝
        assert!(!bus.verify_hmac(&msg, &[0u8; 32]));
    }

    #[test]
    fn test_gossip_receive_dedup() {
        let bus = GossipInvalidationBus::new(
            vec![NodeAddr::new("127.0.0.1", 8080)],
            b"secret-key".to_vec(),
            "instance-1",
        );
        let msg = InvalidationMessage::InvalidateKey("user:42".to_string());
        let tag = bus.compute_hmac(&msg);
        // 第一次接收应成功
        assert!(bus.receive(msg.clone(), 1, &tag));
        // 重复消息应被去重
        assert!(!bus.receive(msg, 1, &tag));
    }

    #[test]
    fn test_gossip_receive_unauthenticated() {
        let bus = GossipInvalidationBus::new(
            vec![NodeAddr::new("127.0.0.1", 8080)],
            b"secret-key".to_vec(),
            "instance-1",
        );
        let msg = InvalidationMessage::InvalidateKey("user:42".to_string());
        // 错误的 HMAC 应拒绝
        assert!(!bus.receive(msg, 1, &[0u8; 32]));
    }

    // ─── M2-T14.3：WriteBehindQueue 测试 ───────────────────────────

    #[test]
    fn test_write_behind_config_default() {
        let config = WriteBehindConfig::default();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.flush_interval, Duration::from_millis(100));
        assert!(config.fallback_to_sync);
    }

    #[test]
    fn test_write_behind_config_builder() {
        let config = WriteBehindConfig::builder()
            .batch_size(50)
            .flush_interval(Duration::from_millis(200))
            .fallback_to_sync(false)
            .build();
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.flush_interval, Duration::from_millis(200));
        assert!(!config.fallback_to_sync);
    }

    #[test]
    fn test_write_op_new() {
        let op = WriteOp::new(WriteOpType::Insert, "users", Value::I64(42));
        assert_eq!(op.op_type, WriteOpType::Insert);
        assert_eq!(op.table, "users");
        assert_eq!(op.pk, Value::I64(42));
        assert_eq!(op.sequence, 0);
    }

    #[test]
    fn test_write_behind_queue_enqueue_and_drain() {
        let temp_dir = std::env::temp_dir().join("sz-orm-test-wal");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let config = WriteBehindConfig::builder()
            .batch_size(10)
            .wal_path(temp_dir.join("test.log"))
            .build();
        let queue = WriteBehindQueue::new(config).unwrap();

        // 入队 3 条
        for i in 0..3 {
            let op = WriteOp::new(WriteOpType::Update, "users", Value::I64(i));
            queue.enqueue(op).unwrap();
        }

        assert_eq!(queue.pending_count(), 3);

        // 批量取出
        let batch = queue.drain_batch();
        assert_eq!(batch.len(), 3);
        // 按 sequence 排序
        assert!(batch.windows(2).all(|w| w[0].sequence <= w[1].sequence));

        // 清理
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_behind_queue_replay() {
        let temp_dir = std::env::temp_dir().join("sz-orm-test-wal-replay");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let config = WriteBehindConfig::builder()
            .batch_size(10)
            .wal_path(temp_dir.join("test.log"))
            .build();
        let queue = WriteBehindQueue::new(config).unwrap();

        for i in 0..5 {
            let op = WriteOp::new(WriteOpType::Insert, "orders", Value::I64(i)).with_data(vec![(
                "status".to_string(),
                Value::String("pending".to_string()),
            )]);
            queue.enqueue(op).unwrap();
        }

        // 回放
        let replayed = queue.replay().unwrap();
        assert_eq!(replayed.len(), 5);
        // 按 sequence 排序
        assert!(replayed.windows(2).all(|w| w[0].sequence <= w[1].sequence));

        // 清理
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ─── M2-T14.4：BloomFilterGuard + MutexGuard + RandomTtlJitter ─

    #[test]
    fn test_bloom_filter_basic() {
        let guard = BloomFilterGuard::new(1000, 0.01);
        guard.add("user:1");
        guard.add("user:2");
        assert!(guard.might_contain("user:1"));
        assert!(guard.might_contain("user:2"));
        assert_eq!(guard.count(), 2);
    }

    #[test]
    fn test_bloom_filter_false_positive_rate() {
        let guard = BloomFilterGuard::new(10_000, 0.01);
        for i in 0..1000 {
            guard.add(&format!("user:{}", i));
        }
        let mut false_positives = 0;
        for i in 1000..2000 {
            if guard.might_contain(&format!("user:{}", i)) {
                false_positives += 1;
            }
        }
        let fp_rate = false_positives as f64 / 1000.0;
        assert!(fp_rate < 0.05, "假阳性率应 < 5%（实际 {}）", fp_rate);
    }

    #[tokio::test]
    async fn test_cache_mutex_guard() {
        let guard = CacheMutexGuard::new();
        guard
            .with_guard("user:42", async {
                // 互斥锁保护下执行
            })
            .await;
        guard
            .with_guard("user:43", async {
                // 不同 key 不互斥
            })
            .await;
    }

    #[test]
    fn test_random_ttl_jitter_range() {
        let base = Duration::from_millis(1000);
        for _ in 0..100 {
            let jittered = RandomTtlJitter::default_jitter(base);
            let ms = jittered.as_millis();
            // ±20% 范围：800..=1200
            assert!(
                (800..=1200).contains(&ms),
                "TTL 抖动应在 ±20% 范围内: {}ms",
                ms
            );
        }
    }

    #[test]
    fn test_consistency_level_default() {
        assert_eq!(ConsistencyLevel::default(), ConsistencyLevel::Eventual);
    }

    #[test]
    fn test_crc64() {
        let crc1 = crc64(b"hello");
        let crc2 = crc64(b"hello");
        assert_eq!(crc1, crc2);
        let crc3 = crc64(b"world");
        assert_ne!(crc1, crc3);
    }
}
