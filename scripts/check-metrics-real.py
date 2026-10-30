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

# Windows 默认 GBK 控制台下 emoji 输出会抛 UnicodeEncodeError；统一 UTF-8。
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8")
    except (AttributeError, ValueError):
        pass

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def _safe_write(path: str, text: str) -> None:
    """将写入路径限定在仓库内（防路径穿越）。"""
    p = os.path.abspath(path)
    root = os.path.abspath(ROOT)
    if not (p == root or p.startswith(root + os.sep)):
        raise ValueError(f"写入路径超出仓库范围: {path}")
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
# 2026-10-25 补盲区：双语 README 均纳入度量扫描（此前只扫英文版，
# 导致 README.zh.md 停留在 v9.4.0 而门禁不报）
README_FILES = [
    os.path.join(ROOT, "README.md"),
    os.path.join(ROOT, "README.zh.md"),
]


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


def count_dialects():
    r"""DbType 枚举变体数（方言支持的真实口径，README 顶部徽章用）。

    2026-10-25 修复：原正则 `^\s+[A-Z][A-Za-z]+,` 只统计带逗号的变体，
    漏掉枚举最后一个不带逗号的变体（Firebird），导致 28 方言被数成 27。
    现限制在枚举体内统计，允许末尾变体不带逗号。
    """
    path = os.path.join(ROOT, "packages", "sz-orm-core", "src", "db_type.rs")
    try:
        txt = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return 0
    m = re.search(r"pub enum DbType \{(.*?)\n\}", txt, re.S)
    if not m:
        return 0
    return len(re.findall(r"^\s+[A-Z][A-Za-z]+,?", m.group(1), re.M))


def read_workspace_version():
    """读取 [workspace.package] 的 version（README 当前版本声称的唯一权威来源）。

    2026-10-27 SDET 评测 ROI-2 整改：此前 metrics-real 只校验数字度量，
    "README 写 v10.12.0 而 Cargo.toml 是 10.13.0" 的版本漂移无法拦截。
    """
    path = os.path.join(ROOT, "Cargo.toml")
    try:
        txt = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return ""
    m = re.search(r"\[workspace\.package\](.*?)(?=\n\[|\Z)", txt, re.S)
    section = m.group(1) if m else txt
    vm = re.search(r'^version\s*=\s*"([^"]+)"', section, re.M)
    return vm.group(1) if vm else ""


def real_metrics():
    pkg_dirs = [d for d in os.listdir(os.path.join(ROOT, "packages")) if d.startswith("sz-orm")]
    tests = count_test_annotations()
    feats = count_features()
    return {
        "packages": len(pkg_dirs),
        "test_annotations": tests,
        "features": feats,
        "dialects": count_dialects(),
        "workspace_members": len(pkg_dirs) + 2,  # + cli + examples
        "version": read_workspace_version(),
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
    # 2026-08-14 补盲区：README 顶部概览行散落数字（"N 工作空间成员"/"N+ 测试"/"N SQL 方言"）
    (r"(\d+)\s*工作空间成员", "workspace_members", "顶部成员数", r"\g<1> 工作空间成员"),
    (r"(\d[\d,]*)\+\s*测试", "test_annotations", "顶部测试数", r"\g<1>+ 测试"),
    (r"(\d+)\s+SQL 方言", "dialects", "顶部方言数", r"\g<1> SQL 方言"),
    # 2026-10-25 补盲区：英文顶部概览行与英文 Total 行（README.md 顶部
    # "21133+ tests" 与 Tests 章节 "Total: 9905 tests" 此前不在检查范围）
    (r"(\d[\d,]*)\+\s*tests", "test_annotations", "顶部测试数(英文)", r"\g<1>+ tests"),
    (r"Total:\s*(\d[\d,]*) tests", "test_annotations", "测试总数(英文 Total)", r"Total: \g<1> tests"),
    # 2026-10-25 补盲区：feature 总数当前总量声称（仅 Overview 的
    # "N feature gates for modular composition"；历史版本日志如
    # "(9 feature gates, off by default)" 不含该后缀，不会误报）
    (r"(\d[\d,]*)\s*feature gates for modular composition", "features",
     "feature 总数(Overview)", r"\g<1> feature gates for modular composition"),
    # 2026-10-29 补盲区：中文 Overview 同一句 "N 个 feature gate 用于模块化组合"。
    # 此前只匹配英文措辞，导致英文修到 509 而中文仍写 508 却能通过门禁；
    # 历史版本日志的 "（9 个 feature gate，默认关闭）" 不含该后缀，不会误报。
    (r"(\d[\d,]*)\s*个 feature gate 用于模块化组合", "features",
     "feature 总数(Overview, 中文)", r"\g<1> 个 feature gate 用于模块化组合"),
    # 2026-10-27 SDET 评测 ROI-2 补盲区：README "当前版本"声称与
    # workspace.package.version 比对。仅匹配当前声称的四处锚点——顶部概览行
    # （"> vX.Y.Z ·"）、版本徽章（version-X.Y.Z）、Overview 行（"vX.Y.Z includes"）
    # 与英文 Note 行（"latest features (vX.Y.Z)"）；历史版本日志
    # （"### v5.1.0 ..."）不含这些锚点，不会误报。
    (r"^>\s*v(\d+\.\d+\.\d+)\s*·", "version", "顶部版本行", r"> v\g<1> ·"),
    (r"version-(\d+\.\d+\.\d+)", "version", "版本徽章", r"version-\g<1>"),
    (r"v(\d+\.\d+\.\d+)\s+includes", "version", "Overview 版本行", r"v\g<1> includes"),
    (r"latest features \(v(\d+\.\d+\.\d+)\)", "version", "Note 最新版本", r"latest features (v\g<1>)"),
]

# ---------------------------------------------------------------------------
# 2026-10-27 性能评测（docs/assessment/2026-10-27-perf-review.md §10 / ROI-6）
# 补盲区：性能数字声称（"8/8 PASS"、"比 sqlx 快 X%"、"吞吐衰减 X%"）此前
# 完全不在校验范围——check-metrics-real 只查包数/测试数/feature 数/方言数/版本。
# 性能声称必须附带证据标记（复现命令 / 报告链接 / 采集日期 / 实测标注），
# 否则报告（本检查为报告性检查，不做 --fix 自动修正——证据无法自动生成）。
# ---------------------------------------------------------------------------
PERF_CLAIM_PATTERNS = [
    # "8/8 PASS" / "8 / 8 通过" 式基准通过声称
    r"\b\d+\s*/\s*\d+\s*(?:PASS|通过)\b",
    # 相对竞品/基线的提升/落后百分比声称（中英）
    r"(?:提升|快于|慢于|改善|退化)[^%\n]{0,40}?\d+(?:\.\d+)?\s*%",
    r"\d+(?:\.\d+)?\s*%\s*(?:提升|改善|退化|faster|slower)",
    r"\b(?:faster|slower|improvement|regression)\b[^%\n]{0,40}?\d+(?:\.\d+)?\s*%",
    # 吞吐衰减 / 性能衰减声称
    r"\d+(?:\.\d+)?\s*%\s*(?:吞吐衰减|throughput decay|性能衰减|performance decay)",
]
# 证据标记：声称行必须命中其一（同行内）
PERF_CLAIM_EVIDENCE_RE = re.compile(
    r"(cargo\s|复现|reproduce|docs/|assessment/|SOAK_DURATION|实测|采集"
    r"|\d{4}-\d{2}-\d{2}|详见|see\s+docs)"
)


def scan_perf_claims(readme_path):
    """扫描性能数字声称缺少证据标记的行，返回 (line_no, text) 列表。"""
    issues = []
    try:
        lines = open(readme_path, encoding="utf-8", errors="replace").read().splitlines()
    except OSError:
        return issues
    for i, ln in enumerate(lines, start=1):
        for pat in PERF_CLAIM_PATTERNS:
            if re.search(pat, ln) and not PERF_CLAIM_EVIDENCE_RE.search(ln):
                issues.append((i, ln.strip()[:90]))
                break
    return issues


def _claimed_value(raw, actual):
    """按度量类型归一化声称值：字符串度量（version）原样比较，数值度量转 int。"""
    if isinstance(actual, str):
        return raw.strip()
    return int(raw.replace(",", "").replace("+", ""))


def scan_readme(metrics, readme_path):
    """扫描单个 README 文件，返回 issues 列表（行号从 1 开始）。"""
    issues = []  # (line_no, claimed, actual, kind, text, new_line)
    try:
        lines = open(readme_path, encoding="utf-8", errors="replace").read().splitlines()
    except OSError:
        return issues
    for i, ln in enumerate(lines, start=1):
        for pat, key, kind, fix_tpl in CLAIM_PATTERNS:
            for m in re.finditer(pat, ln):
                claimed = _claimed_value(m.group(1), metrics[key])
                actual = metrics[key]
                if claimed != actual:
                    new_line = ln[:m.start()] + fix_tpl \
                        .replace(r"\g<1>", str(actual)) \
                        .replace(r"\g<2>", str(metrics["packages"])) + ln[m.end():]
                    issues.append((i, claimed, actual, kind, ln.strip()[:90], new_line))
    return issues


def fix_readme(metrics, readme_path, issue_lines):
    """按行号集合应用 CLAIM_PATTERNS 修正（仅修正不一致处）。"""
    lines = open(readme_path, encoding="utf-8", errors="replace").read().splitlines()
    fixed = 0
    for ln in sorted(issue_lines):
        cur = lines[ln - 1]
        for pat, key, kind, fix_tpl in CLAIM_PATTERNS:
            m = re.search(pat, cur)
            if not m:
                continue
            claimed = _claimed_value(m.group(1), metrics[key])
            if claimed != metrics[key]:
                cur = cur[:m.start()] + fix_tpl \
                    .replace(r"\g<1>", str(metrics[key])) \
                    .replace(r"\g<2>", str(metrics["packages"])) + cur[m.end():]
                fixed += 1
        lines[ln - 1] = cur
    _safe_write(readme_path, "\n".join(lines) + "\n")
    return fixed, lines


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
          f"测试标注={metrics['test_annotations']} feature={metrics['features']} "
          f"版本={metrics['version']}")

    issues = []  # (readme_path, line_no, claimed, actual, kind, text, new_line)
    for rp in README_FILES:
        for it in scan_readme(metrics, rp):
            issues.append((rp, *it))
    for rp, ln, claimed, actual, kind, text, _ in issues:
        print(f"  ❌ {os.path.basename(rp)}:{ln} 声称 {claimed}（{kind}），实际 {actual} | {text}")
    if not issues:
        print("  ✅ README 数字声称与源码统计一致")

    # 性能声称证据检查（报告性，不参与 --fix）
    perf_issues = []
    for rp in README_FILES:
        for it in scan_perf_claims(rp):
            perf_issues.append((rp, *it))
    for rp, ln, text in perf_issues:
        print(f"  ❌ {os.path.basename(rp)}:{ln} 性能声称缺少证据标记（复现命令/报告链接/采集日期） | {text}")
    if not perf_issues:
        print("  ✅ README 性能声称均附带证据标记")

    if args.fix and issues:
        total_fixed = 0
        for rp in README_FILES:
            rp_lines = {it[1] for it in issues if it[0] == rp}
            if not rp_lines:
                continue
            fixed, lines = fix_readme(metrics, rp, rp_lines)
            total_fixed += fixed
            # 保险：修复残留检查（模板占位符泄漏会污染文档）
            leftover = [(i + 1, l) for i, l in enumerate(lines) if r"\g<" in l]
            if leftover:
                print(f"  ❌ {os.path.basename(rp)} 修复残留: "
                      f"{[(n, t.strip()[:60]) for n, t in leftover]}（模板占位符未替换，请人工修复）")
                return 2
        print(f"  → 已自动修正 README {total_fixed} 处数字声称")
        # 2026-10-27 整改：--fix 后按修复后的文件重新扫描，退出码反映修复后状态
        # （原实现复用修复前的 issues，导致修复成功仍打印 ❌ 且退出 1）
        issues = []
        for rp in README_FILES:
            for it in scan_readme(metrics, rp):
                issues.append((rp, *it))
        if not issues:
            print("  → 修复后复查一致")

    if args.json:
        out = os.path.join(ROOT, "docs", "metrics.json")
        _safe_write(out, json.dumps(metrics, indent=2, ensure_ascii=False))
        print(f"  → 已输出 {out}（文档可引用，禁止手写数字）")

    print("\n" + "=" * 60)
    all_ok = not issues and not perf_issues
    print(f"  结果: {'✅ 通过' if all_ok else '❌ 存在不一致数字声称'}")
    print("=" * 60)
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
