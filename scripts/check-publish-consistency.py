#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-publish-consistency.py — 发布一致性扫描（AGENTS.md 门禁 19）
================================================================
检测发布管道声明不一致（2026-08-13 审计：Cargo.toml:6 workspace 版本
4.7.0 vs Cargo.toml:79 依赖声明 sz-orm-core = "4.6.0"——发布时版本对不上）。

检查项：
  1. workspace.package.version 与每个成员包 Cargo.toml 的 version 一致
  2. workspace.dependencies 中 sz-orm 包的 version 声明 == 该包实际版本
     （path 依赖发布到 crates.io 时版本必须可解析）

用法:
  python scripts/check-publish-consistency.py
退出码: 0 = 一致；1 = 存在版本声明不一致
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CARGO = os.path.join(ROOT, "Cargo.toml")
PACKAGES = os.path.join(ROOT, "packages")


def parse(txt, ws_version=None):
    """轻量 TOML 解析：name/version（支持 version.workspace = true）。"""
    name = re.search(r'^name\s*=\s*"([^"]+)"', txt, re.M)
    version = re.search(r'^version\s*=\s*"([^"]+)"', txt, re.M)
    if not version and ws_version and re.search(r"^version\.workspace\s*=\s*true", txt, re.M):
        version = ws_version
    return (name.group(1) if name else None,
            version.group(1) if isinstance(version, re.Match) else version)


# 独立版本线豁免：语言绑定包（PyPI/npm 生态从 0.1.0 起步，有意不跟随 workspace 版本）
VERSION_EXEMPT = {"sz-orm-python", "sz-orm-js", "sz-orm-graph"}


def main():
    ws = open(CARGO, encoding="utf-8", errors="replace").read()
    ws_version = re.search(r'^version\s*=\s*"([^"]+)"', ws, re.M).group(1)

    # 1) 成员包版本
    member_versions = {}
    for d in sorted(os.listdir(PACKAGES)):
        toml = os.path.join(PACKAGES, d, "Cargo.toml")
        if not os.path.isfile(toml):
            continue
        name, ver = parse(open(toml, encoding="utf-8", errors="replace").read(), ws_version)
        if name and ver:
            member_versions[name] = (ver, toml)

    # 2) workspace.dependencies 中的 sz-orm 声明版本
    ws_dep_versions = {}
    for m in re.finditer(r"^(sz-orm-[\w-]+)\s*=\s*\{\s*version\s*=\s*\"([^\"]+)\"", ws, re.M):
        ws_dep_versions[m.group(1)] = m.group(2)

    print("=" * 60)
    print("  发布一致性扫描（门禁 19）")
    print("=" * 60)
    print(f"  workspace.package.version = {ws_version}")
    print(f"  豁免独立版本线: {sorted(VERSION_EXEMPT)}")

    issues = []
    soft = []
    for name, (ver, toml) in sorted(member_versions.items()):
        if name in VERSION_EXEMPT:
            continue
        if ver != ws_version:
            issues.append((name, toml, f"成员版本 {ver} != workspace {ws_version}"))

    for name, decl in sorted(ws_dep_versions.items()):
        if name not in member_versions:
            continue
        actual = member_versions[name][0]
        # semver 语义：依赖声明是下限，decl <= actual 合法；decl > actual 发布时不可解析
        if decl > actual:
            issues.append((name, "workspace.dependencies", f"依赖声明 {decl} > 实际 {actual}（发布 crates.io 时不可解析）"))
        elif decl < actual:
            soft.append((name, f"依赖声明 {decl} < 实际 {actual}（下限合法，发布前确认已发布对应版本）"))

    for name, loc, msg in issues:
        print(f"  ❌ {name}（{loc}）: {msg}")
    for name, msg in soft:
        print(f"  ⚠️  {name}: {msg}")
    if not issues and not soft:
        print("  ✅ 所有版本声明一致")

    print("\n" + "=" * 60)
    print(f"  结果: {'✅ 通过' if not issues else '❌ 存在版本声明不一致'}")
    print("=" * 60)
    return 0 if not issues else 1


if __name__ == "__main__":
    sys.exit(main())
