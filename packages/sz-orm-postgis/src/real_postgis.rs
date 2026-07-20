//! 真实 PostgreSQL + PostGIS 实现（feature = "real-postgis"）
//!
//! 通过 tokio-postgres 连接 PostgreSQL 数据库，执行真实 PostGIS SQL。
//!
//! # 安全性（v0.2.2 修复 Critical C-1）
//!
//! 所有几何查询使用参数化查询（`$1::geometry`），表名/列名通过 `validate_identifier()`
//! 严格校验（仅允许 ASCII 字母数字+下划线），彻底防止 SQL 注入。
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
//! // build() 是同步的，连接在第一次查询时延迟建立
//! let wrapper = PostgisBuilder::new(PostgisProvider::RealPg(config)).build()?;
//! // 第一次查询会触发连接
//! // wrapper.st_distance(...).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::PostgisError;
use crate::geometry::Geometry;
use crate::postgis::{PostgisExt, RealPgConfig};
use async_trait::async_trait;
use tokio::sync::OnceCell;
use tokio_postgres::Client;

/// 真实 PostgreSQL + PostGIS 实现
///
/// 连接在第一次查询时延迟建立（解决同步 `new()` 无法 await 的问题，v0.2.2 修复 V-4）
pub struct RealPostgis {
    config: RealPgConfig,
    client: OnceCell<Client>,
}

impl RealPostgis {
    pub fn new(config: RealPgConfig) -> Result<Self, PostgisError> {
        // v0.2.2 修复 V-4：不在 new() 中调用 tokio_postgres::connect（返回 Future 非 Result），
        // 改用 OnceCell 延迟到第一次查询时建立连接
        Ok(Self {
            config,
            client: OnceCell::new(),
        })
    }

    /// 延迟建立连接
    async fn client(&self) -> Result<&Client, PostgisError> {
        self.client
            .get_or_try_init(|| async {
                let conn_str = format!(
                    "host={} port={} dbname={} user={} password={}",
                    self.config.host,
                    self.config.port,
                    self.config.database,
                    self.config.username,
                    self.config.password
                );
                let (client, connection) =
                    tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
                        .await
                        .map_err(|e| PostgisError::Connection(e.to_string()))?;
                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        eprintln!("[sz-orm-postgis] postgres connection error: {}", e);
                    }
                });
                Ok::<Client, PostgisError>(client)
            })
            .await
    }

    /// 执行返回单个浮点数的查询（参数化）
    async fn query_f64(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<f64, PostgisError> {
        let client = self.client().await?;
        let row = client
            .query_one(sql, params)
            .await
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        let v: f64 = row
            .try_get(0)
            .map_err(|e| PostgisError::Query(format!("type conversion failed: {}", e)))?;
        Ok(v)
    }

    /// 执行返回单个布尔值的查询（参数化）
    async fn query_bool(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<bool, PostgisError> {
        let client = self.client().await?;
        let row = client
            .query_one(sql, params)
            .await
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        let v: bool = row
            .try_get(0)
            .map_err(|e| PostgisError::Query(format!("type conversion failed: {}", e)))?;
        Ok(v)
    }

    /// 执行返回 EWKT 字符串的查询（参数化），并解析为 Geometry
    async fn query_ewkt(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Geometry, PostgisError> {
        let client = self.client().await?;
        let row = client
            .query_one(sql, params)
            .await
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        let ewkt: String = row
            .try_get(0)
            .map_err(|e| PostgisError::Query(format!("conversion failed: {}", e)))?;
        // v0.2.2 修复 V-1/V-2：真实解析 EWKT 返回，不再丢弃结果
        Geometry::from_ewkt(&ewkt)
    }

    /// 执行不返回结果的 DDL（参数化）
    async fn execute_ddl(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<(), PostgisError> {
        let client = self.client().await?;
        client
            .execute(sql, params)
            .await
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        Ok(())
    }
}

/// 校验 SQL 标识符（表名/列名），防止 SQL 注入
///
/// 仅允许 ASCII 字母数字+下划线，不以数字开头，长度 1-63（PostgreSQL 限制）
fn validate_identifier(name: &str, kind: &str) -> Result<(), PostgisError> {
    if name.is_empty() || name.len() > 63 {
        return Err(PostgisError::Query(format!(
            "invalid {}: empty or too long (max 63 chars): {}",
            kind, name
        )));
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(PostgisError::Query(format!(
            "invalid {}: must start with letter or underscore, got '{}'",
            kind, name
        )));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(PostgisError::Query(format!(
            "invalid {}: only alphanumeric and underscore allowed, got '{}'",
            kind, name
        )));
    }
    Ok(())
}

/// 校验几何类型名（AddGeometryColumn 参数）
fn validate_geom_type(geom_type: &str) -> Result<(), PostgisError> {
    let allowed = [
        "POINT",
        "LINESTRING",
        "POLYGON",
        "MULTIPOINT",
        "MULTILINESTRING",
        "MULTIPOLYGON",
        "GEOMETRYCOLLECTION",
        "GEOMETRY",
    ];
    if !allowed.contains(&geom_type.to_uppercase().as_str()) {
        return Err(PostgisError::Query(format!(
            "invalid geom_type: {}, allowed: {:?}",
            geom_type, allowed
        )));
    }
    Ok(())
}

/// 校验维度名（AddGeometryColumn 参数）
fn validate_dim(dim: &str) -> Result<(), PostgisError> {
    if !["2", "3", "4"].contains(&dim) {
        return Err(PostgisError::Query(format!(
            "invalid dim: {}, allowed: 2/3/4",
            dim
        )));
    }
    Ok(())
}

#[async_trait]
impl PostgisExt for RealPostgis {
    async fn st_distance(&self, g1: &Geometry, g2: &Geometry) -> Result<f64, PostgisError> {
        // v0.2.2 修复 C-1：参数化查询，避免 EWKT 字符串拼接 SQL 注入
        let sql = "SELECT ST_Distance($1::geometry, $2::geometry)";
        self.query_f64(sql, &[&g1.to_ewkt(), &g2.to_ewkt()]).await
    }

    async fn st_contains(&self, outer: &Geometry, inner: &Geometry) -> Result<bool, PostgisError> {
        let sql = "SELECT ST_Contains($1::geometry, $2::geometry)";
        self.query_bool(sql, &[&outer.to_ewkt(), &inner.to_ewkt()])
            .await
    }

    async fn st_within(&self, inner: &Geometry, outer: &Geometry) -> Result<bool, PostgisError> {
        let sql = "SELECT ST_Within($1::geometry, $2::geometry)";
        self.query_bool(sql, &[&inner.to_ewkt(), &outer.to_ewkt()])
            .await
    }

    async fn st_intersects(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        let sql = "SELECT ST_Intersects($1::geometry, $2::geometry)";
        self.query_bool(sql, &[&g1.to_ewkt(), &g2.to_ewkt()]).await
    }

    async fn st_area(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        let sql = "SELECT ST_Area($1::geometry)";
        self.query_f64(sql, &[&geom.to_ewkt()]).await
    }

    async fn st_length(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        let sql = "SELECT ST_Length($1::geometry)";
        self.query_f64(sql, &[&geom.to_ewkt()]).await
    }

    async fn st_buffer(&self, geom: &Geometry, distance: f64) -> Result<Geometry, PostgisError> {
        // v0.2.2 修复 V-2：参数化 + 解析 EWKT 返回真实缓冲区几何
        let sql = "SELECT ST_AsEWKT(ST_Buffer($1::geometry, $2))";
        self.query_ewkt(sql, &[&geom.to_ewkt(), &distance]).await
    }

    async fn st_union(&self, g1: &Geometry, g2: &Geometry) -> Result<Geometry, PostgisError> {
        // v0.2.2 修复 V-1：真实执行 SQL 并解析 EWKT 返回（之前 _sql 未执行直接返回 g1）
        let sql = "SELECT ST_AsEWKT(ST_Union($1::geometry, $2::geometry))";
        self.query_ewkt(sql, &[&g1.to_ewkt(), &g2.to_ewkt()]).await
    }

    async fn add_geometry_column(
        &self,
        table: &str,
        column: &str,
        srid: i32,
        geom_type: &str,
        dim: &str,
    ) -> Result<(), PostgisError> {
        // v0.2.2 修复 C-1：表名/列名/几何类型/维度严格校验 + 参数化
        validate_identifier(table, "table")?;
        validate_identifier(column, "column")?;
        validate_geom_type(geom_type)?;
        validate_dim(dim)?;
        // 标识符已校验，可安全拼接（PostgreSQL 不支持 DDL 中的标识符参数化）
        let sql = format!(
            "SELECT AddGeometryColumn('{}', '{}', $1, '{}', '{}')",
            table,
            column,
            geom_type.to_uppercase(),
            dim
        );
        self.execute_ddl(&sql, &[&srid]).await
    }

    async fn create_spatial_index(&self, table: &str, column: &str) -> Result<(), PostgisError> {
        // v0.2.2 修复 C-1：表名/列名严格校验
        validate_identifier(table, "table")?;
        validate_identifier(column, "column")?;
        let idx_name = format!("idx_{}_{}", table, column);
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS {} ON {} USING GIST(\"{}\")",
            idx_name, table, column
        );
        self.execute_ddl(&sql, &[]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("users", "table").is_ok());
        assert!(validate_identifier("_idx", "index").is_ok());
        assert!(validate_identifier("geom_2026", "column").is_ok());
    }

    #[test]
    fn test_validate_identifier_invalid() {
        // 含 SQL 注入字符
        assert!(validate_identifier("users; DROP TABLE", "table").is_err());
        assert!(validate_identifier("col'--", "column").is_err());
        assert!(validate_identifier("col\"x", "column").is_err());
        // 数字开头
        assert!(validate_identifier("1col", "column").is_err());
        // 空字符串
        assert!(validate_identifier("", "table").is_err());
        // 过长
        let long_name = "a".repeat(64);
        assert!(validate_identifier(&long_name, "table").is_err());
    }

    #[test]
    fn test_validate_geom_type() {
        assert!(validate_geom_type("POINT").is_ok());
        assert!(validate_geom_type("point").is_ok()); // 大小写不敏感
        assert!(validate_geom_type("POLYGON").is_ok());
        assert!(validate_geom_type("GEOMETRY").is_ok());
        // 非法类型
        assert!(validate_geom_type("EVIL_TYPE").is_err());
        assert!(validate_geom_type("POINT'; DROP TABLE").is_err());
    }

    #[test]
    fn test_validate_dim() {
        assert!(validate_dim("2").is_ok());
        assert!(validate_dim("3").is_ok());
        assert!(validate_dim("4").is_ok());
        assert!(validate_dim("5").is_err());
        assert!(validate_dim("'; DROP TABLE").is_err());
    }

    #[test]
    fn test_realpostgis_new_does_not_connect() {
        // v0.2.2 修复 V-4：new() 不应该立即连接（应该延迟到第一次查询）
        let config = RealPgConfig {
            host: "nonexistent.invalid".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "postgres".to_string(),
            password: "secret".to_string(),
        };
        // 即使 host 不存在，new() 也应该成功（因为延迟连接）
        let _real = RealPostgis::new(config).expect("new() should not connect");
    }
}
