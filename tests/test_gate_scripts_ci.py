#!/usr/bin/env python3
"""门禁 27 的 CI 登记源自测（2026-10-29 §11.6 P2-3 后半）。

单独成文而不写进 tests/test_gate_scripts.py：本文件只依赖 check-gate-scripts.py
的 CI 规则，可独立提交；共享文件里另有其他会话未落库的改动，不混入同一提交。

覆盖 check-gate-scripts.py 规则 7/8：
- 7：`.github/workflows/*.yml` 里未登记的 `|| true` / `|| :` 必须红牌，登记后必须放过
- 8：检查器只接入 CI 而未登记 CI_ONLY_EXEMPT 理由必须红牌；接入本地门禁则不需要理由
- fail-closed：workflows 目录读不到时按失败处理，不允许"没有文件所以通过"

运行：
    python -m pytest tests/test_gate_scripts_ci.py -v
"""

from __future__ import annotations

import importlib.util as _ilu
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"


def _load(name: str, path: Path):
    spec = _ilu.spec_from_file_location(name, path)
    mod = _ilu.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


gate_auditor = _load("check_gate_scripts_ci", SCRIPTS / "check-gate-scripts.py")


class TestGateAuditorCiSource:
    """登记源扩展到 CI 工作流后的回归。"""

    def _wf(self, tmp_path, body, name="ci.yml"):
        wdir = tmp_path / "workflows"
        wdir.mkdir(exist_ok=True)
        (wdir / name).write_text("\n".join(body) + "\n", encoding="utf-8")
        old = gate_auditor.WORKFLOWS
        gate_auditor.WORKFLOWS = wdir
        try:
            return gate_auditor.check_ci_gate()
        finally:
            gate_auditor.WORKFLOWS = old

    def _registry(self, tmp_path, required, ci_body="", local_body="", exempt=None):
        wdir = tmp_path / "workflows"
        wdir.mkdir(exist_ok=True)
        (wdir / "ci.yml").write_text(ci_body, encoding="utf-8")
        gate = tmp_path / "gate.sh"
        gate.write_text(local_body, encoding="utf-8")
        ps = tmp_path / "gate.ps1"
        ps.write_text("Write-Host ok\n", encoding="utf-8")
        old = (gate_auditor.WORKFLOWS, gate_auditor.SERVER_GATE, gate_auditor.PS_GATE,
               gate_auditor.REQUIRED_CHECKS, gate_auditor.CI_ONLY_EXEMPT)
        gate_auditor.WORKFLOWS, gate_auditor.SERVER_GATE, gate_auditor.PS_GATE = wdir, gate, ps
        gate_auditor.REQUIRED_CHECKS = list(required)
        gate_auditor.CI_ONLY_EXEMPT = dict(exempt or {})
        try:
            return gate_auditor.check_required_scripts()
        finally:
            (gate_auditor.WORKFLOWS, gate_auditor.SERVER_GATE, gate_auditor.PS_GATE,
             gate_auditor.REQUIRED_CHECKS, gate_auditor.CI_ONLY_EXEMPT) = old

    # ---- 规则 7：CI 命令行吞掉失败 ----

    def test_ci_unregistered_or_true_flagged(self, tmp_path):
        problems = self._wf(
            tmp_path,
            ["jobs:", "  test:", "    steps:", "      - run: cargo test --workspace || true"],
        )
        assert any("未登记豁免" in p for p in problems), problems

    def test_ci_registered_or_true_not_flagged(self, tmp_path):
        problems = self._wf(
            tmp_path,
            ["jobs:", "  udeps:", "    steps:",
             "      - run: cargo +nightly udeps --all-targets || true"],
        )
        assert problems == [], problems

    def test_ci_or_colon_flagged(self, tmp_path):
        problems = self._wf(
            tmp_path,
            ["jobs:", "  x:", "    steps:", "      - run: some-check || :"],
        )
        assert any("吞掉失败" in p for p in problems), problems

    def test_ci_reported_line_is_greppable(self, tmp_path):
        """报告里的行号必须能直接 goto（不做反斜杠续行合并）。"""
        problems = self._wf(
            tmp_path,
            ["jobs:", "  x:", "    steps:", "      - name: s", "        run: |",
             "          semgrep \\", "            --config=a.yaml \\", "            src/ || true"],
            name="semgrep.yml",
        )
        assert problems, problems
        import re as _re

        reported = int(_re.search(r"semgrep\.yml:(\d+)", problems[0]).group(1))
        text = (tmp_path / "workflows" / "semgrep.yml").read_text(encoding="utf-8").splitlines()
        assert "|| true" in text[reported - 1], text[reported - 1]

    def test_ci_fail_closed_when_workflows_missing(self, tmp_path):
        old = gate_auditor.WORKFLOWS
        gate_auditor.WORKFLOWS = tmp_path / "nope"
        try:
            problems = gate_auditor.check_ci_gate()
        finally:
            gate_auditor.WORKFLOWS = old
        assert any("无法审计" in p for p in problems), problems

    def test_ci_fail_closed_when_workflows_empty(self, tmp_path):
        wdir = tmp_path / "workflows"
        wdir.mkdir()
        old = gate_auditor.WORKFLOWS
        gate_auditor.WORKFLOWS = wdir
        try:
            problems = gate_auditor.check_ci_gate()
        finally:
            gate_auditor.WORKFLOWS = old
        assert any("没有任何" in p for p in problems), problems

    # ---- 规则 8：登记源归属 ----

    def test_ci_only_without_reason_flagged(self, tmp_path):
        missing, ownership = self._registry(
            tmp_path, ["check-alpha.py", "check-beta.py", "check-gate-scripts.py"],
            ci_body="run: python scripts/check-alpha.py\n", local_body="echo ok\n",
        )
        assert missing == ["check-beta.py"], (missing, ownership)
        assert any("check-alpha.py" in o and "CI_ONLY_EXEMPT" in o for o in ownership), ownership

    def test_ci_only_with_reason_passes(self, tmp_path):
        missing, ownership = self._registry(
            tmp_path, ["check-alpha.py"],
            ci_body="run: python scripts/check-alpha.py\n", local_body="echo ok\n",
            exempt={"check-alpha.py": "仅 CI 有运行时"},
        )
        assert missing == [] and ownership == [], (missing, ownership)

    def test_locally_wired_needs_no_ci_only_reason(self, tmp_path):
        missing, ownership = self._registry(
            tmp_path, ["check-alpha.py"],
            ci_body="nothing\n", local_body="python scripts/check-alpha.py\n",
        )
        assert missing == [] and ownership == [], (missing, ownership)

    # ---- 真实仓库必须保持通过（否则本文件是红灯的源头） ----

    def test_real_repo_ci_gate_and_registry_pass(self):
        assert gate_auditor.check_ci_gate() == []
        missing, ownership = gate_auditor.check_required_scripts()
        assert missing == [] and ownership == [], (missing, ownership)
