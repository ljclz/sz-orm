#!/usr/bin/env python3
"""audit-verify.ps1 v4 自测（2026-10-29 整改 P1-4，闭环评测 R2「audit-verify 语义空转」）

覆盖 v4 的四类行为：
1. 弱语义校验：真实行号承载虚构中文断言 → 默认模式警告（exit 0）、-Strict 红牌（exit 1）；
   修复"纯中文结论提取不到 ASCII token → 校验整体跳过"的旁路。
2. 符号级断言：[must-contain: X] / [must-not-contain: X] 不成立即红牌（R2 的正解）。
3. 通配覆盖：scripts/*.py:NN 等非 .rs 引用进入校验（此前被静默跳过，
   "通过 N 条"的分母本身漏了大半）。
4. 虚构路径仍红牌（文件不存在）。

运行：python -m pytest tests/test_audit_verify_ps1.py -v
本机无 powershell.exe 时跳过（CI 的 windows job 必有）。
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "audit-verify.ps1"

POWERSHELL = (
    shutil.which("powershell.exe")
    or shutil.which("pwsh")
    or shutil.which("powershell")
)

pytestmark = pytest.mark.skipif(
    POWERSHELL is None or not SCRIPT.is_file(), reason="powershell.exe 或脚本不可用"
)

# 稳定锚点：本仓库内不随并行开发漂移的干净文件与真实非空行
CITED_FILE = "scripts/check-secrets.py"
CITED_LINE = 219  # def is_sensitive_filename(filename: str) -> bool:
CITED_TOKEN = "is_sensitive_filename"


def run_verify(tmp_path: Path, body: str, strict: bool = False, at_head: bool = False) -> subprocess.CompletedProcess:
    report = tmp_path / "probe-report.md"
    report.write_text(body, encoding="utf-8")
    cmd = [
        POWERSHELL,
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        "try { [Console]::OutputEncoding=[System.Text.Encoding]::UTF8 } catch {} "
        "$OutputEncoding=[System.Text.Encoding]::UTF8; "
        f"& '{str(SCRIPT).replace(chr(92), '/')}' "        f"-ReportPath '{report.as_posix()}'"
        + (" -Strict" if strict else "")
        + (" -AtHead" if at_head else ""),
    ]
    return subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=180,
        cwd=str(ROOT),
    )


PROBE_BASE = (
    "# 探针报告\n\n"
    "| 结论 | 证据 |\n"
    "|------|------|\n"
    "| {claim} | `{path}` {markers}|\n"
)


def test_fabricated_claim_warns_by_default(tmp_path):
    """真实行号 + 虚构中文断言：默认模式必须给出弱语义警告（v2 是静默通过）。"""
    proc = run_verify(
        tmp_path,
        PROBE_BASE.format(
            claim="凭据扫描器在此接入量子随机数生成器熵源",
            path=f"{CITED_FILE}:{CITED_LINE}",
            markers="",
        ),
    )
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert "弱语义校验" in proc.stdout, proc.stdout
    assert "无共享标识符" in proc.stdout, proc.stdout


def test_fabricated_claim_fails_under_strict(tmp_path):
    """-Strict 下同一虚构断言必须红牌——R2 的复现命令在新版本下不再 exit 0。"""
    proc = run_verify(
        tmp_path,
        PROBE_BASE.format(
            claim="凭据扫描器在此接入量子随机数生成器熵源",
            path=f"{CITED_FILE}:{CITED_LINE}",
            markers="",
        ),
        strict=True,
    )
    assert proc.returncode == 1, proc.stdout + proc.stderr
    assert "Strict：语义校验失败" in proc.stdout, proc.stdout


def test_must_contain_satisfied_passes(tmp_path):
    proc = run_verify(
        tmp_path,
        PROBE_BASE.format(
            claim="敏感文件名判定函数",
            path=f"{CITED_FILE}:{CITED_LINE}",
            markers=f"[must-contain: {CITED_TOKEN}] [must-not-contain: postgresql]",
        ),
    )
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert "通过: 1" in proc.stdout, proc.stdout


def test_must_contain_violated_fails(tmp_path):
    """承载真实行号的虚构符号断言必须被断言机制拦截（R2 正解）。"""
    proc = run_verify(
        tmp_path,
        PROBE_BASE.format(
            claim="该行完成零拷贝 protobuf 序列化",
            path=f"{CITED_FILE}:{CITED_LINE}",
            markers="[must-contain: protobuf]",
        ),
    )
    assert proc.returncode == 1, proc.stdout + proc.stderr
    assert "断言失败" in proc.stdout, proc.stdout
    assert "must-contain 'protobuf'" in proc.stdout, proc.stdout


def test_must_not_contain_violated_fails(tmp_path):
    proc = run_verify(
        tmp_path,
        PROBE_BASE.format(
            claim="该函数不含 ANSI 代码页解析",
            path=f"{CITED_FILE}:{CITED_LINE}",
            markers=f"[must-not-contain: {CITED_TOKEN}]",
        ),
    )
    assert proc.returncode == 1, proc.stdout + proc.stderr
    assert "must-not-contain" in proc.stdout, proc.stdout


def test_non_python_extension_citation_is_verified(tmp_path):
    """通配覆盖回归：scripts/*.py 引用不得再被静默跳过。"""
    proc = run_verify(
        tmp_path,
        PROBE_BASE.format(
            claim="扫描器文件名判定", path=f"{CITED_FILE}:{CITED_LINE}", markers=""
        ),
    )
    assert f"{CITED_FILE}:{CITED_LINE}" in proc.stdout, proc.stdout
    assert "未找到任何 file:line 证据" not in proc.stdout, proc.stdout


def test_fabricated_path_fails(tmp_path):
    proc = run_verify(
        tmp_path,
        PROBE_BASE.format(
            claim="熵源落地", path="scripts/not_a_real_checker.py:88", markers=""
        ),
    )
    assert proc.returncode == 1, proc.stdout + proc.stderr
    assert "文件不存在" in proc.stdout, proc.stdout


def test_marker_binds_only_to_preceding_citation(tmp_path):
    """同一行两条引用时，断言只作用于紧邻其前的那条，不得误伤另一条。"""
    body = (
        "# 探针报告（同行双引用）\n\n"
        "| 结论 | 证据 |\n"
        "|------|------|\n"
        f"| 判定函数 | `{CITED_FILE}:{CITED_LINE}` [must-contain: protobuf] 与 "
        f"`{CITED_FILE}:221` |\n"
    )
    proc = run_verify(tmp_path, body)
    assert proc.returncode == 1, proc.stdout + proc.stderr
    assert proc.stdout.count("断言失败") == 1, proc.stdout
    assert f"{CITED_FILE}:{CITED_LINE} — 断言失败" in proc.stdout, proc.stdout
    # 第二条引用仍被独立评估（不得因第一条的断言而消失）
    assert f"{CITED_FILE}:221" in proc.stdout, proc.stdout


def test_at_head_ignores_worktree_drift(tmp_path):
    """-AtHead 读已提交内容：工作区被并行改动推移的行号不再产生假阳性。"""
    proc = run_verify(
        tmp_path,
        PROBE_BASE.format(
            claim="敏感文件名判定函数",
            path=f"{CITED_FILE}:{CITED_LINE}",
            markers=f"[must-contain: {CITED_TOKEN}]",
        ),
        at_head=True,
    )
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert "通过: 1" in proc.stdout, proc.stdout


def test_at_head_rejects_untracked_file(tmp_path):
    """-AtHead 下未入库文件必须判为不存在（工作区里它确实存在）。"""
    untracked = "tests/__audit_verify_athead_probe__.py"
    victim = ROOT / untracked
    victim.write_text("x = 1\n", encoding="utf-8")
    try:
        proc = run_verify(
            tmp_path,
            PROBE_BASE.format(claim="未入库探针", path=f"{untracked}:1", markers=""),
            at_head=True,
        )
        assert proc.returncode == 1, proc.stdout + proc.stderr
        assert "文件不存在" in proc.stdout, proc.stdout
        assert "HEAD 提交内容中" in proc.stdout, proc.stdout
    finally:
        victim.unlink(missing_ok=True)
