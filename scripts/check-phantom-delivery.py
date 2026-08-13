#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
check-phantom-delivery.py — 幻影交付扫描（AGENTS.md 门禁 15）
================================================================
幻影交付 = 文档宣称"已实现/已生效"（自动/强制/默认/集成语义），但生产路径零调用
或 feature gate 无任何启用点（默认构建不编译）。

两种幻影：
  PHANTOM-1 零调用符号  : 符号在定义文件之外、非测试/示例/benches 的 src 生产代码中
                         无任何调用方（排除 lib.rs 导出声明与注释）
  PHANTOM-2 门控未启用  : feature gate 定义存在，但无任何工作空间成员启用
                         （不在 default，也不在任何依赖声明的 features=[] 中）

用法:
  python scripts/check-phantom-delivery.py                # 全量扫描（内置宣称符号表）
  python scripts/check-phantom-delivery.py --symbols A B  # 自定义符号表
  python scripts/check-phantom-delivery.py --strict       # PHANTOM-2 也按失败计（exit 1）
  python scripts/check-phantom-delivery.py --skip-matrix  # 只查零调用符号

退出码: 0 = 无 PHANTOM-1（PHANTOM-2 默认仅警告）
        1 = 存在 PHANTOM-1，或 --strict 下存在 PHANTOM-2

依据: docs/assessment/2026-08-13-production-zero-call-audit.md（审计方法论固化）
"""

import argparse
import glob
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGES = os.path.join(ROOT, "packages")
CLI = os.path.join(ROOT, "cli", "Cargo.toml")
EXAMPLES = os.path.join(ROOT, "examples", "Cargo.toml")

# 内置宣称符号表：源自 2026-08-13 生产零调用审计报告 §二/§三
# （v4.6.0/v4.7.0 新模块 + 核心宣称组件；新增 feature 交付时应同步补充）
DECLARED_SYMBOLS = [
    # v4.7.0（delayed-priority-queue / forward-compat-sandbox / copy-parallel-shard /
    #         anomaly-remediation-rca / multicloud-cost-forecast / tenant-quota-rls-enhanced /
    #         cache-warmup-protection）
    "DelayScheduler", "PriorityQueue", "ScheduledMessage", "DelayedMessage",
    "ForwardCompatChecker", "SandboxDryRunner", "MigrationDependencyGraph",
    "CopyProtocolAdapter", "ParallelShardExecutor", "ConflictResolution",
    "AutoRemediator", "RootCauseAnalyzer", "AnomalyCorrelator",
    "MultiCloudCostComparator", "CapacityForecaster", "AutoOptimizer",
    "TenantResourceQuota", "QuotaEnforcer", "RlsPolicyEnhancer", "TenantAuditLogger",
    "CacheWarmer", "BloomFilter", "PenetrationGuard", "SingleFlight",
    # v4.6.0
    "RedeliveryScheduler", "RollbackExecutor", "BatchTransactionCoordinator",
    "AnomalyDetector", "CostAnalyzer", "ConnectionTenantBinder", "ProcessL1Cache",
    # 核心宣称组件（README/AGENTS.md 宣称的集成能力）
    "N1QueryDetector", "HookRegistry", "HookDispatcher",
    "GossipInvalidationBus", "RedisPubSubInvalidationBus", "WriteBehindQueue",
    "BehaviorRegistry",
    # v4.7.0 宣称的缓存穿透/击穿防护
    "PenetrationGuard", "SingleFlight",
]

# 导出声明模式（lib.rs 的 pub mod / pub use，不算调用）
EXPORT_RE = re.compile(r"^\s*(pub\s+mod\s+\w+|pub\s+use\s+\S+)")
# 多行 pub use X::{ ... }; 块（续行符号不计调用）
PUB_USE_BLOCK_RE = re.compile(r"pub\s+use\s+[\w:]+::\{[^}]*\};", re.S)
# 非生产目录（测试/示例/基准/fuzz）
NON_PROD_DIR_RE = re.compile(r"(^|/)(tests|benches|fuzz|examples)(/|$)")


def sym_re(symbol):
    """词边界匹配，避免子串误报（如 BloomFilterGuard 含 BloomFilter）。"""
    return re.compile(r"\b" + re.escape(symbol) + r"\b")


def iter_rs_files():
    for root, dirs, files in os.walk(PACKAGES):
        dirs[:] = [d for d in dirs if d not in ("target", "node_modules", ".git")]
        for f in files:
            if f.endswith(".rs"):
                yield os.path.join(root, f)
    for d in (CLI, EXAMPLES):
        pass  # cli/examples 视为生产入口（脚本入口），src 位于各自目录
    for base in (os.path.join(ROOT, "cli", "src"), os.path.join(ROOT, "examples", "src")):
        if os.path.isdir(base):
            for root, dirs, files in os.walk(base):
                dirs[:] = [d for d in dirs if d not in ("target", "node_modules")]
                for f in files:
                    if f.endswith(".rs"):
                        yield os.path.join(root, f)


def strip_test_blocks(txt):
    """去掉 #[cfg(test)] 与顶层 mod tests { ... } 块，返回生产代码。"""
    txt = re.sub(r"#\[cfg\(test\)\][^{]*\{[^{}]*\}", "", txt, flags=re.S)
    # 逐层剥离（cfg(test) 块内可能嵌套 mod）
    for _ in range(8):
        new = re.sub(r"#\[cfg\(test\)\][^{]*\{[^{}]*\}", "", txt, flags=re.S)
        if new == txt:
            break
        txt = new
    txt = re.sub(r"\n\s*mod\s+tests\s*\{[^{}]*\}\n", "\n", txt)
    return txt


def find_definition_file(symbol):
    """返回定义符号的文件（含 pub struct/enum/trait/fn 的行）。"""
    for path in iter_rs_files():
        try:
            lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
        except OSError:
            continue
        for i, ln in enumerate(lines):
            if re.search(rf"^\s*pub\s+(struct|enum|trait|fn)\s+{re.escape(symbol)}\b", ln):
                return path, i + 1
    return None, None


def production_callers(symbol, def_file):
    """统计定义文件之外的生产 src 引用（排除注释、导出声明、测试）。"""
    rx = sym_re(symbol)
    callers = []
    for path in iter_rs_files():
        if path == def_file:
            continue
        rel = os.path.relpath(path, ROOT).replace("\\", "/")
        if NON_PROD_DIR_RE.search(rel):
            continue
        try:
            txt = open(path, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        if not rx.search(txt):
            continue
        body = PUB_USE_BLOCK_RE.sub("", strip_test_blocks(txt))
        hits = 0
        for ln in body.splitlines():
            if not rx.search(ln):
                continue
            s = ln.strip()
            if s.startswith("//") or s.startswith("///") or s.startswith("//!"):
                continue
            if EXPORT_RE.match(s):
                continue
            hits += 1
        if hits:
            callers.append((rel, hits))
    return callers


def check_symbols(symbols, verbose=False):
    phantoms = []
    ok = []
    for sym in sorted(set(symbols)):
        def_file, def_line = find_definition_file(sym)
        if def_file is None:
            # 未定义（可能是外部 crate 符号或已被移除）
            phantoms.append((sym, "未找到定义", []))
            continue
        callers = production_callers(sym, def_file)
        def_rel = os.path.relpath(def_file, ROOT).replace("\\", "/")
        if callers:
            ok.append((sym, def_rel, callers))
        else:
            phantoms.append((sym, def_rel, []))
    return ok, phantoms


def collect_features():
    """{pkg: {feature: [依赖的 feature]}}，含 default。"""
    result = {}
    for toml in sorted(glob.glob(os.path.join(PACKAGES, "*", "Cargo.toml"))) + [CLI, EXAMPLES]:
        txt = open(toml, encoding="utf-8", errors="replace").read()
        m = re.search(r"\[features\]", txt)
        if not m:
            continue
        pkg = re.search(r'^name\s*=\s*"([^"]+)"', txt, re.M).group(1)
        section = txt[m.end():].split("\n[")[0]
        feats = {}
        for ln in section.splitlines():
            mm = re.match(r'^\s*([\w-]+)\s*=\s*\[([^\]]*)\]', ln)
            if mm:
                feats[mm.group(1)] = [x.strip().strip('"') for x in mm.group(2).split(",") if x.strip()]
        result[pkg] = feats
    return result


def collect_enable_points():
    """所有启用点：依赖声明 features=[...] + feature 定义中的 dep/feature 语法。"""
    enabled = {}
    for toml in sorted(glob.glob(os.path.join(PACKAGES, "*", "Cargo.toml"))) + [CLI, EXAMPLES]:
        txt = open(toml, encoding="utf-8", errors="replace").read()
        # 1) 依赖声明 features=[...]
        for m in re.finditer(r"([\w-]+)\s*=\s*\{([^}]*)\}", txt):
            dep, body = m.group(1), m.group(2)
            if not dep.startswith("sz-orm"):
                continue
            fm = re.search(r'features\s*=\s*\[([^\]]*)\]', body)
            if fm:
                for ff in re.findall(r'"([\w-]+)"', fm.group(1)):
                    enabled.setdefault(dep, set()).add(ff)
        # 2) feature 定义内的 dep/feature 语法（如 cli: n1-lint = ["dep:sz-orm-n1-lint", "sz-orm-n1-lint/n1-lint"]）
        for m in re.finditer(r'([\w-]+)/([\w-]+)', txt):
            dep, feat = m.group(1), m.group(2)
            if dep.startswith("sz-orm"):
                enabled.setdefault(dep, set()).add(feat)
    return enabled


def check_feature_matrix():
    """PHANTOM-2：定义存在但无任何跨包启用点且不在 default。"""
    feats = collect_features()
    enabled = collect_enable_points()
    phantoms = []
    for pkg, fs in sorted(feats.items()):
        for f in sorted(fs):
            if f == "default":
                continue
            # 跨包启用
            if f in enabled.get(pkg, set()):
                continue
            # default 链：被 default 直接或间接引用
            if f in fs.get("default", []):
                continue
            # 内部 feature 链不算启用（默认不编译）
            phantoms.append((pkg, f))
    return phantoms


def main():
    ap = argparse.ArgumentParser(description="幻影交付扫描（AGENTS.md 门禁 15）")
    ap.add_argument("--symbols", nargs="*", default=DECLARED_SYMBOLS,
                    help="符号表（默认内置宣称符号表）")
    ap.add_argument("--strict", action="store_true", help="PHANTOM-2 也按失败计")
    ap.add_argument("--skip-matrix", action="store_true", help="跳过 feature 矩阵检查")
    args = ap.parse_args()

    print("=" * 60)
    print("  幻影交付扫描: PHANTOM-1 零调用符号 + PHANTOM-2 门控未启用")
    print("=" * 60)

    ok, phantoms = check_symbols(args.symbols)
    print(f"\n[1/2] 生产调用断言（宣称符号 {len(ok) + len(phantoms)} 个）")
    for sym, def_rel, callers in ok:
        print(f"  ✅ {sym:28s} 定义 {def_rel}  生产调用方 {len(callers)} 处")
    for sym, def_rel, _ in phantoms:
        if def_rel == "未找到定义":
            print(f"  ⚠️  {sym:28s} 未找到定义（符号已移除或非本仓库定义）")
        else:
            print(f"  ❌ PHANTOM-1 {sym:18s} 定义 {def_rel} — 生产路径零调用")

    if not args.skip_matrix:
        p2 = check_feature_matrix()
        print(f"\n[2/2] feature 启用矩阵（未启用 gate {len(p2)} 个）")
        for pkg, f in p2:
            print(f"  ⚠️  PHANTOM-2 {pkg:26s} {f} — 无任何成员启用，默认构建不编译")
        matrix_fail = args.strict and bool(p2)
    else:
        p2 = []
        matrix_fail = False

    n1 = len([p for p in phantoms if p[1] != "未找到定义"])
    print("\n" + "=" * 60)
    print(f"  结果: PHANTOM-1 {n1} 个 | PHANTOM-2 {len(p2)} 个 | 通过 {len(ok)} 个")
    print("=" * 60)
    if n1 > 0 or matrix_fail:
        print("❌ 门禁 15 未通过 — 存在幻影交付（请接线或修正文档措辞）")
        return 1
    print("✅ 门禁 15 通过（PHANTOM-2 为警告，可加 --strict 升级为失败）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
