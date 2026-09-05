#!/usr/bin/env python3
"""v6.2 基准对比报告生成脚本

从 sz-orm / SeaORM / SQLx 的 criterion JSON 输出中提取 P50/P99/P999 + 吞吐量，
生成 Markdown 对比表。所有数字来自 JSON，禁止手写。

用法：
    python scripts/generate-benchmark-report.py --sz-orm target/criterion/ --seaorm target/criterion-seaorm/ --sqlx target/criterion-sqlx/ --output docs/benchmark-report-v6.2.0.md
"""

import argparse
import json
import os
import sys
from pathlib import Path


def extract_metrics(criterion_dir: str) -> dict:
    """从 criterion 目录提取各场景的 P50/P99/P999 + 吞吐量

    返回 {scenario: {p50_ns, p99_ns, p999_ns, throughput_ops_per_s}}
    """
    if not criterion_dir or not os.path.exists(criterion_dir):
        return {}

    results = {}
    criterion_path = Path(criterion_dir)

    for estimates_file in criterion_path.rglob("estimates.json"):
        parts = estimates_file.parts
        try:
            bench_idx = parts.index("criterion") + 1
            bench_name = parts[bench_idx]
            func_name = parts[bench_idx + 1]
        except (ValueError, IndexError):
            continue

        try:
            with open(estimates_file, "r", encoding="utf-8") as f:
                data = json.load(f)
        except (json.JSONDecodeError, OSError):
            continue

        scenario = f"{bench_name}/{func_name}"
        mean_ns = data.get("mean", {}).get("point_estimate", 0)
        throughput = 1e9 / mean_ns if mean_ns > 0 else 0

        results[scenario] = {
            "p50_ns": data.get("median", {}).get("point_estimate", 0),
            "p99_ns": data.get("p99", {}).get("point_estimate", 0),
            "p999_ns": data.get("p999", {}).get("point_estimate", 0),
            "throughput_ops_per_s": throughput,
        }

    return results


def format_ns(ns: float) -> str:
    """格式化纳秒为人类可读"""
    if ns == 0:
        return "N/A"
    if ns < 1000:
        return f"{ns:.0f} ns"
    if ns < 1_000_000:
        return f"{ns / 1000:.1f} μs"
    return f"{ns / 1_000_000:.2f} ms"


def format_throughput(ops: float) -> str:
    """格式化吞吐量"""
    if ops == 0:
        return "N/A"
    if ops >= 1_000_000:
        return f"{ops / 1_000_000:.2f} M ops/s"
    if ops >= 1000:
        return f"{ops / 1000:.1f} K ops/s"
    return f"{ops:.0f} ops/s"


def generate_report(sz_orm: dict, seaorm: dict, sqlx: dict) -> str:
    """生成 Markdown 对比报告"""
    all_scenarios = sorted(set(sz_orm) | set(seaorm) | set(sqlx))

    lines = [
        "# sz-orm v6.2.0 基准对比报告",
        "",
        "> 自动生成，所有数字来自 criterion JSON，禁止手写。",
        "",
        "## 对比表",
        "",
        "| 场景 | sz-orm P50 | sz-orm P99 | sz-orm 吞吐量 | SeaORM P50 | SeaORM P99 | SeaORM 吞吐量 | SQLx P50 | SQLx P99 | SQLx 吞吐量 |",
        "|------|-----------|-----------|-------------|-----------|-----------|-------------|---------|---------|------------|",
    ]

    for scenario in all_scenarios:
        sz = sz_orm.get(scenario, {})
        sea = seaorm.get(scenario, {})
        sql = sqlx.get(scenario, {})

        row = f"| {scenario} "
        row += f"| {format_ns(sz.get('p50_ns', 0))} "
        row += f"| {format_ns(sz.get('p99_ns', 0))} "
        row += f"| {format_throughput(sz.get('throughput_ops_per_s', 0))} "
        row += f"| {format_ns(sea.get('p50_ns', 0))} "
        row += f"| {format_ns(sea.get('p99_ns', 0))} "
        row += f"| {format_throughput(sea.get('throughput_ops_per_s', 0))} "
        row += f"| {format_ns(sql.get('p50_ns', 0))} "
        row += f"| {format_ns(sql.get('p99_ns', 0))} "
        row += f"| {format_throughput(sql.get('throughput_ops_per_s', 0))} |"
        lines.append(row)

    lines.extend([
        "",
        "## 红线指标",
        "",
        "- 池 acquire P99 ≤ 10 μs",
        "- 池复用率 ≥ 90%",
        "- SQL 构建 ≥ 100,000 ops/s",
        "- 流式拉取 ≥ 50,000 elements/s",
        "- 基准回归退化 ≤ 10%",
        "",
    ])

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        description="v6.2 基准对比报告生成"
    )
    parser.add_argument("--sz-orm", required=True, help="sz-orm criterion 目录")
    parser.add_argument("--seaorm", default="", help="SeaORM criterion 目录")
    parser.add_argument("--sqlx", default="", help="SQLx criterion 目录")
    parser.add_argument("--output", required=True, help="输出 Markdown 文件路径")
    args = parser.parse_args()

    sz_orm = extract_metrics(args.sz_orm)
    seaorm = extract_metrics(args.seaorm)
    sqlx = extract_metrics(args.sqlx)

    report = generate_report(sz_orm, seaorm, sqlx)

    with open(args.output, "w", encoding="utf-8") as f:
        f.write(report)

    print(f"报告已生成: {args.output}")


if __name__ == "__main__":
    main()