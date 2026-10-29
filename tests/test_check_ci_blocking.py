#!/usr/bin/env python3
"""
check-ci-blocking.py 自测（2026-10-27 SDET 评测门禁 27 整改）

验收标准：
- 豁免表内的 continue-on-error job 通过；
- 未登记的 continue-on-error job 红牌；
- 真实仓库当前 6 个非阻断 job 全部在豁免表内。

运行：
    python -m pytest tests/test_check_ci_blocking.py -v
"""

from __future__ import annotations

import importlib.util as _ilu
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent

_spec = _ilu.spec_from_file_location("check_ci_blocking", ROOT / "scripts" / "check-ci-blocking.py")
ccb = _ilu.module_from_spec(_spec)
sys.modules["check_ci_blocking"] = ccb
_spec.loader.exec_module(ccb)

try:
    import yaml
except ImportError:  # pragma: no cover
    yaml = None


def _write_workflow(tmp_path, jobs):
    """构造含 jobs 的 workflow 文件。"""
    wf_dir = tmp_path / "workflows"
    wf_dir.mkdir(parents=True, exist_ok=True)
    doc = {"name": "CI", "on": {"push": {"branches": ["main"]}}, "jobs": jobs}
    (wf_dir / "ci.yml").write_text(
        yaml.safe_dump(doc, allow_unicode=True), encoding="utf-8"
    )
    return wf_dir


class TestAuditWorkflows:
    def test_exempt_jobs_pass(self, tmp_path):
        jobs = {
            "benchmark": {"runs-on": "ubuntu-latest", "continue-on-error": True, "steps": []},
            "fuzz": {"runs-on": "ubuntu-latest", "continue-on-error": "true", "steps": []},
        }
        problems, total = ccb.audit_workflows(_write_workflow(tmp_path, jobs))
        assert problems == []
        assert total == 2

    def test_unregistered_job_is_flagged(self, tmp_path):
        jobs = {
            "my-new-gate": {"runs-on": "ubuntu-latest", "continue-on-error": True, "steps": []},
        }
        problems, total = ccb.audit_workflows(_write_workflow(tmp_path, jobs))
        assert total == 1
        assert any("my-new-gate" in p for p in problems)

    def test_non_blocking_jobs_not_counted(self, tmp_path):
        jobs = {
            "lint": {"runs-on": "ubuntu-latest", "steps": []},
            "test": {"runs-on": "ubuntu-latest", "continue-on-error": False, "steps": []},
        }
        problems, total = ccb.audit_workflows(_write_workflow(tmp_path, jobs))
        assert problems == []
        assert total == 0

    def test_invalid_yaml_is_flagged(self, tmp_path):
        wf_dir = tmp_path / "workflows"
        wf_dir.mkdir(parents=True, exist_ok=True)
        (wf_dir / "bad.yml").write_text("jobs: [unclosed", encoding="utf-8")
        problems, _ = ccb.audit_workflows(wf_dir)
        assert any("YAML 解析失败" in p for p in problems)


class TestExemptRegistry:
    def test_current_repo_all_non_blocking_jobs_exempt(self):
        """真实仓库：workflow 中所有 job 级 continue-on-error（无条件 + 条件式）
        必须全部在豁免表内。2026-10-27 并发会话已将 integration/mq-integration/
        benchmark/fuzz 改为条件式（main 分支阻断、PR 分支不阻断），仍登记豁免。"""
        if yaml is None:
            pytest.skip("PyYAML 未安装")
        problems, total = ccb.audit_workflows(ROOT / ".github" / "workflows")
        assert total >= 5, "仓库应至少含 5 个 job 级 continue-on-error（SDET 评测盘点 + 并发改造）"
        assert problems == [], f"存在未登记豁免的非阻断 job: {problems}"

    def test_exempt_jobs_registry_has_reasons(self):
        assert len(ccb.EXEMPT_JOBS) >= 6
        for job, reason in ccb.EXEMPT_JOBS.items():
            assert reason and len(reason) > 10, f"{job} 缺少理由"