#!/usr/bin/env python3
"""audit-verify.sh v3 自测（2026-10-28 架构评测 P2-4）

覆盖四项强化：
1. 子目录根级文件：packages/*/Cargo.toml:NN 解析到子目录文件（不再误判根 Cargo.toml 行数）
2. crate 前缀映射：sz-orm-*/src/xxx.rs 与 cli/src/xxx.rs 解析到正确位置
3. 代码块跳过：``` 围栏内的 file:line 不被二次校验
4. 弱语义校验：被引行与结论无共享 token → 默认警告（exit 0），--strict 红牌（exit 1）

运行：python -m pytest tests/test_audit_verify.py -v
bash 不可用时跳过（GitHub runner / Git Bash 环境必有 bash）。
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "audit-verify.sh"


def _find_bash() -> str | None:
    """优先 Git Bash（WSL bash 对 D:/ 路径语义不同，不能用于本仓库脚本）。"""
    candidates = [
        os.environ.get("SZ_ORM_BASH"),
        r"D:\Git\bin\bash.exe",
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
        shutil.which("bash"),
    ]
    for cand in candidates:
        if cand and Path(cand).exists():
            # 排除 WSL bash（System32/bash.exe 无法执行 Windows 路径语义的仓库脚本）
            if "system32" in cand.lower():
                continue
            return cand
    return None


_BASH = _find_bash()
pytestmark = pytest.mark.skipif(_BASH is None, reason="Git Bash 不可用")


def run_verify(report: Path, *extra: str) -> subprocess.CompletedProcess:
    # bash 下 Windows 反斜杠路径会被吞掉，统一转 POSIX 斜杠
    return subprocess.run(
        [_BASH, SCRIPT.as_posix(), report.as_posix(), *extra],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


def test_real_citations_pass(tmp_path):
    """真实引用（含子目录 Cargo.toml 与 crate 前缀简写）默认通过。"""
    report = tmp_path / "r.md"
    report.write_text(
        "# 报告\n"
        "连接池上限配置见 packages/sz-orm-core/Cargo.toml:95（默认 features 不含该聚合 feature）\n"
        "CLI 校验输出见 cli/src/main.rs:1788（✓ SQL 校验通过 字样）\n"
        "JS 绑定 connect 实现：packages/sz-orm-js/src/pool.rs:101 async 连接\n"
        "core 版本：Cargo.toml:6\n",
        encoding="utf-8",
    )
    r = run_verify(report)
    assert r.returncode == 0, r.stdout + r.stderr
    assert "sz-orm-core/Cargo.toml:95" in r.stdout, "子目录 Cargo.toml 应解析到 packages/ 下"
    assert "cli/src/main.rs:1788" in r.stdout
    assert "sz-orm-js/src/pool.rs:101" in r.stdout
    assert "❌ 失败: 0" in r.stdout


def test_fenced_code_block_skipped(tmp_path):
    """代码块内的路径行不被二次校验（PERFV2-17 内嵌日志误报修复）。"""
    report = tmp_path / "r.md"
    report.write_text(
        "# 报告\n"
        "证据：Cargo.toml:6\n"
        "```text\n"
        "以下为脚本原始输出，不应被校验：\n"
        "❌ Cargo.toml:99999 — 行号超出范围（伪造日志示例）\n"
        "✅ AGENTS.md:99999 (伪造日志)\n"
        "```\n",
        encoding="utf-8",
    )
    r = run_verify(report)
    assert r.returncode == 0, r.stdout + r.stderr
    assert "99999" not in r.stdout, "围栏内的行必须被跳过"
    assert "✅ 通过: 1" in r.stdout


def test_semantic_mismatch_warns_by_default(tmp_path):
    """虚构断言（被引行与结论零共享 token）默认警告不阻断。"""
    report = tmp_path / "r.md"
    report.write_text(
        "# 报告\n"
        "量子随机数生成器已实装并默认启用（packages/sz-orm-core/src/lib.rs:1）\n",
        encoding="utf-8",
    )
    r = run_verify(report)
    assert r.returncode == 0, "默认模式警告不红牌"
    assert "弱语义校验" in r.stdout


def test_semantic_mismatch_fails_under_strict(tmp_path):
    """--strict 下虚构断言红牌（V3-2 空转修复验收）。"""
    report = tmp_path / "r.md"
    report.write_text(
        "# 报告\n"
        "量子随机数生成器已实装并默认启用（packages/sz-orm-core/src/lib.rs:1）\n",
        encoding="utf-8",
    )
    r = run_verify(report, "--strict")
    assert r.returncode == 1, "--strict 下语义校验失败必须红牌"
    assert "语义校验失败" in r.stdout


def test_semantic_match_no_warning(tmp_path):
    """共享 token 的真实结论引用不产生警告。"""
    report = tmp_path / "r.md"
    report.write_text(
        "# 报告\n"
        "pass_result 构造通过态的检查结果（packages/sz-orm-core/src/prod_ready_check.rs:281）\n",
        encoding="utf-8",
    )
    r = run_verify(report)
    assert r.returncode == 0
    assert "弱语义校验" not in r.stdout, "有共享 token 的引用不应警告"
    assert "⚠️  警告: 0" in r.stdout


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v"]))
