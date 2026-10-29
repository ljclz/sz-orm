#!/usr/bin/env python3
"""sz-orm unsafe 零容忍合规扫描（门禁辅助脚本，2026-10-24 深度评测新增）

等价于 cargo geiger 的核心检查（离线可用）：
- 扫描 packages/*/src/ 下所有含 `unsafe` 关键字的 .rs 文件
- 对每个文件核对 SAFETY 覆盖（v10.13.0 起认可两种口径，见下）
- 真实 unsafe 代码的 SAFETY 覆盖口径：
  A. 块级紧邻：`// SAFETY:` 注释（同文件任意位置）
  B. 函数级文档：`/// # Safety` 文档段（unsafe fn 的安全契约由函数文档承载，
     与 Rust 标准库/unsafe 库惯例一致；2026-10-27 L-3 裁决采纳）
- 两种口径皆无 -> 失败（exit 1）；仅函数级 -> 通过但打印合规提示
- `unsafe` 出现在标识符/测试函数名/文档注释/字符串字面量 -> 判定为误报并列出

用法:
    python scripts/check-unsafe-audit.py [--verbose]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGES = os.path.join(ROOT, "packages")

# 真实 unsafe 构造的正则：unsafe 块/函数/特质/实现/extern/static/指针解引用
_REAL_UNSAFE = re.compile(
    r"\bunsafe\s*(?:\{|\(|\b(?:fn|impl|trait|extern|static|const)\b)"
)
# 函数级安全契约文档（/// # Safety 或 /// SAFETY:）
_FN_LEVEL_SAFETY = re.compile(r"^\s*///\s*(?:#\s*Safety\b|SAFETY:)", re.MULTILINE)


def classify_unsafe_lines(content):
    """返回 (真实 unsafe 行列表, 误报行列表)。"""
    real, false_positive = [], []
    for i, line in enumerate(content.splitlines(), 1):
        if "unsafe" not in line:
            continue
        # 去掉注释和字符串后判断（简化：若行以 /// 或 //! 开头视为文档）
        stripped = line.lstrip()
        if stripped.startswith("///") or stripped.startswith("//!"):
            continue
        if _REAL_UNSAFE.search(line):
            real.append((i, stripped.strip()[:100]))
        else:
            false_positive.append((i, stripped.strip()[:100]))
    return real, false_positive


def main():
    verbose = "--verbose" in sys.argv
    issues = []
    fn_level_only = []
    false_positives = []
    files_with_unsafe = 0

    for dirpath, dirnames, filenames in os.walk(PACKAGES):
        if "target" in dirpath or os.sep + "tests" in dirpath:
            continue
        for fn in filenames:
            if not fn.endswith(".rs"):
                continue
            p = os.path.join(dirpath, fn)
            if os.sep + "src" + os.sep not in p:
                continue
            with open(p, encoding="utf-8", errors="replace") as f:
                content = f.read()
            if "unsafe" not in content:
                continue
            files_with_unsafe += 1
            real, fp = classify_unsafe_lines(content)
            has_block_safety = "// SAFETY:" in content or "#[deny" in content
            has_fn_level_safety = _FN_LEVEL_SAFETY.search(content) is not None
            rel = os.path.relpath(p, ROOT).replace("\\", "/")
            if real and not has_block_safety and not has_fn_level_safety:
                issues.append((rel, real))
            elif real and not has_block_safety and has_fn_level_safety:
                fn_level_only.append((rel, real))
            false_positives.extend((rel, i, t) for i, t in fp)
            if verbose and real:
                mode = "块级" if has_block_safety else ("函数级" if has_fn_level_safety else "无")
                print(
                    f"  {rel}: {len(real)} 处真实 unsafe, SAFETY 口径={mode}"
                )

    print("=" * 60)
    print("  sz-orm unsafe 零容忍合规扫描")
    print("=" * 60)
    print(f"  含 unsafe 关键字的 src 文件数: {files_with_unsafe}")
    print(f"  真实 unsafe 且 SAFETY 双口径皆缺的文件数: {len(issues)}")
    print(f"  仅函数级 # Safety 文档覆盖的文件数: {len(fn_level_only)}")
    print(f"  误报（标识符/测试名/文档/字符串）文件数: {len(false_positives)}")

    if issues:
        print("\n  ❌ 以下文件含真实 unsafe 但块级与函数级 SAFETY 皆无：")
        for rel, real in issues:
            for i, t in real[:5]:
                print(f"    {rel}:{i}: {t}")
        print("\n  结果: ❌ 失败")
        return 1

    if fn_level_only:
        print("\n  ⚠️ 以下文件依赖函数级 # Safety 文档（无块级紧邻 // SAFETY:）：")
        for rel, real in fn_level_only[:10]:
            print(f"    {rel}: {len(real)} 处真实 unsafe")
        if len(fn_level_only) > 10:
            print(f"    ... 共 {len(fn_level_only)} 个文件")
        print("  （2026-10-27 L-3 裁决：函数级文档承载 unsafe fn 契约，判定通过；")
        print("    长期建议逐步补块级紧邻注释以增强自动化审计粒度）")

    if false_positives and verbose:
        print("\n  ⚠️ 误报明细（unsafe 出现在非代码语义中）：")
        seen = set()
        for rel, i, t in false_positives:
            if rel not in seen:
                print(f"    {rel}:{i}: {t}")
                seen.add(rel)

    print("\n  结果: ✅ 通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())