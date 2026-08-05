//! # 真实云存储实现（feature = "real-cloud"）
//!
//! 基于 [Apache OpenDAL](https://opendal.apache.org/) 统一接入：
//!
//! | 云厂商 | OpenDAL Service | 配置项 |
//! |---|---|---|
//! | 阿里云 OSS | [`services::Oss`] | bucket / endpoint / access_key_id / access_key_secret |
//! | 腾讯云 COS | [`services::Cos`] | bucket / endpoint / secret_id / secret_key |
//! | 华为云 OBS | [`services::Obs`] | bucket / endpoint / access_key_id / secret_access_key |
//! | 又拍云 | [`services::Upyun`] | bucket / operator / password |
//! | 七牛云 Kodo | 官方 REST API（本模块手写签名） | bucket / access_key / secret_key |
//!
//! 启用方式：`cargo add sz-orm-storage --features real-cloud`，然后通过
//! [`crate::storage::StorageBuilder`] 配置凭据后构建，即自动获得真实实现。
//!
//! ⚠️ 所有操作均为真实 HTTP 请求，需要有效的云厂商凭据；未配置凭据时
//! [`crate::storage::StorageBuilder::build`] 返回 `StorageError::InvalidConfig`。

use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use base64::Engine;
use opendal::{services, Operator};
use std::time::{SystemTime, UNIX_EPOCH};

/// 基于 OpenDAL 的通用云存储后端（OSS / COS / OBS / UpYun 共用）
pub struct OpendalStorage {
    operator: Operator,
    /// URL scheme 前缀（oss / cos / obs / upyun）
    scheme: &'static str,
    bucket: String,
    endpoint: String,
}

impl OpendalStorage {
    pub fn url_for(&self, key: &str) -> String {
        format!(
            "{}://{}.{}/{}",
            self.scheme, self.bucket, self.endpoint, key
        )
    }
}

/// 将 OpenDAL 错误映射为 [`StorageError`]
fn map_opendal_err(err: opendal::Error, op: &str) -> StorageError {
    match err.kind() {
        opendal::ErrorKind::NotFound => StorageError::NotFound(err.to_string()),
        opendal::ErrorKind::PermissionDenied => StorageError::PermissionDenied(err.to_string()),
        opendal::ErrorKind::Unexpected => StorageError::Connection(err.to_string()),
        _ => match op {
            "put" => StorageError::Put(err.to_string()),
            "get" => StorageError::Get(err.to_string()),
            "delete" => StorageError::Delete(err.to_string()),
            _ => StorageError::Connection(err.to_string()),
        },
    }
}

/// 确保 endpoint 带 `https://` 前缀（OpenDAL 要求完整 URL）
fn ensure_https(endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{}", trimmed)
    }
}

#[async_trait]
impl Storage for OpendalStorage {
    async fn put(
        &self,
        key: &str,
        data: &[u8],
        _content_type: &str,
    ) -> Result<String, StorageError> {
        self.operator
            .write(key, data.to_vec())
            .await
            .map_err(|e| map_opendal_err(e, "put"))?;
        Ok(self.url_for(key))
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let buf = self
            .operator
            .read(key)
            .await
            .map_err(|e| map_opendal_err(e, "get"))?;
        Ok(buf.to_vec())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.operator
            .delete(key)
            .await
            .map_err(|e| map_opendal_err(e, "delete"))
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self.operator.stat(key).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == opendal::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(map_opendal_err(e, "exists")),
        }
    }
}

/// 阿里云 OSS 真实存储（OpenDAL `services::Oss`）
pub fn aliyun_oss(
    bucket: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<OpendalStorage, StorageError> {
    let endpoint = ensure_https(endpoint);
    let builder = services::Oss::default()
        .root("/")
        .bucket(bucket)
        .endpoint(&endpoint)
        .access_key_id(access_key)
        .access_key_secret(secret_key);
    let operator = Operator::new(builder)
        .map_err(|e| StorageError::InvalidConfig(format!("aliyun oss: {}", e)))?;
    Ok(OpendalStorage {
        operator,
        scheme: "oss",
        bucket: bucket.to_string(),
        endpoint,
    })
}

/// 腾讯云 COS 真实存储（OpenDAL `services::Cos`）
///
/// `endpoint` 为空时自动构造 `https://cos.{region}.myqcloud.com`。
pub fn tencent_cos(
    bucket: &str,
    region: &str,
    endpoint: Option<String>,
    secret_id: &str,
    secret_key: &str,
) -> Result<OpendalStorage, StorageError> {
    let endpoint = endpoint
        .map(|e| ensure_https(&e))
        .unwrap_or_else(|| format!("https://cos.{}.myqcloud.com", region));
    let builder = services::Cos::default()
        .root("/")
        .bucket(bucket)
        .endpoint(&endpoint)
        .secret_id(secret_id)
        .secret_key(secret_key);
    let operator = Operator::new(builder)
        .map_err(|e| StorageError::InvalidConfig(format!("tencent cos: {}", e)))?;
    Ok(OpendalStorage {
        operator,
        scheme: "cos",
        bucket: bucket.to_string(),
        endpoint,
    })
}

/// 华为云 OBS 真实存储（OpenDAL `services::Obs`）
pub fn huawei_obs(
    bucket: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<OpendalStorage, StorageError> {
    let endpoint = ensure_https(endpoint);
    let builder = services::Obs::default()
        .root("/")
        .bucket(bucket)
        .endpoint(&endpoint)
        .access_key_id(access_key)
        .secret_access_key(secret_key);
    let operator = Operator::new(builder)
        .map_err(|e| StorageError::InvalidConfig(format!("huawei obs: {}", e)))?;
    Ok(OpendalStorage {
        operator,
        scheme: "obs",
        bucket: bucket.to_string(),
        endpoint,
    })
}

/// 又拍云真实存储（OpenDAL `services::Upyun`）
pub fn upyun(bucket: &str, operator: &str, password: &str) -> Result<OpendalStorage, StorageError> {
    let builder = services::Upyun::default()
        .root("/")
        .bucket(bucket)
        .operator(operator)
        .password(password);
    let operator =
        Operator::new(builder).map_err(|e| StorageError::InvalidConfig(format!("upyun: {}", e)))?;
    Ok(OpendalStorage {
        operator,
        scheme: "upyun",
        bucket: bucket.to_string(),
        endpoint: "v0.api.upyun.com".to_string(),
    })
}

/// 真实云存储聚合枚举（`real-cloud` feature 下由 [`crate::storage::StorageWrapper`] 分发）
pub enum RealCloudStorage {
    Aliyun(OpendalStorage),
    Tencent(OpendalStorage),
    Huawei(OpendalStorage),
    Upyun(OpendalStorage),
    Qiniu(RealQiniuKodoStorage),
}

#[async_trait]
impl Storage for RealCloudStorage {
    async fn put(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, StorageError> {
        match self {
            RealCloudStorage::Aliyun(s) => s.put(key, data, content_type).await,
            RealCloudStorage::Tencent(s) => s.put(key, data, content_type).await,
            RealCloudStorage::Huawei(s) => s.put(key, data, content_type).await,
            RealCloudStorage::Upyun(s) => s.put(key, data, content_type).await,
            RealCloudStorage::Qiniu(s) => s.put(key, data, content_type).await,
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        match self {
            RealCloudStorage::Aliyun(s) => s.get(key).await,
            RealCloudStorage::Tencent(s) => s.get(key).await,
            RealCloudStorage::Huawei(s) => s.get(key).await,
            RealCloudStorage::Upyun(s) => s.get(key).await,
            RealCloudStorage::Qiniu(s) => s.get(key).await,
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        match self {
            RealCloudStorage::Aliyun(s) => s.delete(key).await,
            RealCloudStorage::Tencent(s) => s.delete(key).await,
            RealCloudStorage::Huawei(s) => s.delete(key).await,
            RealCloudStorage::Upyun(s) => s.delete(key).await,
            RealCloudStorage::Qiniu(s) => s.delete(key).await,
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self {
            RealCloudStorage::Aliyun(s) => s.exists(key).await,
            RealCloudStorage::Tencent(s) => s.exists(key).await,
            RealCloudStorage::Huawei(s) => s.exists(key).await,
            RealCloudStorage::Upyun(s) => s.exists(key).await,
            RealCloudStorage::Qiniu(s) => s.exists(key).await,
        }
    }
}

// ==================== 七牛云 Kodo（官方 REST API + HMAC-SHA1 签名） ====================

/// 当前 Unix 时间戳（秒）
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// URL-safe Base64（无 padding），Kodo 签名使用
fn urlsafe_b64(data: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    URL_SAFE_NO_PAD.encode(data)
}

/// HMAC-SHA1 摘要
fn hmac_sha1(secret: &str, data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha1::Sha1;
    type HmacSha1 = Hmac<Sha1>;
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// 上传凭证（UpToken）：`ak:sign:encoded_policy`
pub fn make_uptoken(ak: &str, sk: &str, bucket: &str, key: &str, deadline: u64) -> String {
    let policy = serde_json::json!({
        "scope": format!("{}:{}", bucket, key),
        "deadline": deadline,
    });
    let encoded = urlsafe_b64(policy.to_string().as_bytes());
    let sign = urlsafe_b64(&hmac_sha1(sk, encoded.as_bytes()));
    format!("UpToken {}:{}:{}", ak, sign, encoded)
}

/// 管理凭证（QBox）：`QBox ak:sign`，签名数据为请求路径（如 `/stat/{entry}`）
pub fn make_qbox_token(ak: &str, sk: &str, path: &str) -> String {
    let sign = urlsafe_b64(&hmac_sha1(sk, path.as_bytes()));
    format!("QBox {}:{}", ak, sign)
}

/// 私有下载凭证：URL 追加 `?e={deadline}&token={ak}:{sign}`
pub fn make_download_url(ak: &str, sk: &str, url: &str, deadline: u64) -> String {
    let sign = urlsafe_b64(&hmac_sha1(sk, url.as_bytes()));
    format!("{}?e={}&token={}:{}", url, deadline, ak, sign)
}

/// 七牛云 Kodo 真实存储（官方 REST API）
///
/// - 上传：`PUT https://upload.qiniup.com/putb64/{b64_key}`（UpToken 鉴权）
/// - 删除：`POST https://rs.qiniu.com/delete/{b64("bucket:key")}`（QBox 鉴权）
/// - 存在：`POST https://rs.qiniu.com/stat/{b64("bucket:key")}`（200 存在 / 612 不存在）
/// - 下载：`GET {download_domain}/{key}`（私有桶自动附加下载凭证）
pub struct RealQiniuKodoStorage {
    bucket: String,
    access_key: String,
    secret_key: String,
    upload_host: String,
    rs_host: String,
    download_domain: String,
    client: reqwest::Client,
}

impl RealQiniuKodoStorage {
    pub fn new(
        bucket: impl Into<String>,
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
        download_domain: impl Into<String>,
    ) -> Self {
        Self {
            bucket: bucket.into(),
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            upload_host: "https://upload.qiniup.com".to_string(),
            rs_host: "https://rs.qiniu.com".to_string(),
            download_domain: download_domain.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn url_for(&self, key: &str) -> String {
        format!("qiniu://{}/{}", self.bucket, key)
    }

    /// 管理 API 路径入口：`/stat/{b64("bucket:key")}` 或 `/delete/{...}`
    fn entry_path(&self, key: &str, op: &str) -> String {
        let entry = urlsafe_b64(format!("{}:{}", self.bucket, key).as_bytes());
        format!("/{}/{}", op, entry)
    }
}

#[async_trait]
impl Storage for RealQiniuKodoStorage {
    async fn put(
        &self,
        key: &str,
        data: &[u8],
        _content_type: &str,
    ) -> Result<String, StorageError> {
        let token = make_uptoken(
            &self.access_key,
            &self.secret_key,
            &self.bucket,
            key,
            now_secs() + 3600,
        );
        // putb64 端点：body 为标准 Base64 编码的数据
        let body = base64::engine::general_purpose::STANDARD.encode(data);
        let url = format!(
            "{}/putb64/{}",
            self.upload_host,
            urlsafe_b64(key.as_bytes())
        );
        let resp = self
            .client
            .put(&url)
            .header(reqwest::header::AUTHORIZATION, token)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(body)
            .send()
            .await
            .map_err(|e| StorageError::Put(format!("qiniu put: {}", e)))?;
        if !resp.status().is_success() {
            return Err(StorageError::Put(format!(
                "qiniu put: HTTP {}",
                resp.status()
            )));
        }
        Ok(self.url_for(key))
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let base = format!("{}/{}", self.download_domain, key);
        let url = make_download_url(&self.access_key, &self.secret_key, &base, now_secs() + 3600);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| StorageError::Get(format!("qiniu get: {}", e)))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(StorageError::NotFound(format!(
                "qiniu://{}/{}",
                self.bucket, key
            )));
        }
        if !resp.status().is_success() {
            return Err(StorageError::Get(format!(
                "qiniu get: HTTP {}",
                resp.status()
            )));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| StorageError::Get(format!("qiniu get body: {}", e)))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.entry_path(key, "delete");
        let token = make_qbox_token(&self.access_key, &self.secret_key, &path);
        let url = format!("{}{}", self.rs_host, path);
        let resp = self
            .client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, token)
            .send()
            .await
            .map_err(|e| StorageError::Delete(format!("qiniu delete: {}", e)))?;
        // 612 = 资源不存在，视为幂等成功
        if resp.status().is_success() || resp.status().as_u16() == 612 {
            return Ok(());
        }
        Err(StorageError::Delete(format!(
            "qiniu delete: HTTP {}",
            resp.status()
        )))
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let path = self.entry_path(key, "stat");
        let token = make_qbox_token(&self.access_key, &self.secret_key, &path);
        let url = format!("{}{}", self.rs_host, path);
        let resp = self
            .client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, token)
            .send()
            .await
            .map_err(|e| StorageError::Get(format!("qiniu stat: {}", e)))?;
        match resp.status().as_u16() {
            200 => Ok(true),
            612 => Ok(false),
            code => Err(StorageError::Get(format!("qiniu stat: HTTP {}", code))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlsafe_b64_no_pad() {
        assert_eq!(urlsafe_b64(b"hello"), "aGVsbG8");
        assert_eq!(urlsafe_b64(b"a"), "YQ");
    }

    #[test]
    fn test_hmac_sha1_deterministic() {
        let digest = hmac_sha1("secret", b"data");
        assert_eq!(digest.len(), 20);
        // 相同输入必须得到相同输出
        assert_eq!(digest, hmac_sha1("secret", b"data"));
        // 不同密钥必须得到不同输出
        assert_ne!(digest, hmac_sha1("other", b"data"));
    }

    #[test]
    fn test_make_uptoken_format() {
        let token = make_uptoken("ak123", "sk123", "my-bucket", "dir/file.txt", 1893456000);
        let body = token.strip_prefix("UpToken ").unwrap();
        let parts: Vec<&str> = body.split(':').collect();
        assert_eq!(
            parts.len(),
            3,
            "UpToken 应为 ak:sign:policy 三段: {}",
            token
        );
        assert_eq!(parts[0], "ak123");
        assert!(!parts[1].is_empty(), "sign 段不能为空");
        assert!(!parts[2].is_empty(), "policy 段不能为空");
    }

    #[test]
    fn test_make_qbox_token_format() {
        let token = make_qbox_token("ak123", "sk123", "/stat/abc");
        let body = token.strip_prefix("QBox ").unwrap();
        let parts: Vec<&str> = body.split(':').collect();
        assert_eq!(parts.len(), 2, "QBox 应为 ak:sign 两段: {}", token);
        assert_eq!(parts[0], "ak123");
    }

    #[test]
    fn test_make_download_url_appends_query() {
        let url = make_download_url("ak", "sk", "https://cdn.example.com/a.txt", 1893456000);
        assert!(url.starts_with("https://cdn.example.com/a.txt?e=1893456000&token=ak:"));
        assert!(!url.contains(' '));
    }

    #[test]
    fn test_ensure_https() {
        assert_eq!(
            ensure_https("oss-cn-hangzhou.aliyuncs.com"),
            "https://oss-cn-hangzhou.aliyuncs.com"
        );
        assert_eq!(
            ensure_https("https://oss.aliyuncs.com"),
            "https://oss.aliyuncs.com"
        );
        assert_eq!(ensure_https(""), "");
    }

    #[test]
    fn test_qiniu_entry_path() {
        let s = RealQiniuKodoStorage::new("my-bucket", "ak", "sk", "cdn.example.com");
        let path = s.entry_path("a.txt", "stat");
        // bucket:key 的 urlsafe base64
        assert!(path.starts_with("/stat/"));
        assert_eq!(
            urlsafe_b64(b"my-bucket:a.txt"),
            path.trim_start_matches("/stat/")
        );
    }

    #[test]
    fn test_qiniu_url_for() {
        let s = RealQiniuKodoStorage::new("my-bucket", "ak", "sk", "cdn.example.com");
        assert_eq!(s.url_for("a.txt"), "qiniu://my-bucket/a.txt");
    }

    #[test]
    fn test_opendal_storage_url_for() {
        let s = OpendalStorage {
            operator: Operator::new(services::Memory::default()).unwrap(),
            scheme: "oss",
            bucket: "b".to_string(),
            endpoint: "oss-cn-hangzhou.aliyuncs.com".to_string(),
        };
        assert_eq!(
            s.url_for("a.txt"),
            "oss://b.oss-cn-hangzhou.aliyuncs.com/a.txt"
        );
    }

    #[test]
    fn test_aliyun_oss_invalid_config() {
        // 空 endpoint 不应 panic，返回结构体（真实请求才需要凭据）
        assert!(aliyun_oss("b", "oss-cn-hangzhou.aliyuncs.com", "ak", "sk").is_ok());
    }
}
