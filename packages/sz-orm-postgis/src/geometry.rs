//! 几何类型定义
//!
//! 提供 PostGIS 兼容的几何类型，支持 EWKT/EWKB 序列化。
//! 所有几何类型携带 SRID（坐标参考系统 ID），默认 WGS84（SRID=4326）。

use crate::error::PostgisError;
use serde::{Deserialize, Serialize};

/// 默认 SRID：WGS84 经纬度
pub const DEFAULT_SRID: i32 = 4326;

/// 二维点
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub srid: i32,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            srid: DEFAULT_SRID,
        }
    }

    pub fn with_srid(x: f64, y: f64, srid: i32) -> Self {
        Self { x, y, srid }
    }

    /// 计算到另一点的欧氏距离（不考虑 SRID 投影，仅用于内存实现）
    pub fn euclidean_distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// 计算到另一点的大圆距离（Haversine 公式，假设 SRID=4326 经纬度）
    pub fn haversine_distance(&self, other: &Point) -> f64 {
        const EARTH_RADIUS_M: f64 = 6_371_000.0;
        let to_rad = |deg: f64| deg * std::f64::consts::PI / 180.0;
        let lat1 = to_rad(self.y);
        let lat2 = to_rad(other.y);
        let dlat = to_rad(other.y - self.y);
        let dlon = to_rad(other.x - self.x);
        let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().asin();
        EARTH_RADIUS_M * c
    }

    /// 转为 EWKT 字符串：`SRID=4326;POINT(x y)`
    pub fn to_ewkt(&self) -> String {
        format!("SRID={};POINT({} {})", self.srid, self.x, self.y)
    }
}

/// 线串：由有序点组成
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineString {
    pub points: Vec<Point>,
    pub srid: i32,
}

impl LineString {
    pub fn new(points: Vec<Point>) -> Self {
        let srid = points.first().map(|p| p.srid).unwrap_or(DEFAULT_SRID);
        Self { points, srid }
    }

    /// 计算线串总长度（欧氏）
    pub fn euclidean_length(&self) -> f64 {
        self.points
            .windows(2)
            .map(|w| w[0].euclidean_distance(&w[1]))
            .sum()
    }

    /// 计算线串总长度（Haversine，假设经纬度）
    pub fn haversine_length(&self) -> f64 {
        self.points
            .windows(2)
            .map(|w| w[0].haversine_distance(&w[1]))
            .sum()
    }

    pub fn to_ewkt(&self) -> String {
        let coords: Vec<String> = self
            .points
            .iter()
            .map(|p| format!("{} {}", p.x, p.y))
            .collect();
        format!("SRID={};LINESTRING({})", self.srid, coords.join(", "))
    }
}

/// 多边形：由外环和可选内环（洞）组成
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polygon {
    pub rings: Vec<Vec<Point>>,
    pub srid: i32,
}

impl Polygon {
    pub fn new(outer: Vec<Point>) -> Self {
        let srid = outer.first().map(|p| p.srid).unwrap_or(DEFAULT_SRID);
        Self {
            rings: vec![outer],
            srid,
        }
    }

    pub fn with_holes(outer: Vec<Point>, holes: Vec<Vec<Point>>) -> Self {
        let srid = outer.first().map(|p| p.srid).unwrap_or(DEFAULT_SRID);
        let mut rings = vec![outer];
        rings.extend(holes);
        Self { rings, srid }
    }

    /// 计算多边形面积（Shoelace 公式，仅用外环）
    pub fn shoelace_area(&self) -> f64 {
        if self.rings.is_empty() {
            return 0.0;
        }
        let outer = &self.rings[0];
        if outer.len() < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for i in 0..outer.len() {
            let j = (i + 1) % outer.len();
            sum += outer[i].x * outer[j].y;
            sum -= outer[j].x * outer[i].y;
        }
        (sum / 2.0).abs()
    }

    /// 判断点是否在多边形内（射线法，仅用外环）
    pub fn contains_point(&self, point: &Point) -> bool {
        if self.rings.is_empty() {
            return false;
        }
        let outer = &self.rings[0];
        let mut inside = false;
        let mut j = outer.len() - 1;
        for i in 0..outer.len() {
            let intersect = (outer[i].y > point.y) != (outer[j].y > point.y)
                && point.x
                    < (outer[j].x - outer[i].x) * (point.y - outer[i].y)
                        / (outer[j].y - outer[i].y)
                        + outer[i].x;
            if intersect {
                inside = !inside;
            }
            j = i;
        }
        // 若在外环内，检查是否在洞内
        if inside {
            for hole in self.rings.iter().skip(1) {
                let mut hole_inside = false;
                let mut j = hole.len() - 1;
                for i in 0..hole.len() {
                    let intersect = (hole[i].y > point.y) != (hole[j].y > point.y)
                        && point.x
                            < (hole[j].x - hole[i].x) * (point.y - hole[i].y)
                                / (hole[j].y - hole[i].y)
                                + hole[i].x;
                    if intersect {
                        hole_inside = !hole_inside;
                    }
                    j = i;
                }
                if hole_inside {
                    return false;
                }
            }
        }
        inside
    }

    pub fn to_ewkt(&self) -> String {
        let rings: Vec<String> = self
            .rings
            .iter()
            .map(|ring| {
                let coords: Vec<String> = ring.iter().map(|p| format!("{} {}", p.x, p.y)).collect();
                format!("({})", coords.join(", "))
            })
            .collect();
        format!("SRID={};POLYGON({})", self.srid, rings.join(", "))
    }
}

/// 几何类型枚举（统一容器）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Geometry {
    Point(Point),
    LineString(LineString),
    Polygon(Polygon),
    MultiPoint(Vec<Point>),
    MultiLineString(Vec<LineString>),
    MultiPolygon(Vec<Polygon>),
}

impl Geometry {
    /// 获取 SRID
    pub fn srid(&self) -> i32 {
        match self {
            Geometry::Point(p) => p.srid,
            Geometry::LineString(ls) => ls.srid,
            Geometry::Polygon(poly) => poly.srid,
            Geometry::MultiPoint(pts) => pts.first().map(|p| p.srid).unwrap_or(DEFAULT_SRID),
            Geometry::MultiLineString(lss) => lss.first().map(|ls| ls.srid).unwrap_or(DEFAULT_SRID),
            Geometry::MultiPolygon(polys) => polys.first().map(|p| p.srid).unwrap_or(DEFAULT_SRID),
        }
    }

    /// 校验 SRID 一致性（多几何体场景）
    pub fn validate_srid(&self) -> Result<(), PostgisError> {
        let expected = self.srid();
        let check = |srid: i32| -> Result<(), PostgisError> {
            if srid != expected {
                Err(PostgisError::SridMismatch {
                    expected,
                    actual: srid,
                })
            } else {
                Ok(())
            }
        };
        match self {
            Geometry::MultiPoint(pts) => {
                for p in pts {
                    check(p.srid)?;
                }
            }
            Geometry::MultiLineString(lss) => {
                for ls in lss {
                    check(ls.srid)?;
                }
            }
            Geometry::MultiPolygon(polys) => {
                for p in polys {
                    check(p.srid)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// 类型名称（用于错误信息）
    pub fn type_name(&self) -> &'static str {
        match self {
            Geometry::Point(_) => "Point",
            Geometry::LineString(_) => "LineString",
            Geometry::Polygon(_) => "Polygon",
            Geometry::MultiPoint(_) => "MultiPoint",
            Geometry::MultiLineString(_) => "MultiLineString",
            Geometry::MultiPolygon(_) => "MultiPolygon",
        }
    }

    /// 从 EWKT 字符串解析几何体
    ///
    /// 支持格式：`SRID=4326;POINT(x y)` / `SRID=4326;LINESTRING(...)` / `SRID=4326;POLYGON(...)`
    /// 也支持无 SRID 前缀的 WKT（使用 DEFAULT_SRID）
    ///
    /// v0.2.2 新增：用于 real_postgis.rs 解析 ST_AsEWKT 返回值
    pub fn from_ewkt(ewkt: &str) -> Result<Self, PostgisError> {
        let (srid, wkt) = if let Some(semi) = ewkt.find(';') {
            let srid_str = &ewkt[..semi];
            let wkt = &ewkt[semi + 1..];
            if !srid_str.starts_with("SRID=") {
                return Err(PostgisError::Query(format!(
                    "invalid EWKT SRID prefix: {}",
                    srid_str
                )));
            }
            let srid: i32 = srid_str[5..]
                .parse()
                .map_err(|e| PostgisError::Query(format!("invalid SRID: {}", e)))?;
            (srid, wkt)
        } else {
            (DEFAULT_SRID, ewkt)
        };

        let wkt = wkt.trim();
        let upper = wkt.to_uppercase();

        if upper.starts_with("POINT") {
            let coords = extract_paren_content(&upper, "POINT")?;
            let nums = parse_coord_pair(&coords)?;
            Ok(Geometry::Point(Point::with_srid(nums.0, nums.1, srid)))
        } else if upper.starts_with("LINESTRING") {
            let coords = extract_paren_content(&upper, "LINESTRING")?;
            let points = parse_coord_list(&coords)?
                .into_iter()
                .map(|(x, y)| Point::with_srid(x, y, srid))
                .collect();
            Ok(Geometry::LineString(LineString { points, srid }))
        } else if upper.starts_with("POLYGON") {
            let rings_str = extract_paren_content(&upper, "POLYGON")?;
            // rings_str 形如 (x1 y1, x2 y2, ...), (x3 y3, ...)
            let rings = parse_polygon_rings(&rings_str, srid)?;
            Ok(Geometry::Polygon(Polygon { rings, srid }))
        } else if upper.starts_with("MULTIPOINT") {
            let coords = extract_paren_content(&upper, "MULTIPOINT")?;
            let points = parse_coord_list(&coords)?
                .into_iter()
                .map(|(x, y)| Point::with_srid(x, y, srid))
                .collect();
            Ok(Geometry::MultiPoint(points))
        } else {
            Err(PostgisError::Query(format!(
                "unsupported WKT type in: {}",
                wkt
            )))
        }
    }

    /// 转 EWKT 字符串
    pub fn to_ewkt(&self) -> String {
        match self {
            Geometry::Point(p) => p.to_ewkt(),
            Geometry::LineString(ls) => ls.to_ewkt(),
            Geometry::Polygon(poly) => poly.to_ewkt(),
            Geometry::MultiPoint(pts) => {
                let srid = pts.first().map(|p| p.srid).unwrap_or(DEFAULT_SRID);
                let coords: Vec<String> = pts.iter().map(|p| format!("{} {}", p.x, p.y)).collect();
                format!("SRID={};MULTIPOINT({})", srid, coords.join(", "))
            }
            Geometry::MultiLineString(lss) => {
                let srid = lss.first().map(|ls| ls.srid).unwrap_or(DEFAULT_SRID);
                let lines: Vec<String> = lss
                    .iter()
                    .map(|ls| {
                        let coords: Vec<String> = ls
                            .points
                            .iter()
                            .map(|p| format!("{} {}", p.x, p.y))
                            .collect();
                        format!("({})", coords.join(", "))
                    })
                    .collect();
                format!("SRID={};MULTILINESTRING({})", srid, lines.join(", "))
            }
            Geometry::MultiPolygon(polys) => {
                let srid = polys.first().map(|p| p.srid).unwrap_or(DEFAULT_SRID);
                let polygons: Vec<String> = polys
                    .iter()
                    .map(|poly| {
                        let rings: Vec<String> = poly
                            .rings
                            .iter()
                            .map(|ring| {
                                let coords: Vec<String> =
                                    ring.iter().map(|p| format!("{} {}", p.x, p.y)).collect();
                                format!("({})", coords.join(", "))
                            })
                            .collect();
                        format!("({})", rings.join(", "))
                    })
                    .collect();
                format!("SRID={};MULTIPOLYGON({})", srid, polygons.join(", "))
            }
        }
    }
}

/// 从 WKT 中提取括号内容：`POINT(x y)` → `x y`
fn extract_paren_content(upper_wkt: &str, type_name: &str) -> Result<String, PostgisError> {
    let start = upper_wkt
        .find(type_name)
        .ok_or_else(|| PostgisError::Query(format!("missing type name {}", type_name)))?
        + type_name.len();
    let rest = &upper_wkt[start..];
    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return Err(PostgisError::Query(format!(
            "missing opening paren after {}: {}",
            type_name, rest
        )));
    }
    // 找到匹配的右括号（处理嵌套，如 POLYGON((...)(...))）
    let mut depth = 0i32;
    let mut end = 0usize;
    for (i, c) in rest.chars().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(PostgisError::Query(format!(
            "unbalanced parens in {}: {}",
            type_name, rest
        )));
    }
    Ok(rest[1..end].to_string())
}

/// 解析坐标对：`x y` → (f64, f64)
fn parse_coord_pair(s: &str) -> Result<(f64, f64), PostgisError> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(PostgisError::Query(format!(
            "expected 2 coords, got {}: {}",
            parts.len(),
            s
        )));
    }
    let x: f64 = parts[0]
        .parse()
        .map_err(|e| PostgisError::Query(format!("invalid x coord: {}", e)))?;
    let y: f64 = parts[1]
        .parse()
        .map_err(|e| PostgisError::Query(format!("invalid y coord: {}", e)))?;
    Ok((x, y))
}

/// 解析坐标列表：`x1 y1, x2 y2, ...` → Vec<(x, y)>
fn parse_coord_list(s: &str) -> Result<Vec<(f64, f64)>, PostgisError> {
    s.split(',')
        .map(|pair| parse_coord_pair(pair.trim()))
        .collect()
}

/// 解析多边形环：`(x1 y1, x2 y2, ...), (x3 y3, ...)` → Vec<Vec<Point>>
fn parse_polygon_rings(s: &str, srid: i32) -> Result<Vec<Vec<Point>>, PostgisError> {
    let mut rings = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in s.chars() {
        match c {
            '(' => {
                depth += 1;
                if depth == 1 {
                    current.clear();
                } else {
                    current.push(c);
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let ring: Vec<Point> = parse_coord_list(&current)?
                        .into_iter()
                        .map(|(x, y)| Point::with_srid(x, y, srid))
                        .collect();
                    rings.push(ring);
                } else {
                    current.push(c);
                }
            }
            _ if depth >= 1 => {
                current.push(c);
            }
            _ => {} // 顶层（depth==0）的空格/逗号是分隔符
        }
    }
    if rings.is_empty() {
        return Err(PostgisError::Query(format!("no rings parsed from: {}", s)));
    }
    Ok(rings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_ewkt() {
        let p = Point::new(116.404, 39.915);
        assert_eq!(p.to_ewkt(), "SRID=4326;POINT(116.404 39.915)");
    }

    #[test]
    fn test_point_euclidean_distance() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(3.0, 4.0);
        let dist = p1.euclidean_distance(&p2);
        assert!((dist - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_haversine_distance() {
        // 北京到上海约 1067 km
        let beijing = Point::new(116.404, 39.915);
        let shanghai = Point::new(121.474, 31.230);
        let dist = beijing.haversine_distance(&shanghai);
        assert!(dist > 1_000_000.0 && dist < 1_200_000.0);
    }

    #[test]
    fn test_linestring_length() {
        let ls = LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(3.0, 4.0),
            Point::new(3.0, 9.0),
        ]);
        // 0->1: 5, 1->2: 5
        let len = ls.euclidean_length();
        assert!((len - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_polygon_area() {
        let poly = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 3.0),
            Point::new(0.0, 3.0),
        ]);
        let area = poly.shoelace_area();
        assert!((area - 12.0).abs() < 1e-10);
    }

    #[test]
    fn test_polygon_contains_point() {
        let poly = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 3.0),
            Point::new(0.0, 3.0),
        ]);
        assert!(poly.contains_point(&Point::new(2.0, 1.5)));
        assert!(!poly.contains_point(&Point::new(5.0, 1.5)));
    }

    #[test]
    fn test_polygon_with_hole() {
        let outer = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let hole = vec![
            Point::new(3.0, 3.0),
            Point::new(7.0, 3.0),
            Point::new(7.0, 7.0),
            Point::new(3.0, 7.0),
        ];
        let poly = Polygon::with_holes(outer, vec![hole]);
        // 外环内、洞外
        assert!(poly.contains_point(&Point::new(1.0, 1.0)));
        // 洞内
        assert!(!poly.contains_point(&Point::new(5.0, 5.0)));
    }

    #[test]
    fn test_geometry_srid_validation() {
        let g = Geometry::MultiPoint(vec![
            Point::with_srid(0.0, 0.0, 4326),
            Point::with_srid(1.0, 1.0, 3857),
        ]);
        assert!(matches!(
            g.validate_srid(),
            Err(PostgisError::SridMismatch {
                expected: 4326,
                actual: 3857
            })
        ));
    }

    #[test]
    fn test_geometry_ewkt() {
        let g = Geometry::Point(Point::new(116.404, 39.915));
        assert_eq!(g.to_ewkt(), "SRID=4326;POINT(116.404 39.915)");
        assert_eq!(g.type_name(), "Point");
    }
}
