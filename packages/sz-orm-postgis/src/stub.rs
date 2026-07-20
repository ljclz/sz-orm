//! Stub 实现：生成 PostGIS SQL 字符串但不执行
//!
//! 适用于：
//! - 调试场景：检查生成的 SQL 是否正确
//! - 不连接数据库的代码审查
//! - SQL 模板生成器

use crate::error::PostgisError;
use crate::geometry::Geometry;
use crate::postgis::PostgisExt;
use async_trait::async_trait;
use std::sync::Mutex;

/// Stub PostGIS 实现：记录生成的 SQL 语句
pub struct StubPostgis {
    /// 已生成的 SQL 语句历史（线程安全）
    pub sql_log: Mutex<Vec<String>>,
}

impl StubPostgis {
    pub fn new() -> Self {
        Self {
            sql_log: Mutex::new(Vec::new()),
        }
    }

    /// 获取已生成的 SQL 历史
    pub fn sql_history(&self) -> Vec<String> {
        self.sql_log.lock().unwrap().clone()
    }

    /// 清空 SQL 历史
    pub fn clear(&self) {
        self.sql_log.lock().unwrap().clear();
    }

    fn log(&self, sql: String) {
        self.sql_log.lock().unwrap().push(sql);
    }
}

impl Default for StubPostgis {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PostgisExt for StubPostgis {
    async fn st_distance(&self, g1: &Geometry, g2: &Geometry) -> Result<f64, PostgisError> {
        let sql = format!(
            "SELECT ST_Distance('{}'::geometry, '{}'::geometry)",
            g1.to_ewkt(),
            g2.to_ewkt()
        );
        self.log(sql);
        Ok(0.0)
    }

    async fn st_contains(&self, outer: &Geometry, inner: &Geometry) -> Result<bool, PostgisError> {
        let sql = format!(
            "SELECT ST_Contains('{}'::geometry, '{}'::geometry)",
            outer.to_ewkt(),
            inner.to_ewkt()
        );
        self.log(sql);
        Ok(false)
    }

    async fn st_within(&self, inner: &Geometry, outer: &Geometry) -> Result<bool, PostgisError> {
        let sql = format!(
            "SELECT ST_Within('{}'::geometry, '{}'::geometry)",
            inner.to_ewkt(),
            outer.to_ewkt()
        );
        self.log(sql);
        Ok(false)
    }

    async fn st_intersects(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        let sql = format!(
            "SELECT ST_Intersects('{}'::geometry, '{}'::geometry)",
            g1.to_ewkt(),
            g2.to_ewkt()
        );
        self.log(sql);
        Ok(false)
    }

    async fn st_area(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        let sql = format!("SELECT ST_Area('{}'::geometry)", geom.to_ewkt());
        self.log(sql);
        Ok(0.0)
    }

    async fn st_length(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        let sql = format!("SELECT ST_Length('{}'::geometry)", geom.to_ewkt());
        self.log(sql);
        Ok(0.0)
    }

    async fn st_buffer(&self, geom: &Geometry, distance: f64) -> Result<Geometry, PostgisError> {
        let sql = format!(
            "SELECT ST_Buffer('{}'::geometry, {})",
            geom.to_ewkt(),
            distance
        );
        self.log(sql);
        // 返回原始几何体（不实际计算）
        Ok(geom.clone())
    }

    async fn st_union(&self, g1: &Geometry, g2: &Geometry) -> Result<Geometry, PostgisError> {
        let sql = format!(
            "SELECT ST_Union('{}'::geometry, '{}'::geometry)",
            g1.to_ewkt(),
            g2.to_ewkt()
        );
        self.log(sql);
        Ok(g1.clone())
    }

    async fn add_geometry_column(
        &self,
        table: &str,
        column: &str,
        srid: i32,
        geom_type: &str,
        dim: &str,
    ) -> Result<(), PostgisError> {
        let sql = format!(
            "SELECT AddGeometryColumn('{}', '{}', {}, '{}', '{}')",
            table, column, srid, geom_type, dim
        );
        self.log(sql);
        Ok(())
    }

    async fn create_spatial_index(&self, table: &str, column: &str) -> Result<(), PostgisError> {
        let sql = format!(
            "CREATE INDEX idx_{}_{} ON {} USING GIST(\"{}\")",
            table, column, table, column
        );
        self.log(sql);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[tokio::test]
    async fn test_stub_generates_sql() {
        let stub = StubPostgis::new();
        let p1 = Geometry::Point(Point::new(0.0, 0.0));
        let p2 = Geometry::Point(Point::new(3.0, 4.0));
        let _ = stub.st_distance(&p1, &p2).await.unwrap();

        let history = stub.sql_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].contains("ST_Distance"));
        assert!(history[0].contains("SRID=4326;POINT"));
    }

    #[tokio::test]
    async fn test_stub_create_spatial_index() {
        let stub = StubPostgis::new();
        stub.create_spatial_index("cities", "geom").await.unwrap();
        let history = stub.sql_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].contains("CREATE INDEX"));
        assert!(history[0].contains("USING GIST"));
        assert!(history[0].contains("idx_cities_geom"));
    }

    #[tokio::test]
    async fn test_stub_add_geometry_column() {
        let stub = StubPostgis::new();
        stub.add_geometry_column("cities", "geom", 4326, "POINT", "2")
            .await
            .unwrap();
        let history = stub.sql_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].contains("AddGeometryColumn"));
        assert!(history[0].contains("cities"));
        assert!(history[0].contains("4326"));
        assert!(history[0].contains("POINT"));
    }

    #[tokio::test]
    async fn test_stub_clear() {
        let stub = StubPostgis::new();
        let p = Geometry::Point(Point::new(0.0, 0.0));
        let _ = stub.st_area(&p).await.unwrap();
        assert_eq!(stub.sql_history().len(), 1);
        stub.clear();
        assert_eq!(stub.sql_history().len(), 0);
    }

    #[tokio::test]
    async fn test_stub_multiple_operations() {
        let stub = StubPostgis::new();
        let p = Geometry::Point(Point::new(0.0, 0.0));
        let _ = stub.st_area(&p).await.unwrap();
        let _ = stub.st_length(&p).await.unwrap();
        stub.create_spatial_index("t", "g").await.unwrap();
        assert_eq!(stub.sql_history().len(), 3);
    }
}
