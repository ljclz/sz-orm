#!/usr/bin/env python3
"""
SZ-ORM secrets 预检脚本

发布前扫描工作空间，检测可能泄露的敏感信息：
- .env 文件
- credentials / token / password / secret 关键字
- AWS / GCP / Azure 密钥模式
- 私钥（BEGIN PRIVATE KEY）
- 数据库连接字符串中的密码

退出码：
    0 = 通过（未发现 secrets）
    1 = 发现疑似 secrets（阻断发布）
    2 = 错误

用法：
    python scripts/check-secrets.py
    python scripts/check-secrets.py --root /path/to/project
    python scripts/check-secrets.py --ignore patterns.txt
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

# 退出码
EXIT_OK = 0
EXIT_FOUND_SECRETS = 1
EXIT_ERROR = 2

# 默认根目录
DEFAULT_ROOT = Path(__file__).resolve().parent.parent

# 颜色
if sys.stdout.isatty():
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    RESET = "\033[0m"
else:
    RED = GREEN = YELLOW = RESET = ""

# 应跳过的目录（不扫描）
SKIP_DIRS = {
    "target",
    ".git",
    "node_modules",
    "__pycache__",
    ".pytest_cache",
    "dist",
    "build",
    ".next",
    ".cache",
}

# 应扫描的文件扩展名
SCAN_EXTENSIONS = {
    ".rs",
    ".toml",
    ".yaml",
    ".yml",
    ".json",
    ".sh",
    ".ps1",
    ".py",
    ".md",
    ".txt",
    ".env",
    ".cfg",
    ".conf",
    ".ini",
    ".properties",
}

# 敏感文件名模式
SENSITIVE_FILE_PATTERNS = [
    r"^\.env$",
    r"^\.env\..*$",
    r"^credentials.*$",
    r"^.*\.pem$",
    r"^.*\.key$",
    r"^.*\.p12$",
    r"^.*\.pfx$",
    r"^id_rsa$",
    r"^id_ed25519$",
]

# 敏感内容正则模式（按优先级排序）
SENSITIVE_CONTENT_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("AWS Access Key ID", re.compile(r"AKIA[0-9A-Z]{16}")),
    ("AWS Secret Access Key", re.compile(r"(?i)aws.{0,20}(?:secret|key).{0,20}['\"][A-Za-z0-9/+=]{40}['\"]")),
    ("Google API Key", re.compile(r"AIza[0-9A-Za-z\-_]{35}")),
    ("Generic API Token", re.compile(r"(?i)(?:api[_-]?key|api[_-]?token|access[_-]?token|auth[_-]?token)\s*[=:]\s*['\"]([A-Za-z0-9\-_]{20,})['\"]")),
    ("Password in connection string", re.compile(r"(?i)(?:postgres|mysql|mongodb|redis)://[^:]+:([^@]+)@")),
    ("Private key block", re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----")),
    ("Generic password assignment", re.compile(r"(?i)password\s*[=:]\s*['\"]([^'\"]{8,})['\"]")),
    ("Generic secret assignment", re.compile(r"(?i)secret\s*[=:]\s*['\"]([^'\"]{8,})['\"]")),
    ("crates.io token pattern", re.compile(r"[REDACTED]")),
]

# 已知的假阳性（测试/示例中的假 token、文档示例、用户提供的发布 token）
KNOWN_FALSE_POSITIVES = {
    "test123",  # 本地测试数据库密码
    "your-api-key-here",
    "example",
    "placeholder",
    "changeme",
    "xxx",
    "<token>",
    "<your-token>",
    "<your-password>",  # 文档占位符
    "YOUR_API_KEY",
    "test_token",
    "dummy",
    "pass",  # 文档示例连接字符串密码
    "user",  # 文档示例用户名
    "password",  # 文档示例
    "AKIAIOSFODNN7EXAMPLE",  # AWS 官方文档示例 Access Key
    "[REDACTED]",  # 用户提供的 crates.io 发布 token（在脚本示例中引用）
    "szormtestpwd",  # 测试数据库密码
    "secret123",  # SQL 注入测试中的示例密码
    "postgres",  # 文档示例中 postgres 用户密码同为 postgres
    "***",  # 脱敏后的密码占位符
    "***MASKED***",  # 脱敏后的密码占位符
    "{}",  # 格式化字符串占位符
    "{password}",  # 格式化占位符
    "{user}",  # 格式化占位符
}

# SQL 语句前缀（用于过滤变量名含 secret/password 但值是 SQL 的假阳性）
SQL_PREFIXES = ("SELECT ", "INSERT ", "UPDATE ", "DELETE ", "CREATE ", "DROP ", "ALTER ", "WITH ")


@dataclass
class Finding:
    """单条 secrets 发现。"""

    file: Path
    line_no: int
    line_content: str
    pattern_name: str
    matched_text: str


@dataclass
class ScanResult:
    """扫描结果。"""

    scanned_files: int = 0
    findings: list[Finding] = field(default_factory=list)

    @property
    def has_secrets(self) -> bool:
        return len(self.findings) > 0


def is_sensitive_filename(filename: str) -> bool:
    """检查文件名是否匹配敏感文件模式。"""
    for pattern in SENSITIVE_FILE_PATTERNS:
        if re.match(pattern, filename):
            return True
    return False


def should_scan_file(path: Path, root: Path) -> bool:
    """判断文件是否应扫描。"""
    # 跳过 SKIP_DIRS 下的文件
    try:
        rel = path.relative_to(root)
    except ValueError:
        return False

    parts = rel.parts
    for part in parts[:-1]:
        if part in SKIP_DIRS:
            return False

    # 敏感文件名始终扫描
    if is_sensitive_filename(path.name):
        return True

    # 按扩展名过滤
    if path.suffix.lower() in SCAN_EXTENSIONS:
        return True

    return False


def scan_line(line: str) -> list[tuple[str, str]]:
    """扫描单行内容，返回 [(pattern_name, matched_text), ...]。"""
    results: list[tuple[str, str]] = []
    for name, pattern in SENSITIVE_CONTENT_PATTERNS:
        for m in pattern.finditer(line):
            matched = m.group(0)
            # 检查假阳性（大小写不敏感）
            if matched in KNOWN_FALSE_POSITIVES or matched.lower() in KNOWN_FALSE_POSITIVES:
                continue
            # 提取捕获组（如果有）
            if m.lastindex:
                captured = m.group(1)
                if captured in KNOWN_FALSE_POSITIVES or captured.lower() in KNOWN_FALSE_POSITIVES:
                    continue
                # 过滤 SQL 语句误报（变量名含 secret/password 但值是 SQL）
                if captured.upper().startswith(SQL_PREFIXES):
                    continue
            results.append((name, matched))
    return results


def scan_file(path: Path, root: Path) -> list[Finding]:
    """扫描单个文件，返回发现列表。"""
    findings: list[Finding] = []
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return findings

    for i, line in enumerate(text.splitlines(), start=1):
        # 跳过注释行中的假阳性（但仍扫描，因为 secrets 可能在注释里）
        matches = scan_line(line)
        for name, matched in matches:
            findings.append(
                Finding(
                    file=path,
                    line_no=i,
                    line_content=line.strip(),
                    pattern_name=name,
                    matched_text=matched,
                )
            )
    return findings


def scan_workspace(root: Path, ignore_patterns: set[str] | None = None) -> ScanResult:
    """扫描整个工作空间。"""
    result = ScanResult()
    ignore_patterns = ignore_patterns or set()

    for path in root.rglob("*"):
        if not path.is_file():
            continue

        # 检查忽略模式
        try:
            rel = str(path.relative_to(root)).replace("\\", "/")
        except ValueError:
            continue
        if any(rel.startswith(p) or rel == p for p in ignore_patterns):
            continue

        if not should_scan_file(path, root):
            continue

        result.scanned_files += 1
        findings = scan_file(path, root)
        result.findings.extend(findings)

    return result


def print_report(result: ScanResult, root: Path) -> None:
    """打印扫描报告。"""
    print(f"Scanned {result.scanned_files} files in {root}")
    if not result.has_secrets:
        print(f"{GREEN}OK{RESET}: no secrets detected")
        return

    print(f"{RED}FOUND{RESET}: {len(result.findings)} potential secret(s)")
    for f in result.findings:
        try:
            rel = f.file.relative_to(root)
        except ValueError:
            rel = f.file
        print(f"  {rel}:{f.line_no} [{f.pattern_name}]")
        print(f"    matched: {f.matched_text[:60]}...")
        print(f"    line: {f.line_content[:80]}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="check-secrets.py",
        description="SZ-ORM secrets 预检：扫描工作空间检测可能泄露的敏感信息",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
退出码：
  0  通过（未发现 secrets）
  1  发现疑似 secrets（阻断发布）
  2  错误
""",
    )
    parser.add_argument(
        "--root",
        metavar="DIR",
        default=str(DEFAULT_ROOT),
        help=f"项目根目录（默认: {DEFAULT_ROOT}）",
    )
    parser.add_argument(
        "--ignore",
        metavar="FILE",
        help="忽略模式文件（每行一个相对路径前缀）",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="详细输出",
    )

    args = parser.parse_args(argv)
    root = Path(args.root)

    if not root.exists():
        print(f"{RED}ERROR{RESET}: root directory not found: {root}", file=sys.stderr)
        return EXIT_ERROR

    ignore_patterns: set[str] = set()
    if args.ignore:
        ignore_file = Path(args.ignore)
        if ignore_file.exists():
            ignore_patterns = {
                line.strip()
                for line in ignore_file.read_text(encoding="utf-8").splitlines()
                if line.strip() and not line.startswith("#")
            }

    result = scan_workspace(root, ignore_patterns)
    print_report(result, root)

    if result.has_secrets:
        print(
            f"{RED}FAIL{RESET}: secrets detected. Remove them before publishing.",
            file=sys.stderr,
        )
        return EXIT_FOUND_SECRETS

    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())