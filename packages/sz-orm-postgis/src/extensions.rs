//! PostGIS 深度扩展功能
//!
//! 本模块补充 PostGIS 扩展缺失的核心深度功能，包括：
//!
//! - **空间索引管理**：GIST 索引的创建、删除、重建、查询
//! - **扩展空间关系运算**：ST_Crosses / ST_Touches / ST_Overlaps / ST_Disjoint / ST_Equals / ST_Covers / ST_CoveredBy
//! - **空间聚合**：ST_Collect / ST_Envelope / ST_ConvexHull / ST_Centroid
//! - **坐标系转换**：ST_Transform（支持 WGS84 <-> WebMercator 等常见 SRID 互转）
//! - **SRID 管理工具**：SRID 查找、坐标系元信息查询
//!
//! # 设计说明
//!
//! 本模块以**独立函数 + 扩展 trait** 的方式提供，不修改既有 `PostgisExt` trait，
//! 避免破坏已有的 memory / stub / real_postgis 三种实现。
//! 内存计算部分基于纯 Rust 实现，不依赖外部库。

#![allow(dead_code)]

use crate::error::PostgisError;
use crate::geometry::{Geometry, LineString, Point, Polygon};
use std::collections::HashMap;

// =============================================================================
// 一、空间索引管理
// =============================================================================

/// 空间索引类型
///
/// PostGIS 支持多种空间索引，其中 GIST 是默认且最常用的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialIndexType {
    /// GIST 索引（默认，基于 R-tree）
    Gist,
    /// SP-GIST 索引（空间分区 GiST）
    SpGist,
    /// BRIN 索引（块范围索引，适合大表）
    Brin,
}

impl SpatialIndexType {
    /// 转为 SQL 索引方法字符串
    pub fn as_sql_method(&self) -> &'static str {
        match self {
            SpatialIndexType::Gist => "GIST",
            SpatialIndexType::SpGist => "SPGIST",
            SpatialIndexType::Brin => "BRIN",
        }
    }
}

/// 空间索引定义
#[derive(Debug, Clone)]
pub struct SpatialIndexDef {
    /// 索引名称
    pub index_name: String,
    /// 表名
    pub table: String,
    /// 列名
    pub column: String,
    /// 索引类型
    pub index_type: SpatialIndexType,
    /// 是否为唯一索引（空间索引通常不唯一）
    pub unique: bool,
    /// 是否并发创建（CONCURRENTLY，不锁表）
    pub concurrently: bool,
}

impl SpatialIndexDef {
    /// 创建一个 GIST 空间索引定义
    pub fn new_gist(table: &str, column: &str) -> Self {
        Self {
            index_name: format!("idx_{}_{}_gist", table, column),
            table: table.to_string(),
            column: column.to_string(),
            index_type: SpatialIndexType::Gist,
            unique: false,
            concurrently: false,
        }
    }

    /// 设置并发创建（不锁表）
    pub fn with_concurrently(mut self) -> Self {
        self.concurrently = true;
        self
    }

    /// 生成 CREATE INDEX SQL
    pub fn to_create_sql(&self) -> String {
        let unique_str = if self.unique { "UNIQUE " } else { "" };
        let concurrently_str = if self.concurrently {
            "CONCURRENTLY "
        } else {
            ""
        };
        format!(
            "CREATE {}INDEX {} {}ON {} USING {}(\"{}\")",
            unique_str,
            self.index_name,
            concurrently_str,
            self.table,
            self.index_type.as_sql_method(),
            self.column
        )
    }

    /// 生成 DROP INDEX SQL
    pub fn to_drop_sql(&self) -> String {
        let concurrently_str = if self.concurrently {
            "CONCURRENTLY "
        } else {
            ""
        };
        format!("DROP INDEX {}{}", concurrently_str, self.index_name)
    }

    /// 生成 REINDEX SQL
    pub fn to_reindex_sql(&self) -> String {
        format!("REINDEX INDEX {}", self.index_name)
    }
}

/// 空间索引管理器（内存版，用于跟踪已创建的索引）
#[derive(Debug, Clone, Default)]
pub struct SpatialIndexRegistry {
    /// 已注册的索引：index_name -> SpatialIndexDef
    indexes: HashMap<String, SpatialIndexDef>,
}

impl SpatialIndexRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个空间索引
    pub fn register(&mut self, def: SpatialIndexDef) -> Result<(), PostgisError> {
        if self.indexes.contains_key(&def.index_name) {
            return Err(PostgisError::Query(format!(
                "spatial index already exists: {}",
                def.index_name
            )));
        }
        self.indexes.insert(def.index_name.clone(), def);
        Ok(())
    }

    /// 注销一个空间索引
    pub fn unregister(&mut self, index_name: &str) -> Result<SpatialIndexDef, PostgisError> {
        self.indexes
            .remove(index_name)
            .ok_or_else(|| PostgisError::Query(format!("spatial index not found: {}", index_name)))
    }

    /// 检查索引是否存在
    pub fn exists(&self, index_name: &str) -> bool {
        self.indexes.contains_key(index_name)
    }

    /// 列出某表上的所有索引
    pub fn list_for_table(&self, table: &str) -> Vec<&SpatialIndexDef> {
        self.indexes
            .values()
            .filter(|def| def.table == table)
            .collect()
    }

    /// 获取所有索引
    pub fn list_all(&self) -> Vec<&SpatialIndexDef> {
        self.indexes.values().collect()
    }

    /// 生成 REINDEX ALL SQL（重建所有索引）
    pub fn reindex_all_sql(&self) -> Vec<String> {
        self.indexes
            .values()
            .map(|def| def.to_reindex_sql())
            .collect()
    }
}

// =============================================================================
// 二、扩展空间关系运算
// =============================================================================

/// 扩展空间关系运算 trait
///
/// 提供 ST_Crosses / ST_Touches / ST_Overlaps / ST_Disjoint / ST_Equals /
/// ST_Covers / ST_CoveredBy 等高级空间关系判断。
pub trait SpatialRelationsExt {
    /// 判断两个几何体是否相交（内部交叉，非仅边界接触）
    ///
    /// 等价 SQL：`SELECT ST_Crosses(g1, g2)`
    fn st_crosses(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError>;

    /// 判断两个几何体是否在边界上接触
    ///
    /// 等价 SQL：`SELECT ST_Touches(g1, g2)`
    fn st_touches(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError>;

    /// 判断两个几何体是否重叠（同维度，部分相交但互不包含）
    ///
    /// 等价 SQL：`SELECT ST_Overlaps(g1, g2)`
    fn st_overlaps(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError>;

    /// 判断两个几何体是否不相交
    ///
    /// 等价 SQL：`SELECT ST_Disjoint(g1, g2)`
    fn st_disjoint(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError>;

    /// 判断两个几何体是否在空间上完全相等
    ///
    /// 等价 SQL：`SELECT ST_Equals(g1, g2)`
    fn st_equals(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError>;

    /// 判断 g1 是否覆盖 g2（包含边界）
    ///
    /// 等价 SQL：`SELECT ST_Covers(g1, g2)`
    fn st_covers(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError>;

    /// 判断 g1 是否被 g2 覆盖
    ///
    /// 等价 SQL：`SELECT ST_CoveredBy(g1, g2)`
    fn st_covered_by(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError>;
}

/// 内存版扩展空间关系实现
pub struct MemorySpatialRelations;

impl MemorySpatialRelations {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemorySpatialRelations {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialRelationsExt for MemorySpatialRelations {
    fn st_crosses(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        // 简化实现：LineString 与 Polygon 的交叉判断
        match (g1, g2) {
            (Geometry::LineString(ls), Geometry::Polygon(poly)) => {
                check_srid(ls.srid, poly.srid)?;
                Ok(line_crosses_polygon(ls, poly))
            }
            (Geometry::Polygon(poly), Geometry::LineString(ls)) => {
                check_srid(ls.srid, poly.srid)?;
                Ok(line_crosses_polygon(ls, poly))
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_crosses not supported for {}-{} in memory impl",
                g1.type_name(),
                g2.type_name()
            ))),
        }
    }

    fn st_touches(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        match (g1, g2) {
            (Geometry::Polygon(poly), Geometry::Point(p)) => {
                check_srid(poly.srid, p.srid)?;
                // 点在多边形边界上 = touches
                Ok(point_on_polygon_boundary(p, poly))
            }
            (Geometry::Point(p), Geometry::Polygon(poly)) => {
                check_srid(poly.srid, p.srid)?;
                Ok(point_on_polygon_boundary(p, poly))
            }
            (Geometry::Point(p1), Geometry::Point(p2)) => {
                check_srid(p1.srid, p2.srid)?;
                Ok(p1.euclidean_distance(p2) == 0.0)
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_touches not supported for {}-{} in memory impl",
                g1.type_name(),
                g2.type_name()
            ))),
        }
    }

    fn st_overlaps(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        match (g1, g2) {
            (Geometry::Polygon(p1), Geometry::Polygon(p2)) => {
                check_srid(p1.srid, p2.srid)?;
                // 两多边形重叠 = 相交但互不包含
                let intersects = polygons_intersect(p1, p2);
                let p1_contains_p2 = polygon_contains_polygon(p1, p2);
                let p2_contains_p1 = polygon_contains_polygon(p2, p1);
                Ok(intersects && !p1_contains_p2 && !p2_contains_p1)
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_overlaps not supported for {}-{} in memory impl",
                g1.type_name(),
                g2.type_name()
            ))),
        }
    }

    fn st_disjoint(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        match (g1, g2) {
            (Geometry::Point(p1), Geometry::Point(p2)) => {
                check_srid(p1.srid, p2.srid)?;
                Ok(p1.x != p2.x || p1.y != p2.y)
            }
            (Geometry::Polygon(poly), Geometry::Point(p)) => {
                check_srid(poly.srid, p.srid)?;
                Ok(!poly.contains_point(p))
            }
            (Geometry::Point(p), Geometry::Polygon(poly)) => {
                check_srid(poly.srid, p.srid)?;
                Ok(!poly.contains_point(p))
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_disjoint not supported for {}-{} in memory impl",
                g1.type_name(),
                g2.type_name()
            ))),
        }
    }

    fn st_equals(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        check_srid(g1.srid(), g2.srid())?;
        Ok(g1 == g2)
    }

    fn st_covers(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        match (g1, g2) {
            (Geometry::Polygon(poly), Geometry::Point(p)) => {
                check_srid(poly.srid, p.srid)?;
                // covers = contains_point OR point_on_boundary
                Ok(poly.contains_point(p) || point_on_polygon_boundary(p, poly))
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_covers not supported for {}-{} in memory impl",
                g1.type_name(),
                g2.type_name()
            ))),
        }
    }

    fn st_covered_by(&self, g1: &Geometry, g2: &Geometry) -> Result<bool, PostgisError> {
        // st_covered_by(a, b) == st_covers(b, a)
        self.st_covers(g2, g1)
    }
}

// =============================================================================
// 三、空间聚合运算
// =============================================================================

/// 空间聚合运算 trait
///
/// 提供 ST_Collect / ST_Envelope / ST_ConvexHull / ST_Centroid 等聚合操作。
pub trait SpatialAggregateExt {
    /// 将多个几何体收集为一个 Multi 几何体
    ///
    /// 等价 SQL：`SELECT ST_Collect(g1, g2, ...)`
    fn st_collect(&self, geometries: &[Geometry]) -> Result<Geometry, PostgisError>;

    /// 计算几何体的外包矩形（Bounding Box）
    ///
    /// 等价 SQL：`SELECT ST_Envelope(geom)`
    fn st_envelope(&self, geom: &Geometry) -> Result<Geometry, PostgisError>;

    /// 计算几何体的凸包
    ///
    /// 等价 SQL：`SELECT ST_ConvexHull(geom)`
    fn st_convex_hull(&self, geom: &Geometry) -> Result<Geometry, PostgisError>;

    /// 计算几何体的质心
    ///
    /// 等价 SQL：`SELECT ST_Centroid(geom)`
    fn st_centroid(&self, geom: &Geometry) -> Result<Point, PostgisError>;
}

/// 内存版空间聚合实现
pub struct MemorySpatialAggregate;

impl MemorySpatialAggregate {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemorySpatialAggregate {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialAggregateExt for MemorySpatialAggregate {
    fn st_collect(&self, geometries: &[Geometry]) -> Result<Geometry, PostgisError> {
        if geometries.is_empty() {
            return Err(PostgisError::InvalidGeometry(
                "st_collect requires at least one geometry".to_string(),
            ));
        }
        if geometries.len() == 1 {
            return Ok(geometries[0].clone());
        }
        // 统一 SRID
        let srid = geometries[0].srid();
        for g in geometries.iter().skip(1) {
            check_srid(srid, g.srid())?;
        }
        // 根据第一个几何体类型决定 Multi 类型
        match &geometries[0] {
            Geometry::Point(_) => {
                let mut points = Vec::new();
                for g in geometries {
                    match g {
                        Geometry::Point(p) => points.push(*p),
                        _ => {
                            return Err(PostgisError::TypeMismatch {
                                expected: "Point",
                                actual: g.type_name(),
                            })
                        }
                    }
                }
                Ok(Geometry::MultiPoint(points))
            }
            Geometry::LineString(_) => {
                let mut lines = Vec::new();
                for g in geometries {
                    match g {
                        Geometry::LineString(ls) => lines.push(ls.clone()),
                        _ => {
                            return Err(PostgisError::TypeMismatch {
                                expected: "LineString",
                                actual: g.type_name(),
                            })
                        }
                    }
                }
                Ok(Geometry::MultiLineString(lines))
            }
            Geometry::Polygon(_) => {
                let mut polys = Vec::new();
                for g in geometries {
                    match g {
                        Geometry::Polygon(p) => polys.push(p.clone()),
                        _ => {
                            return Err(PostgisError::TypeMismatch {
                                expected: "Polygon",
                                actual: g.type_name(),
                            })
                        }
                    }
                }
                Ok(Geometry::MultiPolygon(polys))
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_collect not supported for {} as first geometry",
                geometries[0].type_name()
            ))),
        }
    }

    fn st_envelope(&self, geom: &Geometry) -> Result<Geometry, PostgisError> {
        let (min_x, min_y, max_x, max_y) = compute_bounding_box(geom)?;
        let srid = geom.srid();
        // 构造矩形多边形
        let ring = vec![
            Point::with_srid(min_x, min_y, srid),
            Point::with_srid(max_x, min_y, srid),
            Point::with_srid(max_x, max_y, srid),
            Point::with_srid(min_x, max_y, srid),
            Point::with_srid(min_x, min_y, srid), // 闭合
        ];
        Ok(Geometry::Polygon(Polygon::new(ring)))
    }

    fn st_convex_hull(&self, geom: &Geometry) -> Result<Geometry, PostgisError> {
        let points = collect_all_points(geom)?;
        if points.len() < 3 {
            return Err(PostgisError::InvalidGeometry(format!(
                "convex hull requires at least 3 points, got {}",
                points.len()
            )));
        }
        let hull = convex_hull(&points);
        if hull.len() < 3 {
            return Err(PostgisError::InvalidGeometry(
                "convex hull degenerate (all points collinear)".to_string(),
            ));
        }
        let srid = geom.srid();
        let hull_points: Vec<Point> = hull
            .into_iter()
            .map(|(x, y)| Point::with_srid(x, y, srid))
            .collect();
        Ok(Geometry::Polygon(Polygon::new(hull_points)))
    }

    fn st_centroid(&self, geom: &Geometry) -> Result<Point, PostgisError> {
        let points = collect_all_points(geom)?;
        if points.is_empty() {
            return Err(PostgisError::InvalidGeometry(
                "centroid requires at least one point".to_string(),
            ));
        }
        let srid = geom.srid();
        let sum_x: f64 = points.iter().map(|(x, _)| x).sum();
        let sum_y: f64 = points.iter().map(|(_, y)| y).sum();
        let count = points.len() as f64;
        Ok(Point::with_srid(sum_x / count, sum_y / count, srid))
    }
}

// =============================================================================
// 四、坐标系转换（ST_Transform）
// =============================================================================

/// 常见 SRID 定义
pub mod srid {
    /// WGS84 经纬度（GPS 标准）
    pub const WGS84: i32 = 4326;
    /// Web Mercator（Google Maps / OpenStreetMap）
    pub const WEB_MERCATOR: i32 = 3857;
    /// WGS84 UTM Zone 50N（中国东部）
    pub const UTM_ZONE_50N: i32 = 32650;
    /// CGCS2000 地理坐标系
    pub const CGCS2000: i32 = 4490;
}

/// 坐标系转换 trait
///
/// 提供 ST_Transform 功能，支持常见 SRID 之间的互转。
pub trait CoordinateTransformExt {
    /// 将几何体从当前 SRID 转换为目标 SRID
    ///
    /// 等价 SQL：`SELECT ST_Transform(geom, target_srid)`
    fn st_transform(&self, geom: &Geometry, target_srid: i32) -> Result<Geometry, PostgisError>;

    /// 获取 SRID 的友好名称
    fn srid_name(&self, srid: i32) -> &'static str;

    /// 检查 SRID 是否受支持
    fn is_srid_supported(&self, srid: i32) -> bool;
}

/// 内存版坐标系转换实现
///
/// 支持 WGS84 (4326) <-> Web Mercator (3857) 互转。
/// 其他 SRID 组合返回 Unsupported 错误。
pub struct MemoryCoordTransform;

impl MemoryCoordTransform {
    pub fn new() -> Self {
        Self
    }

    /// WGS84 经纬度 -> Web Mercator 投影坐标
    fn wgs84_to_mercator(lon: f64, lat: f64) -> (f64, f64) {
        const R: f64 = 6_378_137.0; // 地球赤道半径（米）
        let x = R * lon.to_radians();
        // 标准墨卡托投影 Y = R * ln(tan(π/4 + lat/2))
        let y = R * (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.0).tan().ln();
        (x, y)
    }

    /// Web Mercator 投影坐标 -> WGS84 经纬度
    fn mercator_to_wgs84(x: f64, y: f64) -> (f64, f64) {
        const R: f64 = 6_378_137.0;
        let lon = (x / R).to_degrees();
        let lat = (2.0 * (y / R).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
        (lon, lat)
    }

    /// 转换单个点
    fn transform_point(p: &Point, target_srid: i32) -> Result<Point, PostgisError> {
        match (p.srid, target_srid) {
            (srid::WGS84, srid::WEB_MERCATOR) => {
                let (x, y) = Self::wgs84_to_mercator(p.x, p.y);
                Ok(Point::with_srid(x, y, target_srid))
            }
            (srid::WEB_MERCATOR, srid::WGS84) => {
                let (lon, lat) = Self::mercator_to_wgs84(p.x, p.y);
                Ok(Point::with_srid(lon, lat, target_srid))
            }
            (s, t) if s == t => Ok(*p),
            _ => Err(PostgisError::Unsupported(format!(
                "st_transform not supported for SRID {} -> {} in memory impl",
                p.srid, target_srid
            ))),
        }
    }
}

impl Default for MemoryCoordTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl CoordinateTransformExt for MemoryCoordTransform {
    fn st_transform(&self, geom: &Geometry, target_srid: i32) -> Result<Geometry, PostgisError> {
        match geom {
            Geometry::Point(p) => {
                let transformed = Self::transform_point(p, target_srid)?;
                Ok(Geometry::Point(transformed))
            }
            Geometry::LineString(ls) => {
                let mut new_points = Vec::with_capacity(ls.points.len());
                for p in &ls.points {
                    new_points.push(Self::transform_point(p, target_srid)?);
                }
                Ok(Geometry::LineString(LineString {
                    points: new_points,
                    srid: target_srid,
                }))
            }
            Geometry::Polygon(poly) => {
                let mut new_rings = Vec::with_capacity(poly.rings.len());
                for ring in &poly.rings {
                    let mut new_ring = Vec::with_capacity(ring.len());
                    for p in ring {
                        new_ring.push(Self::transform_point(p, target_srid)?);
                    }
                    new_rings.push(new_ring);
                }
                Ok(Geometry::Polygon(Polygon {
                    rings: new_rings,
                    srid: target_srid,
                }))
            }
            Geometry::MultiPoint(pts) => {
                let mut new_pts = Vec::with_capacity(pts.len());
                for p in pts {
                    new_pts.push(Self::transform_point(p, target_srid)?);
                }
                Ok(Geometry::MultiPoint(new_pts))
            }
            _ => Err(PostgisError::Unsupported(format!(
                "st_transform not supported for {} in memory impl",
                geom.type_name()
            ))),
        }
    }

    fn srid_name(&self, srid: i32) -> &'static str {
        match srid {
            srid::WGS84 => "WGS84",
            srid::WEB_MERCATOR => "Web Mercator",
            srid::UTM_ZONE_50N => "UTM Zone 50N",
            srid::CGCS2000 => "CGCS2000",
            _ => "Unknown",
        }
    }

    fn is_srid_supported(&self, srid: i32) -> bool {
        matches!(srid, srid::WGS84 | srid::WEB_MERCATOR)
    }
}

// =============================================================================
// 五、内部辅助函数
// =============================================================================

/// 检查 SRID 一致性
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

/// 判断线串是否穿过多边形
fn line_crosses_polygon(ls: &LineString, poly: &Polygon) -> bool {
    // 如果线串的任何一段与多边形边界相交，且线串不全在多边形内或外，则为 crosses
    let outer = &poly.rings[0];
    // 检查线串各段与多边形外环各段的交点
    for w in ls.points.windows(2) {
        for j in 0..outer.len() {
            let k = (j + 1) % outer.len();
            if segments_intersect(&w[0], &w[1], &outer[j], &outer[k]) {
                return true;
            }
        }
    }
    false
}

/// 判断点是否在多边形边界上
fn point_on_polygon_boundary(p: &Point, poly: &Polygon) -> bool {
    for ring in &poly.rings {
        for j in 0..ring.len() {
            let k = (j + 1) % ring.len();
            if point_on_segment(p, &ring[j], &ring[k]) {
                return true;
            }
        }
    }
    false
}

/// 判断点是否在线段上
fn point_on_segment(p: &Point, a: &Point, b: &Point) -> bool {
    // 叉积为 0（共线）且在矩形范围内
    let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    if cross.abs() > 1e-10 {
        return false;
    }
    let min_x = a.x.min(b.x);
    let max_x = a.x.max(b.x);
    let min_y = a.y.min(b.y);
    let max_y = a.y.max(b.y);
    p.x >= min_x - 1e-10 && p.x <= max_x + 1e-10 && p.y >= min_y - 1e-10 && p.y <= max_y + 1e-10
}

/// 判断两线段是否相交
fn segments_intersect(p1: &Point, p2: &Point, p3: &Point, p4: &Point) -> bool {
    let d1 = cross_product(p3, p4, p1);
    let d2 = cross_product(p3, p4, p2);
    let d3 = cross_product(p1, p2, p3);
    let d4 = cross_product(p1, p2, p4);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    // 共线情况
    if d1.abs() < 1e-10 && point_on_segment(p1, p3, p4) {
        return true;
    }
    if d2.abs() < 1e-10 && point_on_segment(p2, p3, p4) {
        return true;
    }
    if d3.abs() < 1e-10 && point_on_segment(p3, p1, p2) {
        return true;
    }
    if d4.abs() < 1e-10 && point_on_segment(p4, p1, p2) {
        return true;
    }
    false
}

/// 计算叉积 (p3->p4) x (p3->p1)
fn cross_product(p3: &Point, p4: &Point, p1: &Point) -> f64 {
    (p4.x - p3.x) * (p1.y - p3.y) - (p4.y - p3.y) * (p1.x - p3.x)
}

/// 判断两多边形是否相交
fn polygons_intersect(p1: &Polygon, p2: &Polygon) -> bool {
    // 简化：检查 p2 的任何顶点是否在 p1 内
    if p2.rings.is_empty() || p1.rings.is_empty() {
        return false;
    }
    for p in &p2.rings[0] {
        if p1.contains_point(p) {
            return true;
        }
    }
    for p in &p1.rings[0] {
        if p2.contains_point(p) {
            return true;
        }
    }
    false
}

/// 判断多边形 p1 是否包含多边形 p2（p2 所有顶点都在 p1 内）
fn polygon_contains_polygon(p1: &Polygon, p2: &Polygon) -> bool {
    if p2.rings.is_empty() {
        return false;
    }
    p2.rings[0].iter().all(|p| p1.contains_point(p))
}

/// 计算几何体的外包矩形 (min_x, min_y, max_x, max_y)
fn compute_bounding_box(geom: &Geometry) -> Result<(f64, f64, f64, f64), PostgisError> {
    let points = collect_all_points(geom)?;
    if points.is_empty() {
        return Err(PostgisError::InvalidGeometry(
            "cannot compute bounding box of empty geometry".to_string(),
        ));
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (x, y) in &points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    Ok((min_x, min_y, max_x, max_y))
}

/// 收集几何体中的所有点坐标
fn collect_all_points(geom: &Geometry) -> Result<Vec<(f64, f64)>, PostgisError> {
    let mut points = Vec::new();
    match geom {
        Geometry::Point(p) => points.push((p.x, p.y)),
        Geometry::LineString(ls) => {
            for p in &ls.points {
                points.push((p.x, p.y));
            }
        }
        Geometry::Polygon(poly) => {
            for ring in &poly.rings {
                for p in ring {
                    points.push((p.x, p.y));
                }
            }
        }
        Geometry::MultiPoint(pts) => {
            for p in pts {
                points.push((p.x, p.y));
            }
        }
        Geometry::MultiLineString(lss) => {
            for ls in lss {
                for p in &ls.points {
                    points.push((p.x, p.y));
                }
            }
        }
        Geometry::MultiPolygon(polys) => {
            for poly in polys {
                for ring in &poly.rings {
                    for p in ring {
                        points.push((p.x, p.y));
                    }
                }
            }
        }
    }
    Ok(points)
}

/// Andrew's monotone chain 凸包算法
fn convex_hull(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut pts: Vec<(f64, f64)> = points.to_vec();
    pts.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap()
            .then(a.1.partial_cmp(&b.1).unwrap())
    });
    pts.dedup();
    let n = pts.len();
    if n < 3 {
        return pts;
    }

    let mut hull = vec![Default::default(); 2 * n];
    let mut k = 0;

    // 下凸包
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        while k >= 2 && cross(&hull[k - 2], &hull[k - 1], &pts[i]) <= 0.0 {
            k -= 1;
        }
        hull[k] = pts[i];
        k += 1;
    }

    // 上凸包
    let lower = k + 1;
    for i in (0..n - 1).rev() {
        while k >= lower && cross(&hull[k - 2], &hull[k - 1], &pts[i]) <= 0.0 {
            k -= 1;
        }
        hull[k] = pts[i];
        k += 1;
    }

    hull.truncate(k - 1);
    hull
}

/// 叉积 (o->a) x (o->b)
fn cross(o: &(f64, f64), a: &(f64, f64), b: &(f64, f64)) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

// =============================================================================
// 六、单元测试
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- 空间索引管理测试 ---

    #[test]
    fn test_spatial_index_type_as_sql() {
        assert_eq!(SpatialIndexType::Gist.as_sql_method(), "GIST");
        assert_eq!(SpatialIndexType::SpGist.as_sql_method(), "SPGIST");
        assert_eq!(SpatialIndexType::Brin.as_sql_method(), "BRIN");
    }

    #[test]
    fn test_spatial_index_def_create_sql() {
        let def = SpatialIndexDef::new_gist("cities", "geom");
        let sql = def.to_create_sql();
        assert!(sql.contains("CREATE INDEX"));
        assert!(sql.contains("idx_cities_geom_gist"));
        assert!(sql.contains("USING GIST"));
        assert!(sql.contains("\"geom\""));
    }

    #[test]
    fn test_spatial_index_def_concurrently() {
        let def = SpatialIndexDef::new_gist("cities", "geom").with_concurrently();
        let sql = def.to_create_sql();
        assert!(sql.contains("CONCURRENTLY"));
    }

    #[test]
    fn test_spatial_index_def_drop_sql() {
        let def = SpatialIndexDef::new_gist("cities", "geom");
        let sql = def.to_drop_sql();
        assert!(sql.contains("DROP INDEX"));
        assert!(sql.contains("idx_cities_geom_gist"));
    }

    #[test]
    fn test_spatial_index_def_reindex_sql() {
        let def = SpatialIndexDef::new_gist("cities", "geom");
        let sql = def.to_reindex_sql();
        assert!(sql.contains("REINDEX INDEX"));
    }

    #[test]
    fn test_spatial_index_registry_register_and_exists() {
        let mut reg = SpatialIndexRegistry::new();
        let def = SpatialIndexDef::new_gist("cities", "geom");
        reg.register(def).unwrap();
        assert!(reg.exists("idx_cities_geom_gist"));
    }

    #[test]
    fn test_spatial_index_registry_duplicate_fails() {
        let mut reg = SpatialIndexRegistry::new();
        let def = SpatialIndexDef::new_gist("cities", "geom");
        reg.register(def).unwrap();
        let def2 = SpatialIndexDef::new_gist("cities", "geom");
        let result = reg.register(def2);
        assert!(result.is_err());
    }

    #[test]
    fn test_spatial_index_registry_unregister() {
        let mut reg = SpatialIndexRegistry::new();
        let def = SpatialIndexDef::new_gist("cities", "geom");
        reg.register(def).unwrap();
        let removed = reg.unregister("idx_cities_geom_gist").unwrap();
        assert_eq!(removed.table, "cities");
        assert!(!reg.exists("idx_cities_geom_gist"));
    }

    #[test]
    fn test_spatial_index_registry_list_for_table() {
        let mut reg = SpatialIndexRegistry::new();
        reg.register(SpatialIndexDef::new_gist("cities", "geom"))
            .unwrap();
        reg.register(SpatialIndexDef::new_gist("cities", "bbox"))
            .unwrap();
        reg.register(SpatialIndexDef::new_gist("roads", "geom"))
            .unwrap();
        let city_indexes = reg.list_for_table("cities");
        assert_eq!(city_indexes.len(), 2);
        let road_indexes = reg.list_for_table("roads");
        assert_eq!(road_indexes.len(), 1);
    }

    #[test]
    fn test_spatial_index_registry_reindex_all() {
        let mut reg = SpatialIndexRegistry::new();
        reg.register(SpatialIndexDef::new_gist("t1", "g1")).unwrap();
        reg.register(SpatialIndexDef::new_gist("t2", "g2")).unwrap();
        let sqls = reg.reindex_all_sql();
        assert_eq!(sqls.len(), 2);
        assert!(sqls.iter().all(|s| s.contains("REINDEX")));
    }

    // --- 扩展空间关系运算测试 ---

    #[test]
    fn test_st_disjoint_points() {
        let rel = MemorySpatialRelations::new();
        let p1 = Geometry::Point(Point::new(0.0, 0.0));
        let p2 = Geometry::Point(Point::new(1.0, 1.0));
        assert!(rel.st_disjoint(&p1, &p2).unwrap());
    }

    #[test]
    fn test_st_disjoint_same_point() {
        let rel = MemorySpatialRelations::new();
        let p1 = Geometry::Point(Point::new(0.0, 0.0));
        let p2 = Geometry::Point(Point::new(0.0, 0.0));
        assert!(!rel.st_disjoint(&p1, &p2).unwrap());
    }

    #[test]
    fn test_st_disjoint_point_polygon() {
        let rel = MemorySpatialRelations::new();
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]));
        let inside = Geometry::Point(Point::new(5.0, 5.0));
        let outside = Geometry::Point(Point::new(20.0, 20.0));
        assert!(!rel.st_disjoint(&inside, &poly).unwrap());
        assert!(rel.st_disjoint(&outside, &poly).unwrap());
    }

    #[test]
    fn test_st_equals_same() {
        let rel = MemorySpatialRelations::new();
        let p1 = Geometry::Point(Point::new(1.0, 2.0));
        let p2 = Geometry::Point(Point::new(1.0, 2.0));
        assert!(rel.st_equals(&p1, &p2).unwrap());
    }

    #[test]
    fn test_st_equals_different() {
        let rel = MemorySpatialRelations::new();
        let p1 = Geometry::Point(Point::new(1.0, 2.0));
        let p2 = Geometry::Point(Point::new(3.0, 4.0));
        assert!(!rel.st_equals(&p1, &p2).unwrap());
    }

    #[test]
    fn test_st_equals_srid_mismatch() {
        let rel = MemorySpatialRelations::new();
        let p1 = Geometry::Point(Point::with_srid(1.0, 2.0, 4326));
        let p2 = Geometry::Point(Point::with_srid(1.0, 2.0, 3857));
        let result = rel.st_equals(&p1, &p2);
        assert!(matches!(result, Err(PostgisError::SridMismatch { .. })));
    }

    #[test]
    fn test_st_covers_point_inside() {
        let rel = MemorySpatialRelations::new();
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]));
        let p = Geometry::Point(Point::new(5.0, 5.0));
        assert!(rel.st_covers(&poly, &p).unwrap());
    }

    #[test]
    fn test_st_covers_point_outside() {
        let rel = MemorySpatialRelations::new();
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]));
        let p = Geometry::Point(Point::new(20.0, 20.0));
        assert!(!rel.st_covers(&poly, &p).unwrap());
    }

    #[test]
    fn test_st_covered_by() {
        let rel = MemorySpatialRelations::new();
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ]));
        let p = Geometry::Point(Point::new(5.0, 5.0));
        assert!(rel.st_covered_by(&p, &poly).unwrap());
    }

    // --- 空间聚合运算测试 ---

    #[test]
    fn test_st_collect_points() {
        let agg = MemorySpatialAggregate::new();
        let pts = vec![
            Geometry::Point(Point::new(0.0, 0.0)),
            Geometry::Point(Point::new(1.0, 1.0)),
        ];
        let result = agg.st_collect(&pts).unwrap();
        match result {
            Geometry::MultiPoint(mp) => assert_eq!(mp.len(), 2),
            _ => panic!("expected MultiPoint"),
        }
    }

    #[test]
    fn test_st_collect_single() {
        let agg = MemorySpatialAggregate::new();
        let pts = vec![Geometry::Point(Point::new(0.0, 0.0))];
        let result = agg.st_collect(&pts).unwrap();
        assert!(matches!(result, Geometry::Point(_)));
    }

    #[test]
    fn test_st_collect_empty_fails() {
        let agg = MemorySpatialAggregate::new();
        let result = agg.st_collect(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_st_envelope_polygon() {
        let agg = MemorySpatialAggregate::new();
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(1.0, 2.0),
            Point::new(5.0, 1.0),
            Point::new(4.0, 8.0),
            Point::new(0.0, 6.0),
        ]));
        let envelope = agg.st_envelope(&poly).unwrap();
        // 外包矩形应该覆盖 (0,1) 到 (5,8)
        let (min_x, min_y, max_x, max_y) = compute_bounding_box(&envelope).unwrap();
        assert!((min_x - 0.0).abs() < 1e-10);
        assert!((min_y - 1.0).abs() < 1e-10);
        assert!((max_x - 5.0).abs() < 1e-10);
        assert!((max_y - 8.0).abs() < 1e-10);
        assert!(matches!(envelope, Geometry::Polygon(_)));
    }

    #[test]
    fn test_st_envelope_point() {
        let agg = MemorySpatialAggregate::new();
        let p = Geometry::Point(Point::new(3.0, 7.0));
        let envelope = agg.st_envelope(&p).unwrap();
        match envelope {
            Geometry::Polygon(poly) => {
                assert_eq!(poly.rings[0].len(), 5); // 4 角 + 闭合
            }
            _ => panic!("expected Polygon"),
        }
    }

    #[test]
    fn test_st_centroid_point() {
        let agg = MemorySpatialAggregate::new();
        let p = Geometry::Point(Point::new(3.0, 7.0));
        let centroid = agg.st_centroid(&p).unwrap();
        assert!((centroid.x - 3.0).abs() < 1e-10);
        assert!((centroid.y - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_st_centroid_polygon() {
        let agg = MemorySpatialAggregate::new();
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 4.0),
            Point::new(0.0, 4.0),
        ]));
        let centroid = agg.st_centroid(&poly).unwrap();
        // 四个顶点的算术平均：(0+4+4+0)/4 = 2.0, (0+0+4+4)/4 = 2.0
        assert!((centroid.x - 2.0).abs() < 1e-10);
        assert!((centroid.y - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_st_convex_hull() {
        let agg = MemorySpatialAggregate::new();
        // L 形点集
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 1.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 4.0),
            Point::new(0.0, 4.0),
        ]));
        let hull = agg.st_convex_hull(&poly).unwrap();
        match hull {
            Geometry::Polygon(p) => {
                // 凸包应该至少有 3 个点
                assert!(p.rings[0].len() >= 3);
            }
            _ => panic!("expected Polygon"),
        }
    }

    // --- 坐标系转换测试 ---

    #[test]
    fn test_srid_names() {
        let ct = MemoryCoordTransform::new();
        assert_eq!(ct.srid_name(srid::WGS84), "WGS84");
        assert_eq!(ct.srid_name(srid::WEB_MERCATOR), "Web Mercator");
        assert_eq!(ct.srid_name(9999), "Unknown");
    }

    #[test]
    fn test_is_srid_supported() {
        let ct = MemoryCoordTransform::new();
        assert!(ct.is_srid_supported(srid::WGS84));
        assert!(ct.is_srid_supported(srid::WEB_MERCATOR));
        assert!(!ct.is_srid_supported(9999));
    }

    #[test]
    fn test_st_transform_wgs84_to_mercator() {
        let ct = MemoryCoordTransform::new();
        // 北京天安门: 经度 116.397, 纬度 39.908
        let p = Geometry::Point(Point::with_srid(116.397, 39.908, srid::WGS84));
        let transformed = ct.st_transform(&p, srid::WEB_MERCATOR).unwrap();
        match transformed {
            Geometry::Point(mp) => {
                assert_eq!(mp.srid, srid::WEB_MERCATOR);
                // 墨卡托 X ≈ 12,958,000 米
                assert!(mp.x > 12_900_000.0 && mp.x < 13_000_000.0);
                // 墨卡托 Y ≈ 4,868,000 米
                assert!(mp.y > 4_800_000.0 && mp.y < 4_900_000.0);
            }
            _ => panic!("expected Point"),
        }
    }

    #[test]
    fn test_st_transform_mercator_to_wgs84() {
        let ct = MemoryCoordTransform::new();
        // 使用正向转换得到的精确墨卡托坐标，再反向转回 WGS84（round-trip 验证）
        // 北京天安门：经度 116.397, 纬度 39.908
        let original = Geometry::Point(Point::with_srid(116.397, 39.908, srid::WGS84));
        let mercator = ct.st_transform(&original, srid::WEB_MERCATOR).unwrap();
        let back = ct.st_transform(&mercator, srid::WGS84).unwrap();
        match back {
            Geometry::Point(p) => {
                assert_eq!(p.srid, srid::WGS84);
                // round-trip 误差应极小
                assert!((p.x - 116.397).abs() < 1e-6);
                assert!((p.y - 39.908).abs() < 1e-6);
            }
            _ => panic!("expected Point"),
        }
    }

    #[test]
    fn test_st_transform_same_srid() {
        let ct = MemoryCoordTransform::new();
        let p = Geometry::Point(Point::with_srid(116.0, 39.0, srid::WGS84));
        let transformed = ct.st_transform(&p, srid::WGS84).unwrap();
        match transformed {
            Geometry::Point(tp) => {
                assert!((tp.x - 116.0).abs() < 1e-10);
                assert!((tp.y - 39.0).abs() < 1e-10);
            }
            _ => panic!("expected Point"),
        }
    }

    #[test]
    fn test_st_transform_unsupported_srid() {
        let ct = MemoryCoordTransform::new();
        let p = Geometry::Point(Point::with_srid(116.0, 39.0, srid::WGS84));
        let result = ct.st_transform(&p, 9999);
        assert!(matches!(result, Err(PostgisError::Unsupported(_))));
    }

    #[test]
    fn test_st_transform_linestring() {
        let ct = MemoryCoordTransform::new();
        let ls = Geometry::LineString(LineString::new(vec![
            Point::with_srid(116.0, 39.0, srid::WGS84),
            Point::with_srid(117.0, 40.0, srid::WGS84),
        ]));
        let transformed = ct.st_transform(&ls, srid::WEB_MERCATOR).unwrap();
        match transformed {
            Geometry::LineString(tls) => {
                assert_eq!(tls.srid, srid::WEB_MERCATOR);
                assert_eq!(tls.points.len(), 2);
            }
            _ => panic!("expected LineString"),
        }
    }

    // --- 辅助函数测试 ---

    #[test]
    fn test_point_on_segment() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 0.0);
        let on = Point::new(5.0, 0.0);
        let off = Point::new(5.0, 1.0);
        assert!(point_on_segment(&on, &a, &b));
        assert!(!point_on_segment(&off, &a, &b));
    }

    #[test]
    fn test_segments_intersect_crossing() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(10.0, 10.0);
        let p3 = Point::new(0.0, 10.0);
        let p4 = Point::new(10.0, 0.0);
        assert!(segments_intersect(&p1, &p2, &p3, &p4));
    }

    #[test]
    fn test_segments_intersect_parallel() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(10.0, 0.0);
        let p3 = Point::new(0.0, 5.0);
        let p4 = Point::new(10.0, 5.0);
        assert!(!segments_intersect(&p1, &p2, &p3, &p4));
    }

    #[test]
    fn test_convex_hull_square() {
        let points = vec![
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (5.0, 5.0), // 内部点，不应出现在凸包中
        ];
        let hull = convex_hull(&points);
        assert_eq!(hull.len(), 4); // 正方形的 4 个角
    }

    #[test]
    fn test_compute_bounding_box() {
        let poly = Geometry::Polygon(Polygon::new(vec![
            Point::new(3.0, 7.0),
            Point::new(1.0, 5.0),
            Point::new(8.0, 2.0),
        ]));
        let (min_x, min_y, max_x, max_y) = compute_bounding_box(&poly).unwrap();
        assert!((min_x - 1.0).abs() < 1e-10);
        assert!((min_y - 2.0).abs() < 1e-10);
        assert!((max_x - 8.0).abs() < 1e-10);
        assert!((max_y - 7.0).abs() < 1e-10);
    }
}
