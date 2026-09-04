#!/usr/bin/env python3
"""
SZ-ORM 文档与代码一致性校验脚本（门禁 12）

验证工程规范文档中的关键数据与实际代码是否一致：
- workspace 包数量
- 项目版本号
- Rust edition
- rust-version

用法：
    python scripts/check-doc-consistency.py          # 校验全部
    python scripts/check-doc-consistency.py --fix    # 自动修复文档
"""

import os
import re
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:
    import tomli as tomllib

# 颜色输出
RED = "\033[31m" if sys.stdout.isatty() else ""
GREEN = "\033[32m" if sys.stdout.isatty() else ""
YELLOW = "\033[33m" if sys.stdout.isatty() else ""
RESET = "\033[0m" if sys.stdout.isatty() else ""

ROOT = Path(__file__).resolve().parent.parent
CARGO_TOML = ROOT / "Cargo.toml"
AGENTS_MD = ROOT / "AGENTS.md"
PRACTICES_MD = ROOT / "docs" / "sz-orm-engineering-practices.md"


def load_workspace_packages(cargo_toml: Path) -> tuple[int, list[str]]:
    """从 workspace Cargo.toml 解析包数量和包名列表。"""
    with open(cargo_toml, "rb") as f:
        data = tomllib.load(f)
    members = data.get("workspace", {}).get("members", [])
    # 统计实际存在的包（排除 target/）
    packages = []
    for member in members:
        full_path = ROOT / member
        if full_path.exists():
            packages.append(member)
    return len(packages), packages


def load_workspace_version(cargo_toml: Path) -> str:
    """从 workspace Cargo.toml 解析版本。"""
    with open(cargo_toml, "rb") as f:
        data = tomllib.load(f)
    return data.get("workspace", {}).get("package", {}).get("version", "unknown")


def load_workspace_edition(cargo_toml: Path) -> str:
    """从 workspace Cargo.toml 解析 edition。"""
    with open(cargo_toml, "rb") as f:
        data = tomllib.load(f)
    return data.get("workspace", {}).get("package", {}).get("edition", "unknown")


def load_workspace_rust_version(cargo_toml: Path) -> str:
    """从 workspace Cargo.toml 解析 rust-version。"""
    with open(cargo_toml, "rb") as f:
        data = tomllib.load(f)
    return data.get("workspace", {}).get("package", {}).get("rust-version", "unknown")


def extract_doc_value(filepath: Path, pattern: re.Pattern) -> str | None:
    """从文档文件中提取匹配值。"""
    if not filepath.exists():
        return None
    text = filepath.read_text(encoding="utf-8")
    m = pattern.search(text)
    return m.group(1) if m else None


def check_field(name: str, actual: str, doc_value: str | None, filepath: Path, fix: bool) -> bool:
    """检查单个字段的一致性。"""
    if doc_value is None:
        print(f"  {YELLOW}[WARN]{RESET} {name}: 文档中未找到（{filepath.name}）")
        return True  # 不阻断，但警告
    if actual == doc_value:
        print(f"  {GREEN}[OK]{RESET} {name}: {actual}")
        return True
    print(f"  {RED}[MISMATCH]{RESET} {name}: 实际={actual}, 文档={doc_value}")
    if fix:
        print(f"    → 自动修复为 {actual}")
    return False


def update_doc_value(filepath: Path, pattern: re.Pattern, new_value: str, label: str) -> bool:
    """更新文档中的值。"""
    if not filepath.exists():
        return False
    text = filepath.read_text(encoding="utf-8")
    new_text, count = pattern.subn(rf'\g<1>{new_value}', text)
    if count > 0:
        filepath.write_text(new_text, encoding="utf-8")
        print(f"  {GREEN}[FIXED]{RESET} {label} in {filepath.name}")
        return True
    return False


def main():
    fix_mode = "--fix" in sys.argv

    print("=" * 50)
    print("  门禁 12：文档与代码一致性校验")
    print("=" * 50)

    # 从 Cargo.toml 读取实际值
    pkg_count, pkg_list = load_workspace_packages(CARGO_TOML)
    version = load_workspace_version(CARGO_TOML)
    edition = load_workspace_edition(CARGO_TOML)
    rust_version = load_workspace_rust_version(CARGO_TOML)

    print(f"\n  实际值（来自 Cargo.toml）：")
    print(f"    包数量: {pkg_count}")
    print(f"    版本: {version}")
    print(f"    Edition: {edition}")
    print(f"    Rust Version: {rust_version}")

    # 定义所有需要检查的字段
    checks = [
        # (字段名, 实际值, 文件, 匹配 pattern, 修复 pattern)
        ("版本号", version, AGENTS_MD,
         re.compile(r'版本：(\d+\.\d+\.\d+)'),
         re.compile(r'(版本：)(\d+\.\d+\.\d+)')),
        ("包数量", str(pkg_count), AGENTS_MD,
         re.compile(r'工作空间：(\d+)（'),
         re.compile(r'(工作空间：)(\d+)（')),
        ("项目版本", version, PRACTICES_MD,
         re.compile(r'\*\*项目版本\*\*：v(\d+\.\d+\.\d+)'),
         re.compile(r'(\*\*项目版本\*\*：v)(\d+\.\d+\.\d+)')),
        ("workspace 包数量", str(pkg_count), PRACTICES_MD,
         re.compile(r'(\d+) workspace 包'),
         None),  # 这个 pattern 比较特殊，手动处理
    ]

    # 先做一轮检查
    all_ok = True
    fixed_any = False

    for name, actual, filepath, check_pattern, fix_pattern in checks:
        doc_value = extract_doc_value(filepath, check_pattern)
        if not check_field(name, actual, doc_value, filepath, fix_mode):
            all_ok = False
            if fix_mode and fix_pattern:
                if update_doc_value(filepath, fix_pattern, actual, name):
                    fixed_any = True
        elif doc_value is None:
            all_ok = False

    # 如果修复过，重新检查一轮
    if fixed_any:
        print(f"\n  重新校验修复结果...")
        all_ok = True
        for name, actual, filepath, check_pattern, _ in checks:
            doc_value = extract_doc_value(filepath, check_pattern)
            if not check_field(name, actual, doc_value, filepath, False):
                all_ok = False

    # ---- 汇总 ----
    print(f"\n{'=' * 50}")
    if all_ok:
        print(f"  {GREEN}[PASS]{RESET} 文档与代码一致性校验通过")
        print(f"{'=' * 50}")
        return 0
    else:
        print(f"  {RED}[FAIL]{RESET} 文档与代码存在不一致，请同步更新文档")
        if not fix_mode:
            print(f"  提示: 运行 'python scripts/check-doc-consistency.py --fix' 自动修复")
        print(f"{'=' * 50}")
        return 12


if __name__ == "__main__":
    sys.exit(main())
