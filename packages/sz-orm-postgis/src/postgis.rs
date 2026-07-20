//! PostGIS 核心 trait 与 Builder/Wrapper/Provider
//!
//! 提供 PostGIS 空间几何查询的统一抽象层，支持内存实现、stub 实现（生成 SQL）
//! 和真实 PostgreSQL + PostGIS 实现（feature = "real-postgis"）。

use crate::error::PostgisError;
use crate::geometry::Geometry;
use async_trait::async_trait;

/// PostGIS 扩展 trait
///
/// 提供空间几何查询能力。所有方法均为 async，适用于真实数据库 I/O。
#[async_trait]
pub trait PostgisExt: Send + Sync {
    /// 计算两个几何体之间的距离
    ///
    /// 等价 SQL：`SELECT ST_Distance(geom1, geom2)`
    async fn st_distance(&self, g1: &Geometry, g2: &Geometry) -> Result<f64, PostgisError>;

    /// 判断 outer 是否包含 inner
    ///
    /// 等价 SQL：`SELECT ST_Contains(outer, inner)`
    async fn st_contains(&self, outer: &Geometry, inner: &Geometry) -> Result<bool, PostgisError>;

    /// 判断 inner 是否在 outer 内部
    ///
    /// 等价 SQL：`SELECT ST_Within(inner, outer)`
    async fn st_within(&self, inner: &Geometry, outer: &Geometry) -> Result<bool, PostgisError>;

    /// 判断两个几何体是否相交
    ///
    /// 等价 SQL：`SELECT ST_Intersects(geom1, geom2)`
    async fn st_intersects(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError>;

    /// 计算几何体面积
    ///
    /// 等价 SQL：`SELECT ST_Area(geom)`
    async fn st_area(&self, geom: &Geometry) -> Result<f64, PostgisError>;

    /// 计算几何体长度
    ///
    /// 等价 SQL：`SELECT ST_Length(geom)`
    async fn st_length(&self, geom: &Geometry) -> Result<f64, PostgisError>;

    /// 创建缓冲区
    ///
    /// 等价 SQL：`SELECT ST_Buffer(geom, distance)`
    async fn st_buffer(&self, geom: &Geometry, distance: f64) -> Result<Geometry, PostgisError>;

    /// 合并两个几何体
    ///
    /// 等价 SQL：`SELECT ST_Union(geom1, geom2)`
    async fn st_union(&self, g1: &Geometry, g2: &Geometry) -> Result<Geometry, PostgisError>;

    /// 为表添加几何列
    ///
    /// 等价 SQL：`SELECT AddGeometryColumn('table', 'column', srid, 'type', dim)`
    async fn add_geometry_column(
        &self,
        table: &str,
        column: &str,
        srid: i32,
        geom_type: &str,
        dim: &str,
    ) -> Result<(), PostgisError>;

    /// 创建空间索引
    ///
    /// 等价 SQL：`CREATE INDEX idx_table_column ON table USING GIST(column)`
    async fn create_spatial_index(&self, table: &str, column: &str) -> Result<(), PostgisError>;
}

/// Provider 类型
#[derive(Debug, Clone)]
pub enum PostgisProvider {
    /// 内存实现（不连接数据库，纯计算）
    Memory,
    /// Stub 实现（生成 SQL 字符串但不执行）
    Stub,
    /// 真实 PostgreSQL + PostGIS（需启用 `real-postgis` feature）
    #[cfg(feature = "real-postgis")]
    RealPg(RealPgConfig),
}

/// 真实 PostgreSQL 配置
#[cfg(feature = "real-postgis")]
#[derive(Debug, Clone, Default)]
pub struct RealPgConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

/// Wrapper enum：运行时多态分发
pub enum PostgisWrapper {
    Memory(crate::memory::MemoryPostgis),
    Stub(crate::stub::StubPostgis),
    #[cfg(feature = "real-postgis")]
    RealPg(crate::real_postgis::RealPostgis),
}

#[async_trait]
impl PostgisExt for PostgisWrapper {
    async fn st_distance(&self, g1: &Geometry, g2: &Geometry) -> Result<f64, PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.st_distance(g1, g2).await,
            PostgisWrapper::Stub(p) => p.st_distance(g1, g2).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.st_distance(g1, g2).await,
        }
    }

    async fn st_contains(&self, outer: &Geometry, inner: &Geometry) -> Result<bool, PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.st_contains(outer, inner).await,
            PostgisWrapper::Stub(p) => p.st_contains(outer, inner).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.st_contains(outer, inner).await,
        }
    }

    async fn st_within(&self, inner: &Geometry, outer: &Geometry) -> Result<bool, PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.st_within(inner, outer).await,
            PostgisWrapper::Stub(p) => p.st_within(inner, outer).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.st_within(inner, outer).await,
        }
    }

    async fn st_intersects(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.st_intersects(g1, g2).await,
            PostgisWrapper::Stub(p) => p.st_intersects(g1, g2).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.st_intersects(g1, g2).await,
        }
    }

    async fn st_area(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.st_area(geom).await,
            PostgisWrapper::Stub(p) => p.st_area(geom).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.st_area(geom).await,
        }
    }

    async fn st_length(&self, geom: &Geometry) -> Result<f64, PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.st_length(geom).await,
            PostgisWrapper::Stub(p) => p.st_length(geom).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.st_length(geom).await,
        }
    }

    async fn st_buffer(&self, geom: &Geometry, distance: f64) -> Result<Geometry, PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.st_buffer(geom, distance).await,
            PostgisWrapper::Stub(p) => p.st_buffer(geom, distance).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.st_buffer(geom, distance).await,
        }
    }

    async fn st_union(&self, g1: &Geometry, g2: &Geometry) -> Result<Geometry, PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.st_union(g1, g2).await,
            PostgisWrapper::Stub(p) => p.st_union(g1, g2).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.st_union(g1, g2).await,
        }
    }

    async fn add_geometry_column(
        &self,
        table: &str,
        column: &str,
        srid: i32,
        geom_type: &str,
        dim: &str,
    ) -> Result<(), PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => {
                p.add_geometry_column(table, column, srid, geom_type, dim)
                    .await
            }
            PostgisWrapper::Stub(p) => {
                p.add_geometry_column(table, column, srid, geom_type, dim)
                    .await
            }
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => {
                p.add_geometry_column(table, column, srid, geom_type, dim)
                    .await
            }
        }
    }

    async fn create_spatial_index(&self, table: &str, column: &str) -> Result<(), PostgisError> {
        match self {
            PostgisWrapper::Memory(p) => p.create_spatial_index(table, column).await,
            PostgisWrapper::Stub(p) => p.create_spatial_index(table, column).await,
            #[cfg(feature = "real-postgis")]
            PostgisWrapper::RealPg(p) => p.create_spatial_index(table, column).await,
        }
    }
}

/// Builder：链式构造 PostgisWrapper
pub struct PostgisBuilder {
    provider: PostgisProvider,
}

impl PostgisBuilder {
    pub fn new(provider: PostgisProvider) -> Self {
        Self { provider }
    }

    pub fn build(self) -> Result<PostgisWrapper, PostgisError> {
        match self.provider {
            PostgisProvider::Memory => {
                Ok(PostgisWrapper::Memory(crate::memory::MemoryPostgis::new()))
            }
            PostgisProvider::Stub => Ok(PostgisWrapper::Stub(crate::stub::StubPostgis::new())),
            #[cfg(feature = "real-postgis")]
            PostgisProvider::RealPg(config) => {
                let real = crate::real_postgis::RealPostgis::new(config)?;
                Ok(PostgisWrapper::RealPg(real))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[tokio::test]
    async fn test_wrapper_memory_dispatch() {
        let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
            .build()
            .expect("build memory failed");
        let p1 = Geometry::Point(Point::new(0.0, 0.0));
        let p2 = Geometry::Point(Point::new(3.0, 4.0));
        let dist = wrapper
            .st_distance(&p1, &p2)
            .await
            .expect("distance failed");
        // haversine 距离：3°经 + 4°纬 ≈ 555 km
        assert!(
            dist > 500_000.0 && dist < 600_000.0,
            "expected ~555km, got {}",
            dist
        );
    }

    #[tokio::test]
    async fn test_wrapper_stub_dispatch() {
        let wrapper = PostgisBuilder::new(PostgisProvider::Stub)
            .build()
            .expect("build stub failed");
        let p1 = Geometry::Point(Point::new(0.0, 0.0));
        let p2 = Geometry::Point(Point::new(3.0, 4.0));
        // stub 返回 0.0，仅验证分发不报错
        let _ = wrapper
            .st_distance(&p1, &p2)
            .await
            .expect("stub distance failed");
    }
}
