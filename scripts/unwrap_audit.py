#!/usr/bin/env python3
"""
unwrap/panic/expect 审计脚本
扫描 packages/ 目录下所有 .rs 文件中的危险调用，生成详细报告。

危险等级定义：
  CRITICAL - panic! / unreachable! / unimplemented! / todo!
             （运行时必然 panic，生产环境零容忍）
  HIGH     - unwrap() / expect()
             （Option/Result 强制解包，输入不满足假设时 panic）
  MEDIUM   - 测试代码中的 unwrap/expect（允许但需记录）
  LOW      - 注释或字符串字面量中的关键词（误报，仅提示）
"""

import os
import re
import sys
from dataclasses import dataclass, field
from datetime import date
from enum import Enum
from pathlib import Path
from typing import Optional


class Severity(Enum):
    CRITICAL = "CRITICAL"
    HIGH = "HIGH"
    MEDIUM = "MEDIUM"
    LOW = "LOW"
    INFO = "INFO"


@dataclass
class Finding:
    file: str
    line: int
    col: int
    severity: Severity
    pattern: str
    code: str
    context: str = ""
    suggestion: str = ""


# ---------------------------------------------------------------------------
# 模式定义
# ---------------------------------------------------------------------------

# 真实代码行中匹配危险调用（排除注释和字符串）
PATTERNS = {
    # CRITICAL
    "panic!": (
        Severity.CRITICAL,
        r'\bpanic\s*!\s*\(',
        "运行时 panic，进程终止",
        "返回 Result/Error，或使用 expect() 附带明确错误信息",
    ),
    "unreachable!": (
        Severity.CRITICAL,
        r'\bunreachable\s*!\s*\(',
        "标记为不可达但实际可达时 panic",
        "重构逻辑消除不可达分支，或返回 Result",
    ),
    "unimplemented!": (
        Severity.CRITICAL,
        r'\bunimplemented\s*!\s*\(',
        "功能未实现，调用即 panic",
        "补全功能实现，或返回未实现错误",
    ),
    "todo!": (
        Severity.CRITICAL,
        r'\btodo\s*!\s*\(',
        "待办占位，调用即 panic",
        "补全功能实现",
    ),
    # HIGH
    "unwrap()": (
        Severity.HIGH,
        r'\bunwrap\s*\(\)',
        "Option/Result 为 None/Err 时 panic",
        "使用 ok_or()/ok_or_else() 转换为 Result，或使用 match/if let",
    ),
    "expect()": (
        Severity.HIGH,
        r'\bexpect\s*\(\s*"',
        "Option/Result 为 None/Err 时 panic（带自定义消息）",
        "使用 ok_or()/ok_or_else() 转换为 Result",
    ),
}

# 注释/字符串中的关键词（LOW / INFO 级别）
COMMENT_PATTERNS = {
    "comment_unwrap": (
        Severity.LOW,
        r'//.*\bunwrap\b',
        "注释中的 unwrap 关键词（可能是待办或说明）",
        "确认是否为待处理问题",
    ),
    "comment_panic": (
        Severity.LOW,
        r'//.*\bpanic\b',
        "注释中的 panic 关键词",
        "确认是否为待处理问题",
    ),
}

# 测试文件中的 unwrap/expect（MEDIUM）
TEST_PATTERNS = {
    "test_unwrap": (
        Severity.MEDIUM,
        r'\bunwrap\s*\(\)',
        "测试代码中的 unwrap（允许但需记录）",
        "测试代码可接受，但生产代码中应替换",
    ),
    "test_expect": (
        Severity.MEDIUM,
        r'\bexpect\s*\(\s*"',
        "测试代码中的 expect（允许但记录）",
        "测试代码可接受",
    ),
}


def is_comment_or_string(line: str, match_start: int) -> bool:
    """判断匹配位置是否在注释或字符串字面量中。"""
    stripped = line[:match_start]
    # 单行注释
    if "//" in stripped:
        last_slash = stripped.rfind("//")
        # 检查 // 是否在字符串中（简单启发式）
        quote_count = stripped.count('"') - stripped.count('\\"')
        if quote_count % 2 == 0:
            return True
    return False


def is_test_file(filepath: str) -> bool:
    """判断是否为测试文件。"""
    return (
        "/tests/" in filepath
        or "/test_" in filepath
        or filepath.endswith("_test.rs")
        or "tests/" in filepath
        or "/benches/" in filepath
    )


def scan_file(filepath: str, base_dir: str) -> list[Finding]:
    """扫描单个文件，返回发现列表。"""
    findings = []
    rel_path = os.path.relpath(filepath, base_dir)

    try:
        with open(filepath, "r", encoding="utf-8", errors="replace") as f:
            lines = f.readlines()
    except Exception as e:
        findings.append(Finding(
            file=rel_path,
            line=0,
            col=0,
            severity=Severity.INFO,
            pattern="read_error",
            code=str(e),
            context="文件读取失败",
            suggestion="检查文件编码或权限",
        ))
        return findings

    is_test = is_test_file(filepath)

    for line_no, line in enumerate(lines, start=1):
        stripped = line.lstrip()
        # 跳过整行注释
        if stripped.startswith("//") or stripped.startswith("//!") or stripped.startswith("///"):
            # 记录注释中的关键词
            for name, (sev, pat, ctx, sug) in COMMENT_PATTERNS.items():
                if re.search(pat, line, re.IGNORECASE):
                    findings.append(Finding(
                        file=rel_path,
                        line=line_no,
                        col=line.index(re.search(pat, line, re.IGNORECASE).group()),
                        severity=sev,
                        pattern=name,
                        code=line.strip(),
                        context=ctx,
                        suggestion=sug,
                    ))
            continue

        # 跳过块注释起始
        if stripped.startswith("/*") or stripped.startswith("*"):
            continue

        # 对真实代码行应用主模式
        for name, (sev, pat, ctx, sug) in PATTERNS.items():
            for m in re.finditer(pat, line):
                col = m.start() + 1  # 1-based
                # 排除注释中的匹配
                if is_comment_or_string(line, m.start()):
                    continue
                # 测试文件降级
                if is_test and sev == Severity.HIGH:
                    sev_eff = Severity.MEDIUM
                    ctx_eff = "测试代码中的 " + ctx
                    sug_eff = sug
                else:
                    sev_eff = sev
                    ctx_eff = ctx
                    sug_eff = sug

                findings.append(Finding(
                    file=rel_path,
                    line=line_no,
                    col=col,
                    severity=sev_eff,
                    pattern=name,
                    code=line.strip(),
                    context=ctx_eff,
                    suggestion=sug_eff,
                ))

    return findings


def scan_directory(base_dir: str) -> list[Finding]:
    """递归扫描目录下所有 .rs 文件。"""
    all_findings = []
    packages_dir = os.path.join(base_dir, "packages")

    if not os.path.isdir(packages_dir):
        print(f"错误: {packages_dir} 目录不存在", file=sys.stderr)
        sys.exit(1)

    rs_files = []
    for root, dirs, files in os.walk(packages_dir):
        # 跳过 target 目录
        dirs[:] = [d for d in dirs if d != "target"]
        for fname in files:
            if fname.endswith(".rs"):
                rs_files.append(os.path.join(root, fname))

    rs_files.sort()
    print(f"发现 {len(rs_files)} 个 Rust 源文件，开始扫描...")

    for fpath in rs_files:
        findings = scan_file(fpath, base_dir)
        all_findings.extend(findings)

    return all_findings


def generate_report(findings: list[Finding], output_path: str) -> None:
    """生成 Markdown 审计报告。"""
    today = date.today().isoformat()

    # 统计
    by_severity: dict[Severity, int] = {}
    by_file: dict[str, int] = {}
    by_pattern: dict[str, int] = {}

    for f in findings:
        by_severity[f.severity] = by_severity.get(f.severity, 0) + 1
        by_file[f.file] = by_file.get(f.file, 0) + 1
        by_pattern[f.pattern] = by_pattern.get(f.pattern, 0) + 1

    critical_count = by_severity.get(Severity.CRITICAL, 0)
    high_count = by_severity.get(Severity.HIGH, 0)
    medium_count = by_severity.get(Severity.MEDIUM, 0)
    low_count = by_severity.get(Severity.LOW, 0)
    info_count = by_severity.get(Severity.INFO, 0)

    # 排序
    severity_order = {
        Severity.CRITICAL: 0,
        Severity.HIGH: 1,
        Severity.MEDIUM: 2,
        Severity.LOW: 3,
        Severity.INFO: 4,
    }
    findings_sorted = sorted(findings, key=lambda f: (severity_order[f.severity], f.file, f.line))

    # 生成 Markdown
    lines = []
    lines.append("# unwrap / panic / expect 审计报告")
    lines.append("")
    lines.append(f"- **生成日期**: {today}")
    lines.append(f"- **扫描范围**: `packages/` 目录下所有 `.rs` 文件")
    lines.append(f"- **审计工具**: `scripts/unwrap_audit.py`")
    lines.append(f"- **总发现数**: {len(findings)}")
    lines.append("")

    # 摘要
    lines.append("## 摘要")
    lines.append("")
    lines.append("| 危险等级 | 数量 | 说明 |")
    lines.append("|----------|------|------|")
    lines.append(f"| 🔴 CRITICAL | {critical_count} | 运行时必然 panic，生产环境零容忍 |")
    lines.append(f"| 🟠 HIGH | {high_count} | Option/Result 强制解包，输入不满足假设时 panic |")
    lines.append(f"| 🟡 MEDIUM | {medium_count} | 测试代码中的 unwrap/expect（允许但需记录） |")
    lines.append(f"| 🔵 LOW | {low_count} | 注释中的关键词（误报，仅提示） |")
    lines.append(f"| ⚪ INFO | {info_count} | 其他信息 |")
    lines.append("")

    # 按文件分布
    if by_file:
        lines.append("## 按文件分布（Top 20）")
        lines.append("")
        lines.append("| 文件 | 发现数 |")
        lines.append("|------|--------|")
        top_files = sorted(by_file.items(), key=lambda x: -x[1])[:20]
        for fpath, count in top_files:
            lines.append(f"| `{fpath}` | {count} |")
        lines.append("")

    # 按模式分布
    if by_pattern:
        lines.append("## 按模式分布")
        lines.append("")
        lines.append("| 模式 | 数量 |")
        lines.append("|------|------|")
        for pat, count in sorted(by_pattern.items(), key=lambda x: -x[1]):
            lines.append(f"| `{pat}` | {count} |")
        lines.append("")

    # 详细列表
    lines.append("## 详细发现")
    lines.append("")

    current_file = None
    for f in findings_sorted:
        if f.file != current_file:
            current_file = f.file
            lines.append(f"### `{f.file}`")
            lines.append("")

        emoji = {
            Severity.CRITICAL: "🔴",
            Severity.HIGH: "🟠",
            Severity.MEDIUM: "🟡",
            Severity.LOW: "🔵",
            Severity.INFO: "⚪",
        }.get(f.severity, "⚪")

        lines.append(f"#### {emoji} [{f.severity.value}] `{f.pattern}` @ L{f.line}:C{f.col}")
        lines.append("")
        lines.append(f"- **上下文**: {f.context}")
        lines.append(f"- **建议**: {f.suggestion}")
        lines.append("")
        lines.append("```rust")
        # 截取代码行（最多 200 字符）
        code_snippet = f.code[:200]
        if len(f.code) > 200:
            code_snippet += "..."
        lines.append(code_snippet)
        lines.append("```")
        lines.append("")

    # 修复建议
    lines.append("## 修复优先级建议")
    lines.append("")
    lines.append("1. **立即修复（CRITICAL）**：`panic!` / `unreachable!` / `unimplemented!` / `todo!`")
    lines.append("   - 这些调用在生产环境中会导致进程终止")
    lines.append("   - 替换为 `Result` 返回或适当的错误处理")
    lines.append("")
    lines.append("2. **高优先级（HIGH）**：生产代码中的 `unwrap()` / `expect()`")
    lines.append("   - 使用 `ok_or()` / `ok_or_else()` 转换为 `Result`")
    lines.append("   - 使用 `match` 或 `if let` 显式处理 `None` / `Err` 分支")
    lines.append("")
    lines.append("3. **中优先级（MEDIUM）**：测试代码中的 `unwrap()` / `expect()`")
    lines.append("   - 测试代码允许使用，但建议添加断言消息")
    lines.append("")
    lines.append("4. **低优先级（LOW）**：注释中的关键词")
    lines.append("   - 确认是否为待处理问题，更新注释或创建 issue")
    lines.append("")

    # 写入文件
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    print(f"\n报告已生成: {output_path}")
    print(f"  CRITICAL: {critical_count}")
    print(f"  HIGH:     {high_count}")
    print(f"  MEDIUM:   {medium_count}")
    print(f"  LOW:      {low_count}")
    print(f"  INFO:     {info_count}")


def main():
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    output_path = os.path.join(
        base_dir, "docs", "audit",
        f"unwrap_baseline_{date.today().isoformat()}.md"
    )

    # 支持自定义输出路径
    if len(sys.argv) > 1:
        output_path = sys.argv[1]

    findings = scan_directory(base_dir)
    generate_report(findings, output_path)

    # 退出码：有 CRITICAL 或 HIGH 发现时返回 1
    has_critical = any(f.severity in (Severity.CRITICAL, Severity.HIGH) for f in findings)
    sys.exit(1 if has_critical else 0)


if __name__ == "__main__":
    main()
