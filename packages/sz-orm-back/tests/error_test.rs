use std::error::Error;
use sz_orm_back::BkError;

#[test]
fn test_backup() {
    assert_eq!(
        BkError::Backup("fail".into()).to_string(),
        "Backup error: fail"
    );
}

#[test]
fn test_restore() {
    assert_eq!(
        BkError::Restore("fail".into()).to_string(),
        "Restore error: fail"
    );
}

#[test]
fn test_export() {
    assert_eq!(
        BkError::Export("fail".into()).to_string(),
        "Export error: fail"
    );
}

#[test]
fn test_import() {
    assert_eq!(
        BkError::Import("fail".into()).to_string(),
        "Import error: fail"
    );
}

#[test]
fn test_file_not_found() {
    assert_eq!(
        BkError::FileNotFound("/tmp".into()).to_string(),
        "File not found: /tmp"
    );
}

#[test]
fn test_permission_denied() {
    assert_eq!(
        BkError::PermissionDenied("denied".into()).to_string(),
        "Permission denied: denied"
    );
}

#[test]
fn test_compression() {
    assert_eq!(
        BkError::Compression("bad".into()).to_string(),
        "Compression error: bad"
    );
}

#[test]
fn test_encryption() {
    assert_eq!(
        BkError::Encryption("bad".into()).to_string(),
        "Encryption error: bad"
    );
}

#[test]
fn test_error_trait() {
    let err = BkError::Backup("x".into());
    assert!(err.source().is_none());
}
#[test]
fn test_from_io_error_not_found() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
    let bk: BkError = io_err.into();
    assert!(matches!(bk, BkError::FileNotFound(_)));
}

#[test]
fn test_from_io_error_permission_denied() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no access");
    let bk: BkError = io_err.into();
    assert!(matches!(bk, BkError::PermissionDenied(_)));
}

#[test]
fn test_from_io_error_other() {
    let io_err = std::io::Error::other("misc");
    let bk: BkError = io_err.into();
    assert!(matches!(bk, BkError::Backup(_)));
}
