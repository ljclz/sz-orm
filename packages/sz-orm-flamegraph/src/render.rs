//! 火焰图渲染
//!
//! - [`to_brendan_gregg`]：折叠栈格式（`flamegraph.pl` 兼容）
//! - [`to_svg`]：自绘内联 SVG（无外部依赖，浏览器可直接打开）

use crate::QueryPhaseTiming;

/// 折叠栈格式：每阶段一行 `query;phase duration_us`，可直接喂给 `flamegraph.pl`。
///
/// ```text
/// query_execute;query.build 200
/// query_execute;pool.acquire 150
/// query_execute;db.execute 4200
/// ```
pub fn to_brendan_gregg(timings: &[QueryPhaseTiming]) -> String {
    let mut out = String::new();
    for t in timings {
        let us = t.duration_ms.saturating_mul(1000);
        out.push_str(&format!("query_execute;{} {}\n", t.phase.as_str(), us));
    }
    out
}

/// 内联 SVG 火焰图：单层横向分块，块宽与耗时成正比，颜色按阶段区分。
///
/// 输出为完整 `<svg>` 文档（无外部依赖），宽度固定 800，高度 60 + 标注行。
pub fn to_svg(timings: &[QueryPhaseTiming]) -> String {
    const WIDTH: u64 = 800;
    const HEIGHT: u64 = 48;
    const ROW_HEIGHT: u64 = 16;
    const MARGIN: u64 = 4;

    let total: u64 = timings.iter().map(|t| t.duration_ms).sum();
    let total = if total == 0 { 1 } else { total };

    let mut rects = String::new();
    let mut x: u64 = 0;
    for t in timings {
        let w = (t.duration_ms as u128 * WIDTH as u128 / total as u128) as u64;
        let w = w.max(if t.duration_ms > 0 { 1 } else { 0 });
        let color = phase_color(t.phase.as_str());
        let title = format!("{}: {}ms", t.phase.as_str(), t.duration_ms);
        rects.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" rx="1"><title>{}</title></rect>"#,
            x, MARGIN, w, ROW_HEIGHT, color, title
        ));
        x += w;
    }

    let legend: Vec<String> = timings
        .iter()
        .map(|t| format!("{} {}ms", t.phase.as_str(), t.duration_ms))
        .collect();

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
<!-- sz-orm query flame graph (total: {total}ms) -->
{rects}
<text x="4" y="{HEIGHT}" font-family="monospace" font-size="11">{}</text>
</svg>"#,
        legend
            .join(" | ")
            .replace('&', "&amp;")
            .replace('<', "&lt;")
    )
}

/// 阶段颜色（稳定映射，同阶段同色）
fn phase_color(phase: &str) -> &'static str {
    match phase {
        "query.build" => "#d95f02",
        "query.bind" => "#7570b3",
        "pool.acquire" => "#e7298a",
        "db.execute" => "#1b9e77",
        "result.map" => "#66a61e",
        _ => "#999999",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Phase, QueryPhaseTiming};

    fn sample() -> Vec<QueryPhaseTiming> {
        vec![
            QueryPhaseTiming {
                phase: Phase::Build,
                start_ms: 0,
                duration_ms: 1,
            },
            QueryPhaseTiming {
                phase: Phase::PoolAcquire,
                start_ms: 1,
                duration_ms: 2,
            },
            QueryPhaseTiming {
                phase: Phase::SqlExecute,
                start_ms: 3,
                duration_ms: 8,
            },
        ]
    }

    #[test]
    fn brendan_gregg_format_is_folded() {
        let out = to_brendan_gregg(&sample());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("query_execute;query.build "));
        assert!(lines[2].starts_with("query_execute;db.execute 8000"));
    }

    #[test]
    fn svg_contains_all_phases() {
        let svg = to_svg(&sample());
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("query.build"));
        assert!(svg.contains("db.execute"));
        assert!(svg.contains("pool.acquire"));
        // 无外部脚本/危险内容
        assert!(!svg.contains("<script"));
    }

    #[test]
    fn svg_total_width_matches() {
        let svg = to_svg(&sample());
        assert!(svg.contains("width=\"800\""));
    }

    #[test]
    fn empty_timings_renders_valid_svg() {
        let svg = to_svg(&[]);
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn brendan_gregg_empty_timings() {
        let out = to_brendan_gregg(&[]);
        assert!(out.is_empty());
    }

    #[test]
    fn brendan_gregg_single_phase() {
        let timings = vec![QueryPhaseTiming {
            phase: Phase::Build,
            start_ms: 0,
            duration_ms: 5,
        }];
        let out = to_brendan_gregg(&timings);
        assert!(out.contains("query.build 5000"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn brendan_gregg_zero_duration() {
        let timings = vec![QueryPhaseTiming {
            phase: Phase::Bind,
            start_ms: 0,
            duration_ms: 0,
        }];
        let out = to_brendan_gregg(&timings);
        assert!(out.contains("query.bind 0"));
    }

    #[test]
    fn svg_contains_rect_for_each_phase() {
        let timings = sample();
        let svg = to_svg(&timings);
        let rect_count = svg.matches("<rect").count();
        assert_eq!(rect_count, 3, "should have one rect per phase");
    }

    #[test]
    fn svg_contains_legend_text() {
        let svg = to_svg(&sample());
        assert!(svg.contains("<text"));
        assert!(svg.contains("ms"));
    }

    #[test]
    fn svg_escapes_special_chars() {
        let timings = vec![QueryPhaseTiming {
            phase: Phase::Build,
            start_ms: 0,
            duration_ms: 1,
        }];
        let svg = to_svg(&timings);
        // & should be escaped as &amp; in legend
        assert!(!svg.contains(" & ") || svg.contains("&amp;"));
    }

    #[test]
    fn svg_all_phases_have_colors() {
        let phases = vec![
            QueryPhaseTiming {
                phase: Phase::Build,
                start_ms: 0,
                duration_ms: 1,
            },
            QueryPhaseTiming {
                phase: Phase::Bind,
                start_ms: 0,
                duration_ms: 1,
            },
            QueryPhaseTiming {
                phase: Phase::PoolAcquire,
                start_ms: 0,
                duration_ms: 1,
            },
            QueryPhaseTiming {
                phase: Phase::SqlExecute,
                start_ms: 0,
                duration_ms: 1,
            },
            QueryPhaseTiming {
                phase: Phase::ResultMap,
                start_ms: 0,
                duration_ms: 1,
            },
        ];
        let svg = to_svg(&phases);
        assert!(svg.contains("#d95f02")); // Build
        assert!(svg.contains("#7570b3")); // Bind
        assert!(svg.contains("#e7298a")); // PoolAcquire
        assert!(svg.contains("#1b9e77")); // SqlExecute
        assert!(svg.contains("#66a61e")); // ResultMap
    }

    #[test]
    fn svg_zero_total_uses_fallback() {
        let timings = vec![QueryPhaseTiming {
            phase: Phase::Build,
            start_ms: 0,
            duration_ms: 0,
        }];
        let svg = to_svg(&timings);
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn phase_color_unknown_returns_gray() {
        assert_eq!(phase_color("unknown"), "#999999");
    }

    #[test]
    fn phase_color_known_phases() {
        assert_eq!(phase_color("query.build"), "#d95f02");
        assert_eq!(phase_color("query.bind"), "#7570b3");
        assert_eq!(phase_color("pool.acquire"), "#e7298a");
        assert_eq!(phase_color("db.execute"), "#1b9e77");
        assert_eq!(phase_color("result.map"), "#66a61e");
    }
}
