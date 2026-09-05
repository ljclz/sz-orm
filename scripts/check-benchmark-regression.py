#!/usr/bin/env python3
"""v6.2 基准回归检测脚本

比较基线 JSON 与当前 criterion 输出，检测性能退化。
退化率 = (baseline_t - current_t) / baseline_t，超过阈值计为退化。

用法：
    python scripts/check-benchmark-regression.py --baseline docs/benchmark-baseline-v6.2.0.json --current target/criterion/ --threshold 0.10

退出码：
    0 = 无退化
    1 = 检测到退化
    2 = 基线/criterion 文件缺失或解析错误
"""

import argparse
import json
import os
import sys
from pathlib import Path


def load_baseline(baseline_path: str) -> dict:
    """加载基线 JSON 文件"""
    if not os.path.exists(baseline_path):
        print(f"错误：基线文件不存在: {baseline_path}", file=sys.stderr)
        sys.exit(2)
    try:
        with open(baseline_path, "r", encoding="utf-8") as f:
            return json.load(f)
    except json.JSONDecodeError as e:
        print(f"错误：基线 JSON 解析失败: {e}", file=sys.stderr)
        sys.exit(2)


def load_criterion_estimates(criterion_dir: str) -> dict:
    """遍历 criterion 输出目录，提取各场景的估计值

    criterion 在 <dir>/<bench_name>/<func_name>/new/estimates.json 生成统计文件
    返回 {scenario_name: {mean_ns, p50_ns, p99_ns, p999_ns}}
    """
    criterion_path = Path(criterion_dir)
    if not criterion_path.exists():
        print(f"错误：criterion 目录不存在: {criterion_dir}", file=sys.stderr)
        sys.exit(2)

    results = {}
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
        results[scenario] = {
            "mean_ns": data.get("mean", {}).get("point_estimate", 0),
            "p50_ns": data.get("median", {}).get("point_estimate", 0),
            "p99_ns": data.get("p99", {}).get("point_estimate", 0),
            "p999_ns": data.get("p999", {}).get("point_estimate", 0),
        }

    if not results:
        print(f"错误：未在 {criterion_dir} 中找到 estimates.json", file=sys.stderr)
        sys.exit(2)

    return results


def check_regression(baseline: dict, current: dict, threshold: float) -> list:
    """逐场景比较，返回退化项列表"""
    regressions = []
    baseline_scenarios = {s["name"]: s for s in baseline.get("scenarios", [])}

    for scenario, current_vals in current.items():
        if scenario not in baseline_scenarios:
            print(f"警告：基线中无场景 {scenario}，跳过", file=sys.stderr)
            continue

        base_scenario = baseline_scenarios[scenario]
        base_mean = base_scenario.get("mean_ns", 0)
        current_mean = current_vals["mean_ns"]

        if base_mean == 0:
            print(f"警告：场景 {scenario} 基线 mean=0，跳过", file=sys.stderr)
            continue

        if current_mean < base_mean:
            continue

        regression_rate = (current_mean - base_mean) / base_mean
        if regression_rate > threshold:
            regressions.append({
                "scenario": scenario,
                "baseline_ns": base_mean,
                "current_ns": current_mean,
                "regression_rate": regression_rate,
            })

    return regressions


def main():
    parser = argparse.ArgumentParser(
        description="v6.2 基准回归检测：比较基线与当前 criterion 输出"
    )
    parser.add_argument("--baseline", required=True, help="基线 JSON 文件路径")
    parser.add_argument("--current", required=True, help="criterion 输出目录")
    parser.add_argument("--threshold", type=float, default=0.10,
                        help="退化阈值（默认 0.10 = 10%%）")
    args = parser.parse_args()

    baseline = load_baseline(args.baseline)
    current = load_criterion_estimates(args.current)
    regressions = check_regression(baseline, current, args.threshold)

    if regressions:
        print(f"\n检测到 {len(regressions)} 项退化（阈值 {args.threshold:.0%}）：\n")
        print(f"{'场景':<50} {'基线(ns)':>12} {'当前(ns)':>12} {'退化率':>10}")
        print("-" * 86)
        for r in regressions:
            print(f"{r['scenario']:<50} {r['baseline_ns']:>12.0f} {r['current_ns']:>12.0f} {r['regression_rate']:>9.1%}")
        sys.exit(1)
    else:
        print(f"无退化（阈值 {args.threshold:.0%}）")
        sys.exit(0)


if __name__ == "__main__":
    main()