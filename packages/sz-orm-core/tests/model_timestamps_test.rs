use sz_orm_core::TimestampFields;

#[test]
fn test_timestamps_new_derives_auto_flags() {
    let ts = TimestampFields::new(Some("created_at"), Some("updated_at"));
    assert_eq!(ts.created_at, Some("created_at"));
    assert_eq!(ts.updated_at, Some("updated_at"));
    assert!(ts.auto_now_insert);
    assert!(ts.auto_now_update);

    let ts_none = TimestampFields::new(None, None);
    assert!(ts_none.created_at.is_none());
    assert!(ts_none.updated_at.is_none());
    assert!(!ts_none.auto_now_insert);
    assert!(!ts_none.auto_now_update);

    let ts_partial = TimestampFields::new(Some("created_at"), None);
    assert!(ts_partial.auto_now_insert);
    assert!(!ts_partial.auto_now_update);

    let ts_partial2 = TimestampFields::new(None, Some("updated_at"));
    assert!(!ts_partial2.auto_now_insert);
    assert!(ts_partial2.auto_now_update);
}

#[test]
fn test_timestamps_with_both_enables_both() {
    let ts = TimestampFields::with_both("created_at", "updated_at");
    assert_eq!(ts.created_at, Some("created_at"));
    assert_eq!(ts.updated_at, Some("updated_at"));
    assert!(ts.auto_now_insert);
    assert!(ts.auto_now_update);
}
