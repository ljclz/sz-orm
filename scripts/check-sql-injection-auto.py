#!/usr/bin/env python3
"""SQL Injection Automated Review Engine (Gate 9).

Automatically judges potential SQL injection risks using 7 rules:
  R1: format! WHERE with user input via {} placeholder -> Unsafe
  R2: format! WHERE with column/table name placeholder -> Safe
  R3: format! WHERE with ? placeholder or named params -> Safe
  R4: format! WHERE with constant/literal -> Safe
  R5: Cannot determine -> ManualReview
  R6: ${} interpolation in dynamic SQL without whitelist -> Unsafe
  R7: ${} interpolation with identifier_whitelist validation -> Safe

Usage:
  python scripts/check-sql-injection-auto.py --workspace . --json --strict
  python scripts/check-sql-injection-auto.py --strict
"""

import argparse
import json
import os
import re
import sys
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path


class JudgeResult(Enum):
    SAFE = "Safe"
    UNSAFE = "Unsafe"
    MANUAL_REVIEW = "ManualReview"


@dataclass
class InjectionItem:
    file: str
    line_num: int
    line_content: str
    category: str  # "deprecated", "format_concat", "dollar_interpolation"
    judge: str = ""
    reason: str = ""


@dataclass
class ScanReport:
    total: int = 0
    safe: int = 0
    unsafe: int = 0
    manual_review: int = 0
    items: list = field(default_factory=list)


def is_test_file(file_path: str) -> bool:
    """Check if file is a test file."""
    parts = file_path.replace("\\", "/").lower()
    return "/tests/" in parts or parts.endswith("_test.rs") or "test_" in os.path.basename(parts)


def is_safety_comment(line: str) -> bool:
    """Check if line has SAFETY comment indicating non-SQL context."""
    return "SAFETY" in line and ("非 SQL" in line or "not SQL" in line.lower() or "错误消息" in line or "报告" in line)


def _split_top_level(s: str):
    """按顶层逗号切分（引号与嵌套括号内不切分）。"""
    parts, buf = [], []
    depth = 0
    quote = None
    i = 0
    while i < len(s):
        ch = s[i]
        if quote:
            buf.append(ch)
            if ch == '\\' and i + 1 < len(s):
                buf.append(s[i + 1])
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if ch in ('"', "'"):
            quote = ch
            buf.append(ch)
            i += 1
            continue
        if ch in '([{':
            depth += 1
        elif ch in ')]}':
            depth -= 1
        if ch == ',' and depth == 0:
            parts.append(''.join(buf))
            buf = []
            i += 1
            continue
        buf.append(ch)
        i += 1
    if buf:
        parts.append(''.join(buf))
    return parts


def _extract_format_args(line: str):
    """从单行中提取 format!(...) 的位置参数表达式列表。

    按括号配对定位 format! 自身的收尾 ')'，容忍外层包裹
    （如 ``sql.push_str(&format!(" WHERE id={}", input));``）。

    返回值：
    - None：行内没有 format! 调用，或格式串中没有 ``{}`` 位置占位符
    - list[str]：位置参数表达式
    """
    idx = line.find('format!')
    if idx == -1:
        return None
    rest = line[idx + len('format!'):].lstrip()
    if not rest.startswith('('):
        return None
    depth = 0
    end = -1
    quote = None
    j = 0
    while j < len(rest):
        ch = rest[j]
        if quote:
            if ch == '\\':
                j += 2
                continue
            if ch == quote:
                quote = None
            j += 1
            continue
        if ch in ('"', "'"):
            quote = ch
            j += 1
            continue
        if ch == '(':
            depth += 1
        elif ch == ')':
            depth -= 1
            if depth == 0:
                end = j
                break
        j += 1
    if end == -1:
        return None
    inner = rest[1:end]
    if '{}' not in inner:
        return None
    parts = _split_top_level(inner)
    if len(parts) < 2:
        return []
    return [p.strip() for p in parts[1:] if p.strip()]


def judge_injection_risk(file_path: str, line_num: int, line_content: str, category: str) -> tuple:
    """Judge injection risk for a single line.

    Returns (JudgeResult, reason).
    """
    stripped = line_content.strip()

    # R4: SAFETY comment -> Safe (non-SQL context)
    if is_safety_comment(line_content):
        return JudgeResult.SAFE, "R4: SAFETY 注释标记非 SQL 执行上下文"

    # Deprecated where_cond/or_where in string literal (LSP diagnostics) -> Safe
    if category == "deprecated":
        if '"' in stripped and "diagnose(" in stripped:
            return JudgeResult.SAFE, "R4: where_cond 出现在诊断字符串字面量中，非实际 SQL 执行"
        return JudgeResult.MANUAL_REVIEW, "R5: deprecated where_cond/or_where 需人工确认"

    # Test files -> Safe (test code, not production SQL)
    if is_test_file(file_path):
        return JudgeResult.SAFE, "R4: 测试文件中的 format!，非生产 SQL 执行路径"

    # R3: Uses ? placeholder or $1/$N named params -> Safe
    if "? " in stripped or "= ?" in stripped or "IN (?)" in stripped:
        return JudgeResult.SAFE, "R3: 使用 ? 参数化占位符"

    if re.search(r'\$\d+', stripped):
        return JudgeResult.SAFE, "R3: 使用 $N 命名参数占位符"

    # R2: Column/table name placeholders -> Safe
    # Patterns: {table}, {table_q}, {pk_q}, {fk}, {pk_col}, {alias}, {filter}, {where_clause}, {w}
    table_col_patterns = [
        r'\{table\}', r'\{table_q\}', r'\{pk_q\}', r'\{fk\}', r'\{fk_key\}',
        r'\{pk_col\}', r'\{alias\}', r'\{filter\}', r'\{where_clause\}',
        r'\{w\}', r'\{combined\}', r'\{self\.sql\}', r'\{child_table\}',
        r'\{self\.filter_sql\}', r'\{base\}', r'\{trimmed\}', r'\{sep\}',
        r'\{after\}', r'\{n\}', r'\{end\}',
    ]
    for pattern in table_col_patterns:
        if re.search(pattern, stripped):
            return JudgeResult.SAFE, f"R2: 表名/列名/WHERE 子句占位符 ({pattern})"

    # R2: Positional args {} with safe variable names in format! args
    safe_var_names = {
        'table', 'table_q', 'pk_q', 'fk', 'fk_key', 'pk_col', 'alias', 'filter',
        'where_clause', 'w', 'combined', 'child_table', 'base', 'trimmed', 'sep',
        'after', 'n', 'end', 'group_strs', 'all_conds', 'clauses', 'conditions',
        'filter_sql', 'sql',
    }
    fmt_match = re.search(r'format!\s*\((.*?)(?:,\s*(.*?))?\)\s*;?\s*$', stripped, re.DOTALL)
    if fmt_match and '{}' in fmt_match.group(1):
        args_part = fmt_match.group(2) if fmt_match.group(2) else ""
        args = [a.strip().rstrip(',') for a in args_part.split(',') if a.strip()]
        for arg in args:
            arg_clean = arg.replace('self.', '').strip()
            if arg_clean in safe_var_names:
                return JudgeResult.SAFE, f"R2: 位置参数绑定安全变量名 ({arg_clean})"
            if arg_clean.startswith('self.') and arg_clean.split('.')[-1] in safe_var_names:
                return JudgeResult.SAFE, f"R2: 位置参数绑定安全成员变量 ({arg_clean})"

    # R4: Security module test vectors -> Safe
    security_modules = ['sanitizer', 'sec_scanner', 'injection_guard', 'diagnostics']
    if any(x in file_path.replace("\\", "/").lower() for x in security_modules):
        return JudgeResult.SAFE, "R4: 安全功能模块中的测试向量/诊断文本（非生产 SQL 执行）"

    # R4: LSP diagnostics string literal -> Safe
    if 'diagnose(' in stripped and '"' in stripped:
        return JudgeResult.SAFE, "R4: LSP 诊断字符串字面量，非 SQL 执行"

    # R4: Audit module loop variable / keyword -> Safe
    if 'sz-orm-audit' in file_path.replace("\\", "/"):
        if re.search(r'\{i\}', stripped) or "{kw}" in stripped or "{i}" in stripped:
            return JudgeResult.SAFE, "R4: 审计模块循环变量/关键词（非用户输入）"
        fmt_args_match = re.search(r'format!\s*\(.*?,\s*(\w+)\s*\)', stripped)
        if fmt_args_match and fmt_args_match.group(1) in ('i', 'kw'):
            return JudgeResult.SAFE, f"R4: 审计模块循环变量/关键词 ({fmt_args_match.group(1)})"

    # R2: join(" AND ") / join("") patterns -> Safe (pre-built clause concatenation)
    if '.join(' in stripped and ('AND' in stripped or stripped.count('"') >= 2):
        return JudgeResult.SAFE, "R2: 预构建条件子句 join 拼接"

    # R2: ROWNUM <= N pattern -> Safe (pagination constant)
    if 'ROWNUM' in stripped:
        return JudgeResult.SAFE, "R2: ROWNUM 分页限制常量"

    # R2: Error message format (non-SQL execution)
    # v10.13.0 (M-2)：仅 Err/return Err 上下文豁免；裸 push_str 是字符串构建（可能是 SQL），
    # 不得豁免——由下方 push_str 规则按插值参数判定
    if 'Err(' in stripped or 'return Err' in stripped:
        return JudgeResult.SAFE, "R4: 错误消息拼接，非 SQL 执行"

    # R2: push_str with WHERE clause -> Safe (building SQL string with pre-built clause)
    if 'push_str' in stripped and 'WHERE' in stripped:
        # v10.13.0 (M-2)：仅当推入内容不含 {} 插值、或插值参数均为白名单变量名时判 Safe；
        # 否则降级 ManualReview（防止 push_str(&format!(" WHERE id={}", input)) 被误判 Safe）
        fmt_args = _extract_format_args(stripped)
        if fmt_args is None:
            return JudgeResult.SAFE, "R2: push_str 拼接预构建 WHERE 子句（无插值）"
        args_clean = [a.replace('self.', '').strip().rstrip(',') for a in fmt_args]
        if args_clean and all(a in safe_var_names for a in args_clean):
            return JudgeResult.SAFE, "R2: push_str 拼接预构建 WHERE 子句（白名单变量）"
        return JudgeResult.MANUAL_REVIEW, "R5: push_str + format! 插值参数非白名单，需人工审查"

    # R1: format! with '{}' in WHERE condition binding user input -> check context
    # If in src/ (non-test) and has '{}' in WHERE with user-like variable -> Unsafe
    if re.search(r"WHERE.*\{[^}]*\}", stripped):
        # Check if the placeholder is a simple variable (potential user input)
        placeholders = re.findall(r'\{([^}]+)\}', stripped)
        for ph in placeholders:
            # If placeholder looks like a user input variable (not table/col/clause)
            if ph in ('query', 'input', 'user_input', 'name', 'value', 'id', 'kw', 'long_token'):
                # Check if it's in a security-related module (sanitizer/scanner/guard)
                if any(x in file_path for x in ['sanitizer', 'sec_scanner', 'injection_guard', 'diagnostics']):
                    return JudgeResult.SAFE, "R4: 安全功能模块中的测试向量（验证检测能力）"
                # Check if escaped
                if 'escaped' in stripped or 'sanitiz' in stripped.lower():
                    return JudgeResult.SAFE, "R4: 已转义/消毒的输入"
                return JudgeResult.UNSAFE, f"R1: WHERE 条件中通过 {{}} 绑定用户输入 ({ph})"

    # R6/R7: ${} interpolation in dynamic SQL
    if category == "dollar_interpolation":
        # R7: whitelist validation present -> Safe
        # Check if the parser has identifier_whitelist set (look in surrounding context)
        # We check the file for with_identifier_whitelist usage
        try:
            file_lines = Path(file_path).read_text(encoding="utf-8", errors="replace").splitlines()
            has_whitelist = any("with_identifier_whitelist" in l for l in file_lines)
        except Exception:
            has_whitelist = False

        if has_whitelist:
            return JudgeResult.SAFE, "R7: ${} 插值已配置 identifier_whitelist 白名单校验"
        return JudgeResult.UNSAFE, "R6: ${} 动态 SQL 插值未配置白名单，存在 SQL 注入风险"

    # R5: Cannot determine -> ManualReview
    return JudgeResult.MANUAL_REVIEW, "R5: 无法自动判定，需人工审查"


def scan_workspace(root: str) -> ScanReport:
    """Scan workspace for SQL injection risks."""
    report = ScanReport()
    root_path = Path(root)

    scan_dirs = [root_path / "packages", root_path / "examples", root_path / "cli"]

    for scan_dir in scan_dirs:
        if not scan_dir.exists():
            continue
        for rs_file in scan_dir.rglob("*.rs"):
            try:
                lines = rs_file.read_text(encoding="utf-8", errors="replace").splitlines()
            except Exception:
                continue

            # v10.13.0 (M-3)：`#[cfg(test)]` 之后的行视为测试上下文（src 内联测试模块），
            # 与 tests/ 目录同等对待——测试代码不是生产 SQL 执行路径。
            in_test_module = False

            for i, line in enumerate(lines):
                stripped = line.strip()
                if stripped.startswith("#[cfg(test)]"):
                    in_test_module = True
                if stripped.startswith("//") or stripped.startswith("!"):
                    continue

                is_test_context = is_test_file(str(rs_file)) or in_test_module

                # Check deprecated where_cond/or_where
                if re.search(r'\.where_cond\(|\.or_where\(', line) and "test" not in rs_file.name:
                    item = InjectionItem(
                        file=str(rs_file),
                        line_num=i + 1,
                        line_content=line.rstrip(),
                        category="deprecated",
                    )
                    judge, reason = judge_injection_risk(item.file, item.line_num, item.line_content, "deprecated")
                    item.judge = judge.value
                    item.reason = reason
                    report.items.append(item)

                # Check format! WHERE with {}
                if re.search(r'format!.*WHERE.*\{.*\}', line) and not re.search(
                    r'columns\[|conditions\[|tables\[|updates\[', line
                ):
                    if not is_test_context:
                        item = InjectionItem(
                            file=str(rs_file),
                            line_num=i + 1,
                            line_content=line.rstrip(),
                            category="format_concat",
                        )
                        judge, reason = judge_injection_risk(item.file, item.line_num, item.line_content, "format_concat")
                        item.judge = judge.value
                        item.reason = reason
                        report.items.append(item)

                # R6/R7: Check ${} interpolation in dynamic SQL (non-test files only)
                if not is_test_context:
                    if re.search(r'\$\{[a-zA-Z_]\w*\}', line) and 'push_str' not in stripped and '//' not in stripped.split('${')[0] and 'assert' not in stripped and '#[test]' not in stripped:
                        item = InjectionItem(
                            file=str(rs_file),
                            line_num=i + 1,
                            line_content=line.rstrip(),
                            category="dollar_interpolation",
                        )
                        judge, reason = judge_injection_risk(item.file, item.line_num, item.line_content, "dollar_interpolation")
                        item.judge = judge.value
                        item.reason = reason
                        report.items.append(item)

    report.total = len(report.items)
    report.safe = sum(1 for x in report.items if x.judge == JudgeResult.SAFE.value)
    report.unsafe = sum(1 for x in report.items if x.judge == JudgeResult.UNSAFE.value)
    report.manual_review = sum(1 for x in report.items if x.judge == JudgeResult.MANUAL_REVIEW.value)

    return report


def main():
    parser = argparse.ArgumentParser(description="SQL Injection Automated Review Engine")
    parser.add_argument("--workspace", default=".", help="Workspace root directory")
    parser.add_argument("--json", action="store_true", help="Output JSON format")
    parser.add_argument("--strict", action="store_true", help="Strict mode: fail if any Unsafe or ManualReview")
    args = parser.parse_args()

    report = scan_workspace(args.workspace)

    if args.json:
        output = {
            "total": report.total,
            "safe": report.safe,
            "unsafe": report.unsafe,
            "manual_review": report.manual_review,
            "items": [
                {
                    "file": item.file,
                    "line": item.line_num,
                    "judge": item.judge,
                    "reason": item.reason,
                    "category": item.category,
                    "content": item.line_content,
                }
                for item in report.items
            ],
        }
        print(json.dumps(output, indent=2, ensure_ascii=False))
    else:
        print(f"=== SQL Injection Automated Review (Gate 9) ===")
        print(f"Total: {report.total} | Safe: {report.safe} | Unsafe: {report.unsafe} | ManualReview: {report.manual_review}")
        print()

        if report.unsafe > 0:
            print(f"--- Unsafe ({report.unsafe}) ---")
            for item in report.items:
                if item.judge == JudgeResult.UNSAFE.value:
                    print(f"  {item.file}:{item.line_num}: {item.reason}")
                    print(f"    {item.line_content.strip()}")
            print()

        if report.manual_review > 0:
            print(f"--- ManualReview ({report.manual_review}) ---")
            for item in report.items:
                if item.judge == JudgeResult.MANUAL_REVIEW.value:
                    print(f"  {item.file}:{item.line_num}: {item.reason}")
                    print(f"    {item.line_content.strip()}")
            print()

        if report.unsafe == 0 and report.manual_review == 0:
            print(f"Gate 9 PASSED: all {report.safe} items judged Safe (automated)")
        else:
            print(f"Gate 9: {report.unsafe} Unsafe + {report.manual_review} ManualReview (needs attention)")

    if args.strict and (report.unsafe > 0 or report.manual_review > 0):
        sys.exit(1)


if __name__ == "__main__":
    main()