#!/usr/bin/env python3
"""
check-coverage.py 自测（2026-10-27 SDET 评测 G26-C 整改）

背景：`python scripts/check-coverage.py` 在缺少 cargo-llvm-cov 时抛
FileNotFoundError traceback（WinError 2）。本自测验证优雅预检逻辑：
- find_cargo_bin 找不到工具时返回 None（而非字面量名）
- require_tool 返回可读错误信息
- main 在工具缺失时提前 return 1（无 traceback）

运行：
    python -m pytest tests/test_check_coverage.py -v
"""

from __future__ import annotations

import importlib.util as _ilu
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent

_spec = _ilu.spec_from_file_location("check_coverage", ROOT / "scripts" / "check-coverage.py")
cc = _ilu.module_from_spec(_spec)
sys.modules["check_coverage"] = cc
_spec.loader.exec_module(cc)


class TestFindCargoBin:
    def test_missing_tool_returns_none(self, monkeypatch):
        monkeypatch.setattr(cc.shutil, "which", lambda name: None)
        monkeypatch.setattr(cc.os.path, "isfile", lambda p: False)
        assert cc.find_cargo_bin("cargo-llvm-cov") is None

    def test_known_tool_returns_path(self, monkeypatch):
        monkeypatch.setattr(cc.shutil, "which", lambda name: "C:/tools/cargo-llvm-cov.exe")
        assert cc.find_cargo_bin("cargo-llvm-cov") == "C:/tools/cargo-llvm-cov.exe"


class TestRequireTool:
    def test_missing_tool_returns_error_message(self, monkeypatch):
        monkeypatch.setattr(cc, "find_cargo_bin", lambda name: None)
        err = cc.require_tool("cargo-llvm-cov", "cargo install cargo-llvm-cov --locked")
        assert err is not None
        assert "未安装" in err and "cargo install" in err

    def test_present_tool_returns_none(self, monkeypatch):
        monkeypatch.setattr(cc, "find_cargo_bin", lambda name: "cargo-llvm-cov")
        assert cc.require_tool("cargo-llvm-cov", "cargo install cargo-llvm-cov --locked") is None


class TestMainGracefulPreflight:
    def _make_args(self):
        import argparse

        return argparse.Namespace(
            package=None,
            modules=None,
            features="",
            threshold=0.8,
            branch_threshold=0.7,
        )

    def test_missing_tool_exits_one_without_traceback(self, monkeypatch, capsys):
        ns = self._make_args()
        monkeypatch.setattr(cc, "find_cargo_bin", lambda name: None)
        monkeypatch.setattr(cc.argparse.ArgumentParser, "parse_args", lambda parser: ns)
        rc = cc.main()
        out = capsys.readouterr().out
        assert rc == 1
        assert "未安装" in out
        assert "Traceback" not in out

    def test_existing_tool_reaches_llvm_cov(self, monkeypatch):
        ns = self._make_args()
        # 工具存在时 main 应继续到 llvm-cov 调用（这里让它失败也不应 traceback）
        monkeypatch.setattr(cc, "find_cargo_bin", lambda name: "cargo-llvm-cov")
        monkeypatch.setattr(cc.argparse.ArgumentParser, "parse_args", lambda parser: ns)

        def fake_run(cmd, **kwargs):
            raise AssertionError("不应执行到 subprocess.run 就提前返回")

        monkeypatch.setattr(cc.subprocess, "run", fake_run)
        # 存在工具时应继续执行；fake_run 抛 AssertionError 证明走到了 subprocess.run。
        with pytest.raises(AssertionError):
            cc.main()