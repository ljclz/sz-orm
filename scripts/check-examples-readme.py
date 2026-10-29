#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-examples-readme.py — 示例清单同步门禁（AGENTS.md 门禁 24）
================================================================
2026-10-25 产品评测修复（M3）：README Examples 表格长期滞后于
examples/src/bin/ 实际可运行示例（表格 8 个 vs 实际 38 个）。

双向校验：
  1. README Examples/示例 章节中 `--bin X` 但 X 不存在于 examples/src/bin/
     → 文档指向幽灵示例（红牌）
  2. examples/src/bin/*.rs 存在但未在 README Examples/示例 章节登记
     → 示例清单脱节（红牌）

仅统计 "## Examples" / "## 示例" 章节内的 --bin 引用，避免历史版本
日志（如 v3.4.0 段落中的 `cargo run --bin sz_pay_pattern`）误判。

用法:
  python scripts/check-examples-readme.py
退出码: 0 = 一致；1 = 存在脱节
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN_DIR = os.path.join(ROOT, "examples", "src", "bin")
README_FILES = ["README.md", "README.zh.md"]
SECTION_RE = re.compile(r"^## (Examples|示例)$")
ANY_SECTION_RE = re.compile(r"^## ")
BIN_RE = re.compile(r"--bin\s+([a-zA-Z0-9_]+)")


def actual_bins():
    """examples/src/bin/ 下的 .rs 文件名（Cargo autobins 自动发现）。"""
    if not os.path.isdir(BIN_DIR):
        return []
    return sorted(f[:-3] for f in os.listdir(BIN_DIR) if f.endswith(".rs"))


def documented_bins(readme_path):
    """README Examples/示例 章节内登记的 --bin 名集合。"""
    bins = set()
    try:
        lines = open(readme_path, encoding="utf-8", errors="replace").read().splitlines()
    except OSError:
        return bins
    in_section = False
    for ln in lines:
        if SECTION_RE.match(ln.strip()):
            in_section = True
            continue
        if in_section and ANY_SECTION_RE.match(ln.strip()):
            in_section = False
        if in_section:
            bins.update(BIN_RE.findall(ln))
    return bins


def main():
    bins = actual_bins()
    doc = set()
    for rf in README_FILES:
        doc |= documented_bins(os.path.join(ROOT, rf))

    missing = sorted(doc - set(bins))          # 文档引用不存在
    unregistered = sorted(set(bins) - doc)      # 存在但未登记

    if missing:
        print("❌ README 引用了不存在的示例 bin（文档指向幽灵示例）:")
        for b in missing:
            print(f"   - {b}")
    if unregistered:
        print(f"❌ 未在 README Examples/示例 表格登记的 bin（共 {len(unregistered)} 个）:")
        for b in unregistered:
            print(f"   - {b}")
    if not missing and not unregistered:
        print(f"✅ 示例清单与 README 一致（{len(bins)} 个 bin 全部登记）")
        return 0
    print(f"  （实际 bin 数: {len(bins)}；README 登记数: {len(doc)}）")
    return 1


if __name__ == "__main__":
    sys.exit(main())