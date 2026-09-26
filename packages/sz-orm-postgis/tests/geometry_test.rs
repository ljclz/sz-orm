//! M3: geometry.rs 补充测试 — 覆盖 Point/LineString/Polygon/Geometry 方法

use sz_orm_postgis::{Geometry, LineString, Point, Polygon, DEFAULT_SRID};

// === Point ===

#[test]
fn test_point_new_default_srid() {
    let p = Point::new(1.0, 2.0);
    assert_eq!(p.x, 1.0);
    assert_eq!(p.y, 2.0);
    assert_eq!(p.srid, DEFAULT_SRID);
}

#[test]
fn test_point_with_srid_custom() {
    let p = Point::with_srid(1.0, 2.0, 3857);
    assert_eq!(p.srid, 3857);
}

#[test]
fn test_point_euclidean_distance() {
    let p1 = Point::new(0.0, 0.0);
    let p2 = Point::new(3.0, 4.0);
    assert!((p1.euclidean_distance(&p2) - 5.0).abs() < 1e-9);
}

#[test]
fn test_point_euclidean_distance_zero() {
    let p = Point::new(1.0, 1.0);
    assert!((p.euclidean_distance(&p) - 0.0).abs() < 1e-9);
}

#[test]
fn test_point_haversine_distance_same_point() {
    let p = Point::new(116.404, 39.915);
    assert!(p.haversine_distance(&p) < 1e-6);
}

#[test]
fn test_point_haversine_distance_beijing_shanghai() {
    let beijing = Point::new(116.404, 39.915);
    let shanghai = Point::new(121.474, 31.230);
    let dist = beijing.haversine_distance(&shanghai);
    assert!(dist > 1_000_000.0 && dist < 1_200_000.0);
}

#[test]
fn test_point_to_ewkt() {
    let p = Point::with_srid(1.0, 2.0, 4326);
    assert_eq!(p.to_ewkt(), "SRID=4326;POINT(1 2)");
}

#[test]
fn test_point_to_wkt() {
    let p = Point::new(1.0, 2.0);
    assert_eq!(p.to_wkt(), "POINT(1 2)");
}

#[test]
fn test_point_midpoint() {
    let p1 = Point::new(0.0, 0.0);
    let p2 = Point::new(10.0, 20.0);
    let mid = p1.midpoint(&p2);
    assert!((mid.x - 5.0).abs() < 1e-9);
    assert!((mid.y - 10.0).abs() < 1e-9);
}

#[test]
fn test_point_bearing_north() {
    let p1 = Point::new(0.0, 0.0);
    let p2 = Point::new(0.0, 1.0);
    let bearing = p1.bearing(&p2);
    assert!((bearing - 0.0).abs() < 1e-6);
}

#[test]
fn test_point_bearing_east() {
    let p1 = Point::new(0.0, 0.0);
    let p2 = Point::new(1.0, 0.0);
    let bearing = p1.bearing(&p2);
    assert!((bearing - 90.0).abs() < 1e-6);
}

#[test]
fn test_point_equality() {
    let p1 = Point::new(1.0, 2.0);
    let p2 = Point::new(1.0, 2.0);
    let p3 = Point::new(1.0, 3.0);
    assert_eq!(p1, p2);
    assert_ne!(p1, p3);
}

// === LineString ===

#[test]
fn test_linestring_new() {
    let ls = LineString::new(vec![Point::new(0.0, 0.0), Point::new(3.0, 4.0)]);
    assert_eq!(ls.point_count(), 2);
    assert_eq!(ls.srid, DEFAULT_SRID);
}

#[test]
fn test_linestring_euclidean_length() {
    let ls = LineString::new(vec![
        Point::new(0.0, 0.0),
        Point::new(3.0, 4.0),
        Point::new(3.0, 9.0),
    ]);
    assert!((ls.euclidean_length() - 10.0).abs() < 1e-9);
}

#[test]
fn test_linestring_haversine_length_positive() {
    let ls = LineString::new(vec![
        Point::new(116.404, 39.915),
        Point::new(121.474, 31.230),
    ]);
    assert!(ls.haversine_length() > 1_000_000.0);
}

#[test]
fn test_linestring_to_ewkt() {
    let ls = LineString::new(vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0)]);
    let ewkt = ls.to_ewkt();
    assert!(ewkt.starts_with("SRID=4326;LINESTRING("));
    assert!(ewkt.contains("1 2"));
    assert!(ewkt.contains("3 4"));
}

#[test]
fn test_linestring_to_wkt() {
    let ls = LineString::new(vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0)]);
    let wkt = ls.to_wkt();
    assert!(wkt.starts_with("LINESTRING("));
    assert!(!wkt.contains("SRID"));
}

#[test]
fn test_linestring_empty() {
    let ls = LineString::new(vec![]);
    assert_eq!(ls.point_count(), 0);
    assert!((ls.euclidean_length() - 0.0).abs() < 1e-9);
}

// === Polygon ===

#[test]
fn test_polygon_new() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
        Point::new(0.0, 0.0),
    ]);
    assert_eq!(poly.ring_count(), 1);
    assert_eq!(poly.srid, DEFAULT_SRID);
}

#[test]
fn test_polygon_shoelace_area_square() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ]);
    assert!((poly.shoelace_area() - 100.0).abs() < 1e-9);
}

#[test]
fn test_polygon_shoelace_area_triangle() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(6.0, 0.0),
        Point::new(3.0, 4.0),
    ]);
    assert!((poly.shoelace_area() - 12.0).abs() < 1e-9);
}

#[test]
fn test_polygon_shoelace_area_empty() {
    let poly = Polygon::new(vec![]);
    assert!((poly.shoelace_area() - 0.0).abs() < 1e-9);
}

#[test]
fn test_polygon_contains_point_inside() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ]);
    let center = Point::new(5.0, 5.0);
    assert!(poly.contains_point(&center));
}

#[test]
fn test_polygon_contains_point_outside() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ]);
    let outside = Point::new(15.0, 15.0);
    assert!(!poly.contains_point(&outside));
}

#[test]
fn test_polygon_to_ewkt() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 1.0),
    ]);
    let ewkt = poly.to_ewkt();
    assert!(ewkt.starts_with("SRID=4326;POLYGON("));
}

#[test]
fn test_polygon_to_wkt() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
    ]);
    let wkt = poly.to_wkt();
    assert!(wkt.starts_with("POLYGON("));
    assert!(!wkt.contains("SRID"));
}

#[test]
fn test_polygon_perimeter() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ]);
    assert!((poly.perimeter() - 40.0).abs() < 1e-9);
}

#[test]
fn test_polygon_with_holes() {
    let outer = vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ];
    let hole = vec![
        Point::new(2.0, 2.0),
        Point::new(8.0, 2.0),
        Point::new(8.0, 8.0),
        Point::new(2.0, 8.0),
    ];
    let poly = Polygon::with_holes(outer, vec![hole]);
    assert_eq!(poly.ring_count(), 2);
}

#[test]
fn test_polygon_contains_point_in_hole() {
    let outer = vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ];
    let hole = vec![
        Point::new(2.0, 2.0),
        Point::new(8.0, 2.0),
        Point::new(8.0, 8.0),
        Point::new(2.0, 8.0),
    ];
    let poly = Polygon::with_holes(outer, vec![hole]);
    let in_hole = Point::new(5.0, 5.0);
    assert!(!poly.contains_point(&in_hole));
}

// === Geometry enum ===

#[test]
fn test_geometry_point_srid() {
    let g = Geometry::Point(Point::with_srid(1.0, 2.0, 3857));
    assert_eq!(g.srid(), 3857);
}

#[test]
fn test_geometry_linestring_srid() {
    let ls = LineString::new(vec![Point::new(1.0, 2.0)]);
    let g = Geometry::LineString(ls);
    assert_eq!(g.srid(), DEFAULT_SRID);
}

#[test]
fn test_geometry_polygon_srid() {
    let poly = Polygon::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
    let g = Geometry::Polygon(poly);
    assert_eq!(g.srid(), DEFAULT_SRID);
}

#[test]
fn test_geometry_multipoint_srid() {
    let pts = vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0)];
    let g = Geometry::MultiPoint(pts);
    assert_eq!(g.srid(), DEFAULT_SRID);
}

#[test]
fn test_geometry_type_name() {
    assert_eq!(Geometry::Point(Point::new(0.0, 0.0)).type_name(), "Point");
    assert_eq!(
        Geometry::LineString(LineString::new(vec![])).type_name(),
        "LineString"
    );
    assert_eq!(
        Geometry::Polygon(Polygon::new(vec![])).type_name(),
        "Polygon"
    );
    assert_eq!(Geometry::MultiPoint(vec![]).type_name(), "MultiPoint");
    assert_eq!(
        Geometry::MultiLineString(vec![]).type_name(),
        "MultiLineString"
    );
    assert_eq!(Geometry::MultiPolygon(vec![]).type_name(), "MultiPolygon");
}

#[test]
fn test_geometry_validate_srid_consistent() {
    let pts = vec![
        Point::with_srid(1.0, 2.0, 4326),
        Point::with_srid(3.0, 4.0, 4326),
    ];
    let g = Geometry::MultiPoint(pts);
    assert!(g.validate_srid().is_ok());
}

#[test]
fn test_geometry_validate_srid_mismatch() {
    let pts = vec![
        Point::with_srid(1.0, 2.0, 4326),
        Point::with_srid(3.0, 4.0, 3857),
    ];
    let g = Geometry::MultiPoint(pts);
    assert!(g.validate_srid().is_err());
}

#[test]
fn test_geometry_bounding_box_point() {
    let g = Geometry::Point(Point::new(5.0, 10.0));
    let bbox = g.bounding_box().unwrap();
    assert_eq!(bbox, (5.0, 10.0, 5.0, 10.0));
}

#[test]
fn test_geometry_bounding_box_linestring() {
    let ls = LineString::new(vec![
        Point::new(1.0, 5.0),
        Point::new(3.0, 2.0),
        Point::new(7.0, 8.0),
    ]);
    let g = Geometry::LineString(ls);
    let bbox = g.bounding_box().unwrap();
    assert_eq!(bbox, (1.0, 2.0, 7.0, 8.0));
}

#[test]
fn test_geometry_bounding_box_polygon() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ]);
    let g = Geometry::Polygon(poly);
    let bbox = g.bounding_box().unwrap();
    assert_eq!(bbox, (0.0, 0.0, 10.0, 10.0));
}

#[test]
fn test_geometry_to_wkt_point() {
    let g = Geometry::Point(Point::new(1.0, 2.0));
    assert_eq!(g.to_wkt(), "POINT(1 2)");
}

#[test]
fn test_geometry_to_ewkt_point() {
    let g = Geometry::Point(Point::with_srid(1.0, 2.0, 4326));
    assert_eq!(g.to_ewkt(), "SRID=4326;POINT(1 2)");
}

#[test]
fn test_geometry_from_ewkt_point() {
    let g = Geometry::from_ewkt("SRID=4326;POINT(1 2)").unwrap();
    assert_eq!(g.type_name(), "Point");
    assert_eq!(g.srid(), 4326);
}

#[test]
fn test_geometry_from_ewkt_linestring() {
    let g = Geometry::from_ewkt("SRID=4326;LINESTRING(1 2, 3 4)").unwrap();
    assert_eq!(g.type_name(), "LineString");
    assert_eq!(g.srid(), 4326);
}

#[test]
fn test_geometry_from_ewkt_polygon() {
    let g = Geometry::from_ewkt("SRID=4326;POLYGON((0 0, 1 0, 1 1, 0 1, 0 0))").unwrap();
    assert_eq!(g.type_name(), "Polygon");
    assert_eq!(g.srid(), 4326);
}

#[test]
fn test_geometry_from_ewkt_no_srid() {
    let g = Geometry::from_ewkt("POINT(1 2)").unwrap();
    assert_eq!(g.srid(), DEFAULT_SRID);
}

#[test]
fn test_geometry_from_ewkt_invalid_srid() {
    let result = Geometry::from_ewkt("SRID=abc;POINT(1 2)");
    assert!(result.is_err());
}

#[test]
fn test_geometry_from_ewkt_invalid_prefix() {
    let result = Geometry::from_ewkt("XYZ=4326;POINT(1 2)");
    assert!(result.is_err());
}

#[test]
fn test_geometry_from_ewkt_unsupported_type() {
    let result = Geometry::from_ewkt("SRID=4326;GEOMETRYCOLLECTION(1 2)");
    assert!(result.is_err());
}

#[test]
fn test_geometry_from_ewkt_multipoint() {
    let g = Geometry::from_ewkt("SRID=4326;MULTIPOINT(1 2, 3 4)").unwrap();
    assert_eq!(g.type_name(), "MultiPoint");
    assert_eq!(g.srid(), 4326);
}

#[test]
fn test_geometry_to_wkt_multipoint() {
    let pts = vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0)];
    let g = Geometry::MultiPoint(pts);
    let wkt = g.to_wkt();
    assert!(wkt.starts_with("MULTIPOINT("));
    assert!(wkt.contains("1 2"));
    assert!(wkt.contains("3 4"));
}

#[test]
fn test_geometry_to_wkt_multilinestring() {
    let ls1 = LineString::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
    let ls2 = LineString::new(vec![Point::new(2.0, 2.0), Point::new(3.0, 3.0)]);
    let g = Geometry::MultiLineString(vec![ls1, ls2]);
    let wkt = g.to_wkt();
    assert!(wkt.starts_with("MULTILINESTRING("));
}

#[test]
fn test_geometry_to_wkt_multipolygon() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 1.0),
    ]);
    let g = Geometry::MultiPolygon(vec![poly]);
    let wkt = g.to_wkt();
    assert!(wkt.starts_with("MULTIPOLYGON("));
}

#[test]
fn test_geometry_to_ewkt_multipoint() {
    let pts = vec![
        Point::with_srid(1.0, 2.0, 4326),
        Point::with_srid(3.0, 4.0, 4326),
    ];
    let g = Geometry::MultiPoint(pts);
    let ewkt = g.to_ewkt();
    assert!(ewkt.starts_with("SRID=4326;MULTIPOINT("));
}

#[test]
fn test_geometry_to_ewkt_multilinestring() {
    let ls = LineString::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
    let g = Geometry::MultiLineString(vec![ls]);
    let ewkt = g.to_ewkt();
    assert!(ewkt.starts_with("SRID=4326;MULTILINESTRING("));
}

#[test]
fn test_geometry_bounding_box_multipoint() {
    let pts = vec![
        Point::new(1.0, 5.0),
        Point::new(3.0, 2.0),
        Point::new(7.0, 8.0),
    ];
    let g = Geometry::MultiPoint(pts);
    let bbox = g.bounding_box().unwrap();
    assert_eq!(bbox, (1.0, 2.0, 7.0, 8.0));
}

#[test]
fn test_geometry_bounding_box_empty_multipoint() {
    let g = Geometry::MultiPoint(vec![]);
    assert!(g.bounding_box().is_none());
}

#[test]
fn test_geometry_validate_srid_single_point() {
    let g = Geometry::Point(Point::with_srid(1.0, 2.0, 4326));
    assert!(g.validate_srid().is_ok());
}

#[test]
fn test_geometry_validate_srid_multipolygon_consistent() {
    let poly1 = Polygon::new(vec![
        Point::with_srid(0.0, 0.0, 4326),
        Point::with_srid(1.0, 1.0, 4326),
    ]);
    let poly2 = Polygon::new(vec![
        Point::with_srid(2.0, 2.0, 4326),
        Point::with_srid(3.0, 3.0, 4326),
    ]);
    let g = Geometry::MultiPolygon(vec![poly1, poly2]);
    assert!(g.validate_srid().is_ok());
}

#[test]
fn test_geometry_validate_srid_multilinestring_mismatch() {
    let ls1 = LineString::new(vec![
        Point::with_srid(0.0, 0.0, 4326),
        Point::with_srid(1.0, 1.0, 4326),
    ]);
    let ls2 = LineString::new(vec![
        Point::with_srid(0.0, 0.0, 3857),
        Point::with_srid(1.0, 1.0, 3857),
    ]);
    let g = Geometry::MultiLineString(vec![ls1, ls2]);
    assert!(g.validate_srid().is_err());
}
#[test]
fn test_geometry_from_ewkt_missing_paren() {
    let result = Geometry::from_ewkt("SRID=4326;POINT 1 2");
    assert!(result.is_err());
}

#[test]
fn test_geometry_from_ewkt_unbalanced_parens() {
    let result = Geometry::from_ewkt("SRID=4326;POINT(1 2");
    assert!(result.is_err());
}

#[test]
fn test_geometry_from_ewkt_invalid_coord() {
    let result = Geometry::from_ewkt("SRID=4326;POINT(a b)");
    assert!(result.is_err());
}

#[test]
fn test_geometry_from_ewkt_single_coord() {
    let result = Geometry::from_ewkt("SRID=4326;POINT(1)");
    assert!(result.is_err());
}

#[test]
fn test_geometry_from_ewkt_empty_linestring() {
    let result = Geometry::from_ewkt("SRID=4326;LINESTRING()");
    assert!(result.is_err());
}

#[test]
fn test_geometry_from_ewkt_invalid_polygon_coord() {
    let result = Geometry::from_ewkt("SRID=4326;POLYGON((0 0, x y, 0 0))");
    assert!(result.is_err());
}

#[test]
fn test_geometry_to_ewkt_multilinestring_extra() {
    let ls1 = LineString::new(vec![Point::with_srid(0.0, 0.0, 4326), Point::with_srid(1.0, 1.0, 4326)]);
    let g = Geometry::MultiLineString(vec![ls1]);
    let ewkt = g.to_ewkt();
    assert!(ewkt.contains("MULTILINESTRING"));
    assert!(ewkt.contains("SRID=4326"));
}

#[test]
fn test_geometry_to_ewkt_multipolygon_extra() {
    let poly = Polygon::new(vec![
        Point::with_srid(0.0, 0.0, 4326),
        Point::with_srid(1.0, 0.0, 4326),
        Point::with_srid(1.0, 1.0, 4326),
        Point::with_srid(0.0, 0.0, 4326),
    ]);
    let g = Geometry::MultiPolygon(vec![poly]);
    let ewkt = g.to_ewkt();
    assert!(ewkt.contains("MULTIPOLYGON"));
    assert!(ewkt.contains("SRID=4326"));
}

#[test]
fn test_geometry_to_wkt_multilinestring_extra() {
    let ls = LineString::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
    let g = Geometry::MultiLineString(vec![ls]);
    let wkt = g.to_wkt();
    assert!(wkt.contains("MULTILINESTRING"));
}

#[test]
fn test_geometry_to_wkt_multipolygon_extra() {
    let poly = Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 0.0),
    ]);
    let g = Geometry::MultiPolygon(vec![poly]);
    let wkt = g.to_wkt();
    assert!(wkt.contains("MULTIPOLYGON"));
}

#[test]
fn test_geometry_bounding_box_linestring_multi() {
    let ls = LineString::new(vec![
        Point::new(3.0, 4.0),
        Point::new(1.0, 2.0),
        Point::new(5.0, 0.0),
    ]);
    let g = Geometry::LineString(ls);
    let bbox = g.bounding_box().unwrap();
    assert_eq!(bbox.0, 1.0);
    assert_eq!(bbox.1, 0.0);
    assert_eq!(bbox.2, 5.0);
    assert_eq!(bbox.3, 4.0);
}

#[test]
fn test_geometry_bounding_box_multipoint_extra() {
    let pts = vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0), Point::new(0.0, 5.0)];
    let g = Geometry::MultiPoint(pts);
    let bbox = g.bounding_box().unwrap();
    assert_eq!(bbox.0, 0.0);
    assert_eq!(bbox.1, 2.0);
    assert_eq!(bbox.2, 3.0);
    assert_eq!(bbox.3, 5.0);
}
