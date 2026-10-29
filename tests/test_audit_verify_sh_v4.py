#!/usr/bin/env python3
"""audit-verify.sh v4 自测（2026-10-29 §11.6 P1-4 后半：服务器口径与 .ps1 对齐）

覆盖 v4 相对 v3 新增的四项能力：
1. 符号级断言 `[must-contain: X]` / `[must-not-contain: X]`（含"标记只绑定紧邻前一条
   引用"与"一个标记 = 一个字面子串，逗号不分割"两条语义约束）；
2. 通配覆盖：`scripts/*.py:NN`、`docs/*.md:NN` 等任意扩展名引用进入校验；
3. CJK token：纯中文结论不再因"ASCII 标识符集合为空"整体跳过语义校验；
4. --at-head：被引内容取 `git show HEAD:<path>`，且路径在 HEAD 不存在时 fail-closed。

v3 的四项行为由 tests/test_audit_verify.py 锁定（另一会话维护），本文件只测新增面。

运行：python -m pytest tests/test_audit_verify_sh_v4.py -v
bash 不可用时跳过。
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
    """优先 Git Bash；WSL bash 对 D:/ 路径语义不同，不能用于本仓库脚本。"""
    candidates = [
        os.environ.get("SZ_ORM_BASH"),
        r"D:\Git\bin\bash.exe",
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
        shutil.which("bash"),
    ]
    for cand in candidates:
        if cand and Path(cand).exists():
            if "system32" in cand.lower():
                continue
            return cand
    return None


_BASH = _find_bash()
pytestmark = pytest.mark.skipif(_BASH is None, reason="Git Bash 不可用")


def run_verify(report: Path, *extra: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [_BASH, SCRIPT.as_posix(), report.as_posix(), *extra],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )


# 稳定的仓库内锚点：check-gate-scripts.py 的 CI 检查函数定义行
PY_REF = "scripts/check-gate-scripts.py:156"
PY_NEEDLE = "def check_ci_gate"
POOL_REF = "packages/sz-orm-core/src/pool.rs:2372"
POOL_NEEDLE = "条件 notify_one"


def write(tmp_path: Path, body: str) -> Path:
    report = tmp_path / "r.md"
    report.write_text("# 报告\n" + body, encoding="utf-8")
    return report


class TestWildcardCoverage:
    """v3 完全不校验 .py/.md/.yml 等扩展名引用；v4 必须读进来看。"""

    def test_py_ref_now_validated(self, tmp_path):
        r = run_verify(write(tmp_path, f"CI 检查器入口见 {PY_REF}。\n"))
        assert r.returncode == 0, r.stdout + r.stderr
        assert f"✅ {PY_REF}" in r.stdout, "任意扩展名引用必须进入校验"

    def test_unknown_dir_path_is_fail_closed(self, tmp_path):
        r = run_verify(write(tmp_path, "证据：scripts/no_such_checker_xyz.py:9\n"))
        assert r.returncode == 1, "带目录的不存在路径必须红牌，不能静默跳过"
        assert "文件不存在" in r.stdout


class TestSymbolAssertions:
    def test_must_contain_passes(self, tmp_path):
        r = run_verify(write(tmp_path, f"入口 {PY_REF} [must-contain: {PY_NEEDLE}]\n"))
        assert r.returncode == 0, r.stdout + r.stderr
        assert f"✅ {PY_REF}" in r.stdout

    def test_must_contain_failure_is_red(self, tmp_path):
        r = run_verify(write(tmp_path, f"{PY_REF} [must-contain: def check_never_written]\n"))
        assert r.returncode == 1, "断言不成立必须红牌（v3 对此完全无感）"
        assert "断言失败" in r.stdout and "must-contain" in r.stdout

    def test_must_not_contain_failure_is_red(self, tmp_path):
        r = run_verify(write(tmp_path, f"{POOL_REF} [must-not-contain: {POOL_NEEDLE}]\n"))
        assert r.returncode == 1
        assert "断言失败" in r.stdout and "must-not-contain" in r.stdout

    def test_two_markers_are_two_needles_not_csv(self, tmp_path):
        """一个标记 = 一个字面子串：逗号不分割，多条必须重复标记。"""
        ok = f"两处锚点 {PY_REF} [must-contain: {PY_NEEDLE}][must-contain: check_ci_gate]\n"
        assert run_verify(write(tmp_path, ok)).returncode == 0
        csv = f"逗号写法 {PY_REF} [must-contain: {PY_NEEDLE}, check_ci_gate]\n"
        r = run_verify(write(tmp_path, csv))
        assert r.returncode == 1, "把逗号当分隔符的写法必须判失败"

    def test_needle_binds_only_to_preceding_citation(self, tmp_path):
        """同行两条引用：标记只作用于紧邻的前一条，不得误伤后一条。"""
        body = (
            f"先引用 {POOL_REF} [must-contain: {POOL_NEEDLE}]，"
            f"再引用 {PY_REF}，行尾无标记。\n"
        )
        r = run_verify(write(tmp_path, body))
        assert r.returncode == 0, r.stdout + r.stderr
        assert f"✅ {POOL_REF}" in r.stdout
        assert f"✅ {PY_REF}" in r.stdout
        assert "断言失败" not in r.stdout

    def test_marker_after_second_citation_binds_second(self, tmp_path):
        """标记跟在第二条引用之后：断言对象是第二条，第一条不受影响。"""
        body = (
            f"先 {POOL_REF}，再 {PY_REF} [must-contain: 不存在于该行的文字]\n"
        )
        r = run_verify(write(tmp_path, body))
        assert r.returncode == 1
        assert f"❌ {PY_REF} — 断言失败" in r.stdout, "失败应归给第二条引用"
        assert f"✅ {POOL_REF}" in r.stdout, "第一条不应被误判"


class TestCjkTokens:
    """只取 ASCII 会让纯中文断言得到空集合，从而整体跳过语义校验（v3 的旁路）。"""

    def test_pure_chinese_mismatch_warns(self, tmp_path):
        body = f"量子随机数生成器已实装并默认启用（{POOL_REF}）\n"
        v3_like = run_verify(write(tmp_path, body))
        assert "弱语义校验" in v3_like.stdout, "中文对中文也必须做语义比对"

    def test_pure_chinese_shared_run_no_warning(self, tmp_path):
        # 被引行 `// v8.8.0 优化：条件 notify_one。` 的中文词块是「优化」「条件」。
        # 引用行的 ASCII token 只有 packages/core/pool，与被引行的 notify_one 等
        # 零交集——本用例只能通过「条件」这个中文词块放行，因此它同时是
        # "CJK 分支真的读到了输入" 的回归锁（若 CJK 提取失效，这里必然警告）。
        body = f"连接池唤醒优化：条件 判定（{POOL_REF}）\n"
        r = run_verify(write(tmp_path, body))
        assert "弱语义校验" not in r.stdout, "共享中文词块即视为可信引用"


class TestAtHead:
    def test_banner_reports_scope(self, tmp_path):
        r = run_verify(write(tmp_path, f"锚点 {PY_REF} [must-contain: {PY_NEEDLE}]\n"), "--at-head")
        assert r.returncode == 0, r.stdout + r.stderr
        assert "内容口径: HEAD 提交" in r.stdout

    def test_missing_in_head_is_fail_closed(self, tmp_path):
        r = run_verify(write(tmp_path, "证据：scripts/no_such_checker_xyz.py:9\n"), "--at-head")
        assert r.returncode == 1
        assert "文件不存在（HEAD 提交内容中）" in r.stdout

    def test_head_mode_shields_committed_drift(self, tmp_path):
        """--at-head 的真实价值：工作区在制品造成的行号漂移不再产生假红牌。

        前提检查：README.zh.md:44 的 feature 数字在工作区与 HEAD 是否真的不同；
        一致时（他人在制品已落库）本用例跳过，不伪装成通过。
        """
        wt = (ROOT / "README.zh.md").read_text(encoding="utf-8").splitlines()[43]
        head = subprocess.run(
            ["git", "show", "HEAD:README.zh.md"], cwd=str(ROOT),
            capture_output=True, text=True, encoding="utf-8",
        ).stdout.splitlines()[43]
        if wt == head:
            pytest.skip("工作区与 HEAD 的 README.zh.md:44 一致，漂移前提不成立")
        needle_wt = wt[wt.index("提供 ") + 3 : wt.index(" 个 feature")]
        needle_head = head[head.index("提供 ") + 3 : head.index(" 个 feature")]
        body = f"feature 数量见 README.zh.md:44 [must-contain: 提供 {needle_head} 个 feature]\n"
        report = write(tmp_path, body)
        assert run_verify(report).returncode == 1, "工作区口径下 HEAD 的旧数字应判断言失败"
        assert run_verify(report, "--at-head").returncode == 0, "--at-head 应按提交内容放行"
        assert needle_wt != needle_head


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v"]))
