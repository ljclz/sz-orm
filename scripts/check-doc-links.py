#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""G35 门禁：文档链接健康检查（2026-10-30 TW 评测收尾新增）

背景：
  仓库历史曾出现 14 处死链（ADR 乱码文件名、已不存在的 assessment 报告、README.en 幽灵行等），
  人工遍历无法防复发。本脚本把链接检查固化为门禁：遍历核心用户文档的相对链接，
  目标不存在即红牌。

检查范围（核心用户文档）：
  - 根目录 *.md（README/README.zh/README.en/CHANGELOG 等）
  - docs/*.md、docs/adr/*.md、docs/migration/*.md、docs/assessment/*.md、docs/delivery/*.md

规则：
  - 仅检查 Markdown 相对链接 `[text](target)`；目标含 `#anchor` 时剥离锚点后判断文件。
  - 绝对协议（http/https/mailto/tel/`a:b`）与纯 `#锚点` 跳过。
  - 代码块（``` 围栏）内的链接不检查（可能是示意文本）。
  - 路径规范化：`\`→`/`，`./`/`../` 由 Path.resolve 处理。

用法：
    python scripts/check-doc-links.py
    python scripts/check-doc-links.py --self-test   # 自测：构造死链样本验证检测能力
退出码：0 = 无死链；1 = 存在死链（或自测失败）。
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

SCAN_DIRS = ["", "docs", "docs/adr", "docs/migration", "docs/assessment", "docs/delivery"]
LINK_RE = re.compile(r"\[([^\]]*)\]\(([^)]+)\)")
CODE_FENCE_RE = re.compile(r"^```", re.M)


def collect_markdown_files() -> list[Path]:
    files: list[Path] = []
    for rel_dir in SCAN_DIRS:
        d = ROOT / rel_dir if rel_dir else ROOT
        if d.is_dir():
            files.extend(sorted(d.glob("*.md")))
    return files


def strip_code_fences(text: str) -> str:
    """把 ``` 围栏内的行替换为空，避免检查代码块内的示意链接。"""
    lines = text.splitlines()
    out = []
    in_fence = False
    for ln in lines:
        if CODE_FENCE_RE.match(ln):
            in_fence = not in_fence
            continue
        if not in_fence:
            out.append(ln)
    return "\n".join(out)


def check_file(path: Path) -> list[tuple[int, str, str]]:
    """返回该文件的死链列表 [(行号, 原始链接, 目标路径)]。"""
    broken: list[tuple[int, str, str]] = []
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return broken
    body = strip_code_fences(text)
    base_dir = path.parent
    for i, ln in enumerate(body.splitlines(), 1):
        for m in LINK_RE.finditer(ln):
            url = m.group(2).strip()
            # 跳过绝对协议与纯锚点
            if re.match(r"^[a-zA-Z][a-zA-Z0-9+.\-]*:", url):
                continue
            if url.startswith("#"):
                continue
            target = url.split("#", 1)[0]
            if not target:
                continue
            target = target.replace("\\", "/")
            resolved = (base_dir / target).resolve()
            if not resolved.exists():
                broken.append((i, url, str(resolved)))
    return broken


def main() -> int:
    parser = argparse.ArgumentParser(description="G35 文档链接健康检查")
    parser.add_argument("--self-test", action="store_true", help="自测：构造死链样本验证")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    all_broken: list[tuple[Path, int, str, str]] = []
    files = collect_markdown_files()
    for f in files:
        for line_no, url, resolved in check_file(f):
            all_broken.append((f, line_no, url, resolved))

    print(f"G35 文档链接健康检查：扫描 {len(files)} 个文件")
    if not all_broken:
        print("G35 文档链接健康检查：通过（0 死链）")
        return 0
    for f, line_no, url, resolved in all_broken:
        rel = f.relative_to(ROOT).as_posix()
        print(f"  ❌ {rel}:{line_no} 链接 `{url}` -> 目标不存在：{resolved}")
    print(f"G35 文档链接健康检查：{len(all_broken)} 处死链")
    return 1


def self_test() -> int:
    """在临时目录构造样本：一个有效链接、一个死链，验证 check_file 能区分。"""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = Path(tmp)
        (tmp_dir / "ok.md").write_text("# ok\n", encoding="utf-8")
        sample = tmp_dir / "sample.md"
        sample.write_text(
            "# sample\n\n[有效](ok.md)\n[死链](no-such-file.md)\n```\n[示意](ignored-in-code.md)\n```\n[锚点](#anchor)\n",
            encoding="utf-8",
        )
        broken = check_file(sample)
        # 只应发现死链 no-such-file.md；ok.md 与代码块内/纯锚点不应报
        urls = [b[1] for b in broken]
        assert urls == ["no-such-file.md"], f"自测失败：{urls}"
        print("G35 自测通过：有效/死链/代码块/锚点区分正确")
        return 0


if __name__ == "__main__":
    sys.exit(main())