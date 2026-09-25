use sz_orm_back::BkError;
use std::error::Error;

#[test]
fn test_backup() { assert_eq!(BkError::Backup("fail".into()).to_string(), "Backup error: fail"); }

#[test]
fn test_restore() { assert_eq!(BkError::Restore("fail".into()).to_string(), "Restore error: fail"); }

#[test]
fn test_export() { assert_eq!(BkError::Export("fail".into()).to_string(), "Export error: fail"); }

#[test]
fn test_import() { assert_eq!(BkError::Import("fail".into()).to_string(), "Import error: fail"); }

#[test]
fn test_file_not_found() { assert_eq!(BkError::FileNotFound("/tmp".into()).to_string(), "File not found: /tmp"); }

#[test]
fn test_permission_denied() { assert_eq!(BkError::PermissionDenied("denied".into()).to_string(), "Permission denied: denied"); }

#[test]
fn test_compression() { assert_eq!(BkError::Compression("bad".into()).to_string(), "Compression error: bad"); }

#[test]
fn test_encryption() { assert_eq!(BkError::Encryption("bad".into()).to_string(), "Encryption error: bad"); }

#[test]
fn test_error_trait() { let err = BkError::Backup("x".into()); assert!(err.source().is_none()); }