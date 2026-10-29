#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""G32 门禁：文件名编码合法性检查（2026-10-27 新增，UXR 评测第三轮落地）

背景：
  曾发现多份中文文档的 NTFS 文件名是 UTF-8→GBK 乱码（如 `sz-ormAPI鍙傝€_md`），
  导致 README 直链在 GitHub/UTF-8 检出上 404；另有乱码文件名丢失 `.md` 扩展名
  变成 `_md` 结尾。传统门禁只检查文档内容，不检查文件名编码，导致此类问题漏网。

检查规则：
  1. 文件名含 U+FFFD 替换字符（无法无损解码）→ 红牌；
  2. 文件名以 `_md` 结尾（乱码吞掉扩展名）→ 红牌；
  3. 非 ASCII 文件名按 UTF-8 无法 round-trip 解码 → 红牌。

排除目录：.git / target / node_modules / .cargo / __pycache__ / .pytest_cache / dist / build。

用法：
    python scripts/check-filename-encoding.py
退出码：0 = 通过；1 = 存在乱码文件名。
"""
from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXCLUDE_DIRS = {
    ".git", "target", "node_modules", ".cargo", "__pycache__",
    ".pytest_cache", "dist", "build", ".mimosa", ".semgrep", ".zcodeignore",
    ".trae", ".codeartsdoer", ".codegraph", ".inscode", ".github",
}


def _name_roundtrips(name: str) -> bool:
    """非 ASCII 文件名按 UTF-8 无损 round-trip。"""
    try:
        return name.encode("utf-8").decode("utf-8") == name
    except Exception:
        return False


def scan() -> list[str]:
    issues: list[str] = []
    for p in ROOT.rglob("*"):
        # 跳过排除目录
        if any(part in EXCLUDE_DIRS for part in p.parts):
            continue
        if not p.is_file():
            continue
        name = p.name
        rel = str(p.relative_to(ROOT))
        # 1. 替换字符
        if "\ufffd" in name:
            issues.append(f"[{rel}] 文件名含 U+FFFD 替换字符（乱码）")
            continue
        # 2. 丢失扩展名
        if name.endswith("_md"):
            issues.append(f"[{rel}] 文件名以 `_md` 结尾（疑似乱码吞掉 .md 扩展名）")
            continue
        # 3. 非 ASCII 无法 round-trip（乱码字节序列）
        if any(ord(c) > 127 for c in name) and not _name_roundtrips(name):
            issues.append(f"[{rel}] 文件名无法按 UTF-8 无损解码（疑似编码乱码）")
    return issues


def main() -> int:
    print("=" * 60)
    print("  文件名编码合法性检查（门禁 32）")
    print("=" * 60)
    issues = scan()
    if issues:
        print(f"  ❌ 发现 {len(issues)} 个乱码文件名:")
        for it in issues:
            print(f"    - {it}")
        print("\n  结果: ❌ 存在乱码文件名（请在 UTF-8 环境下重命名）")
        return 1
    print("  ✅ 全部文件名编码合法")
    print("\n  结果: ✅ 通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())