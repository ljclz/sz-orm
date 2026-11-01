#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""G13 收尾：把 legacy-citation-ledger 附录 A 的 39 份零证据历史报告归档隔离。

背景（docs/assessment/2026-10-29-legacy-citation-ledger.md §四-3 / 附录 A）：
  45 份"零可校验引用"历史报告中，39 份自称给出结论却无任何 file:line 证据。
  账本铁律：禁止批量改写行号（=把编造证据洗成结构合规）。
  本脚本只做**机械操作**，不触碰正文内容、不生成任何新证据：
    1. git mv 到 docs/assessment/archive/（与活跃报告隔离）
    2. 页首插入"历史存档声明"（如实标注：本报告无可校验证据，不作为证据来源）
    3. 全仓改写指向这 39 份的 markdown 链接（防 G35 死链）
  内容真伪问题不自动判定——归档后如实暴露为"不可复核存档"，由 fail-closed
  门禁语义（活跃报告零引用=红牌）守住边界，而非人工逐份核实。

用法：
    python scripts/archive-zero-evidence-reports.py            # 执行归档
    python scripts/archive-zero-evidence-reports.py --dry-run  # 只打印计划
退出码：0 = 成功（或幂等无操作）。
"""
from __future__ import annotations

import argparse
import io
import os
import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSESS = ROOT / "docs" / "assessment"
ARCHIVE_DIR = ASSESS / "archive"

# 账本附录 A 的 45 份清单，排除 6 份 prompt/spec（本就不承载代码证据）
ZERO_EVIDENCE_REPORTS = [
    "2026-08-14-blackhat-security-audit.md",
    "2026-08-16-full-gate-block-report.md",
    "2026-08-16-full-gate-review-report.md",
    "2026-08-16-gate-review-report.md",
    "2026-08-19-phantom2-feature-gate-evaluation.md",
    "2026-08-20-v4.9.1-performance-benchmark.md",
    "2026-08-22-phantom2-feature-gate-evaluation.md",
    "2026-08-22-supplement-tasks-v3-delivery.md",
    "2026-09-01-ai-optimization-delivery.md",
    "2026-09-03-strategy-assessment.md",
    "2026-09-04-v6.0.x-delivery-record.md",
    "2026-09-06-v64-vs-v65-comparison.md",
    "2026-09-17-v7.2.0-gate-report.md",
    "2026-09-21-v780-gate-report.md",
    "2026-09-22-v840-coverage-matrix.md",
    "2026-09-22-v840-quality-baseline.md",
    "2026-09-23-v870-baseline-comparison.md",
    "2026-09-23-v870-coverage-report.md",
    "2026-09-23-v870-low-coverage-modules.md",
    "2026-09-23-v870-mutation-report.md",
    "2026-09-23-v870-perf-after-optimization.md",
    "2026-09-23-v870-perf-benchmark-report.md",
    "2026-09-23-v870-perf-regressions.md",
    "2026-09-23-v870-survived-mutants.md",
    "2026-09-25-v880-gate-report.md",
    "2026-09-28-v920-coverage-report.md",
    "2026-09-28-v920-delivery-record.md",
    "2026-10-06-crates-io-publish-evaluation.md",
    "2026-10-07-v951-coverage-report.md",
    "2026-10-08-v951-five-dimensional-review.md",
    "2026-10-08-v960-perf-baseline.md",
    "2026-10-08-v970-perf-baseline.md",
    "2026-10-09-v990-reliability-baseline.md",
    "2026-10-14-v1010-phantom-audit.md",
    "2026-10-16-architecture-review.md",
    "2026-10-16-v1070-coverage.md",
    "2026-10-17-v1080-coverage.md",
    "2026-10-18-v10110-delivery-record.md",
    "2026-10-20-v10120-delivery-record.md",
]

BANNER = (
    "> **📦 历史存档（2026-11-02 自动化归档）**：本报告属"
    " [legacy-citation-ledger 附录 A](../2026-10-29-legacy-citation-ledger.md)"
    " 的「零可校验引用」历史报告——正文结论未附可校验 `file:line` 证据，所引代码/行号"
    "可能已漂移或所属文件已删除。**本报告不作为当前审计证据来源**；复核请以现行报告为准。\n"
)

LINK_RE = re.compile(r"\[([^\]]*)\]\(([^)]+)\)")


def find_md_files() -> list[Path]:
    """与 G35 同口径的核心文档范围 + 全 docs（用于链接改写扫描）。"""
    files: list[Path] = []
    for rel in ["", "docs", "docs/adr", "docs/migration", "docs/assessment", "docs/delivery", "docs/assessment/archive"]:
        d = ROOT / rel if rel else ROOT
        if d.is_dir():
            files.extend(sorted(d.glob("*.md")))
    return files


def rewrite_links(dry_run: bool) -> list[str]:
    """把指向 39 份归档报告的链接目标改写为 archive 相对路径。"""
    archived_names = set(ZERO_EVIDENCE_REPORTS)
    changed = []
    for md in find_md_files():
        try:
            text = md.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        orig = text
        body_lines = text.splitlines()
        out_lines = []
        in_fence = False
        for ln in body_lines:
            if re.match(r"^```", ln):
                in_fence = not in_fence
                out_lines.append(ln)
                continue
            if not in_fence:
                def _sub(m: re.Match) -> str:
                    url = m.group(2).strip()
                    if re.match(r"^[a-zA-Z][a-zA-Z0-9+.\-]*:", url) or url.startswith("#"):
                        return m.group(0)
                    target = url.split("#", 1)[0].replace("\\", "/")
                    if os.path.basename(target) in archived_names:
                        # 计算从当前文件目录到归档目标的相对路径
                        new_target = os.path.relpath(
                            ARCHIVE_DIR / os.path.basename(target),
                            md.parent,
                        ).replace("\\", "/")
                        anchor = url.split("#", 1)[1] if "#" in url else ""
                        new_url = new_target + (f"#{anchor}" if anchor else "")
                        return f"[{m.group(1)}]({new_url})"
                    return m.group(0)
                ln = LINK_RE.sub(_sub, ln)
            out_lines.append(ln)
        text = "\n".join(out_lines)
        if text != orig:
            changed.append(md)
            if not dry_run:
                with io.open(md, "w", encoding="utf-8", newline="\n") as f:
                    f.write(text)
    return changed


def main() -> int:
    parser = argparse.ArgumentParser(description="归档 39 份零证据历史报告")
    parser.add_argument("--dry-run", action="store_true", help="只打印计划，不落盘")
    args = parser.parse_args()

    ARCHIVE_DIR.mkdir(exist_ok=True)
    moved, missing = [], []
    for name in ZERO_EVIDENCE_REPORTS:
        src = ASSESS / name
        dst = ARCHIVE_DIR / name
        if dst.exists():
            continue  # 幂等
        if not src.exists():
            missing.append(name)
            continue
        moved.append((name, dst))
        if not args.dry_run:
            # 先插入存档声明（保留原内容），再移动
            text = src.read_text(encoding="utf-8", errors="replace")
            if "历史存档（2026-11-02 自动化归档）" not in text:
                with io.open(src, "w", encoding="utf-8", newline="\n") as f:
                    f.write(BANNER + "\n" + text)
            shutil.move(str(src), str(dst))

    print(f"归档 {len(moved)} 份；缺失 {len(missing)} 份")
    for name, _ in moved:
        print(f"  -> docs/assessment/archive/{name}")
    for name in missing:
        print(f"  !! 工作区不存在（可能已被移走）：{name}")

    # 链接改写
    changed = rewrite_links(args.dry_run)
    print(f"链接改写 {len(changed)} 个文件")
    for c in changed:
        print(f"  ~ {c.relative_to(ROOT).as_posix()}")
    return 0


if __name__ == "__main__":
    main()