use crate::aliyun::AliyunOssStorage;
use crate::error::StorageError;
use crate::huawei::HuaweiObsStorage;
use crate::local::LocalStorage;
use crate::qiniu::QiniuKodoStorage;
use crate::s3::S3Storage;
use crate::tencent::TencentCosStorage;
use crate::upyun::UpYunStorage;
use async_trait::async_trait;

#[async_trait]
pub trait Storage: Send + Sync {
    async fn put(&self, key: &str, data: &[u8], content_type: &str)
        -> Result<String, StorageError>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
}

pub struct StorageBuilder {
    provider: StorageProvider,
    config: StorageConfig,
}

impl StorageBuilder {
    pub fn new(provider: StorageProvider) -> Self {
        Self {
            provider,
            config: StorageConfig::default(),
        }
    }

    pub fn with_bucket(mut self, bucket: impl Into<String>) -> Self {
        self.config.bucket = bucket.into();
        self
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.config.region = region.into();
        self
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.config.endpoint = Some(endpoint.into());
        self
    }

    pub fn with_access_key(mut self, key: impl Into<String>) -> Self {
        self.config.access_key = Some(key.into());
        self
    }

    pub fn with_secret_key(mut self, key: impl Into<String>) -> Self {
        self.config.secret_key = Some(key.into());
        self
    }

    pub fn with_path_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.config.path_prefix = Some(prefix.into());
        self
    }

    pub fn with_base_path(mut self, base_path: impl Into<String>) -> Self {
        self.config.base_path = Some(base_path.into());
        self
    }

    pub fn build(self) -> Result<StorageWrapper, StorageError> {
        match self.provider {
            StorageProvider::Local => {
                let base_path = self
                    .config
                    .base_path
                    .clone()
                    .unwrap_or_else(|| ".".to_string());
                Ok(StorageWrapper::Local(LocalStorage::new(base_path)))
            }
            StorageProvider::S3(_) => {
                let bucket = self.config.bucket.clone();
                let region = self.config.region.clone();
                Ok(StorageWrapper::S3(S3Storage::new(bucket, region)))
            }
            StorageProvider::AliyunOss(_) => {
                let bucket = self.config.bucket.clone();
                let endpoint = self.config.endpoint.clone().unwrap_or_default();
                Ok(StorageWrapper::Aliyun(AliyunOssStorage::new(
                    bucket, endpoint,
                )))
            }
            StorageProvider::TencentCos(_) => {
                let bucket = self.config.bucket.clone();
                let region = self.config.region.clone();
                Ok(StorageWrapper::Tencent(TencentCosStorage::new(
                    bucket, region,
                )))
            }
            StorageProvider::QiniuKodo(_) => {
                let bucket = self.config.bucket.clone();
                Ok(StorageWrapper::Qiniu(QiniuKodoStorage::new(bucket)))
            }
            StorageProvider::HuaweiObs(_) => {
                let bucket = self.config.bucket.clone();
                let endpoint = self.config.endpoint.clone().unwrap_or_default();
                Ok(StorageWrapper::Huawei(HuaweiObsStorage::new(
                    bucket, endpoint,
                )))
            }
            StorageProvider::UpYun(_) => {
                let bucket = self.config.bucket.clone();
                Ok(StorageWrapper::Upyun(UpYunStorage::new(bucket)))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub path_prefix: Option<String>,
    pub base_path: Option<String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            bucket: "default-bucket".to_string(),
            region: "us-east-1".to_string(),
            endpoint: None,
            access_key: None,
            secret_key: None,
            path_prefix: None,
            base_path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum StorageProvider {
    Local,
    S3(S3Config),
    AliyunOss(AliyunConfig),
    TencentCos(TencentConfig),
    QiniuKodo(QiniuConfig),
    HuaweiObs(HuaweiConfig),
    UpYun(UpYunConfig),
}

#[derive(Debug, Clone, Default)]
pub struct S3Config {
    pub region: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AliyunConfig {
    pub endpoint: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TencentConfig {
    pub region: String,
    pub secret_id: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QiniuConfig {
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HuaweiConfig {
    pub endpoint: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpYunConfig {
    pub operator: Option<String>,
    pub password: Option<String>,
}

pub enum StorageWrapper {
    Local(LocalStorage),
    S3(S3Storage),
    Aliyun(AliyunOssStorage),
    Tencent(TencentCosStorage),
    Qiniu(QiniuKodoStorage),
    Huawei(HuaweiObsStorage),
    Upyun(UpYunStorage),
}

#[async_trait]
impl Storage for StorageWrapper {
    async fn put(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, StorageError> {
        match self {
            StorageWrapper::Local(s) => s.put(key, data, content_type).await,
            StorageWrapper::S3(s) => s.put(key, data, content_type).await,
            StorageWrapper::Aliyun(s) => s.put(key, data, content_type).await,
            StorageWrapper::Tencent(s) => s.put(key, data, content_type).await,
            StorageWrapper::Qiniu(s) => s.put(key, data, content_type).await,
            StorageWrapper::Huawei(s) => s.put(key, data, content_type).await,
            StorageWrapper::Upyun(s) => s.put(key, data, content_type).await,
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        match self {
            StorageWrapper::Local(s) => s.get(key).await,
            StorageWrapper::S3(s) => s.get(key).await,
            StorageWrapper::Aliyun(s) => s.get(key).await,
            StorageWrapper::Tencent(s) => s.get(key).await,
            StorageWrapper::Qiniu(s) => s.get(key).await,
            StorageWrapper::Huawei(s) => s.get(key).await,
            StorageWrapper::Upyun(s) => s.get(key).await,
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        match self {
            StorageWrapper::Local(s) => s.delete(key).await,
            StorageWrapper::S3(s) => s.delete(key).await,
            StorageWrapper::Aliyun(s) => s.delete(key).await,
            StorageWrapper::Tencent(s) => s.delete(key).await,
            StorageWrapper::Qiniu(s) => s.delete(key).await,
            StorageWrapper::Huawei(s) => s.delete(key).await,
            StorageWrapper::Upyun(s) => s.delete(key).await,
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self {
            StorageWrapper::Local(s) => s.exists(key).await,
            StorageWrapper::S3(s) => s.exists(key).await,
            StorageWrapper::Aliyun(s) => s.exists(key).await,
            StorageWrapper::Tencent(s) => s.exists(key).await,
            StorageWrapper::Qiniu(s) => s.exists(key).await,
            StorageWrapper::Huawei(s) => s.exists(key).await,
            StorageWrapper::Upyun(s) => s.exists(key).await,
        }
    }
}
