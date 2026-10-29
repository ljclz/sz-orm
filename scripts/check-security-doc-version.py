#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""门禁：Security.md 版本与 workspace 版本一致性检查。

背景：2026-10-27 安全评测 M-5 发现 `docs/Security.md` 停在 v3.4.0，
而工作空间已是 v10.12.0——安全策略/VDP/功能矩阵与代码严重漂移。

判定：
- Security.md 头部声明的版本号 == Cargo.toml workspace.package.version  -> PASS
- 否则 FAIL（exit 1）
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARGO = ROOT / "Cargo.toml"
SECURITY = ROOT / "docs" / "Security.md"


def workspace_version():
    text = CARGO.read_text(encoding="utf-8", errors="replace")
    m = re.search(r'\[workspace\.package\]', text)
    if not m:
        return None
    # 在 workspace.package 段内找 version
    seg = text[m.end():]
    seg = seg.split("\n\n", 1)[0]
    v = re.search(r'version\s*=\s*"([^"]+)"', seg)
    return v.group(1) if v else None


def security_doc_version():
    text = SECURITY.read_text(encoding="utf-8", errors="replace")
    # 匹配 "版本：vX.Y.Z" 或 "版本: vX.Y.Z"
    m = re.search(r"版本\s*[:：]\s*v?(\d+\.\d+\.\d+)", text)
    return m.group(1) if m else None


def main():
    print("=== 门禁：Security.md 版本与 workspace 版本一致性 ===")
    ws_ver = workspace_version()
    doc_ver = security_doc_version()
    print(f"  workspace.package.version = {ws_ver}")
    print(f"  docs/Security.md 版本头   = {doc_ver}")

    if not ws_ver:
        print("  ❌ 无法从 Cargo.toml 读取 workspace.package.version")
        return 1
    if not doc_ver:
        print("  ❌ docs/Security.md 未找到 `版本：vX.Y.Z` 头（文档漂移高风险）")
        return 1
    if ws_ver == doc_ver:
        print("  ✅ PASS：安全文档版本与工作空间版本一致")
        return 0
    print(f"  ❌ FAIL：安全文档版本 {doc_ver} 与工作空间版本 {ws_ver} 不一致（请更新 docs/Security.md）")
    return 1


if __name__ == "__main__":
    sys.exit(main())