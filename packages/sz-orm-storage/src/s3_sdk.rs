#![cfg(feature = "s3-sdk")]

use crate::error::StorageError;
use crate::storage::Storage;
use async_trait::async_trait;
use s3::{creds::Credentials, Bucket, Region};

pub struct S3SdkStorage {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    bucket_handle: Box<Bucket>,
}

impl S3SdkStorage {
    pub fn new(
        bucket: impl Into<String>,
        region: impl Into<String>,
        endpoint: Option<String>,
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Result<Self, StorageError> {
        let bucket_name: String = bucket.into();
        let region_str: String = region.into();
        let endpoint_str = endpoint.clone();

        let region = match endpoint_str {
            Some(ep) => Region::Custom {
                region: region_str.clone(),
                endpoint: ep,
            },
            None => Region::Custom {
                region: region_str.clone(),
                endpoint: format!("https://s3.{}.amazonaws.com", region_str),
            },
        };

        let access_key_str: String = access_key.into();
        let secret_key_str: String = secret_key.into();

        let credentials = Credentials::new(
            Some(&access_key_str),
            Some(&secret_key_str),
            None,
            None,
            None,
        )
        .map_err(|e| StorageError::InvalidConfig(format!("create credentials: {}", e)))?;

        let bucket_handle = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| StorageError::InvalidConfig(format!("create bucket: {}", e)))?;

        Ok(Self {
            bucket: bucket_name,
            region: region_str,
            endpoint: endpoint,
            bucket_handle: bucket_handle.with_path_style(),
        })
    }

    pub fn url_for(&self, key: &str) -> String {
        match &self.endpoint {
            Some(ep) => format!("{}/{}/{}", ep.trim_end_matches('/'), self.bucket, key),
            None => format!("s3://{}.{}.amazonaws.com/{}", self.bucket, self.region, key),
        }
    }
}

#[async_trait]
impl Storage for S3SdkStorage {
    async fn put(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, StorageError> {
        let resp = self
            .bucket_handle
            .put_object_with_content_type(key, data, content_type)
            .await
            .map_err(|e| StorageError::Put(format!("s3 put: {}", e)))?;

        if resp.status_code() >= 200 && resp.status_code() < 300 {
            Ok(self.url_for(key))
        } else {
            Err(StorageError::Put(format!(
                "s3 put status {}: {}",
                resp.status_code(),
                String::from_utf8_lossy(resp.as_slice())
            )))
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let resp = self.bucket_handle.get_object(key).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("404") || msg.to_lowercase().contains("not found") {
                StorageError::NotFound(key.to_string())
            } else {
                StorageError::Get(format!("s3 get: {}", msg))
            }
        })?;

        if resp.status_code() == 404 {
            return Err(StorageError::NotFound(key.to_string()));
        }

        if resp.status_code() >= 200 && resp.status_code() < 300 {
            Ok(resp.to_vec())
        } else {
            Err(StorageError::Get(format!(
                "s3 get status {}: {}",
                resp.status_code(),
                String::from_utf8_lossy(resp.as_slice())
            )))
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let resp = self
            .bucket_handle
            .delete_object(key)
            .await
            .map_err(|e| StorageError::Delete(format!("s3 delete: {}", e)))?;

        if resp.status_code() >= 200 && resp.status_code() < 300 {
            Ok(())
        } else {
            Err(StorageError::Delete(format!(
                "s3 delete status {}: {}",
                resp.status_code(),
                String::from_utf8_lossy(resp.as_slice())
            )))
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self.bucket_handle.head_object(key).await {
            Ok((_, status)) => Ok(status >= 200 && status < 300),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("404") || msg.to_lowercase().contains("not found") {
                    Ok(false)
                } else {
                    Err(StorageError::Get(format!("s3 head: {}", msg)))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_sdk_storage_new() {
        let storage = S3SdkStorage::new(
            "my-bucket",
            "us-east-1",
            Some("http://localhost:9000".to_string()),
            "access_key",
            "secret_key",
        );
        assert!(storage.is_ok());
        let storage = storage.unwrap();
        assert_eq!(storage.bucket, "my-bucket");
        assert_eq!(storage.region, "us-east-1");
        assert_eq!(storage.endpoint.as_deref(), Some("http://localhost:9000"));
    }

    #[test]
    fn test_s3_sdk_storage_new_without_endpoint() {
        let storage = S3SdkStorage::new("my-bucket", "us-east-1", None, "access_key", "secret_key");
        assert!(storage.is_ok());
    }

    #[test]
    fn test_s3_sdk_storage_url_for_with_endpoint() {
        let storage = S3SdkStorage::new(
            "my-bucket",
            "us-east-1",
            Some("http://localhost:9000".to_string()),
            "ak",
            "sk",
        )
        .unwrap();
        let url = storage.url_for("path/to/file.txt");
        assert_eq!(url, "http://localhost:9000/my-bucket/path/to/file.txt");
    }

    #[test]
    fn test_s3_sdk_storage_url_for_with_trailing_slash() {
        let storage = S3SdkStorage::new(
            "my-bucket",
            "us-east-1",
            Some("http://localhost:9000/".to_string()),
            "ak",
            "sk",
        )
        .unwrap();
        let url = storage.url_for("file.txt");
        assert_eq!(url, "http://localhost:9000/my-bucket/file.txt");
    }

    #[test]
    fn test_s3_sdk_storage_url_for_without_endpoint() {
        let storage = S3SdkStorage::new("my-bucket", "us-west-2", None, "ak", "sk").unwrap();
        let url = storage.url_for("file.txt");
        assert_eq!(url, "s3://my-bucket.us-west-2.amazonaws.com/file.txt");
    }

    #[tokio::test]
    #[ignore = "requires a real S3 or MinIO endpoint"]
    async fn test_s3_sdk_storage_put_not_connected_fails() {
        let storage = S3SdkStorage::new(
            "my-bucket",
            "us-east-1",
            Some("http://127.0.0.1:1".to_string()),
            "ak",
            "sk",
        )
        .unwrap();
        let result = storage.put("k.txt", b"data", "text/plain").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires a real S3 or MinIO endpoint"]
    async fn test_s3_sdk_storage_get_not_connected_fails() {
        let storage = S3SdkStorage::new(
            "my-bucket",
            "us-east-1",
            Some("http://127.0.0.1:1".to_string()),
            "ak",
            "sk",
        )
        .unwrap();
        let result = storage.get("k.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires a real S3 or MinIO endpoint"]
    async fn test_s3_sdk_real_put_get_delete() {
        let endpoint = std::env::var("S3_TEST_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:9000".to_string());
        let bucket = std::env::var("S3_TEST_BUCKET").unwrap_or_else(|_| "test-bucket".to_string());
        let region = std::env::var("S3_TEST_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let access_key =
            std::env::var("S3_TEST_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
        let secret_key =
            std::env::var("S3_TEST_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());

        let storage =
            S3SdkStorage::new(&bucket, &region, Some(endpoint), &access_key, &secret_key).unwrap();

        let key = format!("s3_sdk_test_{}.txt", uuid_simple());
        let data = b"hello s3 sdk";

        let url = storage.put(&key, data, "text/plain").await.unwrap();
        assert!(url.contains(&key));

        let fetched = storage.get(&key).await.unwrap();
        assert_eq!(fetched, data);

        assert!(storage.exists(&key).await.unwrap());

        storage.delete(&key).await.unwrap();
        assert!(!storage.exists(&key).await.unwrap());
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
