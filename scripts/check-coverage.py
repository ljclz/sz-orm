#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-coverage.py — 覆盖率门禁（AGENTS.md 门禁 22）
================================================================
解决"测试数量 ≠ 测试质量"（szrsql 审查根因 2/阶段 3）：
8,927 个 #[test] 标注无覆盖率度量——本脚本用 cargo-llvm-cov 统计
关键模块的行覆盖率，低于阈值即失败。

用法:
  python scripts/check-coverage.py                        # 默认关键模块子集
  python scripts/check-coverage.py --package sz-orm-core  # 指定包
  python scripts/check-coverage.py --threshold 0.6        # 自定义阈值（默认 60%）
  python scripts/check-coverage.py --features "tenant-quota-rls-enhanced"

退出码: 0 = 覆盖率达标；1 = 低于阈值或运行失败
"""

import argparse
import json
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# 内置关键模块子集（语义敏感/变更高频，覆盖率优先保障）
DEFAULT_MODULES = [
    "packages/sz-orm-core/src/tenant_quota_rls.rs",
    "packages/sz-orm-core/src/cache_warmup_protection.rs",
    "packages/sz-orm-core/src/bloom.rs",
    "packages/sz-orm-core/src/process_l1_cache.rs",
]
# 默认子集对应 feature（2026-08-15 修复：默认调用此前不带 features，feature 门控模块
# 不编译导致"无覆盖率数据"静默通过——见验证报告发现 2b）
DEFAULT_FEATURES = "tenant-quota-rls-enhanced,cache-warmup-protection,process-l1-cache,multi-tenant-enhanced"


def find_cargo_bin(name):
    exe = shutil.which(name)
    if exe:
        return exe
    home = os.path.expanduser("~")
    for cand in (os.path.join(home, ".cargo", "bin", name),
                 os.path.join(home, ".cargo", "bin", name + ".exe")):
        if os.path.isfile(cand):
            return cand
    return name


def main():
    ap = argparse.ArgumentParser(description="覆盖率门禁（门禁 22）")
    ap.add_argument("--package", default="sz-orm-core", help="目标包")
    ap.add_argument("--modules", action="append", default=None, help="目标文件（可多次），默认内置子集")
    ap.add_argument("--features", default="", help="cargo features（默认自动使用 DEFAULT_FEATURES）")
    ap.add_argument("--threshold", type=float, default=0.6, help="行覆盖率阈值（默认 60%）")
    args = ap.parse_args()
    # 默认子集全部位于 feature 门控内：未显式指定 features 时自动使用 DEFAULT_FEATURES
    # （2026-08-15 修复：此前默认调用不带 features，模块不编译 → "无数据"静默通过）
    args.features = args.features or DEFAULT_FEATURES

    modules = args.modules or DEFAULT_MODULES
    print("=" * 60)
    print("  覆盖率门禁（门禁 22）")
    print("=" * 60)
    print(f"  目标: {args.package} {modules}")

    llvm_cov = find_cargo_bin("cargo-llvm-cov")
    cmd = [llvm_cov, "llvm-cov", "--package", args.package,
           "--json", "--quiet"]
    if args.features:
        cmd += ["--features", args.features]
    print("  $ " + " ".join(cmd))
    proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=1800)
    if proc.returncode != 0:
        print("  ❌ cargo-llvm-cov 运行失败")
        print("  " + (proc.stderr[-1500:] or proc.stdout[-1500:]))
        return 1
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError:
        print("  ❌ 无法解析覆盖率 JSON")
        return 1

    # cargo-llvm-cov 结构：data[0].files（v0.6+ 嵌套在 data 数组）
    files = []
    for entry in data.get("data", [data]):
        files.extend(entry.get("files", []))

    # 提取目标模块的行覆盖率
    total_covered = 0
    total_regions = 0
    missing = []
    for f in files:
        path = f.get("filename", "").replace("\\", "/")
        if not any(path.endswith(m.split("/")[-1]) for m in modules):
            continue
        summary = f.get("summary", {}).get("lines", {})
        # llvm-cov: covered=被覆盖行数, uncovered=未覆盖行数, count=执行计数总和（非行数）
        covered = summary.get("covered", 0)
        regions = covered + summary.get("uncovered", 0)
        total_covered += covered
        total_regions += regions
        if regions > 0:
            pct = covered / regions
            missing.append((path.split("/")[-1], pct, covered, regions))

    if total_regions == 0:
        # fail-closed（2026-08-15 修复）：目标模块无覆盖率数据 = 未编译或路径不匹配，
        # 属门禁失效而非通过——按失败处理并引导检查 features
        print("  ❌ 目标模块无覆盖率数据（模块未编译或路径不匹配——请检查 --features / --modules）")
        return 1

    rate = total_covered / total_regions
    print("\n  模块行覆盖率:")
    for name, pct, cov, reg in sorted(missing):
        flag = "✅" if pct >= args.threshold else "❌"
        print(f"    {flag} {name:40s} {pct:.1%} ({cov}/{reg})")
    print(f"\n  合计: {rate:.1%}（阈值 {args.threshold:.0%}）")
    print("\n" + "=" * 60)
    try:
        if rate < args.threshold:
            print(f"❌ 门禁 22 未通过 — 覆盖率 {rate:.1%} < {args.threshold:.0%}（请补测试）")
            return 1
        print(f"✅ 门禁 22 通过 — 覆盖率 {rate:.1%} ≥ {args.threshold:.0%}")
        return 0
    finally:
        # 清理 cargo-llvm-cov 产物（~14G），避免反复占满磁盘（2026-08-19）
        cov_dir = os.path.join(ROOT, "target", "llvm-cov-target")
        if os.path.isdir(cov_dir):
            shutil.rmtree(cov_dir, ignore_errors=True)
            print("  🧹 已清理 target/llvm-cov-target/")


if __name__ == "__main__":
    sys.exit(main())
