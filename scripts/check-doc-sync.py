#!/usr/bin/env python3
"""
SZ-ORM 文档同步更新检查脚本（门禁 14）

检查代码变更是否触发了对应文档的同步更新。当代码变更匹配某类规则时，
对应的文档必须在同一次提交中更新，否则退出码 1 阻断提交。

退出码：
    0 = 通过（所有受影响文档已同步更新或已标记跳过）
    1 = 未同步（存在未更新的受影响文档）
    2 = 错误（脚本执行错误，如配置文件缺失、git 命令失败等）

用法：
    python scripts/check-doc-sync.py --help
    python scripts/check-doc-sync.py --diff HEAD
    python scripts/check-doc-sync.py --base HEAD~1 --head HEAD
    python scripts/check-doc-sync.py --diff HEAD --skip-file Cargo.toml

规则配置：scripts/doc-sync-rules.yaml
"""

from __future__ import annotations

import argparse
import fnmatch
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

try:
    import yaml
except ImportError:
    print(
        "ERROR: PyYAML not installed. Run: pip install pyyaml",
        file=sys.stderr,
    )
    sys.exit(2)

# 退出码常量
EXIT_OK = 0
EXIT_NOT_SYNCED = 1
EXIT_ERROR = 2

# 路径常量（可通过 --root 覆盖）
DEFAULT_ROOT = Path(__file__).resolve().parent.parent
ROOT = DEFAULT_ROOT
RULES_FILE = Path(__file__).resolve().parent / "doc-sync-rules.yaml"

# 颜色输出（仅在 TTY 时启用）
if sys.stdout.isatty():
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    RESET = "\033[0m"
else:
    RED = GREEN = YELLOW = RESET = ""


@dataclass
class SyncRule:
    """单条文档同步映射规则。"""

    rule_id: int
    name: str
    description: str
    code_patterns: list[str]
    affected_docs: list[str]
    code_regex: str | None = None
    doc_anchor: str | None = None

    def matches_code_path(self, path: str) -> bool:
        """检查代码文件路径是否匹配本规则。"""
        for pattern in self.code_patterns:
            if fnmatch.fnmatch(path, pattern):
                return True
        return False

    def matches_diff_line(self, line: str) -> bool:
        """检查 diff 行是否匹配本规则的正则（如有）。"""
        if not self.code_regex:
            return True
        try:
            return re.search(self.code_regex, line) is not None
        except re.error:
            return False


@dataclass
class DiffEntry:
    """单个文件的 diff 信息。"""

    path: str
    added_lines: list[str] = field(default_factory=list)
    modified: bool = False

    @property
    def is_doc(self) -> bool:
        return self.path.endswith(".md")


@dataclass
class CheckResult:
    """单条规则的检查结果。"""

    rule: SyncRule
    triggered: bool
    triggered_by: list[str] = field(default_factory=list)
    affected_docs: list[str] = field(default_factory=list)
    missing_docs: list[str] = field(default_factory=list)
    skipped_docs: list[str] = field(default_factory=list)


def load_rules(rules_file: Path = RULES_FILE) -> list[SyncRule]:
    """从 YAML 配置文件加载规则列表。"""
    if not rules_file.exists():
        print(f"{RED}ERROR{RESET}: rules file not found: {rules_file}", file=sys.stderr)
        sys.exit(EXIT_ERROR)

    try:
        with open(rules_file, "r", encoding="utf-8") as f:
            data = yaml.safe_load(f)
    except yaml.YAMLError as e:
        print(f"{RED}ERROR{RESET}: invalid YAML in {rules_file}: {e}", file=sys.stderr)
        sys.exit(EXIT_ERROR)

    rules_raw = data.get("rules", [])
    rules: list[SyncRule] = []
    for r in rules_raw:
        try:
            rules.append(
                SyncRule(
                    rule_id=r["id"],
                    name=r["name"],
                    description=r["description"],
                    code_patterns=r["code_patterns"],
                    affected_docs=r["affected_docs"],
                    code_regex=r.get("code_regex"),
                    doc_anchor=r.get("doc_anchor"),
                )
            )
        except KeyError as e:
            print(
                f"{RED}ERROR{RESET}: rule {r} missing key {e}",
                file=sys.stderr,
            )
            sys.exit(EXIT_ERROR)

    return rules


def get_skip_marker(rules_file: Path = RULES_FILE) -> str:
    """从配置文件读取跳过标记。"""
    try:
        with open(rules_file, "r", encoding="utf-8") as f:
            data = yaml.safe_load(f)
        return data.get("config", {}).get("skip_marker", "# doc-sync-skip")
    except (yaml.YAMLError, OSError):
        return "# doc-sync-skip"


def run_git(args: list[str], cwd: Path | None = None) -> tuple[int, str, str]:
    """执行 git 命令，返回 (returncode, stdout, stderr)。"""
    if cwd is None:
        cwd = ROOT
    try:
        proc = subprocess.run(
            ["git", "-c", "core.quotePath=false"] + args,
            cwd=str(cwd),
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        return proc.returncode, proc.stdout, proc.stderr
    except FileNotFoundError:
        print(f"{RED}ERROR{RESET}: git not found in PATH", file=sys.stderr)
        sys.exit(EXIT_ERROR)
    except subprocess.SubprocessError as e:
        print(f"{RED}ERROR{RESET}: git command failed: {e}", file=sys.stderr)
        sys.exit(EXIT_ERROR)


def parse_git_diff_name_only(base: str | None, head: str | None, diff_ref: str | None) -> list[str]:
    """获取变更文件名清单。

    优先使用 --base/--head 组合，否则使用 --diff <ref>。
    """
    if base and head:
        ref = f"{base}..{head}"
    elif diff_ref:
        ref = diff_ref
    else:
        ref = "HEAD"

    rc, stdout, stderr = run_git(["diff", "--name-only", ref])
    if rc != 0:
        # 可能是未提交变更，尝试对比工作区
        if diff_ref == "HEAD" and not base:
            rc, stdout, stderr = run_git(["diff", "--name-only"])
            if rc == 0:
                return [line.strip() for line in stdout.splitlines() if line.strip()]
        print(
            f"{RED}ERROR{RESET}: git diff --name-only {ref} failed: {stderr}",
            file=sys.stderr,
        )
        sys.exit(EXIT_ERROR)

    return [line.strip() for line in stdout.splitlines() if line.strip()]


def parse_git_diff_content(base: str | None, head: str | None, diff_ref: str | None) -> dict[str, DiffEntry]:
    """解析 git diff 完整内容，返回 {path: DiffEntry}。

    提取每个变更文件的路径和新增行（+ 开头）。
    """
    if base and head:
        ref = f"{base}..{head}"
    elif diff_ref:
        ref = diff_ref
    else:
        ref = "HEAD"

    rc, stdout, stderr = run_git(["diff", "--unified=0", ref])
    if rc != 0:
        if diff_ref == "HEAD" and not base:
            rc, stdout, stderr = run_git(["diff", "--unified=0"])
            if rc != 0:
                print(
                    f"{RED}ERROR{RESET}: git diff failed: {stderr}",
                    file=sys.stderr,
                )
                sys.exit(EXIT_ERROR)
        else:
            print(
                f"{RED}ERROR{RESET}: git diff --unified=0 {ref} failed: {stderr}",
                file=sys.stderr,
            )
            sys.exit(EXIT_ERROR)

    entries: dict[str, DiffEntry] = {}
    current_path: str | None = None

    for line in stdout.splitlines():
        # diff --git a/path b/path
        m = re.match(r"^diff --git a/(.+?) b/(.+)$", line)
        if m:
            current_path = m.group(2)
            entries[current_path] = DiffEntry(path=current_path, modified=True)
            continue

        if current_path is None:
            continue

        # +++ b/path  或  +++ /dev/null
        if line.startswith("+++ "):
            if line == "+++ /dev/null":
                # 文件被删除
                if current_path in entries:
                    entries[current_path].modified = True
            continue

        # 新增行：+ 开头但不是 +++ （保留 + 前缀以便 code_regex 匹配 diff 行格式）
        if line.startswith("+") and not line.startswith("+++"):
            entries[current_path].added_lines.append(line)

    return entries


def doc_has_skip_marker(doc_path: Path, skip_marker: str) -> bool:
    """检查文档是否包含跳过标记。"""
    full_path = ROOT / doc_path
    if not full_path.exists():
        return False
    try:
        text = full_path.read_text(encoding="utf-8", errors="replace")
        return skip_marker in text
    except OSError:
        return False


def doc_was_modified(doc_path: str, diff_entries: dict[str, DiffEntry]) -> bool:
    """检查文档是否在本次 diff 中被修改。"""
    if doc_path in diff_entries:
        return True
    # 也检查相对路径变体
    for key in diff_entries:
        if key == doc_path or key.endswith(f"/{doc_path}") or key.replace("\\", "/") == doc_path:
            return True
    return False


def evaluate_rule(
    rule: SyncRule,
    diff_entries: dict[str, DiffEntry],
    skip_files: set[str],
    skip_marker: str,
) -> CheckResult:
    """评估单条规则，返回检查结果。"""
    triggered_by: list[str] = []
    affected_docs: list[str] = []

    # 检查代码变更是否触发本规则
    for path, entry in diff_entries.items():
        if path in skip_files:
            continue
        if not rule.matches_code_path(path):
            continue

        # 如果规则有正则，检查新增行是否匹配
        if rule.code_regex:
            for added_line in entry.added_lines:
                if rule.matches_diff_line(added_line):
                    triggered_by.append(path)
                    break
        else:
            triggered_by.append(path)

    triggered = len(triggered_by) > 0

    if not triggered:
        return CheckResult(
            rule=rule,
            triggered=False,
            triggered_by=[],
            affected_docs=[],
            missing_docs=[],
            skipped_docs=[],
        )

    # 检查受影响文档是否同步更新
    missing_docs: list[str] = []
    skipped_docs: list[str] = []

    for doc in rule.affected_docs:
        if doc in skip_files:
            skipped_docs.append(doc)
            continue

        doc_path = Path(doc)
        full_path = ROOT / doc_path

        # 文档不存在不算缺失（可能是尚未创建的新文档）
        if not full_path.exists():
            continue

        if doc_has_skip_marker(doc_path, skip_marker):
            skipped_docs.append(doc)
            continue

        if doc_was_modified(doc, diff_entries):
            affected_docs.append(doc)
        else:
            missing_docs.append(doc)

    return CheckResult(
        rule=rule,
        triggered=True,
        triggered_by=triggered_by,
        affected_docs=affected_docs,
        missing_docs=missing_docs,
        skipped_docs=skipped_docs,
    )


def print_report(results: list[CheckResult], verbose: bool = False) -> None:
    """打印检查报告。"""
    triggered_results = [r for r in results if r.triggered]
    missing_results = [r for r in results if r.missing_docs]

    if not triggered_results:
        print(f"{GREEN}OK{RESET}: no code changes triggered doc-sync rules")
        return

    print(f"Triggered rules: {len(triggered_results)}")
    for r in triggered_results:
        status = (
            f"{GREEN}SYNCED{RESET}" if not r.missing_docs else f"{RED}NOT SYNCED{RESET}"
        )
        print(f"  Rule {r.rule.rule_id} ({r.rule.name}): {status}")
        if verbose or r.missing_docs:
            print(f"    description: {r.rule.description}")
            print(f"    triggered_by: {', '.join(r.triggered_by)}")
            if r.affected_docs:
                print(f"    synced docs: {', '.join(r.affected_docs)}")
            if r.skipped_docs:
                print(
                    f"    skipped docs: {', '.join(r.skipped_docs)} (skip marker)"
                )
            if r.missing_docs:
                print(
                    f"{RED}    missing docs: {', '.join(r.missing_docs)}{RESET}"
                )

    if not missing_results:
        print(f"{GREEN}OK{RESET}: all affected docs synced")


def main(argv: list[str] | None = None) -> int:
    """主入口。"""
    parser = argparse.ArgumentParser(
        prog="check-doc-sync.py",
        description="SZ-ORM 文档同步更新检查（门禁 14）：检查代码变更是否触发文档同步更新",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
退出码：
  0  通过（所有受影响文档已同步更新或已标记跳过）
  1  未同步（存在未更新的受影响文档）
  2  错误（脚本执行错误）

示例：
  python scripts/check-doc-sync.py --diff HEAD
  python scripts/check-doc-sync.py --base HEAD~1 --head HEAD
  python scripts/check-doc-sync.py --diff HEAD --skip-file Cargo.toml
""",
    )
    parser.add_argument(
        "--diff",
        metavar="REF",
        help="git diff 引用（如 HEAD、HEAD~1..HEAD），优先级低于 --base/--head",
    )
    parser.add_argument(
        "--base",
        metavar="REF",
        help="diff 基线引用（如 HEAD~1），与 --head 配合使用",
    )
    parser.add_argument(
        "--head",
        metavar="REF",
        help="diff 目标引用（如 HEAD），与 --base 配合使用",
    )
    parser.add_argument(
        "--skip-file",
        metavar="PATH",
        action="append",
        default=[],
        help="跳过指定文件（可多次使用），支持 glob",
    )
    parser.add_argument(
        "--rules",
        metavar="FILE",
        default=str(RULES_FILE),
        help=f"规则配置文件（默认: {RULES_FILE.name}）",
    )
    parser.add_argument(
        "--root",
        metavar="DIR",
        default=str(DEFAULT_ROOT),
        help=f"项目根目录（默认: {DEFAULT_ROOT}）",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="详细输出",
    )
    parser.add_argument(
        "--list-rules",
        action="store_true",
        help="列出所有规则并退出",
    )

    args = parser.parse_args(argv)

    rules_file = Path(args.rules)
    rules = load_rules(rules_file)
    skip_marker = get_skip_marker(rules_file)

    # 覆盖 ROOT（影响文档存在性/跳过标记检查）
    global ROOT
    ROOT = Path(args.root)

    if args.list_rules:
        print(f"Loaded {len(rules)} rules from {rules_file}")
        for r in rules:
            print(f"  Rule {r.rule_id}: {r.name}")
            print(f"    {r.description}")
            print(f"    code_patterns: {r.code_patterns}")
            if r.code_regex:
                print(f"    code_regex: {r.code_regex}")
            print(f"    affected_docs: {r.affected_docs}")
        return EXIT_OK

    # 解析 diff
    diff_entries = parse_git_diff_content(args.base, args.head, args.diff)

    if not diff_entries:
        print(f"{GREEN}OK{RESET}: no changes detected")
        return EXIT_OK

    # 展开 skip-file glob
    skip_files: set[str] = set()
    for pattern in args.skip_file:
        if any(c in pattern for c in "*?[]"):
            for path in diff_entries:
                if fnmatch.fnmatch(path, pattern):
                    skip_files.add(path)
        else:
            skip_files.add(pattern)

    # 评估所有规则
    results = [evaluate_rule(r, diff_entries, skip_files, skip_marker) for r in rules]

    # 打印报告
    print_report(results, verbose=args.verbose)

    # 判定退出码
    has_missing = any(r.missing_docs for r in results)
    if has_missing:
        missing_count = sum(len(r.missing_docs) for r in results if r.missing_docs)
        print(
            f"{RED}FAIL{RESET}: {missing_count} affected doc(s) not synced. "
            "Update them or add '# doc-sync-skip' marker.",
            file=sys.stderr,
        )
        return EXIT_NOT_SYNCED

    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())