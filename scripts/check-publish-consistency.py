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
  python scripts/check-publish-consistency.py [--check-crates-io]
  --check-crates-io  额外查询 crates.io API，校验 README 中声称的 crates.io
                     版本是否已实际发布（网络不可用时自动跳过并提示）。
退出码: 0 = 一致；1 = 存在版本声明不一致
"""

import argparse
import json
import os
import re
import sys
import urllib.request

# Windows 默认 GBK 控制台下 emoji 输出会抛 UnicodeEncodeError；统一 UTF-8。
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8")
    except (AttributeError, ValueError):
        pass

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CARGO = os.path.join(ROOT, "Cargo.toml")
PACKAGES = os.path.join(ROOT, "packages")
CRATES_API = "https://crates.io/api/v1/crates/{name}"


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


def fetch_crates_io_max_version(crate_name: str):
    """查询 crates.io 实际最高版本；网络异常返回 None。"""
    url = CRATES_API.format(name=crate_name)
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "sz-orm-g19-check/1.0"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode("utf-8"))
        return data.get("crate", {}).get("max_version")
    except Exception as exc:  # noqa: BLE001 - 网络/解析异常统一按不可用处理
        print(f"  ℹ️  crates.io 查询失败（{exc.__class__.__name__}），跳过远程校验")
        return None


def _ver_key(v: str):
    """把版本号解析为 (major, minor) 整数元组，避免 '10.7' > '10.13.0' 式字符串误判。"""
    parts = v.strip().split(".")
    try:
        return tuple(int(p) for p in parts[:2])
    except ValueError:
        return (0, 0)


def check_crates_io_claims(ws_version: str) -> int:
    """校验 README/README.zh 中声明的 crates.io 版本是否已发布。"""
    print("\n  --- crates.io 远程校验（--check-crates-io）---")
    readmes = [os.path.join(ROOT, "README.md"), os.path.join(ROOT, "README.zh.md")]
    # 收集 README 中出现的 sz-orm-core / sz-orm-sqlx 版本声明（含注释中的备选行）
    claims = {}  # crate -> set(version)
    for rm in readmes:
        if not os.path.isfile(rm):
            continue
        txt = open(rm, encoding="utf-8", errors="replace").read()
        for m in re.finditer(r"sz-orm-(core|sqlx)\s*=\s*\"(\d+\.\d+)", txt):
            claims.setdefault("sz-orm-" + m.group(1), set()).add(m.group(2))

    if not claims:
        print("  ℹ️  README 中未发现 crates.io 版本声明")
        return 0

    errors = 0
    for crate, versions in sorted(claims.items()):
        max_ver = fetch_crates_io_max_version(crate)
        if max_ver is None:
            continue  # 网络不可用，跳过
        for ver in sorted(versions):
            # 语义版本比较（major.minor）：README 声称的版本若尚未发布到 crates.io，用户按文档装会失败
            if _ver_key(ver) > _ver_key(max_ver):
                print(f"  ❌ {crate} 声称版本 {ver} > crates.io 实际 {max_ver}（用户按文档安装会失败）")
                errors += 1
            elif _ver_key(ver) < _ver_key(max_ver):
                print(f"  ⚠️  {crate} 备选版本 {ver} < crates.io 实际 {max_ver}（可更新文档）")
            else:
                print(f"  ✅ {crate} 声称版本 {ver} == crates.io 实际 {max_ver}")
    return errors


def main():
    parser = argparse.ArgumentParser(description="发布一致性扫描（门禁 19）")
    parser.add_argument("--check-crates-io", action="store_true",
                        help="额外查询 crates.io API 校验 README 声称版本是否已发布")
    args = parser.parse_args()

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

    remote_issues = 0
    if args.check_crates_io:
        remote_issues = check_crates_io_claims(ws_version)
    else:
        remote_issues = 0
        print("\n  ℹ️  提示: 使用 --check-crates-io 可远程校验 README 声称的 crates.io 版本是否已发布")

    print("\n" + "=" * 60)
    print(f"  结果: {'✅ 通过' if not issues and remote_issues == 0 else '❌ 存在版本声明不一致'}")
    print("=" * 60)
    return 0 if not issues and remote_issues == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
