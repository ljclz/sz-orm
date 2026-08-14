#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-architecture.py — 架构一致性扫描（AGENTS.md 门禁 17）
================================================================
检测 szrsql 审查 B 类风险（架构孤岛/重复实现/抽象泄漏）：

  1. 概念重复实现：同一概念多个实现并存（如 BloomFilter vs BloomFilterGuard）
  2. 核心包依赖方向：sz-orm-core 依赖白名单（新依赖需登记）
  3. 孤儿包：无任何非 dev 依赖者的包（生产依赖图不可达）

概念表与白名单可扩展（--concepts / --allowlist 传 JSON）。

用法:
  python scripts/check-architecture.py
退出码: 0 = 通过；1 = 硬问题（重复实现/依赖越界）
"""

import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGES = os.path.join(ROOT, "packages")

# 概念重复实现表：{概念名: [实现符号...]}（2026-08-13 审计实例）
# 命中 = 同一概念存在 ≥2 个实现符号
DEFAULT_CONCEPTS = {
    "bloom_filter": ["BloomFilter", "BloomFilterGuard"],
    "cache_ttl_holder": ["cache_ttl", "cache_ttl_override"],
}

# sz-orm-core 依赖白名单（Cargo.toml:40-60 现有依赖；新增依赖必须登记）
CORE_ALLOWLIST = {
    "sz-orm-audit", "sz-orm-crypto", "sz-orm-health", "sz-orm-limit",
    "sz-orm-macros", "sz-orm-masking", "sz-orm-n1-lint",
    "sz-orm-query-builder", "sz-orm-sql-validator",
}


def iter_rs_files():
    for root, dirs, files in os.walk(PACKAGES):
        dirs[:] = [d for d in dirs if d not in ("target", "node_modules", ".git")]
        for f in files:
            if f.endswith(".rs"):
                yield os.path.join(root, f)


def strip_tests(txt):
    """去掉 #[cfg(test)] 与顶层 mod tests 块（括号匹配，支持嵌套花括号——
    正则 [^{}]* 会在测试体内嵌套 { } 处失败导致剥离不完整，2026-08-14 修复）。"""

    def strip_block(src, marker):
        out = []
        i = 0
        n = len(src)
        while True:
            m = re.search(marker, src[i:])
            if not m:
                out.append(src[i:])
                break
            start = i + m.start()
            out.append(src[i:start])
            j = src.find('{', start)
            if j == -1:
                out.append(src[start:])
                break
            depth = 0
            k = j
            while k < n:
                if src[k] == '{':
                    depth += 1
                elif src[k] == '}':
                    depth -= 1
                    if depth == 0:
                        break
                k += 1
            i = k + 1 if k < n else n
        return ''.join(out)

    txt = strip_block(txt, r'#\[cfg\(test\)\]')
    return strip_block(txt, r'\bmod\s+tests\s*\{')


def find_symbols(symbols):
    """返回 {symbol: [(file, line)]}——只统计定义处（pub struct/enum/trait）。"""
    found = {s: [] for s in symbols}
    for path in iter_rs_files():
        try:
            lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
        except OSError:
            continue
        for i, ln in enumerate(lines, start=1):
            for s in symbols:
                if re.search(rf"^\s*pub\s+(struct|enum|trait)\s+{re.escape(s)}\b", ln):
                    found[s].append((os.path.relpath(path, ROOT).replace("\\", "/"), i))
    return found


def check_concepts(concepts):
    problems = []
    for concept, syms in concepts.items():
        found = find_symbols(syms)
        present = {s: locs for s, locs in found.items() if locs}
        if len(present) >= 2:
            locs = "; ".join(f"{s}@{locs[0][0]}:{locs[0][1]}" for s, locs in present.items())
            problems.append((concept, locs))
    return problems


def check_core_deps():
    """读取 sz-orm-core Cargo.toml 的 workspace 内依赖，对照白名单。"""
    toml = os.path.join(PACKAGES, "sz-orm-core", "Cargo.toml")
    txt = open(toml, encoding="utf-8", errors="replace").read()
    deps = set(re.findall(r"^(sz-orm-[\w-]+)\s*=", txt, re.M))
    return sorted(deps - CORE_ALLOWLIST - {"sz-orm-core"})


def find_cargo():
    """自动发现 cargo：PATH 或常见安装路径。"""
    import shutil
    exe = shutil.which("cargo")
    if exe:
        return exe
    home = os.path.expanduser("~")
    for cand in (os.path.join(home, ".cargo", "bin", "cargo"),
                 os.path.join(home, ".cargo", "bin", "cargo.exe"),
                 r"C:\Users\Administrator\.cargo\bin\cargo.exe"):
        if os.path.isfile(cand):
            return cand
    return "cargo"


def check_orphans():
    """孤儿包：workspace 内无任何非 dev 依赖者。"""
    try:
        out = subprocess.run(
            [find_cargo(), "metadata", "--no-deps", "--format-version", "1"],
            cwd=ROOT, capture_output=True, text=True, timeout=120,
        )
        meta = json.loads(out.stdout)
    except Exception:
        return []
    pkgs = {p["name"]: p for p in meta["packages"]}
    names = {p["name"] for p in meta["packages"] if p["id"] in meta["workspace_members"]}
    rev = {n: set() for n in names}
    for n in names:
        for d in pkgs[n]["dependencies"]:
            if d["name"] in names and d["kind"] is None:
                rev[d["name"]].add(n)
    orphans = sorted(n for n in names if not rev[n] and n not in ("sz-orm-cli", "sz-orm-examples"))
    return orphans


def main():
    ap = argparse.ArgumentParser(description="架构一致性扫描（门禁 17）")
    ap.add_argument("--concepts", default=None, help="概念表 JSON（覆盖内置）")
    ap.add_argument("--exempt", default=None, help="豁免概念（逗号分隔，登记在案的理由见 AGENTS.md/根因文档）")
    ap.add_argument("--skip-orphans", action="store_true", help="跳过孤儿包检查")
    args = ap.parse_args()

    concepts = json.load(open(args.concepts, encoding="utf-8")) if args.concepts else DEFAULT_CONCEPTS
    exempted = set(x.strip() for x in args.exempt.split(",")) if args.exempt else set()

    print("=" * 60)
    print("  架构一致性扫描（门禁 17）")
    print("=" * 60)

    print("\n[1/3] 概念重复实现")
    dup = check_concepts(concepts)
    active = [d for d in dup if d[0] not in exempted]
    for concept, locs in dup:
        if concept in exempted:
            print(f"  ⚠️  豁免登记: 概念「{concept}」{locs}（已登记架构债，见 AGENTS.md 门禁 17 注记）")
        else:
            print(f"  ❌ 概念「{concept}」存在多个实现: {locs}")
    if not dup:
        print("  ✅ 无重复实现")

    print("\n[2/3] sz-orm-core 依赖白名单")
    extra = check_core_deps()
    for d in extra:
        print(f"  ❌ 白名单外依赖: {d}（若确需，登记到 check-architecture.py CORE_ALLOWLIST）")
    if not extra:
        print("  ✅ 依赖在白名单内")

    orphans = [] if args.skip_orphans else check_orphans()
    print(f"\n[3/3] 孤儿包（{len(orphans)} 个，生产依赖图不可达）")
    if orphans:
        print("  ⚠️  " + ", ".join(orphans[:12]) + ("..." if len(orphans) > 12 else ""))
        print("     ↳ 独立库包属正常形态；若文档宣称「集成/自动」能力则违反门禁 15")

    print("\n" + "=" * 60)
    ok = not active and not extra
    print(f"  结果: {'✅ 通过' if ok else '❌ 存在硬问题（重复实现/依赖越界）'}")
    print("=" * 60)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
