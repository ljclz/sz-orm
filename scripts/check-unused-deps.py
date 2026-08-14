#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-unused-deps.py — 未用依赖扫描（AGENTS.md 门禁 23）
================================================================
检测"死依赖"（声明了但代码未引用，szrsql 审查：pub API 冰山下的
隐性死代码之一）。基于 cargo-machete。

feature 门控依赖在默认 features 下会被误报——已登记的误报见各包
`[package.metadata.cargo-machete] ignored`（2026-08-14 已登记：
sz-orm-core 的 sz-orm-health/sz-orm-limit/quote）。

用法:
  python scripts/check-unused-deps.py                # 警告级（默认 exit 0）
  python scripts/check-unused-deps.py --strict       # 有命中即失败
退出码: 0 = 通过（警告级）或无命中；1 = --strict 且有命中
"""

import argparse
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


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
    ap = argparse.ArgumentParser(description="未用依赖扫描（门禁 23）")
    ap.add_argument("--strict", action="store_true", help="有命中即失败")
    args = ap.parse_args()

    print("=" * 60)
    print("  未用依赖扫描（门禁 23，基于 cargo-machete）")
    print("=" * 60)

    proc = subprocess.run(
        [find_cargo_bin("cargo"), "machete"],
        cwd=ROOT, capture_output=True, text=True, timeout=600,
    )
    out = proc.stdout
    # 解析：package -- path: \n\t dep \n...
    hits = []
    current = None
    for line in out.splitlines():
        if "-- " in line and ".toml:" in line:
            current = line.split(" -- ")[0].strip()
        elif line.startswith("\t") and line.strip():
            if current:
                hits.append((current, line.strip()))

    if not hits:
        print("  ✅ 无未用依赖")
        print("\n" + "=" * 60)
        print("  结果: ✅ 通过")
        return 0

    print(f"  未用依赖 {len(hits)} 个（警告级；确认无用的请从 Cargo.toml 移除，"
          f"feature 门控误报请登记 ignored）:\n")
    for pkg, dep in hits:
        print(f"    {pkg:28s} {dep}")
    print("\n" + "=" * 60)
    if args.strict:
        print(f"❌ 门禁 23 未通过（--strict）— {len(hits)} 个未用依赖")
        return 1
    print(f"  ⚠️  警告级：{len(hits)} 个未用依赖（--strict 升级为失败）")
    print("  结果: ✅ 通过（警告）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
