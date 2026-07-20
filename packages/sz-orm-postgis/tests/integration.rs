//! 集成测试：验证 PostgisExt 的端到端行为

use sz_orm_postgis::{
    Geometry, LineString, Point, Polygon, PostgisBuilder, PostgisExt, PostgisProvider, DEFAULT_SRID,
};

/// 构造北京-上海-广州-深圳四城市的几何体
fn china_cities() -> [Point; 4] {
    [
        Point::new(116.404, 39.915), // 北京
        Point::new(121.474, 31.230), // 上海
        Point::new(113.264, 23.129), // 广州
        Point::new(114.057, 22.543), // 深圳
    ]
}

#[tokio::test]
async fn integration_memory_distance_matrix() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
        .build()
        .expect("build failed");
    let cities = china_cities();

    let pairs = [(0, 1), (0, 2), (1, 3), (2, 3)];
    for (i, j) in pairs {
        let p1 = Geometry::Point(cities[i]);
        let p2 = Geometry::Point(cities[j]);
        let dist = wrapper
            .st_distance(&p1, &p2)
            .await
            .unwrap_or_else(|e| panic!("distance {}-{} failed: {}", i, j, e));
        // 城市间距离应在 100km - 2000km 之间
        assert!(
            dist > 100_000.0 && dist < 2_000_000.0,
            "distance {}-{}: {} m out of expected range",
            i,
            j,
            dist
        );
    }
}

#[tokio::test]
async fn integration_memory_polygon_contains() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
        .build()
        .expect("build failed");

    // 构造一个矩形多边形覆盖北京周边
    let poly = Geometry::Polygon(Polygon::new(vec![
        Point::new(115.0, 39.0),
        Point::new(118.0, 39.0),
        Point::new(118.0, 41.0),
        Point::new(115.0, 41.0),
    ]));

    // 北京在矩形内
    let beijing = Geometry::Point(Point::new(116.404, 39.915));
    assert!(wrapper.st_contains(&poly, &beijing).await.unwrap());

    // 上海不在矩形内
    let shanghai = Geometry::Point(Point::new(121.474, 31.230));
    assert!(!wrapper.st_contains(&poly, &shanghai).await.unwrap());

    // st_within 是 st_contains 的反向
    assert!(wrapper.st_within(&beijing, &poly).await.unwrap());
    assert!(!wrapper.st_within(&shanghai, &poly).await.unwrap());
}

#[tokio::test]
async fn integration_memory_linestring_length() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
        .build()
        .expect("build failed");

    // 北京 -> 上海 -> 广州 的折线
    let ls = Geometry::LineString(LineString::new(vec![
        Point::new(116.404, 39.915),
        Point::new(121.474, 31.230),
        Point::new(113.264, 23.129),
    ]));
    let length = wrapper.st_length(&ls).await.unwrap();
    // 两段合计应 > 1500 km
    assert!(length > 1_500_000.0, "length {} m too short", length);
}

#[tokio::test]
async fn integration_memory_buffer_around_point() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
        .build()
        .expect("build failed");

    let beijing = Geometry::Point(Point::new(116.404, 39.915));
    let buffer = wrapper
        .st_buffer(&beijing, 5_000.0) // 5km 缓冲区
        .await
        .expect("buffer failed");

    match buffer {
        Geometry::Polygon(poly) => {
            // 32 边 + 闭合点 = 33 个点
            assert_eq!(poly.rings[0].len(), 33);
            // 缓冲区应包含原点
            assert!(poly.contains_point(&Point::new(116.404, 39.915)));
        }
        _ => panic!("expected Polygon"),
    }
}

#[tokio::test]
async fn integration_stub_sql_generation() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Stub)
        .build()
        .expect("build failed");

    let p1 = Geometry::Point(Point::new(116.404, 39.915));
    let p2 = Geometry::Point(Point::new(121.474, 31.230));

    // 触发多个操作
    let _ = wrapper.st_distance(&p1, &p2).await;
    let _ = wrapper.st_intersects(&p1, &p2).await;
    wrapper
        .add_geometry_column("cities", "geom", DEFAULT_SRID, "POINT", "2")
        .await
        .unwrap();
    wrapper
        .create_spatial_index("cities", "geom")
        .await
        .unwrap();

    // 验证 stub 记录的 SQL
    match &wrapper {
        sz_orm_postgis::PostgisWrapper::Stub(stub) => {
            let history = stub.sql_history();
            assert_eq!(history.len(), 4);
            assert!(history[0].contains("ST_Distance"));
            assert!(history[1].contains("ST_Intersects"));
            assert!(history[2].contains("AddGeometryColumn"));
            assert!(history[3].contains("CREATE INDEX"));
            assert!(history[3].contains("USING GIST"));
        }
        _ => panic!("expected Stub variant"),
    }
}

#[tokio::test]
async fn integration_memory_srid_mismatch_error() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
        .build()
        .expect("build failed");

    let p1 = Geometry::Point(Point::with_srid(0.0, 0.0, 4326));
    let p2 = Geometry::Point(Point::with_srid(1.0, 1.0, 3857));

    let result = wrapper.st_distance(&p1, &p2).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(
        err,
        sz_orm_postgis::PostgisError::SridMismatch {
            expected: 4326,
            actual: 3857
        }
    ));
}

#[tokio::test]
async fn integration_memory_polygon_with_hole() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
        .build()
        .expect("build failed");

    // 外环：0,0 - 10,10
    let outer = vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ];
    // 洞：3,3 - 7,7
    let hole = vec![
        Point::new(3.0, 3.0),
        Point::new(7.0, 3.0),
        Point::new(7.0, 7.0),
        Point::new(3.0, 7.0),
    ];
    let poly = Geometry::Polygon(Polygon::with_holes(outer, vec![hole]));

    // 在外环内、洞外
    let p_in_outer = Geometry::Point(Point::new(1.0, 1.0));
    assert!(wrapper.st_contains(&poly, &p_in_outer).await.unwrap());

    // 在洞内
    let p_in_hole = Geometry::Point(Point::new(5.0, 5.0));
    assert!(!wrapper.st_contains(&poly, &p_in_hole).await.unwrap());
}

#[tokio::test]
async fn integration_memory_area_polygon() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
        .build()
        .expect("build failed");

    let poly = Geometry::Polygon(Polygon::new(vec![
        Point::new(0.0, 0.0),
        Point::new(6.0, 0.0),
        Point::new(6.0, 8.0),
        Point::new(0.0, 8.0),
    ]));
    let area = wrapper.st_area(&poly).await.unwrap();
    assert!((area - 48.0).abs() < 1e-10);
}

#[tokio::test]
async fn integration_memory_union_points() {
    let wrapper = PostgisBuilder::new(PostgisProvider::Memory)
        .build()
        .expect("build failed");

    let p1 = Geometry::Point(Point::new(0.0, 0.0));
    let p2 = Geometry::Point(Point::new(1.0, 1.0));
    let union = wrapper.st_union(&p1, &p2).await.unwrap();

    match union {
        Geometry::MultiPoint(pts) => {
            assert_eq!(pts.len(), 2);
            assert_eq!(pts[0], Point::new(0.0, 0.0));
            assert_eq!(pts[1], Point::new(1.0, 1.0));
        }
        _ => panic!("expected MultiPoint"),
    }
}
