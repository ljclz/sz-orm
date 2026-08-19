//! SZ-ORM PostGIS Extension
//!
//! Provides PostgreSQL PostGIS spatial geometry query capabilities, supporting three implementations:
//!
//! - **In-memory implementation** (`Memory`): Pure Rust geometry computation, no database connection, suitable for testing and benchmarking
//! - **Stub implementation** (`Stub`): Generates PostGIS SQL string but does not execute, suitable for debugging
//! - **Real implementation** (`RealPg`, requires `real-postgis` feature): Connects to PostgreSQL via tokio-postgres
//!
//! # Supported Geometry Types
//!
//! - `Point`: Point
//! - `LineString`: Line string
//! - `Polygon`: Polygon (with holes)
//! - `MultiPoint` / `MultiLineString` / `MultiPolygon`: Multi-geometry
//!
//! All geometry types carry SRID (coordinate reference system ID), default WGS84 (SRID=4326).
//!
//! # Supported Spatial Operations
//!
//! | Method | SQL equivalent | Description |
//! |------|---------|------|
//! | `st_distance` | `ST_Distance` | Distance between two points |
//! | `st_contains` | `ST_Contains` | Contains check |
//! | `st_within` | `ST_Within` | Within check |
//! | `st_intersects` | `ST_Intersects` | Intersects check |
//! | `st_area` | `ST_Area` | Area calculation |
//! | `st_length` | `ST_Length` | Length calculation |
//! | `st_buffer` | `ST_Buffer` | Buffer zone |
//! | `st_union` | `ST_Union` | Geometry union |
//! | `add_geometry_column` | `AddGeometryColumn` | Add geometry column |
//! | `create_spatial_index` | `CREATE INDEX ... USING GIST` | Spatial index |
//!
//! # Quick Start
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
pub mod extensions;
pub mod geometry;
pub mod postgis;

pub mod memory;
pub mod stub;

#[cfg(feature = "real-postgis")]
pub mod real_postgis;

pub use error::PostgisError;
pub use extensions::{
    CoordinateTransformExt, MemoryCoordTransform, MemorySpatialAggregate, MemorySpatialRelations,
    SpatialAggregateExt, SpatialIndexDef, SpatialIndexRegistry, SpatialIndexType,
    SpatialRelationsExt,
};
pub use geometry::{Geometry, LineString, Point, Polygon, DEFAULT_SRID};
pub use postgis::{PostgisBuilder, PostgisExt, PostgisProvider, PostgisWrapper};

#[cfg(feature = "real-postgis")]
pub use postgis::RealPgConfig;

#[cfg(feature = "real-postgis")]
pub use real_postgis::RealPostgis;

pub use memory::MemoryPostgis;
pub use stub::StubPostgis;
