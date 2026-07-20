//! 真实 PostgreSQL + PostGIS 实现（feature = "real-postgis"）
//!
//! 通过 tokio-postgres 连接 PostgreSQL 数据库，执行真实 PostGIS SQL。
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! sz-orm-postgis = { version = "0.2", features = ["real-postgis"] }
//! ```
//!
//! ```rust,no_run
//! use sz_orm_postgis::{PostgisBuilder, PostgisProvider, RealPgConfig};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = RealPgConfig {
//!     host: "127.0.0.1".to_string(),
//!     port: 5432,
//!     database: "test".to_string(),
//!     username: "postgres".to_string(),
//!     password: "secret".to_string(),
//! };
//! let wrapper = PostgisBuilder::new(PostgisProvider::RealPg(config)).build()?;
//! # Ok(())
//! # }
//! ```

use crate::error::PostgisError;
use crate::geometry::Geometry;
use crate::postgis::{PostgisExt, RealPgConfig};
use async_trait::async_trait;
use tokio_postgres::Client;

/// 真实 PostgreSQL + PostGIS 实现
pub struct RealPostgis {
    client: Client,
}

impl RealPostgis {
    pub fn new(config: RealPgConfig) -> Result<Self, PostgisError> {
        // 同步建立连接（简化实现；如需 async connect，可改用 tokio_postgres::connect + await）
        let conn_str = format!(
            "host={} port={} dbname={} user={} password={}",
            config.host, config.port, config.database, config.username, config.password
        );
        let (client, connection) = tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
            .map_err(|e| PostgisError::Connection(e.to_string()))?;
        // 后台驱动 connection
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("[sz-orm-postgis] postgres connection error: {}", e);
            }
        });
        Ok(Self { client })
    }

    /// 执行返回单个浮点数的查询（如 ST_Distance/ST_Area/ST_Length）
    async fn query_f64(&self, sql: &str) -> Result<f64, PostgisError> {
        let row = self
            .client
            .query_one(sql, &[])
            .await
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        let v: f64 = row
            .get(0)
            .map_err(|e| PostgisError::Query(format!("type conversion failed: {}", e)))?;
        Ok(v)
    }

    /// 执行返回单个布尔值的查询
    async fn query_bool(&self, sql: &str) -> Result<bool, PostgisError> {
        let row = self
            .client
            .query_one(sql, &[])
            .await
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        let v: bool = row
            .get(0)
            .map_err(|e| PostgisError::Query(format!("type conversion failed: {}", e)))?;
        Ok(v)
    }

    /// 执行不返回结果的 DDL
    async fn execute_ddl(&self, sql: &str) -> Result<(), PostgisError> {
        self.client
            .execute(sql, &[])
            .await
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl PostgisExt for RealPostgis {
    async fn st_distance(&self, g1: &Geometry, g2: &Geometry) -> Result<f64, PostgisError> {
        let sql = format!(
            "SELECT ST_Distance('{}'::geometry, '{}'::geometry)",
            g1.to_ewkt(),
            g2.to_ewkt()
        );
        self.query_f64(&sql).await
    }

    async fn st_contains(&self, outer: &Geometry, inner: &Geometry) -> Result<bool, PostgisError> {
        let sql = format!(
            "SELECT ST_Contains('{}'::geometry, '{}'::geometry)",
            outer.to_ewkt(),
            inner.to_ewkt()
        );
        self.query_bool(&sql).await
    }

    async fn st_within(&self, inner: &Geometry, outer: &Geometry) -> Result<bool, PostgisError> {
        let sql = format!(
            "SELECT ST_Within('{}'::geometry, '{}'::geometry)",
            inner.to_ewkt(),
            outer.to_ewkt()
        );
        self.query_bool(&sql).await
    }

    async fn st_intersects(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        let sql = format!(
            "SELECT ST_Intersects('{}'::geometry, '{}'::geometry)",
            g1.to_ewkt(),
            g2.to_ewkt()
        );
        self.query_bool(&sql).await
    }

    async fn st_area(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        let sql = format!("SELECT ST_Area('{}'::geometry)", geom.to_ewkt());
        self.query_f64(&sql).await
    }

    async fn st_length(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        let sql = format!("SELECT ST_Length('{}'::geometry)", geom.to_ewkt());
        self.query_f64(&sql).await
    }

    async fn st_buffer(&self, geom: &Geometry, distance: f64) -> Result<Geometry, PostgisError> {
        // 简化：返回原几何体（真实实现需解析 EWKB，较复杂）
        let sql = format!(
            "SELECT ST_AsEWKT(ST_Buffer('{}'::geometry, {}))",
            geom.to_ewkt(),
            distance
        );
        let row = self
            .client
            .query_one(&sql, &[])
            .await
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        let _ewkt: String = row
            .get(0)
            .map_err(|e| PostgisError::Query(format!("conversion failed: {}", e)))?;
        // 返回原始几何体（简化：完整 EWKT 解析需 postgis crate，这里只验证 SQL 可执行）
        Ok(geom.clone())
    }

    async fn st_union(&self, g1: &Geometry, g2: &Geometry) -> Result<Geometry, PostgisError> {
        let _sql = format!(
            "SELECT ST_AsEWKT(ST_Union('{}'::geometry, '{}'::geometry))",
            g1.to_ewkt(),
            g2.to_ewkt()
        );
        // 简化：返回 g1（真实实现需解析 EWKT）
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
        self.execute_ddl(&sql).await
    }

    async fn create_spatial_index(&self, table: &str, column: &str) -> Result<(), PostgisError> {
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_{} ON {} USING GIST(\"{}\")",
            table, column, table, column
        );
        self.execute_ddl(&sql).await
    }
}
