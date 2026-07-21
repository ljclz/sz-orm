//! # SZ-ORM Storage — 对象存储
//!
//! 提供统一的对象存储抽象，支持 S3、阿里云 OSS、七牛 Kodo、华为 OBS、
//! 腾讯 COS、又拍云以及本地文件系统，可通过 `StorageBuilder` 配置多 provider。
//!
//! ## 主要模块
//!
//! - [`storage`] — 统一 trait 与构建器
//! - [`s3`] / [`aliyun`] / [`huawei`] / [`tencent`] / [`qiniu`] / [`upyun`] / [`local`] — 各 provider 实现

pub mod error;
pub mod storage;

pub mod aliyun;
pub mod huawei;
pub mod local;
pub mod qiniu;
pub mod s3;
pub mod tencent;
pub mod upyun;

pub use error::StorageError;
pub use storage::*;

pub use storage::StorageBuilder;
pub use storage::StorageConfig;
pub use storage::StorageProvider;
pub use storage::StorageWrapper;

pub use aliyun::AliyunOssStorage;
pub use huawei::HuaweiObsStorage;
pub use local::LocalStorage;
pub use qiniu::QiniuKodoStorage;
pub use s3::S3Storage;
pub use tencent::TencentCosStorage;
pub use upyun::UpYunStorage;

#[cfg(feature = "s3-sdk")]
pub mod s3_sdk;

#[cfg(feature = "s3-sdk")]
pub use s3_sdk::S3SdkStorage;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_storage_put_and_get() {
        let temp_dir = std::env::temp_dir().join(format!("storage_test_{}", uuid_simple()));
        let storage = LocalStorage::new(temp_dir.to_string_lossy());

        let key = "test.txt";
        let data = b"Hello, World!";
        let content_type = "text/plain";

        storage.put(key, data, content_type).await.unwrap();

        let retrieved = storage.get(key).await.unwrap();
        assert_eq!(retrieved, data);

        tokio::fs::remove_dir_all(&temp_dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_storage_delete() {
        let temp_dir = std::env::temp_dir().join(format!("storage_test_{}", uuid_simple()));
        let storage = LocalStorage::new(temp_dir.to_string_lossy());

        let key = "delete_me.txt";
        storage.put(key, b"test", "text/plain").await.unwrap();

        storage.delete(key).await.unwrap();

        let exists = storage.exists(key).await.unwrap();
        assert!(!exists);

        tokio::fs::remove_dir_all(&temp_dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_storage_exists() {
        let temp_dir = std::env::temp_dir().join(format!("storage_test_{}", uuid_simple()));
        let storage = LocalStorage::new(temp_dir.to_string_lossy());

        let key = "exists.txt";
        assert!(!storage.exists(key).await.unwrap());

        storage.put(key, b"test", "text/plain").await.unwrap();
        assert!(storage.exists(key).await.unwrap());

        tokio::fs::remove_dir_all(&temp_dir).await.ok();
    }

    #[tokio::test]
    async fn test_local_storage_not_found() {
        let temp_dir = std::env::temp_dir().join(format!("storage_test_{}", uuid_simple()));
        let storage = LocalStorage::new(temp_dir.to_string_lossy());

        let result = storage.get("nonexistent.txt").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_storage_builder_local() {
        let builder = StorageBuilder::new(StorageProvider::Local)
            .with_bucket("test-bucket")
            .with_path_prefix("prefix/");

        let wrapper = builder.build().unwrap();
        assert!(matches!(wrapper, StorageWrapper::Local(_)));
    }

    #[tokio::test]
    async fn test_storage_builder_s3() {
        let builder = StorageBuilder::new(StorageProvider::S3(S3Config::default()))
            .with_bucket("test-bucket")
            .with_region("us-west-2")
            .with_access_key("ak")
            .with_secret_key("sk");

        let wrapper = builder.build().unwrap();
        assert!(matches!(wrapper, StorageWrapper::S3(_)));
    }

    #[tokio::test]
    async fn test_storage_builder_aliyun() {
        let builder = StorageBuilder::new(StorageProvider::AliyunOss(AliyunConfig::default()))
            .with_bucket("test-bucket")
            .with_endpoint("oss-cn-hangzhou.aliyuncs.com")
            .with_access_key("ak")
            .with_secret_key("sk");

        let wrapper = builder.build().unwrap();
        assert!(matches!(wrapper, StorageWrapper::Aliyun(_)));
    }

    #[tokio::test]
    async fn test_storage_builder_tencent() {
        let builder = StorageBuilder::new(StorageProvider::TencentCos(TencentConfig::default()))
            .with_bucket("test-bucket")
            .with_region("ap-guangzhou");

        let wrapper = builder.build().unwrap();
        assert!(matches!(wrapper, StorageWrapper::Tencent(_)));
    }

    #[tokio::test]
    async fn test_storage_builder_qiniu() {
        let builder = StorageBuilder::new(StorageProvider::QiniuKodo(QiniuConfig::default()))
            .with_bucket("test-bucket");

        let wrapper = builder.build().unwrap();
        assert!(matches!(wrapper, StorageWrapper::Qiniu(_)));
    }

    #[tokio::test]
    async fn test_storage_builder_huawei() {
        let builder = StorageBuilder::new(StorageProvider::HuaweiObs(HuaweiConfig::default()))
            .with_bucket("test-bucket")
            .with_endpoint("obs.cn-north-4.myhuaweicloud.com");

        let wrapper = builder.build().unwrap();
        assert!(matches!(wrapper, StorageWrapper::Huawei(_)));
    }

    #[tokio::test]
    async fn test_storage_builder_upyun() {
        let builder = StorageBuilder::new(StorageProvider::UpYun(UpYunConfig::default()))
            .with_bucket("test-bucket");

        let wrapper = builder.build().unwrap();
        assert!(matches!(wrapper, StorageWrapper::Upyun(_)));
    }

    #[tokio::test]
    async fn test_builder_s3_actually_works() {
        let wrapper = StorageBuilder::new(StorageProvider::S3(S3Config::default()))
            .with_bucket("test-bucket")
            .with_region("us-west-2")
            .build()
            .unwrap();

        let url = wrapper
            .put("k.txt", b"s3-via-builder", "text/plain")
            .await
            .unwrap();
        assert!(url.starts_with("s3://test-bucket.us-west-2/"));

        let data = wrapper.get("k.txt").await.unwrap();
        assert_eq!(data, b"s3-via-builder");

        assert!(wrapper.exists("k.txt").await.unwrap());
        wrapper.delete("k.txt").await.unwrap();
        assert!(!wrapper.exists("k.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_builder_aliyun_actually_works() {
        let wrapper = StorageBuilder::new(StorageProvider::AliyunOss(AliyunConfig::default()))
            .with_bucket("bucket")
            .with_endpoint("oss-cn-hangzhou.aliyuncs.com")
            .build()
            .unwrap();

        wrapper
            .put("k", b"aliyun-via-builder", "text/plain")
            .await
            .unwrap();
        assert_eq!(wrapper.get("k").await.unwrap(), b"aliyun-via-builder");
    }

    #[tokio::test]
    async fn test_builder_tencent_actually_works() {
        let wrapper = StorageBuilder::new(StorageProvider::TencentCos(TencentConfig::default()))
            .with_bucket("bucket")
            .with_region("ap-guangzhou")
            .build()
            .unwrap();

        wrapper
            .put("k", b"tencent-via-builder", "text/plain")
            .await
            .unwrap();
        assert_eq!(wrapper.get("k").await.unwrap(), b"tencent-via-builder");
        assert!(wrapper.exists("k").await.unwrap());
    }

    #[tokio::test]
    async fn test_builder_qiniu_actually_works() {
        let wrapper = StorageBuilder::new(StorageProvider::QiniuKodo(QiniuConfig::default()))
            .with_bucket("bucket")
            .build()
            .unwrap();

        wrapper
            .put("k", b"qiniu-via-builder", "text/plain")
            .await
            .unwrap();
        assert_eq!(wrapper.get("k").await.unwrap(), b"qiniu-via-builder");
    }

    #[tokio::test]
    async fn test_builder_huawei_actually_works() {
        let wrapper = StorageBuilder::new(StorageProvider::HuaweiObs(HuaweiConfig::default()))
            .with_bucket("bucket")
            .with_endpoint("obs.cn-north-4.myhuaweicloud.com")
            .build()
            .unwrap();

        wrapper
            .put("k", b"huawei-via-builder", "text/plain")
            .await
            .unwrap();
        assert_eq!(wrapper.get("k").await.unwrap(), b"huawei-via-builder");
    }

    #[tokio::test]
    async fn test_builder_upyun_actually_works() {
        let wrapper = StorageBuilder::new(StorageProvider::UpYun(UpYunConfig::default()))
            .with_bucket("bucket")
            .build()
            .unwrap();

        wrapper
            .put("k", b"upyun-via-builder", "text/plain")
            .await
            .unwrap();
        assert_eq!(wrapper.get("k").await.unwrap(), b"upyun-via-builder");
    }

    #[tokio::test]
    async fn test_storage_wrapper_put() {
        let temp_dir = std::env::temp_dir().join(format!("storage_test_{}", uuid_simple()));
        let wrapper = StorageWrapper::Local(LocalStorage::new(temp_dir.to_string_lossy()));

        let url = wrapper
            .put("test.txt", b"data", "text/plain")
            .await
            .unwrap();
        assert!(url.starts_with("local://"));

        tokio::fs::remove_dir_all(&temp_dir).await.ok();
    }

    #[tokio::test]
    async fn test_storage_wrapper_get() {
        let temp_dir = std::env::temp_dir().join(format!("storage_test_{}", uuid_simple()));
        let wrapper = StorageWrapper::Local(LocalStorage::new(temp_dir.to_string_lossy()));

        wrapper
            .put("test.txt", b"data", "text/plain")
            .await
            .unwrap();
        let data = wrapper.get("test.txt").await.unwrap();
        assert_eq!(data, b"data");

        tokio::fs::remove_dir_all(&temp_dir).await.ok();
    }

    #[tokio::test]
    async fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.bucket, "default-bucket");
        assert_eq!(config.region, "us-east-1");
    }

    #[test]
    fn test_storage_config_debug_masks_secrets() {
        let config = StorageConfig {
            bucket: "my-bucket".to_string(),
            region: "us-east-1".to_string(),
            endpoint: Some("https://example.com".to_string()),
            access_key: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
            secret_key: Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string()),
            path_prefix: Some("prefix/".to_string()),
            base_path: Some("/tmp".to_string()),
        };

        let debug_output = format!("{:?}", config);

        // 非敏感字段应正常输出
        assert!(debug_output.contains("my-bucket"));
        assert!(debug_output.contains("us-east-1"));
        // 敏感字段必须被遮掩
        assert!(debug_output.contains("\"***\""));
        assert!(!debug_output.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!debug_output.contains("wJalrXUtnFEMI"));
    }

    fn uuid_simple() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{:x}", now)
    }
}
