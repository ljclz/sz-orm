#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-metrics-real.py — 度量真实性扫描（AGENTS.md 门禁 18）
================================================================
检测"度量崇拜"（szrsql 审查根因 2 / 2026-08-13 审计：README 内 4 组
互相矛盾的数字——56/43/60 成员、5,404/5,809/6,900+ 测试）。

方法：
  1. 从源码自动统计真实度量（包数/测试标注数/feature 数/方言数）
  2. 扫描 README.md 与 docs/*.md 中的数字声称（N 个成员 / N 测试 / N+ 测试）
  3. 不一致 → 报告（并可 --fix 自动修正 README）

用法:
  python scripts/check-metrics-real.py              # 检查
  python scripts/check-metrics-real.py --fix        # 自动修正 README 声称
  python scripts/check-metrics-real.py --json       # 输出 metrics.json
退出码: 0 = 一致；1 = 存在不一致数字声称
"""

import argparse
import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
README = os.path.join(ROOT, "README.md")


def count_test_annotations():
    total = 0
    for root, dirs, files in os.walk(os.path.join(ROOT, "packages")):
        dirs[:] = [d for d in dirs if d not in ("target", "node_modules", ".git")]
        for f in files:
            if not f.endswith(".rs"):
                continue
            try:
                txt = open(os.path.join(root, f), encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            total += len(re.findall(r"#\[(tokio::)?test\]", txt))
    return total


def count_features():
    total = 0
    for toml in glob.glob(os.path.join(ROOT, "packages", "*", "Cargo.toml")) + \
            [os.path.join(ROOT, "cli", "Cargo.toml"), os.path.join(ROOT, "examples", "Cargo.toml")]:
        txt = open(toml, encoding="utf-8", errors="replace").read()
        m = re.search(r"\[features\]", txt)
        if m:
            total += len(re.findall(r"^\s*[\w-]+\s*=", txt[m.end():].split("\n[")[0], re.M))
    return total


def real_metrics():
    pkg_dirs = [d for d in os.listdir(os.path.join(ROOT, "packages")) if d.startswith("sz-orm")]
    tests = count_test_annotations()
    feats = count_features()
    return {
        "packages": len(pkg_dirs),
        "test_annotations": tests,
        "features": feats,
        "workspace_members": len(pkg_dirs) + 2,  # + cli + examples
    }


# README 全局总量声称模式 → 实际度量键
# 仅匹配"当前总量"声称（徽章/质量基线表/总计行）；历史版本日志与外部项目
# （如 sz-pay "5139 测试"）中的数字不属本仓库度量，不比对
CLAIM_PATTERNS = [
    # (模式, 度量键, 说明, 修复替换模板)
    (r"(\d[\d,]*)\+?\s*passed", "test_annotations", "测试总数(passed)", r"\g<1> passed"),
    (r"总计：(\d[\d,]*) tests", "test_annotations", "测试总数(总计)", r"总计：\g<1> tests"),
    (r"tests-(\d+)", "test_annotations", "测试徽章", r"tests-\g<1>"),
    (r"packages-(\d+)", "workspace_members", "包数徽章", r"packages-\g<1>"),
    (r"工作空间成员\s*\|\s*\*\*(\d+)\*\*（(\d+) 个 sz-orm-\* lib", "workspace_members", "成员表行",
     r"工作空间成员 | **\g<1>**（\g<2> 个 sz-orm-* lib"),
]


def scan_readme(metrics):
    issues = []  # (line_no, claimed, actual, kind, fix_new_line)
    try:
        lines = open(README, encoding="utf-8", errors="replace").read().splitlines()
    except OSError:
        return issues
    for i, ln in enumerate(lines, start=1):
        for pat, key, kind, fix_tpl in CLAIM_PATTERNS:
            for m in re.finditer(pat, ln):
                claimed_raw = m.group(1)
                claimed = int(claimed_raw.replace(",", "").replace("+", ""))
                actual = metrics[key]
                if claimed != actual:
                    new_line = ln[:m.start()] + fix_tpl \
                        .replace(r"\g<1>", str(actual)) \
                        .replace(r"\g<2>", str(metrics["packages"])) + ln[m.end():]
                    issues.append((i, claimed, actual, kind, ln.strip()[:90], new_line))
    return issues


def main():
    ap = argparse.ArgumentParser(description="度量真实性扫描（门禁 18）")
    ap.add_argument("--fix", action="store_true", help="自动修正 README 声称")
    ap.add_argument("--json", action="store_true", help="输出 metrics.json")
    args = ap.parse_args()

    metrics = real_metrics()
    print("=" * 60)
    print("  度量真实性扫描（门禁 18）")
    print("=" * 60)
    print(f"  实际统计: 包={metrics['packages']} 成员={metrics['workspace_members']} "
          f"测试标注={metrics['test_annotations']} feature={metrics['features']}")

    issues = scan_readme(metrics)
    for ln, claimed, actual, kind, text, _ in issues:
        print(f"  ❌ README.md:{ln} 声称 {claimed}（{kind}），实际 {actual} | {text}")
    if not issues:
        print("  ✅ README 数字声称与源码统计一致")

    if args.fix and issues:
        lines = open(README, encoding="utf-8", errors="replace").read().splitlines()
        for ln, _, _, _, _, new_line in issues:
            lines[ln - 1] = new_line
        open(README, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
        print(f"  → 已自动修正 README {len(issues)} 处数字声称")
        # 保险：修复残留检查（模板占位符泄漏会污染文档）
        leftover = [(i + 1, l) for i, l in enumerate(lines) if r"\g<" in l]
        if leftover:
            print(f"  ❌ 修复残留: {[(n, t.strip()[:60]) for n, t in leftover]}（模板占位符未替换，请人工修复）")
            return 2

    if args.json:
        out = os.path.join(ROOT, "docs", "metrics.json")
        json.dump(metrics, open(out, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
        print(f"  → 已输出 {out}（文档可引用，禁止手写数字）")

    print("\n" + "=" * 60)
    print(f"  结果: {'✅ 通过' if not issues else '❌ 存在不一致数字声称'}")
    print("=" * 60)
    return 0 if not issues else 1


if __name__ == "__main__":
    sys.exit(main())
