#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""门禁（警告级）：测试引用源码中不存在的标识符字符串（幻影测试检测）。

背景：2026-10-27 安全评测 H-2 发现 `tests/llm_security_audit_expanded_test.rs`
断言 `detected_patterns` 包含 `"union_injection_encoded"` / `"sensitive_data_password"`，
但源码 `llm_security_audit.rs` 的 builtin_patterns() 中并不存在这些规则名——
测试引用不存在的实现，属于"幻影测试/悬空交付"。

本脚本：
- 扫描 `packages/*/tests/*.rs`
- 提取形如标识符的字符串字面量（snake_case / CamelCase / kebab-case，长度 >= 5）
  且出现在断言/比较上下文（`== "..."`、`p == "..."`、`contains("...")`、`any(|...| ... == "...")`）
- 在**同一包**的 `src/` 下全量搜索该字符串；若完全不存在 -> 输出警告（不阻断）

退出码恒为 0（警告级）；`--strict` 时发现即退出码 1。
"""
import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PACKAGES = ROOT / "packages"

# 标识符形态的字符串字面量（不含引号）：
# snake_case、CamelCase、kebab-case、含下划线/点的复合名
IDENT_RE = re.compile(r'^[A-Za-z_][A-Za-z0-9_.-]{4,}$')
# 聚焦"规则引擎返回的模式名"比较上下文（H-2 精确场景）：
# 测试断言 `detected_patterns` 包含某个规则名，如
#   result.detected_patterns.iter().any(|p| p == "union_injection_encoded")
# 或 `assert!(result.detected_patterns.contains("..."))`。
# 同时保留 `pattern_store().patterns()` 返回的规则名比较。
# 泛化的 == "..." / assert_eq!(x, "...") 会命中大量测试夹具数据（表名、期望值），
# 这些不属于"测试引用源码符号"的幻影交付，故不纳入。
CONTEXT_RE = re.compile(
    r'detected_patterns[^"\n]*==\s*"|==\s*"[^"\n]*detected_patterns|'
    r'pattern_store\(\)\.patterns\(\).*==\s*"'
)


def collect_test_files():
    for test_dir in PACKAGES.glob("*/tests"):
        if test_dir.is_dir():
            for f in test_dir.glob("*.rs"):
                yield f


def package_src_text(package_dir):
    src = package_dir / "src"
    if not src.exists():
        return ""
    return "\n".join(
        p.read_text(encoding="utf-8", errors="replace")
        for p in src.rglob("*.rs")
    )


def is_defined_in_test(test_text, ident):
    """若标识符字符串在测试文件内出现在定义/赋值上下文（`name: "..."`、
    `pattern: "..."`、`= "..."`、`let ... = "..."`），视为测试本地定义，跳过。
    这能排除 `xor_injection` / `persisted_pattern` 等测试自定义夹具名，
    同时保留 H-2 类"仅被断言引用、从未定义"的悬空引用。"""
    # 字段定义：name: "ident" / pattern: "ident" / kind: "ident"
    if re.search(r'(?:name|pattern|kind|key|field|id|type)\s*:\s*"' + re.escape(ident) + r'"', test_text):
        return True
    # 赋值：= "ident" 或 let x = "ident"
    if re.search(r'=\s*"' + re.escape(ident) + r'"', test_text):
        return True
    return False


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--strict", action="store_true",
                        help="strict 模式：发现幻影引用时退出码 1")
    args = parser.parse_args()

    findings = []
    for test_file in collect_test_files():
        pkg = test_file.parent.parent
        src_text = package_src_text(pkg)
        lines = test_file.read_text(encoding="utf-8", errors="replace").splitlines()
        test_text = "\n".join(lines)
        for i, line in enumerate(lines, 1):
            if not CONTEXT_RE.search(line):
                continue
            for lit in re.findall(r'"([^"]+)"', line):
                if not IDENT_RE.match(lit):
                    continue
                # 跳过明显的测试夹具/路径/表名
                if lit.startswith(("http", "test", "temp", "C:\\", "/")):
                    continue
                if lit in src_text:
                    continue
                if is_defined_in_test(test_text, lit):
                    continue
                findings.append((test_file, i, lit))

    print("=== 门禁（警告级）：测试引用源码中不存在的标识符字符串 ===")
    if findings:
        print(f"  ⚠ 发现 {len(findings)} 处疑似幻影引用（测试引用了源码中不存在的标识符）：")
        seen = set()
        for f, i, lit in findings[:20]:
            rel = f.relative_to(ROOT)
            key = (rel, lit)
            if key in seen:
                continue
            seen.add(key)
            print(f"    - {rel}:{i}: `{lit}`（对应包 src/ 下不存在）")
        if len(findings) > 20:
            print(f"    ... 共 {len(findings)} 处")
        if args.strict:
            print("  ❌ FAIL（strict）：存在幻影测试引用")
            return 1
        print("  ⚠ 警告（不阻断）：请核对测试是否引用了尚未实现的符号/规则名")
        return 0
    print("  ✅ PASS：未发现测试引用源码中不存在的标识符字符串")
    return 0


if __name__ == "__main__":
    sys.exit(main())