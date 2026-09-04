#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-mutation-coverage.py — 变异测试杀率门禁（AGENTS.md 门禁 20）
================================================================
解决"测试自证闭环"（szrsql 审查：测试是 AI 写给自己看的验收标准）——
变异测试验证测试是否真的能抓住实现错误（杀率 = killed / total）。

用法:
  python scripts/check-mutation-coverage.py                      # 默认子集（内置关键模块）
  python scripts/check-mutation-coverage.py --file packages/sz-orm-core/src/pool.rs
  python scripts/check-mutation-coverage.py --package sz-orm-core --features multi-tenant-enhanced
  python scripts/check-mutation-coverage.py --threshold 0.7      # 自定义阈值（默认 0.7）

实现：
  1. 调用 cargo-mutants（--output-format json）对目标模块生成变异体并跑测试
  2. 解析 caught/missed/timeout，计算杀率
  3. 杀率 < 阈值 → 失败（测试质量不足，需补测试）

退出码: 0 = 杀率达标；1 = 杀率不足或运行失败
"""

import argparse
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# 内置关键模块子集（变更高频/语义敏感，优先保障测试质量）
DEFAULT_FILES = [
    "packages/sz-orm-core/src/tenant_quota_rls.rs",   # 配额（P0 教训：release 只增不减）
    "packages/sz-orm-core/src/cache_warmup_protection.rs",  # 布隆/单飞（不漏判语义）
]
# 默认子集对应 feature（2026-08-15 修复：默认调用此前不带 features，
# feature 门控模块不编译导致 cargo-mutants 必然失败——见验证报告发现 2）
DEFAULT_FEATURES = "tenant-quota-rls-enhanced,auto-prewarm"


def find_cargo():
    import shutil
    exe = shutil.which("cargo")
    if exe:
        return exe
    home = os.path.expanduser("~")
    for cand in (os.path.join(home, ".cargo", "bin", "cargo"),
                 os.path.join(home, ".cargo", "bin", "cargo.exe")):
        if os.path.isfile(cand):
            return cand
    return "cargo"


def run_mutants(pkg, files, features):
    """运行 cargo-mutants（输出到 mutants.out），返回 (caught, missed, timeout)。"""
    out_dir = os.path.join(ROOT, "mutants.out")
    # 测试串行（--test-threads=1）排除高负载下的计时敏感 flake，
    # 否则 baseline 随机挂 1~5 个测试导致整轮失败（2026-09-03 审查实测）。
    # 注意：--features/--file 是 mutants 自身参数必须在第一个 -- 之前；
    # 第一个 -- 后的参数转交给内部 cargo test，第二个 -- 后的才是 test binary 参数
    cmd = [
        find_cargo(), "mutants", "-p", pkg, "-o", out_dir, "--in-place",
    ]
    if features:
        cmd += ["--features", features]
    for f in files:
        cmd += ["--file", f]
    cmd += ["--", "--", "--test-threads=1"]
    print("  $ " + " ".join(cmd))
    proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=10800)
    # cargo-mutants 成功退出码为 0；变异体导致的测试失败不影响退出码
    def count(name):
        # cargo-mutants 会在 -o 指定目录下再建一层 mutants.out 子目录
        for cand in (os.path.join(out_dir, name),
                     os.path.join(out_dir, "mutants.out", name)):
            if os.path.isfile(cand):
                with open(cand, encoding="utf-8", errors="replace") as fh:
                    return sum(1 for ln in fh if ln.strip())
        return 0
    caught = count("caught.txt")
    missed = count("missed.txt")
    timeout = count("timeout.txt")
    if caught + missed + timeout == 0 and proc.returncode != 0:
        print("  ❌ cargo-mutants 运行失败")
        print("  " + (proc.stderr[-1500:] or proc.stdout[-1500:]))
        return None
    return caught, missed, timeout


def main():
    ap = argparse.ArgumentParser(description="变异测试杀率门禁（门禁 20）")
    ap.add_argument("--package", default="sz-orm-core", help="目标包")
    ap.add_argument("--file", action="append", default=None, help="目标文件（可多次），默认内置子集")
    ap.add_argument("--features", default="", help="cargo features（如 multi-tenant-enhanced）")
    ap.add_argument("--threshold", type=float, default=0.7, help="杀率阈值（默认 0.7）")
    args = ap.parse_args()
    # 默认子集全部位于 feature 门控内：未显式指定 features 时自动使用 DEFAULT_FEATURES
    # （2026-08-15 修复：此前默认调用不带 features，cargo-mutants 必然失败——见验证报告发现 2）
    args.features = args.features or DEFAULT_FEATURES

    files = args.file or DEFAULT_FILES
    print("=" * 60)
    print("  变异测试杀率门禁（门禁 20）")
    print("=" * 60)
    print(f"  目标: {args.package} {files}")

    result = run_mutants(args.package, files, args.features)
    if result is None:
        print("❌ 门禁 20 未通过 — 变异测试运行失败")
        return 1
    caught, missed, timeout = result
    total = caught + missed + timeout
    if total == 0:
        # fail-closed（2026-08-15 修复）：目标模块零变异体 = 未被编译覆盖或无有效测试，
        # 与"杀率不足"同属测试质量缺陷，按失败处理，不能静默通过
        print("❌ 无变异体生成（目标模块未被编译覆盖或无有效测试——请检查 --features / 目标文件）")
        return 1
    killed = caught
    rate = killed / total
    print(f"\n  caught(被杀) {killed} | missed(存活) {missed} | timeout {timeout} | 总数 {total}")
    print(f"  杀率: {rate:.1%}（阈值 {args.threshold:.0%}）")
    if missed:
        print("\n  存活的变异体（测试未覆盖的实现行为）— 见 mutants.out/missed.txt:")
        missed_path = os.path.join(ROOT, "mutants.out", "missed.txt")
        if os.path.isfile(missed_path):
            with open(missed_path, encoding="utf-8", errors="replace") as fh:
                for ln in list(fh)[:10]:
                    print(f"    - {ln.strip()[:100]}")
            if missed > 10:
                print(f"    ... 其余 {missed - 10} 个（见 mutants.out/missed.txt）")
    print("\n" + "=" * 60)
    if rate < args.threshold:
        print(f"❌ 门禁 20 未通过 — 杀率 {rate:.1%} < {args.threshold:.0%}（测试质量不足，请补测试）")
        return 1
    print(f"✅ 门禁 20 通过 — 杀率 {rate:.1%} ≥ {args.threshold:.0%}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
