//! 内存实现：纯 Rust 几何计算（不连接数据库）
//!
//! 适用于：
//! - 单元测试
//! - 不需要真实 PostGIS 的场景（如距离/面积计算）
//! - 性能基准（无 I/O 开销）

use crate::error::PostgisError;
use crate::geometry::{Geometry, Point, Polygon};
use crate::postgis::PostgisExt;
use async_trait::async_trait;

/// 内存 PostGIS 实现
pub struct MemoryPostgis;

impl MemoryPostgis {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemoryPostgis {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PostgisExt for MemoryPostgis {
    async fn st_distance(&self, g1: &Geometry, g2: &Geometry) -> Result<f64, PostgisError> {
        match (g1, g2) {
            (Geometry::Point(p1), Geometry::Point(p2)) => {
                check_srid(p1.srid, p2.srid)?;
                Ok(p1.haversine_distance(p2))
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_distance only supports Point-Point in memory impl, got {}-{}",
                g1.type_name(),
                g2.type_name()
            ))),
        }
    }

    async fn st_contains(&self, outer: &Geometry, inner: &Geometry) -> Result<bool, PostgisError> {
        match (outer, inner) {
            (Geometry::Polygon(poly), Geometry::Point(p)) => {
                check_srid(poly.srid, p.srid)?;
                Ok(poly.contains_point(p))
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_contains only supports Polygon-Point in memory impl, got {}-{}",
                outer.type_name(),
                inner.type_name()
            ))),
        }
    }

    async fn st_within(&self, inner: &Geometry, outer: &Geometry) -> Result<bool, PostgisError> {
        // st_within(a, b) == st_contains(b, a)
        self.st_contains(outer, inner).await
    }

    async fn st_intersects(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        match (g1, g2) {
            (Geometry::Polygon(poly), Geometry::Point(p)) => Ok(poly.contains_point(p)),
            (Geometry::Point(p), Geometry::Polygon(poly)) => Ok(poly.contains_point(p)),
            (Geometry::Point(p1), Geometry::Point(p2)) => {
                check_srid(p1.srid, p2.srid)?;
                Ok(p1.x == p2.x && p1.y == p2.y)
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_intersects not supported for {}-{} in memory impl",
                g1.type_name(),
                g2.type_name()
            ))),
        }
    }

    async fn st_area(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        match geom {
            Geometry::Polygon(poly) => Ok(poly.shoelace_area()),
            _ => Err(PostgisError::Unsupported(format!(
                "st_area only supports Polygon in memory impl, got {}",
                geom.type_name()
            ))),
        }
    }

    async fn st_length(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        match geom {
            Geometry::LineString(ls) => Ok(ls.haversine_length()),
            _ => Err(PostgisError::Unsupported(format!(
                "st_length only supports LineString in memory impl, got {}",
                geom.type_name()
            ))),
        }
    }

    async fn st_buffer(&self, geom: &Geometry, distance: f64) -> Result<Geometry, PostgisError> {
        match geom {
            Geometry::Point(p) => {
                // 简化实现：生成一个 32 边的正多边形近似圆
                let mut ring = Vec::with_capacity(33);
                let steps = 32;
                // 经度调整：1 度经度距离取决于纬度
                let lat_rad = p.y.to_radians();
                let dx = distance / (111_320.0 * lat_rad.cos());
                let dy = distance / 110_540.0;
                for i in 0..steps {
                    let angle = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
                    let x = p.x + dx * angle.cos();
                    let y = p.y + dy * angle.sin();
                    ring.push(Point::with_srid(x, y, p.srid));
                }
                // 闭合环
                ring.push(ring[0]);
                Ok(Geometry::Polygon(Polygon::new(ring)))
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_buffer only supports Point in memory impl, got {}",
                geom.type_name()
            ))),
        }
    }

    async fn st_union(&self, g1: &Geometry, g2: &Geometry) -> Result<Geometry, PostgisError> {
        match (g1, g2) {
            (Geometry::Point(_), Geometry::Point(_)) => {
                // 简化：返回 MultiPoint
                match (g1, g2) {
                    (Geometry::Point(p1), Geometry::Point(p2)) => {
                        check_srid(p1.srid, p2.srid)?;
                        Ok(Geometry::MultiPoint(vec![*p1, *p2]))
                    }
                    _ => unreachable!(),
                }
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_union only supports Point-Point in memory impl, got {}-{}",
                g1.type_name(),
                g2.type_name()
            ))),
        }
    }

    async fn add_geometry_column(
        &self,
        _table: &str,
        _column: &str,
        _srid: i32,
        _geom_type: &str,
        _dim: &str,
    ) -> Result<(), PostgisError> {
        // 内存实现：no-op
        Ok(())
    }

    async fn create_spatial_index(&self, _table: &str, _column: &str) -> Result<(), PostgisError> {
        // 内存实现：no-op
        Ok(())
    }
}

fn check_srid(srid1: i32, srid2: i32) -> Result<(), PostgisError> {
    if srid1 != srid2 {
        Err(PostgisError::SridMismatch {
            expected: srid1,
            actual: srid2,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{LineString, DEFAULT_SRID};

    #[tokio::test]
    async fn test_memory_distance() {
        let pg = MemoryPostgis::new();
        let p1 = Geometry::Point(Point::new(116.404, 39.915)); // 北京
        let p2 = Geometry::Point(Point::new(121.474, 31.230)); // 上海
        let dist = pg.st_distance(&p1, &p2).await.unwrap();
        assert!(dist > 1_000_000.0 && dist < 1_200_000.0); // ~1067 km
    }

    #[tokio::test]
    async fn test_memory_contains() {
        let pg = MemoryPostgis::new();
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]));
        let inside = Geometry::Point(Point::new(5.0, 5.0));
        let outside = Geometry::Point(Point::new(15.0, 15.0));
        assert!(pg.st_contains(&poly, &inside).await.unwrap());
        assert!(!pg.st_contains(&poly, &outside).await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_area() {
        let pg = MemoryPostgis::new();
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 3.0),
            Point::new(0.0, 3.0),
        ]));
        let area = pg.st_area(&poly).await.unwrap();
        assert!((area - 12.0).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_memory_length() {
        let pg = MemoryPostgis::new();
        let ls = Geometry::LineString(LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(3.0, 4.0),
        ]));
        // haversine 距离（经纬度坐标）：3°经 + 4°纬 ≈ 555 km
        let len = pg.st_length(&ls).await.unwrap();
        assert!(
            len > 500_000.0 && len < 600_000.0,
            "expected ~555km, got {}",
            len
        );
    }

    #[tokio::test]
    async fn test_memory_buffer() {
        let pg = MemoryPostgis::new();
        let p = Geometry::Point(Point::new(116.404, 39.915));
        let buffer = pg.st_buffer(&p, 1_000.0).await.unwrap(); // 1km
        match buffer {
            Geometry::Polygon(poly) => {
                assert_eq!(poly.rings[0].len(), 33); // 32 边 + 闭合点
            }
            _ => panic!("expected Polygon"),
        }
    }

    #[tokio::test]
    async fn test_memory_union() {
        let pg = MemoryPostgis::new();
        let p1 = Geometry::Point(Point::new(0.0, 0.0));
        let p2 = Geometry::Point(Point::new(1.0, 1.0));
        let result = pg.st_union(&p1, &p2).await.unwrap();
        match result {
            Geometry::MultiPoint(pts) => assert_eq!(pts.len(), 2),
            _ => panic!("expected MultiPoint"),
        }
    }

    #[tokio::test]
    async fn test_memory_srid_mismatch() {
        let pg = MemoryPostgis::new();
        let p1 = Geometry::Point(Point::with_srid(0.0, 0.0, 4326));
        let p2 = Geometry::Point(Point::with_srid(1.0, 1.0, 3857));
        let result = pg.st_distance(&p1, &p2).await;
        assert!(matches!(result, Err(PostgisError::SridMismatch { .. })));
    }

    #[tokio::test]
    async fn test_memory_unsupported() {
        let pg = MemoryPostgis::new();
        let ls = Geometry::LineString(LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ]));
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ]));
        // st_distance 仅支持 Point-Point
        let result = pg.st_distance(&ls, &poly).await;
        assert!(matches!(result, Err(PostgisError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_memory_noop_ddl() {
        let pg = MemoryPostgis::new();
        pg.add_geometry_column("t", "geom", DEFAULT_SRID, "POINT", "2")
            .await
            .unwrap();
        pg.create_spatial_index("t", "geom").await.unwrap();
    }
}
