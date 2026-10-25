#!/usr/bin/env python3
"""sz-orm unsafe 零容忍合规扫描（门禁辅助脚本，2026-10-24 深度评测新增）

等价于 cargo geiger 的核心检查（离线可用）：
- 扫描 packages/*/src/ 下所有含 `unsafe` 关键字的 .rs 文件
- 对每个文件核对 SAFETY 注释覆盖
- 若文件中存在真实 unsafe 代码块但没有任何 `// SAFETY:` 注释 -> 失败（exit 1）
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
            has_safety = "SAFETY" in content
            rel = os.path.relpath(p, ROOT).replace("\\", "/")
            if real and not has_safety:
                issues.append((rel, real))
            false_positives.extend((rel, i, t) for i, t in fp)
            if verbose and real:
                print(
                    f"  {rel}: {len(real)} 处真实 unsafe, "
                    f"SAFETY={'是' if has_safety else '否'}"
                )

    print("=" * 60)
    print("  sz-orm unsafe 零容忍合规扫描")
    print("=" * 60)
    print(f"  含 unsafe 关键字的 src 文件数: {files_with_unsafe}")
    print(f"  真实 unsafe 且缺 SAFETY 的文件数: {len(issues)}")
    print(f"  误报（标识符/测试名/文档/字符串）文件数: {len(false_positives)}")

    if issues:
        print("\n  ❌ 以下文件含真实 unsafe 但无 SAFETY 注释：")
        for rel, real in issues:
            for i, t in real[:5]:
                print(f"    {rel}:{i}: {t}")
        print("\n  结果: ❌ 失败")
        return 1

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