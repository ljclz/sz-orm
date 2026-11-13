#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""门禁 37：dialect.rs DDL/FTS 生成面标识符裸拼静态检测（2026-11-12 ALTER INDEX/FK
quote 批二 §7 遗留项 2 落地）。

背景：quote 类同型缺口在 5 个批次被真连矩阵+红队先后独立发现（G-OR1 AddIndex /
MY G2 AddIndex+FK / G-OR9 CONTAINS / FTS 横向批 11 方言 / ALTER 批二 8 方言），
另有红队 R-2 深挖出 Informix/SapHana/Firebird FK 臂「ADD CONSTRAINT FOREIGN KEY」
无名非法 DDL 变体。本脚本把人工横向扫描固化为机器门禁。

规则（对 packages/sz-orm-core/src/dialect.rs 全量 format! 调用块）：
- Q1 无名约束（精确）：模板含 `ADD CONSTRAINT FOREIGN KEY`（CONSTRAINT 后直接
  FOREIGN KEY，缺约束名）→ 红牌。非法 DDL 变体，零容忍。
- Q2 敏感模板裸参数：模板含敏感关键词（CREATE INDEX / DROP INDEX / ADD CONSTRAINT /
  FOREIGN KEY / REFERENCES / CONTAINS( / CONTAINING( / MATCH( / to_tsvector( /
  position( / ILIKE / LIKE '%）时，块内「裸标识符参数」（独立变量或 .join 表达式，
  未经 self.quote / escape_string 包裹、赋值链不含 quote/escape_string）→ 红牌。

白名单（判定为安全的参数形态）：
- 含 `quote(` 的表达式（self.quote(&fk_name) 等）
- 含 `escape_string(` 的表达式（关键字/值转义）
- 字符串/数字/bool 字面量及其 .to_string() 链
- 安全 let 变量：文件内赋值表达式含 quote( 或 escape_string( 的变量
  （quoted_cols / escaped / cols 构造链等）
- 方法调用参数（含 `(` 的表达式，保守放行避免误报——历史缺口形态均为裸标识符）

用法：
    python scripts/check-dialect-ddl-quoting.py            # 扫描，命中即 EXIT=1
    python scripts/check-dialect-ddl-quoting.py --self-test # 内置样例自测
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

TARGET = "packages/sz-orm-core/src/dialect.rs"

# 敏感模板关键词（DDL 索引/约束面 + FTS 函数面 + JSON 函数面）
SENSITIVE_PATTERNS = [
    r"CREATE\s+INDEX",
    r"DROP\s+INDEX",
    r"ADD\s+CONSTRAINT",
    r"FOREIGN\s+KEY",
    r"REFERENCES\s+",
    r"CONTAINS\s*\(",
    r"CONTAINING\s*\(",
    r"MATCH\s*\(",
    r"to_tsvector\s*\(",
    r"position\s*\(",
    r"ILIKE",
    r"LIKE\s*'",
    # json_extract quote 横向修复批（2026-11-13，G-OR1 家族第七形态后增补）：
    # JSON 函数面——PG 批红队 §3.3 实证 json_extract 列名裸拼为 G37 盲区
    r"JSON_EXTRACT\s*\(",
    r"json_extract\s*\(",
    r"JSON_VALUE\s*\(",
    r"JSONExtract",
    r"JSON_GET\s*\(",
    r"#>>",
    # 红队 P2-3 增补两臂（Snowflake `{}:{}` / DuckDB `{} -> ` 模板字面形态；
    # 泛关键词 `->`/`:` 会误报普通 Rust 代码，故锚定 format! 模板形态）
    r"\{\}\s*:\s*\{\}",
    r"\{\}\s*->",
]

# Q1：无名约束（非法 DDL 变体，零容忍）
RULE_NAMELESS_CONSTRAINT = re.compile(
    r"ADD\s+CONSTRAINT\s+FOREIGN\s+KEY", re.IGNORECASE
)

# 裸标识符参数：独立变量（可选 .as_str()）或 <var>.join(...) 形态
BARE_IDENT_PARAM = re.compile(
    r"^(?:&?\s*[a-z_][a-z0-9_]*(?:\.as_str\(\))?|[a-z_][a-z0-9_]*\.join\([^)]*\))\s*$",
    re.IGNORECASE,
)


def extract_format_blocks(source: str) -> list[tuple[int, str, str]]:
    """提取所有 format!(...) 调用块 → [(起始行号, 模板串, 参数区文本)]。

    简化解析：format!( 后的第一个字符串字面量为模板；其后到平衡收尾 `)` 为参数区。
    支持模板内的 \\ 续行与 {{}} 转义。仅服务本文件的规则需求，非通用 Rust 解析器。
    """
    blocks: list[tuple[int, str, str]] = []
    for m in re.finditer(r"format!\s*\(", source):
        start = m.end()
        # 第一个字符串字面量（支持 \" 转义）
        sm = re.match(r'\s*(("(?:[^"\\]|\\.)*"))', source[start:])
        if not sm:
            continue
        template = sm.group(1)
        pos = start + sm.end()
        # 平衡括号提取参数区
        depth = 1
        i = pos
        while i < len(source) and depth > 0:
            ch = source[i]
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
            elif ch == '"':
                # 跳过字符串字面量（参数区可能含内嵌字符串）
                i += 1
                while i < len(source) and source[i] != '"':
                    if source[i] == "\\":
                        i += 1
                    i += 1
            i += 1
        params_text = source[pos : i - 1] if depth == 0 else source[pos:i]
        line_no = source.count("\n", 0, m.start()) + 1
        blocks.append((line_no, template, params_text))
    return blocks


def build_safe_var_set(source: str) -> set[str]:
    """收集「赋值链含 quote(/escape_string(」的 let 变量名（视为已 quote 安全）。"""
    safe: set[str] = set()
    for m in re.finditer(
        r"let\s+(?:mut\s+)?([a-z_][a-z0-9_]*)\s*(?::[^=]+)?=\s*([^;]{0,400});",
        source,
    ):
        var, rhs = m.group(1), m.group(2)
        if "quote(" in rhs or "escape_string(" in rhs:
            safe.add(var)
    return safe


def bare_params_in_block(
    params_text: str, safe_vars: set[str]
) -> list[str]:
    """返回参数区中的裸标识符参数列表（判危险）。"""
    hits = []
    # 按顶层逗号切分参数（简化：直接按逗号切，宏调用参数在此文件极少嵌套逗号歧义；
    # quote(...)/escape_string(...) 内逗号会被切开——但切出的片段含 `self.quote(`/
    # `escape_string(` 前缀的残片不含裸标识符首段，用整段重建法避免：
    # 先抹掉 quote(...)/escape_string(...) 括号内容再切分。
    masked = re.sub(
        r"(?:self\.)?quote\([^()]*\)", "QUOTE_OK", params_text
    )
    masked = re.sub(
        r"(?:self\.)?escape_string\([^()]*\)", "ESCAPE_OK", masked
    )
    for raw in masked.split(","):
        seg = raw.strip()
        if not seg or seg in ("QUOTE_OK", "ESCAPE_OK"):
            continue
        # 还原：若片段含被抹掉标记则安全
        if "QUOTE_OK" in seg or "ESCAPE_OK" in seg:
            continue
        seg_clean = seg.strip()
        if BARE_IDENT_PARAM.match(seg_clean):
            ident = seg_clean.split(".")[0].lstrip("&").strip()
            if ident in safe_vars:
                continue
            hits.append(seg_clean)
    return hits


def strip_comments(source: str) -> str:
    """剥离 // 与 /* */ 注释但**保留字符串字面量内容**（Q1 目标形态在字符串内；
    行数对齐保留——以空格回填被剥区段）。"""
    out: list[str] = []
    i = 0
    n = len(source)
    in_str = False
    while i < n:
        ch = source[i]
        if in_str:
            out.append(ch)
            if ch == "\\" and i + 1 < n:
                out.append(source[i + 1])
                i += 2
                continue
            if ch == '"':
                in_str = False
            i += 1
            continue
        if ch == '"':
            in_str = True
            out.append(ch)
            i += 1
            continue
        if ch == "/" and i + 1 < n and source[i + 1] == "/":
            # 行注释：剥到行尾（保留换行）
            j = source.find("\n", i)
            if j == -1:
                j = n
            out.append(" " * (j - i))
            i = j
            continue
        if ch == "/" and i + 1 < n and source[i + 1] == "*":
            j = source.find("*/", i + 2)
            j = n if j == -1 else j + 2
            seg = source[i:j]
            out.append("".join(c if c == "\n" else " " for c in seg))
            i = j
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def scan(source: str) -> list[tuple[int, str, str]]:
    """返回 [(行号, 规则, 命中详情)]。"""
    findings: list[tuple[int, str, str]] = []
    safe_vars = build_safe_var_set(source)
    code_only_lines = strip_comments(source).splitlines()

    # Q1：无名约束（逐行，零容忍；剥离注释——R-2 修复注释引用的字样不误报）
    for i, line in enumerate(code_only_lines, 1):
        if RULE_NAMELESS_CONSTRAINT.search(line):
            findings.append(
                (i, "Q1-nameless-constraint", line.strip()[:120])
            )

    # Q2：敏感模板裸参数（块落于注释内跳过；块含 quote-gate-safe 豁免标记跳过）
    for line_no, template, params_text in extract_format_blocks(source):
        flat = template.replace("\\\n", "")
        if not any(re.search(p, flat) for p in SENSITIVE_PATTERNS):
            continue
        line_code = (
            code_only_lines[line_no - 1]
            if line_no <= len(code_only_lines)
            else ""
        )
        if "format!(" not in line_code:
            continue  # 块起始行在注释内（如注释中的样例模板）
        # 行内豁免登记：标记可在 format! 行或其上方 2 行的注释中
        # （白名单校验值 / 常量 match 值；同 G15/G17 豁免语义）
        ctx = "\n".join(
            source.splitlines()[max(0, line_no - 3) : line_no]
        )
        if "quote-gate-safe" in ctx:
            continue
        for bare in bare_params_in_block(params_text, safe_vars):
            findings.append(
                (
                    line_no,
                    "Q2-sensitive-bare-param",
                    f"bare param `{bare}` in sensitive template {flat[:80]}...",
                )
            )
    return findings


def self_test() -> int:
    """内置样例自测：历史缺口形态必须命中、已修复形态必须不命中。"""
    cases_pass = 0
    cases_fail = 0

    # 命中样例（历史缺口形态，G-OR1/MY G2/批二/R-2）
    hit_samples = [
        # G-OR1：AddIndex 裸拼
        'format!("CREATE INDEX {} ON {} ({})", name, self.quote(table), cols.join(", "))',
        # MY G2：FK 约束名裸拼
        'format!("ALTER TABLE {} ADD CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {} ({})", self.quote(table), table, columns.join("_"))',
        # R-2：无名约束
        'format!("ALTER TABLE {} ADD CONSTRAINT FOREIGN KEY ({}) REFERENCES {} ({})", self.quote(table), cols.join(", "))',
        # G-OR9：CONTAINS 列裸拼
        '.map(|c| format!("CONTAINS({}, \'{}\', 1) > 0", c, escaped))',
        # FTS：LIKE 列裸拼
        '.map(|c| format!("{} LIKE \'%{}%\'", c, escaped))',
        # json_extract quote 批（2026-11-13 增补 JSON 函数面）：JSON_EXTRACT 列裸拼
        'format!("JSON_EXTRACT({}, \'{}\')", column, self.escape_string(&normalized))',
        # json_extract quote 批：JSON_VALUE 列裸拼
        'format!("JSON_VALUE({}, \'{}\')", column, self.escape_string(&normalized))',
        # json_extract quote 批：#>> 列裸拼（PG 形态）
        'format!("{}#>>\'{{{}}}\'", column, path_lit)',
        # json_extract quote 批（红队 P2-3）：Snowflake 裸拼臂
        'format!("{}:{}", column, normalized)',
        # json_extract quote 批（红队 P2-3）：DuckDB 裸拼臂
        'format!("{} -> \'{}\'", column, normalized)',
    ]
    for sample in hit_samples:
        if scan(sample):
            cases_pass += 1
        else:
            print(f"SELF-TEST FAIL（应命中未命中）: {sample[:80]}")
            cases_fail += 1

    # 不命中样例（已修复形态；带 let 赋值上下文——安全变量判定依赖赋值链）
    safe_samples = [
        # 批二修复形态（quoted_cols 赋值链含 quote）
        'let quoted_cols = cols.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", ");\nformat!("CREATE INDEX {} ON {} ({})", self.quote(name), self.quote(table), quoted_cols)',
        # FK 约束名 quote（cols 赋值链含 quote）
        'let cols = columns.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", ");\nlet fk_name = format!("fk_{}_{}", table, columns.join("_"));\nformat!("ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({})", self.quote(table), self.quote(&fk_name), cols)',
        # G-OR9 修复形态（escaped 赋值链含 escape_string）
        'let escaped = self.escape_string(keyword);\n.map(|c| format!("CONTAINS({}, \'{}\', 1) > 0", self.quote(c), escaped))',
        # 无名约束修复形态
        'let cols = columns.iter().map(|c| self.quote(c)).collect::<Vec<_>>().join(", ");\nlet fk_name = format!("fk_{}_{}", table, columns.join("_"));\nformat!("ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({})", self.quote(table), self.quote(&fk_name), cols)',
        # 非敏感模板不受限（cols 裸也不命中——模板无敏感关键词）
        'format!("INSERT INTO {} ({}) VALUES (:1, :2)", self.quote(table), cols)',
        # json_extract quote 批修复形态（列名走 self.quote）
        'format!("JSON_EXTRACT({}, \'{}\')", self.quote(column), self.escape_string(&normalized))',
        # json_extract quote 批修复形态（PG #>> + 路径字面量豁免登记）
        '// quote-gate-safe: 列名已走 self.quote；path_lit_escaped 为路径字面量（非标识符）\nformat!("{}#>>\'{{{}}}\'", self.quote(column), path_lit_escaped)',
        # json_extract quote 批修复形态（红队 P2-3 两臂 quote 后 + 路径片段豁免登记）
        '// quote-gate-safe: normalized 为路径片段（非标识符），列名已走 self.quote\nformat!("{}:{}", self.quote(column), normalized)',
        '// quote-gate-safe: normalized 为路径片段（非标识符），列名已走 self.quote\nformat!("{} -> \'{}\'", self.quote(column), normalized)',
    ]
    for sample in safe_samples:
        if scan(sample):
            print(f"SELF-TEST FAIL（应不命中却命中）: {sample[:80]}")
            cases_fail += 1
        else:
            cases_pass += 1

    print(f"self-test: {cases_pass} passed / {cases_fail} failed")
    return 0 if cases_fail == 0 else 1


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    repo = Path(__file__).resolve().parent.parent
    target = repo / TARGET
    if not target.exists():
        print(f"门禁 37：目标文件不存在 {target}")
        return 1
    source = target.read_text(encoding="utf-8")
    findings = scan(source)

    print("=== 门禁 37：dialect.rs DDL/FTS 标识符裸拼静态检测 ===")
    if findings:
        for line_no, rule, detail in findings:
            print(f"  ❌ {TARGET}:{line_no} [{rule}] {detail}")
        print(f"  命中 {len(findings)} 处")
        return 1
    print("  ✅ 0 命中（12 方言 DDL/FTS 生成面 quote 一致化保持）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
