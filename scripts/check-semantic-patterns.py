#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-semantic-patterns.py — 语义反模式扫描（AGENTS.md 门禁 16）
================================================================
检测"接对了但算错了"的代码模式（szrsql 审查 A 类风险 / 2026-08-13 审计：
release_with_tenant 传 0 导致配额只增不减的 P0 bug 即此类）。

内置规则库（--rules 可传 JSON 文件覆盖/扩展）：
  R1  record_usage(..., 0)         : 无效递增（+= 0 无操作，通常是递减语义被误写）
  R2  let _ = <check/verify/...>;  : 丢弃检查/校验结果（检查后不处理）
  R3  uN 类型上的 < 0 / >= 0       : 无符号恒真/恒假比较（可疑逻辑）
  R4  release/close/drop 类调用传 0: 释放路径空操作

用法:
  python scripts/check-semantic-patterns.py
  python scripts/check-semantic-patterns.py --rules rules.json --only R1,R2
退出码: 0 = 无命中（或仅警告规则）；1 = 存在硬规则命中
"""

import argparse
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGES = os.path.join(ROOT, "packages")

# 内置规则库：每条规则 = 名称 + 正则 + 硬/软 + 说明
# 硬规则命中 → exit 1；软规则仅报告
DEFAULT_RULES = [
    {
        "id": "R1",
        "name": "无效递增(+=0)",
        "pattern": r"\b(record_usage|record\w*)\s*\([^)]*,\s*0\s*\)",
        "hard": True,
        "hint": "对 u64 累加器传 0 是无操作——若意图是递减应使用 release_usage/saturating_sub（2026-08-13 P0 教训）",
    },
    {
        "id": "R2",
        "name": "丢弃检查结果",
        "pattern": r"\blet\s+_\s*=\s*[^;]*(check|verify|validate|assert)[^;]*;",
        "hard": False,
        "hint": "检查/校验结果被丢弃——若检查有副作用可忽略，否则应处理错误",
    },
    {
        "id": "R3",
        "name": "无符号恒假比较",
        "pattern": r"\b(u8|u16|u32|u64|usize)\b[^;=<>]*[<>]=?\s*-?\d+",
        "hard": False,
        "hint": "无符号类型与负数/越界比较恒真或恒假，通常是逻辑错误",
    },
    {
        "id": "R4",
        "name": "释放路径空操作",
        "pattern": r"\b(release|close|drop|remove|delete)\w*\s*\([^)]*,\s*0\s*\)",
        "hard": True,
        "hint": "释放/删除路径传 0——确认是否为递减语义被误写（P0 类）",
    },
]


def iter_rs_files():
    for root, dirs, files in os.walk(PACKAGES):
        dirs[:] = [d for d in dirs if d not in ("target", "node_modules", ".git")]
        for f in files:
            if f.endswith(".rs"):
                yield os.path.join(root, f)
    for base in (os.path.join(ROOT, "cli", "src"), os.path.join(ROOT, "examples", "src")):
        if os.path.isdir(base):
            for root, dirs, files in os.walk(base):
                dirs[:] = [d for d in dirs if d not in ("target", "node_modules")]
                for f in files:
                    if f.endswith(".rs"):
                        yield os.path.join(root, f)


def strip_tests(txt):
    """去掉 #[cfg(test)] 与顶层 mod tests 块（测试代码不参与语义扫描）。"""
    for _ in range(8):
        new = re.sub(r"#\[cfg\(test\)\][^{]*\{[^{}]*\}", "", txt, flags=re.S)
        if new == txt:
            break
        txt = new
    return re.sub(r"\n\s*mod\s+tests\s*\{[^{}]*\}\n", "\n", txt)


def scan(rules):
    hits = []  # (rule_id, file, line_no, line_text)
    for path in iter_rs_files():
        try:
            txt = open(path, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        body = strip_tests(txt)
        lines = body.splitlines()
        for rule in rules:
            rx = re.compile(rule["pattern"])
            for i, ln in enumerate(lines, start=1):
                s = ln.strip()
                if s.startswith("//") or s.startswith("///") or s.startswith("//!"):
                    continue  # 跳过注释（文档示例含模式字串会误报）
                if rx.search(ln):
                    hits.append((rule["id"], os.path.relpath(path, ROOT).replace("\\", "/"), i, s[:120]))
    return hits


def main():
    ap = argparse.ArgumentParser(description="语义反模式扫描（门禁 16）")
    ap.add_argument("--rules", default=None, help="JSON 规则文件（覆盖内置）")
    ap.add_argument("--only", default=None, help="只跑指定规则，逗号分隔（如 R1,R4）")
    args = ap.parse_args()

    rules = json.load(open(args.rules, encoding="utf-8")) if args.rules else DEFAULT_RULES
    if args.only:
        keep = set(x.strip() for x in args.only.split(","))
        rules = [r for r in rules if r["id"] in keep]

    print("=" * 60)
    print("  语义反模式扫描（门禁 16）")
    print("=" * 60)
    hits = scan(rules)
    hard = [h for h in hits if next(r for r in rules if r["id"] == h[0])["hard"]]
    soft = [h for h in hits if h not in hard]

    for rid, f, ln, text in sorted(hits, key=lambda x: (x[0], x[1], x[2])):
        r = next(r for r in rules if r["id"] == rid)
        tag = "❌ 硬" if r["hard"] else "⚠️  软"
        print(f"  {tag} [{rid}] {f}:{ln}  {text}")
        print(f"        ↳ {r['hint']}")

    print("\n" + "=" * 60)
    print(f"  结果: 硬规则命中 {len(hard)} | 软规则命中 {len(soft)}")
    print("=" * 60)
    if hard:
        print("❌ 门禁 16 未通过 — 存在语义反模式（请修复或登记豁免）")
        return 1
    print("✅ 门禁 16 通过（软规则为提示）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
