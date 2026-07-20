//! SZ-ORM PostGIS 扩展
//!
//! 提供 PostgreSQL PostGIS 空间几何查询能力，支持三种实现：
//!
//! - **内存实现**（`Memory`）：纯 Rust 几何计算，不连接数据库，适用于测试和基准
//! - **Stub 实现**（`Stub`）：生成 PostGIS SQL 字符串但不执行，适用于调试
//! - **真实实现**（`RealPg`，需启用 `real-postgis` feature）：通过 tokio-postgres 连接 PostgreSQL
//!
//! # 支持的几何类型
//!
//! - `Point`：点
//! - `LineString`：线串
//! - `Polygon`：多边形（含洞）
//! - `MultiPoint` / `MultiLineString` / `MultiPolygon`：多几何体
//!
//! 所有几何类型携带 SRID（坐标参考系统 ID），默认 WGS84（SRID=4326）。
//!
//! # 支持的空间操作
//!
//! | 方法 | SQL 等价 | 说明 |
//! |------|---------|------|
//! | `st_distance` | `ST_Distance` | 两点距离 |
//! | `st_contains` | `ST_Contains` | 包含判断 |
//! | `st_within` | `ST_Within` | 在内部判断 |
//! | `st_intersects` | `ST_Intersects` | 相交判断 |
//! | `st_area` | `ST_Area` | 面积计算 |
//! | `st_length` | `ST_Length` | 长度计算 |
//! | `st_buffer` | `ST_Buffer` | 缓冲区 |
//! | `st_union` | `ST_Union` | 几何合并 |
//! | `add_geometry_column` | `AddGeometryColumn` | 添加几何列 |
//! | `create_spatial_index` | `CREATE INDEX ... USING GIST` | 空间索引 |
//!
//! # 快速入门
//!
//! ```rust
//! use sz_orm_postgis::{PostgisBuilder, PostgisExt, PostgisProvider, Geometry, Point};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let wrapper = PostgisBuilder::new(PostgisProvider::Memory).build()?;
//!
//! let beijing = Geometry::Point(Point::new(116.404, 39.915));
//! let shanghai = Geometry::Point(Point::new(121.474, 31.230));
//!
//! let distance = wrapper.st_distance(&beijing, &shanghai).await?;
//! println!("distance: {:.2} m", distance);
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod geometry;
pub mod postgis;

pub mod memory;
pub mod stub;

#[cfg(feature = "real-postgis")]
pub mod real_postgis;

pub use error::PostgisError;
pub use geometry::{Geometry, LineString, Point, Polygon, DEFAULT_SRID};
pub use postgis::{PostgisBuilder, PostgisExt, PostgisProvider, PostgisWrapper};

#[cfg(feature = "real-postgis")]
pub use postgis::RealPgConfig;

#[cfg(feature = "real-postgis")]
pub use real_postgis::RealPostgis;

pub use memory::MemoryPostgis;
pub use stub::StubPostgis;
