#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""幻影测试扫描门禁（AGENTS.md 门禁 30，2026-10-27 SDET 评测新增）
================================================================
背景：2026-10-27 SDET 评测发现测试套件存在幻影断言（测试存在但无效）：
- `dialect_advanced_test.rs:90-91` 永真断言 `assert!(x.is_empty() || !x.is_empty())`
- `multi_tenant_security_test.rs:9` 双字面量比较 `assert_ne!("tenant_a", "tenant_b")`
- `dialect_advanced_test.rs:180-181` 测试函数零断言（`let _ =` 丢弃结果）
- `smallstring_differential.rs` 名义差分（名为差分实为子串包含）

本脚本用静态规则扫描这些模式。每条规则的验收标准：能命中上述历史样本，
且对当前已修复代码零误报（回归样本见 tests/test_check_phantom_tests.py）。

规则：
  P1（硬）同表达式互斥或：`x || !x` 恒真断言
  P2（硬）双字面量断言：`assert_eq!("a", "b")` 比较两个编译期常量
  P2b（软）变量 vs 字面量断言：`assert_eq!(x, "a")` 比较局部变量与字面量
  P3（硬）测试函数零断言：`#[test]` 函数体内没有任何断言/panic（含 let _ 丢弃）
  P4（软）contains 兜底或：`sql.contains(x) || sql.is_empty()` 弱断言
  P5（硬）差分命名规范：文件名含 differential/diff 的测试文件必须含双活对拍断言
  P6（软）集成测试未引用产品 crate：`tests/` 文件无 `use sz_orm_core` 且测试体无产品符号

用法：
  python scripts/check-phantom-tests.py                # 检查
  python scripts/check-phantom-tests.py --only P1,P3   # 只跑指定规则
退出码：0 = 通过（无硬规则命中）；1 = 存在硬规则命中
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGES = os.path.join(ROOT, "packages")


def _safe_output_path(path: str) -> str:
    """将输出路径限制在仓库内（防路径穿越）。"""
    p = os.path.abspath(path)
    root = os.path.abspath(ROOT)
    if not (p == root or p.startswith(root + os.sep)):
        raise ValueError(f"输出路径超出仓库范围（仅允许写入 {root} 内）: {path}")
    return p

# 断言宏（P3 统计为"验证操作"的断言/panic）
ASSERT_RE = re.compile(
    r"\b(?:prop_)?assert(?:_eq|_ne|_approx|_ok|_lt|_le|_gt|_ge)?!\s*\("
    r"|\bpanic!\s*\("
    r"|\bunreachable!\s*\("
)

# 编译期断言模式（Send/Sync/Clone/Unpin 等 trait 约束检查）：
# 这类测试函数体无运行时 assert，但编译失败即测试失败，属合法零断言测试。
# 形如 `fn _assert_clone<T: Clone>() {}` + `_assert_clone::<T>();`
# 或 `assert_send::<T>();` / `assert_sync::<T>();`。
COMPILE_TIME_ASSERT_RE = re.compile(
    r"fn\s+_?assert_\w*\s*<|assert_(?:send|sync|clone|unpin|ref_unwind_safe|eq_type)\s*<"
)

# P1：同表达式互斥或（反向引用要求两侧标识符/调用完全一致）
# 组 1 捕获标识符（不含括号），`(?:\(\))?` 允许方法调用形式的尾部空括号。
P1_RE = re.compile(r"([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?)(?:\(\))?\s*\|\|\s*!\s*\1(?:\(\))?")
# P2：双字面量断言（assert_eq!/assert_ne! 两个参数都是字符串字面量）
P2_RE = re.compile(r'assert_(?:eq|ne)!\s*\(\s*"[^"]*"\s*,\s*"[^"]*"')
# P2b：变量 vs 字面量断言（软）
P2B_RE = re.compile(r'assert_(?:eq|ne)!\s*\(\s*\w+\s*,\s*"[^"]*"\s*[,)]')
# P4：contains 兜底或
P4_RE = re.compile(r"\.contains\([^)]*\)\s*\|\|\s*[\w.]+\.is_empty\(\)")
# P5：差分有效性证据断言——存在 `assert_eq!(expr, ...)` 且第一个参数是
# 非字符串字面量的表达式（变量/成员访问/索引/方法调用/函数调用）。字面量期望值
# （`assert_eq!(sql, "SELECT ...")`）与 `prop_assert_eq!` 均视为有效对拍。
P5_RE = re.compile(
    r"(?:prop_)?assert_eq!\(\s*([A-Za-z_]\w*(?:\([^)]*\)|\.\w+(?:\([^)]*\))?|\[\w+\])*)\s*,"
)

# P5 触发条件：仅文件名明确为"差分测试"（differential / _diff_test / diff_fuzz）。
# 不含 `schema_diff_*`/`perf_diff`/`mock_real_diff`/`diff.rs` 等"差异对比"语义文件。
P5_TRIGGER_RE = re.compile(r"(differential|_diff_test|diff_fuzz)")

# P5 存量豁免（2026-10-28：qb_migration_diff_test 已整改为归一化对拍断言，
# 7/7 通过，从豁免表移除——见 docs/assessment/2026-10-27-sdet-gates-fixes.md）
P5_LEGACY_EXEMPT = set()

# 测试函数定位：#[test] / #[tokio::test] / #[test(ignore=...)] 后的 fn
TEST_FN_RE = re.compile(
    r'#\[(?:tokio::)?test(?:\([^)]*\))?\]\s*(?:async\s+)?fn\s+(\w+)'
)

# 文件路径含 /tests/（集成测试目录）
INTEGRATION_TEST_DIR_RE = re.compile(r"(^|/)tests(/|$)")


def iter_rs_files():
    """扫描全部 .rs 文件（packages + cli/src + examples/src）。"""
    roots = [PACKAGES,
             os.path.join(ROOT, "cli", "src"),
             os.path.join(ROOT, "examples", "src")]
    for base in roots:
        if not os.path.isdir(base):
            continue
        for root, dirs, files in os.walk(base):
            dirs[:] = [d for d in dirs if d not in ("target", "node_modules", ".git")]
            for f in files:
                if f.endswith(".rs"):
                    yield os.path.join(root, f)


def strip_comments_and_strings(text):
    """剥离 // 行注释、/* */ 块注释与字符串字面量，降低规则误报。"""
    # 去行注释（保留行结构）
    text = re.sub(r"//[^\n]*", "", text)
    # 去块注释
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    # 去字符串字面量（含原始字符串）
    text = re.sub(r'r#"[^"]*"#', "", text)
    text = re.sub(r'"(?:\\.|[^"\\])*"', '""', text)
    return text


def extract_test_fn_bodies(lines):
    """提取每个 #[test]/#[tokio::test] 函数体的行区间。

    返回 [(fn_name, start_line_1based, end_line_1based, body_text)]
    """
    results = []
    txt = "\n".join(lines)
    for m in TEST_FN_RE.finditer(txt):
        fn_name = m.group(1)
        # 匹配起始位置所在行（0-based）
        start_line = txt[: m.start()].count("\n")
        # 从该行起找函数体起始 {（可能跨行）；进入函数体后从 0 重新累计括号
        depth = 0
        j = start_line
        while j < len(lines):
            depth += lines[j].count("{") - lines[j].count("}")
            if depth >= 1:
                depth = 0
                k = j
                while k < len(lines):
                    depth += lines[k].count("{") - lines[k].count("}")
                    if depth <= 0:
                        results.append((fn_name, start_line + 1, k + 1, lines[start_line:k + 1]))
                        break
                    k += 1
                break
            j += 1
    return results


def scan_file(path):
    """扫描单个文件，返回 [(rule, line_no, message)]。"""
    hits = []
    try:
        txt = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return hits
    lines = txt.splitlines()
    code = strip_comments_and_strings(txt)
    try:
        rel = os.path.relpath(path, ROOT).replace("\\", "/")
    except ValueError:
        # 跨盘符（如临时目录与仓库不在同一驱动器）时退化为绝对路径
        rel = path.replace("\\", "/")

    # P1：同表达式互斥或
    for m in P1_RE.finditer(code):
        ln = txt[: m.start()].count("\n") + 1
        hits.append(("P1", ln, f"永真断言 `{m.group(0).strip()}`（x || !x 恒为 true）"))

    # P2：双字面量断言
    for m in P2_RE.finditer(code):
        ln = txt[: m.start()].count("\n") + 1
        hits.append(("P2", ln, f"双字面量比较 `{m.group(0)[:60]}`（恒真/恒假，无测试价值）"))

    # P2b：变量 vs 字面量（软）
    for m in P2B_RE.finditer(code):
        ln = txt[: m.start()].count("\n") + 1
        hits.append(("P2b", ln, f"变量 vs 字面量断言 `{m.group(0)[:60]}`（疑似配置回显，请人工确认）"))

    # P4：contains 兜底或（软）
    for m in P4_RE.finditer(code):
        ln = txt[: m.start()].count("\n") + 1
        hits.append(("P4", ln, f"弱断言 `{m.group(0)[:70]}`（contains 结果以 is_empty 兜底）"))

    # P3：测试函数零断言（硬；豁免编译期 trait 约束检查）
    for fn_name, start, end, body_lines in extract_test_fn_bodies(lines):
        body = "\n".join(body_lines)
        if not ASSERT_RE.search(body) and not COMPILE_TIME_ASSERT_RE.search(body):
            hits.append(("P3", start,
                         f"测试函数 `{fn_name}`（行 {start}-{end}）零断言（无 assert/panic），"
                         "疑似 `let _ = ...` 丢弃或配置回显"))

    # P5：差分命名规范（硬；存量豁免见 P5_LEGACY_EXEMPT）
    base = os.path.basename(path)
    if P5_TRIGGER_RE.search(base):
        if rel not in P5_LEGACY_EXEMPT and not P5_RE.search(txt):
            hits.append(("P5", 1,
                         f"文件 `{rel}` 名称含差分语义（differential/_diff_test/diff_fuzz）"
                         "但无有效对拍断言（`assert_eq!(expr, ...)`，expr 非字符串字面量），疑似名义差分"))

    # P6：集成测试未引用任何产品 crate（软）
    # 2026-10-28 收紧：原规则只检查 `use sz_orm_core`，误报大量通过
    # `use sz_orm_advisor`/`sz_orm_sqlx` 等引用其他产品包的合法测试。
    # 现检查任意 `sz_orm_` 产品引用（use 或函数体）。
    if INTEGRATION_TEST_DIR_RE.search(rel):
        file_code = strip_comments_and_strings(txt)
        if "sz_orm_" not in file_code:
            for fn_name, start, end, body_lines in extract_test_fn_bodies(lines):
                body = "\n".join(body_lines)
                if "sz_orm_" not in body:
                    hits.append(("P6", start,
                                 f"测试函数 `{fn_name}` 未引用任何 sz_orm-* 产品 crate"
                                 "，疑似在测标准库/测试自身逻辑"))

    return hits


RULES = {
    "P1": {"desc": "同表达式互斥或（永真断言）", "hard": True},
    "P2": {"desc": "双字面量断言", "hard": True},
    "P3": {"desc": "测试函数零断言", "hard": True},
    "P5": {"desc": "差分命名规范", "hard": True},
    "P2b": {"desc": "变量 vs 字面量断言", "hard": False},
    "P4": {"desc": "contains 兜底或", "hard": False},
    "P6": {"desc": "集成测试未引用产品 crate", "hard": False},
}

# 软提示最多打印条数（避免 P6 等软规则输出刷屏）
MAX_SOFT_PRINT = 50


def load_p3_exempt(exempt_path):
    """加载 P3 存量豁免清单（每行 `rel_path:fn_name`，支持 # 注释）。

    2026-10-27 落地策略：当前仓库存在 227 个"只构造不验证"的存量零断言测试
    （历史遗留）。门禁对存量登记豁免（增量拦截），修复后从清单移除。
    """
    exempt = set()
    if exempt_path and os.path.isfile(exempt_path):
        for ln in open(exempt_path, encoding="utf-8", errors="replace").read().splitlines():
            ln = ln.strip()
            if ln and not ln.startswith("#"):
                exempt.add(ln)
    return exempt


def main():
    ap = argparse.ArgumentParser(description="幻影测试扫描门禁（门禁 30）")
    ap.add_argument("--only", default=None, help="只跑指定规则，逗号分隔（如 P1,P3）")
    ap.add_argument("--verbose", action="store_true", help="打印每个文件的扫描进度")
    ap.add_argument("--exempt-file", default=os.path.join(
        ROOT, "docs", "assessment", "2026-10-27-p3-legacy-exempt.txt"),
        help="P3 存量豁免清单文件（默认 docs/assessment/2026-10-27-p3-legacy-exempt.txt）")
    ap.add_argument("--dump-p3-exempt", metavar="FILE",
                    help="输出当前全部 P3 命中到指定文件（供初始化存量豁免基线），然后退出")
    args = ap.parse_args()

    only = set(args.only.split(",")) if args.only else set(RULES)
    for r in only:
        if r not in RULES:
            print(f"❌ 未知规则: {r}")
            return 2

    print("=" * 60)
    print("  幻影测试扫描门禁（门禁 30）")
    print("=" * 60)
    hard_hits = []
    soft_hits = []
    scanned = 0
    for path in iter_rs_files():
        scanned += 1
        hits = scan_file(path)
        for rule, ln, msg in hits:
            if rule not in only:
                continue
            entry = (rule, path, ln, msg)
            if RULES[rule]["hard"]:
                hard_hits.append(entry)
            else:
                soft_hits.append(entry)
    print(f"  扫描文件数: {scanned}")

    # --dump-p3-exempt：输出 P3 命中清单（供人工确认后作为存量豁免基线）
    if args.dump_p3_exempt:
        dump_path = _safe_output_path(args.dump_p3_exempt)
        p3 = [e for e in hard_hits if e[0] == "P3"]
        with open(dump_path, "w", encoding="utf-8", newline="\n") as f:
            f.write("# 2026-10-27 P3 存量零断言测试豁免基线（自动生成，供人工确认）\n")
            f.write("# 格式: <相对路径>:<函数名>\n")
            for rule, path, ln, msg in sorted(p3, key=lambda e: e[1]):
                rel = os.path.relpath(path, ROOT).replace("\\", "/")
                fn = msg.split("`")[1] if "`" in msg else "?"
                f.write(f"{rel}:{fn}\n")
        print(f"  → 已输出 {len(p3)} 条 P3 命中到 {dump_path}")
        return 0

    # 加载 P3 存量豁免清单（仅对 P3 生效）
    p3_exempt = load_p3_exempt(args.exempt_file)
    exempted_count = 0
    if p3_exempt:
        kept = []
        for e in hard_hits:
            rule, path, ln, msg = e
            if rule == "P3":
                rel = os.path.relpath(path, ROOT).replace("\\", "/")
                fn = msg.split("`")[1] if "`" in msg else "?"
                if f"{rel}:{fn}" in p3_exempt:
                    exempted_count += 1
                    continue
            kept.append(e)
        hard_hits = kept

    print()
    if hard_hits:
        print("  ❌ 硬规则命中：")
        for rule, path, ln, msg in hard_hits:
            rel = os.path.relpath(path, ROOT).replace("\\", "/")
            print(f"    [{rule}] {rel}:{ln}  {msg}")
    if soft_hits:
        shown = soft_hits[:MAX_SOFT_PRINT]
        print(f"  ⚠️  软规则命中（提示，共 {len(soft_hits)} 条，显示前 {len(shown)} 条）：")
        for rule, path, ln, msg in shown:
            rel = os.path.relpath(path, ROOT).replace("\\", "/")
            print(f"    [{rule}] {rel}:{ln}  {msg}")
        if len(soft_hits) > MAX_SOFT_PRINT:
            print(f"    ... 其余 {len(soft_hits) - MAX_SOFT_PRINT} 条")
    print()
    print("=" * 60)
    if hard_hits:
        print(f"  结果: ❌ 硬规则命中 {len(hard_hits)} 处（幻影测试）")
        if exempted_count:
            print(f"  （P3 存量豁免 {exempted_count} 条，见 {args.exempt_file}）")
        print("=" * 60)
        return 1
    print(f"  结果: ✅ 通过（硬规则 0 命中；软规则 {len(soft_hits)} 条提示）")
    if exempted_count:
        print(f"  （P3 存量豁免 {exempted_count} 条，见 {args.exempt_file}）")
    print("=" * 60)
    return 0


if __name__ == "__main__":
    sys.exit(main())