#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""G36 门禁：CHANGELOG 版本线一致性检查（2026-10-30 TW 评测收尾新增）

背景：
  CHANGELOG 曾存在大量中间版本缺口（54 条，缺 10.4/10.5/9.5-9.7 等）且尾部混入
  [Unreleased] 乱序条目；版本线是用户回溯能力与文档站版本归档（versions.html）
  的数据源。本脚本把版本线一致性固化为门禁。

规则：
  1. 解析 `## [x.y.z] — date` 版本头（[Unreleased] 跳过）。
  2. 版本号必须符合 MAJOR.MINOR.PATCH。
  3. 版本号不得重复。
  4. 最新版本（第一个数字版本头）必须 == workspace.package.version（Cargo.toml）。
  5. 若条目含 `YYYY-MM-DD` 日期，则日期应沿文件顺序非递增（从新到旧，允许同日）；
     含"推断区间"或"补录"的条目日期字段不做严格顺序判定（允许 `~` 区间形式）。

用法：
    python scripts/check-changelog-consistency.py
    python scripts/check-changelog-consistency.py --self-test
退出码：0 = 通过；1 = 存在不一致。
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:
    import tomli as tomllib  # Python 3.8/3.9/3.10 兼容（同 check-doc-consistency.py）

ROOT = Path(__file__).resolve().parent.parent
CHANGELOG = ROOT / "CHANGELOG.md"
CARGO_TOML = ROOT / "Cargo.toml"

VERSION_HEAD_RE = re.compile(r"^##\s*\[([\d.]+)\]\s*(?:—|-)\s*(.*)$")
SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+$")
DATE_RE = re.compile(r"(\d{4}-\d{2}-\d{2})")


def load_workspace_version() -> str:
    with open(CARGO_TOML, "rb") as f:
        data = tomllib.load(f)
    return data["workspace"]["package"]["version"]


def parse_versions(text: str) -> list[tuple[str, str]]:
    """返回 [(version, date_line)]，按文件出现顺序。"""
    versions = []
    for line in text.splitlines():
        m = VERSION_HEAD_RE.match(line.strip())
        if m:
            versions.append((m.group(1), m.group(2).strip()))
    return versions


def check_changelog(text: str, ws_version: str) -> list[str]:
    issues: list[str] = []
    versions = parse_versions(text)

    if not versions:
        issues.append("CHANGELOG 未找到任何 `## [x.y.z]` 版本头")
        return issues

    # 1) semver 合法性
    for v, _date in versions:
        if not SEMVER_RE.match(v):
            issues.append(f"版本号格式非法：`{v}`（应为 MAJOR.MINOR.PATCH）")

    # 2) 重复版本
    seen = set()
    for v, _date in versions:
        if v in seen:
            issues.append(f"版本号重复：`{v}`")
        seen.add(v)

    # 3) 最新版本与 workspace 一致
    latest = versions[0][0]
    if latest != ws_version:
        issues.append(f"最新版本 {latest} != workspace 版本 {ws_version}")

    # 4) 日期非递增（从新到旧；仅对含标准日期的条目判定）
    prev_date = ""
    for v, date_line in versions:
        m = DATE_RE.search(date_line)
        if not m:
            continue  # 补录/推断区间/无日期条目不参与严格判定
        cur = m.group(1)
        if prev_date and cur > prev_date:
            issues.append(
                f"版本 {v} 日期 {cur} 晚于上一版本 {prev_date}（版本线应从新到旧）"
            )
        prev_date = cur

    return issues


def main() -> int:
    parser = argparse.ArgumentParser(description="G36 CHANGELOG 版本线一致性")
    parser.add_argument("--self-test", action="store_true", help="自测")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    text = CHANGELOG.read_text(encoding="utf-8", errors="replace")
    ws_version = load_workspace_version()
    issues = check_changelog(text, ws_version)

    print(f"G36 CHANGELOG 版本线一致性：解析 {len(parse_versions(text))} 个版本头，workspace={ws_version}")
    if not issues:
        print("G36 CHANGELOG 版本线一致性：通过")
        return 0
    for issue in issues:
        print(f"  ❌ {issue}")
    return 1


def self_test() -> int:
    bad = """# Changelog

## [9.0.0] — 2026-09-26

## [10.0.0] — 2026-09-25

## [9.1.0] — 2026-09-27

## [9.0.0] — 2026-09-26
"""
    issues = check_changelog(bad, "10.13.0")
    # 应命中：最新版本 9.0.0 != 10.13.0；9.1.0 日期晚于上一版本；9.0.0 重复
    assert any("最新版本 9.0.0 != workspace 版本 10.13.0" in i for i in issues), issues
    assert any("晚于上一版本" in i for i in issues), issues
    assert any("版本号重复" in i for i in issues), issues

    good = """# Changelog

## [10.13.0] — 2026-10-27

## [10.12.0] — 2026-10-20

## [10.3.0] — 补录条目（推断区间）
"""
    issues_good = check_changelog(good, "10.13.0")
    assert not issues_good, issues_good

    print("G36 自测通过：版本线顺序/重复/最新版本/补录豁免判定正确")
    return 0


if __name__ == "__main__":
    sys.exit(main())