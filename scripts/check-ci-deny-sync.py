#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""门禁：CI cargo-audit --ignore 与 deny.toml [advisories].ignore 一致性检查。

背景：2026-10-27 安全评测 M-1 发现 `.github/workflows/security.yml` 与 `ci.yml`
只传了 8 条 --ignore，而 deny.toml 登记 24 条，导致 cargo-audit CI 在真实锁文件下必然假红
（或门禁形同虚设）。本门禁强制两者一致，防止再次漂移。

判定：
- CI --ignore 集合 == deny.toml ignore 集合  -> PASS
- 否则列出差异 -> FAIL（exit 1）
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = [ROOT / ".github" / "workflows" / "security.yml",
             ROOT / ".github" / "workflows" / "ci.yml"]
DENY = ROOT / "deny.toml"

RUSTSEC_RE = re.compile(r"RUSTSEC-\d{4}-\d{4}")


def collect_from_workflows():
    """收集所有工作流中 cargo-audit 命令块里的 --ignore RUSTSEC-XXXX。"""
    ignores = set()
    for wf in WORKFLOWS:
        if not wf.exists():
            continue
        text = wf.read_text(encoding="utf-8", errors="replace")
        # 查找包含 'cargo audit' 的命令块，收集其中的 --ignore <ID>
        in_audit_block = False
        for line in text.splitlines():
            if re.search(r"\bcargo\s+audit\b", line):
                in_audit_block = True
                continue
            if in_audit_block:
                # 命令块结束：遇到下一个 name: 或 job 定义（缩进回退到顶层步骤）时停止
                if re.match(r"^\S", line) and line.strip() and not line.strip().startswith("#"):
                    in_audit_block = False
                m = re.search(r"--ignore\s+(RUSTSEC-\d{4}-\d{4})", line)
                if m:
                    ignores.add(m.group(1))
    return ignores


def collect_from_deny():
    """收集 deny.toml [advisories].ignore 中的 id = "RUSTSEC-XXXX"。"""
    if not DENY.exists():
        return set()
    text = DENY.read_text(encoding="utf-8", errors="replace")
    # 只统计 ignore 列表内（ignore = [ ... ] 块中）的 RUSTSEC id
    in_ignore_block = False
    ignores = set()
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("ignore"):
            in_ignore_block = True
            continue
        if in_ignore_block and stripped.startswith("]"):
            in_ignore_block = False
            continue
        if in_ignore_block:
            m = re.search(r'id\s*=\s*"(RUSTSEC-\d{4}-\d{4})"', line)
            if m:
                ignores.add(m.group(1))
    return ignores


def main():
    ci_ignores = collect_from_workflows()
    deny_ignores = collect_from_deny()

    print("=== 门禁：CI cargo-audit --ignore 与 deny.toml 一致性 ===")
    print(f"  CI 工作流 --ignore 数量: {len(ci_ignores)}")
    print(f"  deny.toml ignore 数量: {len(deny_ignores)}")
    only_ci = ci_ignores - deny_ignores
    only_deny = deny_ignores - ci_ignores
    if only_ci:
        print(f"  ⚠ 仅在 CI 中（deny.toml 缺失）: {sorted(only_ci)}")
    if only_deny:
        print(f"  ⚠ 仅在 deny.toml 中（CI 缺失）: {sorted(only_deny)}")

    if not ci_ignores:
        print("  ❌ 未在任何 CI 工作流中找到 cargo-audit --ignore 指令")
        return 1
    if ci_ignores == deny_ignores:
        print("  ✅ PASS：CI --ignore 与 deny.toml 完全一致")
        return 0
    print("  ❌ FAIL：CI --ignore 与 deny.toml 不一致（见上方差异）")
    return 1


if __name__ == "__main__":
    sys.exit(main())