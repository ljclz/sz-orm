#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""G33 门禁：文档"当前版本声明"一致性检查（2026-10-27 新增，UXR 评测第三轮落地）

背景：
  仓库曾同时存在多套版本口径（workspace 10.13.0 / 使用指南 7.7.0 / 迁移指南 3.5.0 /
  README 徽章 10.12.0 / crates.io 备选 10.7），用户无法确定"当前该用什么版本"。
  现有 G12 只检查 AGENTS.md 与工程实践文档，不检查 README 顶部/徽章、使用指南头部、
  学习路线图头部等"当前版本声明"。

检查规则：
  对每份核心文档的"当前版本声明"行（README 顶部 vX.Y.Z、版本徽章、适用版本、
  当前工作空间版本、当前代码版本），提取版本号并与 workspace.package.version 比较：
  - 不一致 → 红牌；
  - 一致 → 通过。
  历史版本日志（CHANGELOG 条目、README 中 `## v3.4.0 New Features` 等标题）不在检查范围；
  迁移指南/选型指南/诊断指南等明确标注"基于历史版本编写"的文档不在检查范围。

  v10.13.0 (2026-10-28 架构评测 P0-2) 新增门禁数声明校验：
  以 AGENTS.md 门禁表行数为权威，扫描 README/README.zh/README.en/docs/README.md/
  engineering-practices 中的"N 道门禁 / N gates / 门禁总表（N 道）"声称，
  N 与权威数不一致且所在行无"历史"字样 → 红牌（防"25 道门禁 vs 34 道"类漂移复发）。

用法：
    python scripts/check-doc-version-consistency.py
退出码：0 = 通过；1 = 存在不一致的版本声明。
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# (文件, [(正则, 说明)]) —— 仅匹配"当前版本声明"行，避免历史段落误报
CHECKS = {
    "README.md": [
        (r'v(\d+\.\d+\.\d+)\s*·', "顶部版本行"),
        (r'version-(\d+\.\d+\.\d+)', "版本徽章"),
    ],
    "README.zh.md": [
        (r'v(\d+\.\d+\.\d+)\s*·', "顶部版本行"),
        (r'version-(\d+\.\d+\.\d+)', "版本徽章"),
    ],
    "docs/sz-orm使用指南.md": [
        (r'适用版本[:：][^0-9]*(\d+\.\d+\.\d+)', "适用版本"),
    ],
    "docs/sz-orm学习路线图.md": [
        (r'适用版本[:：][^0-9]*(\d+\.\d+\.\d+)', "适用版本"),
        (r'当前工作空间版本\s*(\d+\.\d+\.\d+)', "当前工作空间版本"),
    ],
    "AGENTS.md": [
        (r'当前代码版本\s*(\d+\.\d+\.\d+)', "当前代码版本"),
    ],
}

# 门禁数声称扫描范围（"当前"文档；历史演进章节行含"历史"字样自动豁免）
GATE_COUNT_DOCS = [
    "README.md",
    "README.zh.md",
    "README.en.md",
    "docs/README.md",
    "docs/sz-orm-engineering-practices.md",
]
GATE_COUNT_PATTERNS = [
    (r'(\d+)\s*道门禁', "门禁数（道门禁）"),
    (r'门禁总表（(\d+)\s*道）', "门禁总表数"),
    (r'(\d+)\s*gates', "gate 数（gates）"),
]


def count_agents_gates() -> int | None:
    """从 AGENTS.md 门禁表（| # | 门禁 | 命令 |）统计权威门禁行数。"""
    agents = ROOT / "AGENTS.md"
    if not agents.exists():
        return None
    rows = 0
    in_table = False
    for ln in agents.read_text(encoding="utf-8", errors="replace").splitlines():
        if ln.strip().startswith("| #") and "门禁" in ln:
            in_table = True
            rows = 0
            continue
        if in_table:
            if re.match(r"^\|\s*\d+\s*\|", ln):
                rows += 1
            elif ln.strip() == "" or not ln.strip().startswith("|"):
                break
    return rows or None


def check_gate_count_claims(authoritative: int) -> list[str]:
    issues: list[str] = []
    history_markers = ("历史", "historical")
    for rel in GATE_COUNT_DOCS:
        p = ROOT / rel
        if not p.exists():
            issues.append(f"[{rel}] 门禁数扫描目标文件不存在")
            continue
        current_heading = ""
        for i, ln in enumerate(p.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            stripped = ln.strip()
            if stripped.startswith("#"):
                current_heading = stripped  # 标题上下文：历史章节内的历史数字一并豁免
                if any(mk in stripped or mk in stripped.lower() for mk in history_markers):
                    continue
            line_text = f"{current_heading}\n{ln}"
            if any(mk in ln or mk in ln.lower() for mk in history_markers):
                continue  # 行内自带历史标记（如"历史记录""historical record"）
            if any(mk in current_heading or mk in current_heading.lower() for mk in history_markers):
                continue  # 所属历史章节标题（如"### 门禁验证（v2.0.0 历史记录…）"）
            for pat, desc in GATE_COUNT_PATTERNS:
                for m in re.finditer(pat, line_text):
                    claimed = int(m.group(1))
                    # 仅对当前行本身报告（标题中的声称已由历史豁免或属权威声明）
                    if claimed != authoritative:
                        issues.append(
                            f"[{rel}:{i}] {desc} 声称 {claimed}，AGENTS.md 门禁表实际 {authoritative}"
                        )
    return issues


def main() -> int:
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8", errors="replace")
    m = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.M)
    if not m:
        print("错误: 无法从 Cargo.toml 读取 workspace 版本")
        return 2
    ws_version = m.group(1)

    print("=" * 60)
    print("  文档版本声明一致性检查（门禁 33）")
    print(f"  workspace.package.version = {ws_version}")
    print("=" * 60)

    issues = []
    checked = 0
    for rel, patterns in CHECKS.items():
        p = ROOT / rel
        if not p.exists():
            issues.append(f"[{rel}] 文件不存在")
            continue
        text = p.read_text(encoding="utf-8", errors="replace")
        for i, ln in enumerate(text.splitlines(), 1):
            for pat, desc in patterns:
                m2 = re.search(pat, ln)
                if m2:
                    claimed = m2.group(1)
                    checked += 1
                    if claimed != ws_version:
                        issues.append(
                            f"[{rel}:{i}] {desc} 声称 {claimed}，workspace 实际 {ws_version}"
                        )

    # 门禁数声明校验（2026-10-28 P0-2 新增）
    gates = count_agents_gates()
    if gates is None:
        print("  ⚠️  无法从 AGENTS.md 统计门禁表行数，跳过门禁数校验")
    else:
        gate_issues = check_gate_count_claims(gates)
        issues.extend(gate_issues)
        print(f"  ✅ 权威门禁数（AGENTS.md 门禁表）= {gates}，门禁数声称扫描完成")

    if issues:
        print(f"  ❌ 发现 {len(issues)} 处不一致:")
        for it in issues:
            print(f"    - {it}")
        print("\n  结果: ❌ 存在文档版本声明不一致")
        return 1

    print(f"  ✅ 检查了 {checked} 处当前版本声明，全部与 workspace 一致")
    print("\n  结果: ✅ 通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())