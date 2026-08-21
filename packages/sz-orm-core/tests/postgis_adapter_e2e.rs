//! PostGIS 适配层端到端测试
use sz_orm_core::postgis_adapter::{postgis_point, postgis_query_count, postgis_st_distance};

#[test]
fn test_postgis_st_distance() {
    let p1 = postgis_point(0.0, 0.0);
    let p2 = postgis_point(3.0, 4.0);
    let dist = postgis_st_distance(&p1, &p2).unwrap();
    assert!(
        dist > 0.0,
        "distance between different points should be positive, got {}",
        dist
    );
}

#[test]
fn test_postgis_count_increments() {
    let before = postgis_query_count();
    let p1 = postgis_point(0.0, 0.0);
    let p2 = postgis_point(1.0, 1.0);
    let _ = postgis_st_distance(&p1, &p2);
    let after = postgis_query_count();
    assert!(after > before);
}
