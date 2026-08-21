//! # PostGIS Adapter — sz-orm-core 空间数据库适配层
//!
//! v5.0.0 M4：将 sz-orm-postgis 的 MemoryPostgis 接入 sz-orm-core，
//! 提供 `postgis_st_distance` / `postgis_st_contains` / `postgis_query_count` 入口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_postgis::{Geometry, MemoryPostgis, Point, PostgisExt};

static POSTGIS: OnceLock<RwLock<MemoryPostgis>> = OnceLock::new();
static QUERY_COUNT: AtomicU64 = AtomicU64::new(0);
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn postgis() -> &'static RwLock<MemoryPostgis> {
    POSTGIS.get_or_init(|| RwLock::new(MemoryPostgis::new()))
}

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
    })
}

/// 计算两个几何体之间的距离
pub fn postgis_st_distance(
    g1: &Geometry,
    g2: &Geometry,
) -> Result<f64, sz_orm_postgis::PostgisError> {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let pg = postgis().read();
    runtime().block_on(pg.st_distance(g1, g2))
}

/// 判断 outer 是否包含 inner
pub fn postgis_st_contains(
    outer: &Geometry,
    inner: &Geometry,
) -> Result<bool, sz_orm_postgis::PostgisError> {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let pg = postgis().read();
    runtime().block_on(pg.st_contains(outer, inner))
}

/// 获取查询计数
pub fn postgis_query_count() -> u64 {
    QUERY_COUNT.load(Ordering::Relaxed)
}

/// 创建 Point 几何体（便捷方法）
pub fn postgis_point(x: f64, y: f64) -> Geometry {
    Geometry::Point(Point::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgis_st_distance() {
        let p1 = postgis_point(0.0, 0.0);
        let p2 = postgis_point(3.0, 4.0);
        let dist = postgis_st_distance(&p1, &p2).unwrap();
        assert!(dist > 0.0, "distance should be positive, got {}", dist);
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
}
