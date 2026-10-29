#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""门禁（警告级）：deny.toml 中已不再适用的 RUSTSEC ignore 提示。

背景：2026-10-27 供应链治理发现 deny.toml 中曾滞留"已升级消除"的 ignore
（webpki-root-certs/ctutils/tokio 升级后 0383/0385/0386 仍登记）。
本脚本比对 Cargo.lock 与本地 advisory 数据库：
- 若某条 ignore 对应 crate 已不在 Cargo.lock 中 -> 提示可移除（过时）
- 若 advisory 数据库不可用 -> SKIP（不阻断）

默认警告级（exit 0）；`--strict` 时发现过时 ignore 退出码 1。
"""
import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "Cargo.lock"
DENY = ROOT / "deny.toml"

# 可能的本地 advisory 数据库位置
ADVISORY_DB_CANDIDATES = [
    Path("D:/tools/advisory-db-main"),
    Path.home() / ".cargo" / "advisory-db",
    ROOT / ".." / "advisory-db-main",
]


def lockfile_crate_names():
    text = LOCK.read_text(encoding="utf-8", errors="replace")
    return set(re.findall(r'name = "([^"]+)"', text))


def deny_ignores():
    if not DENY.exists():
        return set()
    text = DENY.read_text(encoding="utf-8", errors="replace")
    in_ignore = False
    ids = set()
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("ignore"):
            in_ignore = True
            continue
        if in_ignore and stripped.startswith("]"):
            in_ignore = False
            continue
        if in_ignore:
            m = re.search(r'id\s*=\s*"(RUSTSEC-\d{4}-\d{4})"', line)
            if m:
                ids.add(m.group(1))
    return ids


def build_advisory_crate_map(db):
    """建立 RUSTSEC ID -> crate 名 映射（从 crates/<name>/RUSTSEC-*.md 路径推断）。"""
    mapping = {}
    crates_dir = db / "crates"
    if not crates_dir.is_dir():
        return mapping
    for crate_dir in crates_dir.iterdir():
        if not crate_dir.is_dir():
            continue
        for adv in crate_dir.glob("RUSTSEC-*.md"):
            mapping[adv.stem] = crate_dir.name
    return mapping


def find_advisory_db():
    for cand in ADVISORY_DB_CANDIDATES:
        if cand.is_dir() and (cand / "crates").is_dir():
            return cand
    # 环境变量
    env = __import__("os").environ.get("SZ_ORM_ADVISORY_DB")
    if env and Path(env).is_dir():
        return Path(env)
    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--db", default=None, help="本地 advisory 数据库路径")
    args = parser.parse_args()

    db = Path(args.db) if args.db else find_advisory_db()
    print("=== 门禁（警告级）：deny.toml 过时 ignore 提示 ===")
    if db is None:
        print("  ⚠ SKIP：未找到本地 advisory 数据库（可设置 --db 或 SZ_ORM_ADVISORY_DB）")
        return 0

    lock_crates = lockfile_crate_names()
    ignores = deny_ignores()
    mapping = build_advisory_crate_map(db)
    stale = []
    for rid in sorted(ignores):
        crate = mapping.get(rid)
        if crate is None:
            print(f"  ⚠ 本地 advisory 库中未找到 {rid}（数据库可能不完整，跳过）")
            continue
        if crate not in lock_crates:
            stale.append((rid, crate))

    if stale:
        print(f"  ⚠ 发现 {len(stale)} 条过时 ignore（对应 crate 已不在 Cargo.lock）：")
        for rid, crate in stale:
            print(f"    - {rid}（{crate} 已不在锁中，可考虑移除 ignore）")
        if args.strict:
            print("  ❌ FAIL（strict）：存在过时 ignore，请清理 deny.toml")
            return 1
        print("  ⚠ 警告（不阻断）：请在确认上游状态后清理 deny.toml 过时 ignore")
        return 0
    print("  ✅ PASS：deny.toml 中无过时 ignore（或所有 ignore 对应 crate 仍在锁中）")
    return 0


if __name__ == "__main__":
    sys.exit(main())