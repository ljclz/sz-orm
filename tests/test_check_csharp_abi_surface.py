#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check-csharp-abi-surface.py 自测（2026-10-29 整改 P1-5 配套防复发门禁）

用合成仓库逐一命中 R1~R5 与 fail-closed 分支，另加一条真实仓库 --at-head 冒烟。
运行：python -m pytest tests/test_check_csharp_abi_surface.py -v
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "check-csharp-abi-surface.py"

EXPORTS = ["sz_orm_pool_new", "sz_orm_query", "sz_orm_pool_free"]
CSHARP = """
public sealed class SzOrmPool : IDisposable
{
    private const string Lib = "sz_orm_cabi";

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr sz_orm_pool_new(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string dsn);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr sz_orm_query(IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string sql);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    private static extern void sz_orm_pool_free(IntPtr handle);

    public static string GetString(IntPtr p) => Marshal.PtrToStringUTF8(p);
}
"""


def make_repo(tmp_path: Path, exports=EXPORTS, csharp=CSHARP, header_extra="", readme="37 C ABI exports\n"):
    cabi = tmp_path / "packages" / "sz-orm-cabi"
    (cabi / "src").mkdir(parents=True)
    (cabi / "include").mkdir(parents=True)
    (cabi / "csharp").mkdir(parents=True)

    body = "\n".join(
        '#[no_mangle]\npub unsafe extern "C" fn %s(handle: SzOrmPoolHandle) -> i32 { 0 }\n' % name
        for name in exports
    )
    (cabi / "src" / "lib.rs").write_text(body, encoding="utf-8")

    decls = "\n".join("int32_t %s(void *h);" % name for name in exports)
    header = (
        "/* sz-orm C ABI 契约\n"
        " * 字符串编码：所有 C 字符串均为 UTF-8 且以 NUL 结尾。\n"
        " * DSN 限制：仅支持 sqlite: 前缀。\n"
        " */\n" + decls + header_extra
    )
    (cabi / "include" / "sz_orm_cabi.h").write_text(header, encoding="utf-8")
    (cabi / "csharp" / "SzOrm.cs").write_text(csharp, encoding="utf-8")

    total = str(len(exports))
    (tmp_path / "README.md").write_text("Real but narrow (9 of the %s C ABI exports)\n" % total, encoding="utf-8")
    (tmp_path / "README.zh.md").write_text("37 个 C ABI 导出中封装 9 个\n".replace("37", total), encoding="utf-8")
    return tmp_path


def run(root: Path, *flags: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--root", str(root)] + list(flags),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=120,
    )


def test_consistent_repo_passes(tmp_path):
    proc = run(make_repo(tmp_path, readme="3 C ABI exports\n"))
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert "OK: C# 绑定与 Rust 导出/头文件契约一致" in proc.stdout


def test_r1_import_without_rust_export(tmp_path):
    """C# 调用了 lib.rs 里不存在的符号 → 红牌（运行时会 EntryPointNotFound）。"""
    bad = CSHARP + """
    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr sz_orm_ghost_only_in_csharp(IntPtr handle);
"""
    repo = make_repo(tmp_path, csharp=bad)
    (repo / "packages" / "sz-orm-cabi" / "csharp" / "SzOrm.cs").write_text(bad, encoding="utf-8")
    proc = run(repo)
    assert proc.returncode == 1, proc.stdout
    assert "R1" in proc.stdout and "sz_orm_ghost_only_in_csharp" in proc.stdout


def test_r2_symbol_missing_from_header(tmp_path):
    repo = make_repo(tmp_path, header_extra="\n/* 故意不声明 sz_orm_query */\n")
    lines = (repo / "packages" / "sz-orm-cabi" / "include" / "sz_orm_cabi.h").read_text(encoding="utf-8")
    (repo / "packages" / "sz-orm-cabi" / "include" / "sz_orm_cabi.h").write_text(
        lines.replace("int32_t sz_orm_query(void *h);", "/* removed */"), encoding="utf-8"
    )
    proc = run(repo)
    assert proc.returncode == 1, proc.stdout
    assert "R2" in proc.stdout and "sz_orm_query" in proc.stdout


def test_r3_ansi_marshalling_rejected(tmp_path):
    """R5 回归：LPStr / PtrToStringAnsi 一旦被改回 ANSI 编码必须红牌。"""
    bad = CSHARP.replace("LPUTF8Str", "LPStr").replace("PtrToStringUTF8", "PtrToStringAnsi")
    repo = make_repo(tmp_path, csharp=bad)
    (repo / "packages" / "sz-orm-cabi" / "csharp" / "SzOrm.cs").write_text(bad, encoding="utf-8")
    proc = run(repo)
    assert proc.returncode == 1, proc.stdout
    assert "R3" in proc.stdout
    assert "LPStr" in proc.stdout and "PtrToStringAnsi" in proc.stdout


def test_r4_missing_contract_banner(tmp_path):
    repo = make_repo(tmp_path)
    (repo / "packages" / "sz-orm-cabi" / "include" / "sz_orm_cabi.h").write_text(
        "int32_t sz_orm_pool_new(void *h);\nint32_t sz_orm_query(void *h);\nint32_t sz_orm_pool_free(void *h);\n",
        encoding="utf-8",
    )
    proc = run(repo)
    assert proc.returncode == 1, proc.stdout
    assert "R4 头文件缺少 UTF-8" in proc.stdout
    assert "SQLite" in proc.stdout


def test_r5_readme_total_drift(tmp_path):
    repo = make_repo(tmp_path)
    (repo / "README.zh.md").write_text("99 个 C ABI 导出中封装 9 个\n", encoding="utf-8")
    proc = run(repo)
    assert proc.returncode == 1, proc.stdout
    assert "R5" in proc.stdout and "README.zh.md" in proc.stdout


def test_fail_closed_when_inputs_missing(tmp_path):
    empty = tmp_path / "empty"
    empty.mkdir()
    proc = run(empty)
    assert proc.returncode == 2, proc.stdout + proc.stderr
    assert "fail-closed" in proc.stdout


def test_real_repository_at_head_is_consistent():
    """真实仓库在当前提交上必须自洽（37 导出 = 37 声明 = 文档宣称）。"""
    if not (ROOT / ".git").exists():
        pytest.skip("无 git 元数据")
    proc = subprocess.run(
        [sys.executable, str(SCRIPT), "--root", str(ROOT), "--at-head"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=180,
    )
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert "OK:" in proc.stdout
