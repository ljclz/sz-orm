#!/usr/bin/env python3
"""
scripts/check-doc-sync.py 单元测试

覆盖：
- 规则加载与匹配
- git diff 解析
- 跳过标记
- 退出码（正常/未同步/错误）

运行：
    python -m pytest tests/test_check_doc_sync.py -v
    python -m pytest tests/test_check_doc_sync.py --cov=scripts/check_doc_sync
"""

from __future__ import annotations

import os
import subprocess
import sys
import textwrap
from pathlib import Path
from unittest.mock import patch

import pytest

# 通过 importlib 从文件路径加载被测模块（避免 pytest 收集阶段 sys.path 失效）
SCRIPTS_DIR = Path(__file__).resolve().parent.parent / "scripts"
import importlib.util as _ilu

_spec = _ilu.spec_from_file_location("check_doc_sync", SCRIPTS_DIR / "check-doc-sync.py")
cds = _ilu.module_from_spec(_spec)
sys.modules["check_doc_sync"] = cds  # 注册到 sys.modules 以支持 dataclass 内省
_spec.loader.exec_module(cds)

ROOT = Path(__file__).resolve().parent.parent


# ========== 规则加载 ==========

class TestRuleLoading:
    def test_load_rules_returns_10_rules(self):
        rules = cds.load_rules()
        assert len(rules) == 10

    def test_rule_ids_are_1_to_10(self):
        rules = cds.load_rules()
        ids = sorted(r.rule_id for r in rules)
        assert ids == list(range(1, 11))

    def test_rule_has_required_fields(self):
        rules = cds.load_rules()
        for r in rules:
            assert r.rule_id > 0
            assert r.name
            assert r.description
            assert r.code_patterns
            assert r.affected_docs

    def test_load_rules_missing_file_exits_with_error(self, tmp_path):
        with pytest.raises(SystemExit) as exc:
            cds.load_rules(tmp_path / "nonexistent.yaml")
        assert exc.value.code == cds.EXIT_ERROR

    def test_load_rules_invalid_yaml_exits_with_error(self, tmp_path):
        bad = tmp_path / "bad.yaml"
        bad.write_text("rules: [invalid: yaml: content", encoding="utf-8")
        with pytest.raises(SystemExit) as exc:
            cds.load_rules(bad)
        assert exc.value.code == cds.EXIT_ERROR


# ========== 规则匹配 ==========

class TestRuleMatching:
    def test_matches_code_path_glob(self):
        rules = cds.load_rules()
        rule1 = next(r for r in rules if r.rule_id == 1)
        assert rule1.matches_code_path("Cargo.toml")
        assert rule1.matches_code_path("packages/sz-orm-core/Cargo.toml")

    def test_matches_code_path_no_match(self):
        rules = cds.load_rules()
        rule4 = next(r for r in rules if r.rule_id == 4)
        assert not rule4.matches_code_path("Cargo.toml")
        assert rule4.matches_code_path("packages/sz-orm-core/src/pool.rs")

    def test_matches_diff_line_no_regex(self):
        rules = cds.load_rules()
        rule4 = next(r for r in rules if r.rule_id == 4)
        # 无 code_regex，始终返回 True
        assert rule4.matches_diff_line("anything")

    def test_matches_diff_line_with_regex(self):
        rules = cds.load_rules()
        rule1 = next(r for r in rules if r.rule_id == 1)
        assert rule1.matches_diff_line('+version = "3.5.0"')
        assert not rule1.matches_diff_line('+edition = "2021"')

    def test_rule_3_pub_api_regex(self):
        rules = cds.load_rules()
        rule3 = next(r for r in rules if r.rule_id == 3)
        assert rule3.matches_diff_line("+pub fn new() -> Self {")
        assert rule3.matches_diff_line("+pub struct Foo;")
        assert rule3.matches_diff_line("+pub enum Bar {")
        assert not rule3.matches_diff_line("+fn private() {}")
        assert not rule3.matches_diff_line("+// pub comment")


# ========== DiffEntry / CheckResult 数据类 ==========

class TestDataclasses:
    def test_diff_entry_is_doc(self):
        e = cds.DiffEntry(path="README.md")
        assert e.is_doc is True

        e2 = cds.DiffEntry(path="src/main.rs")
        assert e2.is_doc is False

    def test_diff_entry_added_lines_default(self):
        e = cds.DiffEntry(path="x.rs")
        assert e.added_lines == []
        assert e.modified is False

    def test_check_result_default(self):
        rule = cds.SyncRule(
            rule_id=99,
            name="test",
            description="test",
            code_patterns=["*.rs"],
            affected_docs=["README.md"],
        )
        r = cds.CheckResult(rule=rule, triggered=False)
        assert r.triggered_by == []
        assert r.missing_docs == []
        assert r.skipped_docs == []


# ========== git diff 解析 ==========

class TestGitDiffParsing:
    def test_parse_git_diff_name_only_returns_list(self):
        # 在仓库内调用，应返回 list[str]
        result = cds.parse_git_diff_name_only(None, None, "HEAD~1..HEAD")
        assert isinstance(result, list)
        # 每个元素都是字符串
        for item in result:
            assert isinstance(item, str)

    def test_parse_git_diff_content_returns_dict(self):
        result = cds.parse_git_diff_content(None, None, "HEAD~1..HEAD")
        assert isinstance(result, dict)
        for k, v in result.items():
            assert isinstance(k, str)
            assert isinstance(v, cds.DiffEntry)

    def test_run_git_returns_tuple(self):
        rc, out, err = cds.run_git(["--version"])
        assert rc == 0
        assert "git version" in out


# ========== 跳过标记 ==========

class TestSkipMarker:
    def test_get_skip_marker_default(self):
        marker = cds.get_skip_marker()
        assert marker == "# doc-sync-skip"

    def test_doc_has_skip_marker_true(self, tmp_path):
        doc = tmp_path / "test.md"
        doc.write_text("# Title\n\n# doc-sync-skip\n", encoding="utf-8")
        with patch.object(cds, "ROOT", tmp_path):
            assert cds.doc_has_skip_marker(Path("test.md"), "# doc-sync-skip") is True

    def test_doc_has_skip_marker_false(self, tmp_path):
        doc = tmp_path / "test.md"
        doc.write_text("# Title\n\nNo marker here\n", encoding="utf-8")
        with patch.object(cds, "ROOT", tmp_path):
            assert cds.doc_has_skip_marker(Path("test.md"), "# doc-sync-skip") is False

    def test_doc_has_skip_marker_nonexistent(self, tmp_path):
        with patch.object(cds, "ROOT", tmp_path):
            assert cds.doc_has_skip_marker(Path("nonexistent.md"), "# doc-sync-skip") is False


# ========== 文档修改检查 ==========

class TestDocModified:
    def test_doc_was_modified_true(self):
        entries = {"README.md": cds.DiffEntry(path="README.md", modified=True)}
        assert cds.doc_was_modified("README.md", entries) is True

    def test_doc_was_modified_false(self):
        entries = {"src/main.rs": cds.DiffEntry(path="src/main.rs")}
        assert cds.doc_was_modified("README.md", entries) is False

    def test_doc_was_modified_path_variant(self):
        entries = {"foo/bar/README.md": cds.DiffEntry(path="foo/bar/README.md")}
        assert cds.doc_was_modified("README.md", entries) is True


# ========== 规则评估 ==========

class TestEvaluateRule:
    def _make_rule(self) -> cds.SyncRule:
        return cds.SyncRule(
            rule_id=99,
            name="test-rule",
            description="test",
            code_patterns=["Cargo.toml"],
            affected_docs=["README.md"],
            code_regex=r'^\+version\s*=\s*"\d+\.\d+\.\d+"',
        )

    def test_not_triggered(self):
        rule = self._make_rule()
        entries = {"src/main.rs": cds.DiffEntry(path="src/main.rs", added_lines=["+fn foo() {}"])}
        result = cds.evaluate_rule(rule, entries, set(), "# doc-sync-skip")
        assert result.triggered is False
        assert result.missing_docs == []

    def test_triggered_and_synced(self, tmp_path):
        rule = self._make_rule()
        entries = {
            "Cargo.toml": cds.DiffEntry(path="Cargo.toml", added_lines=['+version = "3.5.0"']),
            "README.md": cds.DiffEntry(path="README.md", modified=True),
        }
        # 创建临时 README.md
        readme = tmp_path / "README.md"
        readme.write_text("# README\n", encoding="utf-8")
        with patch.object(cds, "ROOT", tmp_path):
            result = cds.evaluate_rule(rule, entries, set(), "# doc-sync-skip")
        assert result.triggered is True
        assert result.missing_docs == []
        assert "README.md" in result.affected_docs

    def test_triggered_and_missing(self, tmp_path):
        rule = self._make_rule()
        entries = {
            "Cargo.toml": cds.DiffEntry(path="Cargo.toml", added_lines=['+version = "3.5.0"']),
        }
        readme = tmp_path / "README.md"
        readme.write_text("# README\n", encoding="utf-8")
        with patch.object(cds, "ROOT", tmp_path):
            result = cds.evaluate_rule(rule, entries, set(), "# doc-sync-skip")
        assert result.triggered is True
        assert "README.md" in result.missing_docs

    def test_triggered_and_skip_marker(self, tmp_path):
        rule = self._make_rule()
        entries = {
            "Cargo.toml": cds.DiffEntry(path="Cargo.toml", added_lines=['+version = "3.5.0"']),
        }
        readme = tmp_path / "README.md"
        readme.write_text("# README\n\n# doc-sync-skip\n", encoding="utf-8")
        with patch.object(cds, "ROOT", tmp_path):
            result = cds.evaluate_rule(rule, entries, set(), "# doc-sync-skip")
        assert result.triggered is True
        assert result.missing_docs == []
        assert "README.md" in result.skipped_docs

    def test_triggered_and_skip_file(self, tmp_path):
        rule = self._make_rule()
        entries = {
            "Cargo.toml": cds.DiffEntry(path="Cargo.toml", added_lines=['+version = "3.5.0"']),
        }
        readme = tmp_path / "README.md"
        readme.write_text("# README\n", encoding="utf-8")
        with patch.object(cds, "ROOT", tmp_path):
            result = cds.evaluate_rule(rule, entries, {"README.md"}, "# doc-sync-skip")
        assert result.triggered is True
        assert result.missing_docs == []
        assert "README.md" in result.skipped_docs

    def test_nonexistent_doc_not_missing(self, tmp_path):
        rule = self._make_rule()
        entries = {
            "Cargo.toml": cds.DiffEntry(path="Cargo.toml", added_lines=['+version = "3.5.0"']),
        }
        # 不创建 README.md
        with patch.object(cds, "ROOT", tmp_path):
            result = cds.evaluate_rule(rule, entries, set(), "# doc-sync-skip")
        assert result.triggered is True
        # 文档不存在不算缺失
        assert result.missing_docs == []


# ========== 退出码常量 ==========

class TestExitCodes:
    def test_exit_constants(self):
        assert cds.EXIT_OK == 0
        assert cds.EXIT_NOT_SYNCED == 1
        assert cds.EXIT_ERROR == 2


# ========== 端到端 main ==========

class TestMain:
    def test_main_list_rules(self, capsys):
        rc = cds.main(["--list-rules"])
        assert rc == cds.EXIT_OK
        captured = capsys.readouterr()
        assert "Loaded 10 rules" in captured.out

    def test_main_help_exits_zero(self):
        with pytest.raises(SystemExit) as exc:
            cds.main(["--help"])
        assert exc.value.code == 0

    def test_main_no_changes(self, capsys):
        # 使用空 diff 范围（同一 commit 对比）
        rc = cds.main(["--diff", "HEAD..HEAD"])
        assert rc == cds.EXIT_OK


# ========== 端到端集成场景（M1-T2.4 同步覆盖） ==========

class TestEndToEndScenarios:
    """三种场景：未同步（退出 1）/ 同步（退出 0）/ 跳过标记（退出 0）。"""

    def _run_script(self, args: list[str], cwd: Path) -> tuple[int, str, str]:
        proc = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "check-doc-sync.py")] + args,
            cwd=str(cwd),
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        return proc.returncode, proc.stdout, proc.stderr

    def test_scenario_not_synced(self, tmp_path):
        """场景 1：代码变更未同步文档 → 退出码 1。"""
        # 构造最小 git 仓库
        self._init_mini_repo(tmp_path)
        # 修改 Cargo.toml 触发规则 1
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text('[package]\nname = "x"\nversion = "3.5.0"\n', encoding="utf-8")
        # 创建 README.md（受影响文档，但本次不修改）
        (tmp_path / "README.md").write_text("# README\n", encoding="utf-8")
        # 复制规则文件
        rules_dst = tmp_path / "doc-sync-rules.yaml"
        rules_dst.write_text(
            textwrap.dedent(
                """
                rules:
                  - id: 1
                    name: cargo-version
                    description: test
                    code_patterns: ["Cargo.toml"]
                    code_regex: '^\\+version\\s*=\\s*"\\d+\\.\\d+\\.\\d+"'
                    affected_docs: ["README.md"]
                config:
                  skip_marker: "# doc-sync-skip"
                """
            ).strip(),
            encoding="utf-8",
        )
        subprocess.run(["git", "add", "-A"], cwd=str(tmp_path), capture_output=True)
        subprocess.run(
            ["git", "commit", "-m", "test"],
            cwd=str(tmp_path),
            capture_output=True,
            env={**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"},
        )
        # 再次修改 Cargo.toml 但不修改 README.md
        cargo.write_text('[package]\nname = "x"\nversion = "3.5.1"\n', encoding="utf-8")
        subprocess.run(["git", "add", "-A"], cwd=str(tmp_path), capture_output=True)
        subprocess.run(
            ["git", "commit", "-m", "bump"],
            cwd=str(tmp_path),
            capture_output=True,
            env={**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"},
        )

        rc, out, err = self._run_script(["--diff", "HEAD~1..HEAD", "--rules", str(rules_dst), "--root", str(tmp_path)], tmp_path)
        assert rc == 1, f"expected exit 1, got {rc}; stdout={out}; stderr={err}"

    def test_scenario_synced(self, tmp_path):
        """场景 2：代码变更 + 同步文档 → 退出码 0。"""
        self._init_mini_repo(tmp_path)
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text('[package]\nname = "x"\nversion = "3.5.0"\n', encoding="utf-8")
        (tmp_path / "README.md").write_text("# README\n", encoding="utf-8")
        rules_dst = tmp_path / "doc-sync-rules.yaml"
        rules_dst.write_text(
            textwrap.dedent(
                """
                rules:
                  - id: 1
                    name: cargo-version
                    description: test
                    code_patterns: ["Cargo.toml"]
                    code_regex: '^\\+version\\s*=\\s*"\\d+\\.\\d+\\.\\d+"'
                    affected_docs: ["README.md"]
                config:
                  skip_marker: "# doc-sync-skip"
                """
            ).strip(),
            encoding="utf-8",
        )
        subprocess.run(["git", "add", "-A"], cwd=str(tmp_path), capture_output=True)
        subprocess.run(
            ["git", "commit", "-m", "init"],
            cwd=str(tmp_path),
            capture_output=True,
            env={**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"},
        )
        # 同时修改 Cargo.toml 和 README.md
        cargo.write_text('[package]\nname = "x"\nversion = "3.5.1"\n', encoding="utf-8")
        (tmp_path / "README.md").write_text("# README v3.5.1\n", encoding="utf-8")
        subprocess.run(["git", "add", "-A"], cwd=str(tmp_path), capture_output=True)
        subprocess.run(
            ["git", "commit", "-m", "bump+doc"],
            cwd=str(tmp_path),
            capture_output=True,
            env={**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"},
        )

        rc, out, err = self._run_script(["--diff", "HEAD~1..HEAD", "--rules", str(rules_dst), "--root", str(tmp_path)], tmp_path)
        assert rc == 0, f"expected exit 0, got {rc}; stdout={out}; stderr={err}"

    def test_scenario_skip_marker(self, tmp_path):
        """场景 3：代码变更 + 文档含跳过标记 → 退出码 0。"""
        self._init_mini_repo(tmp_path)
        cargo = tmp_path / "Cargo.toml"
        cargo.write_text('[package]\nname = "x"\nversion = "3.5.0"\n', encoding="utf-8")
        (tmp_path / "README.md").write_text("# README\n\n# doc-sync-skip\n", encoding="utf-8")
        rules_dst = tmp_path / "doc-sync-rules.yaml"
        rules_dst.write_text(
            textwrap.dedent(
                """
                rules:
                  - id: 1
                    name: cargo-version
                    description: test
                    code_patterns: ["Cargo.toml"]
                    code_regex: '^\\+version\\s*=\\s*"\\d+\\.\\d+\\.\\d+"'
                    affected_docs: ["README.md"]
                config:
                  skip_marker: "# doc-sync-skip"
                """
            ).strip(),
            encoding="utf-8",
        )
        subprocess.run(["git", "add", "-A"], cwd=str(tmp_path), capture_output=True)
        subprocess.run(
            ["git", "commit", "-m", "init"],
            cwd=str(tmp_path),
            capture_output=True,
            env={**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"},
        )
        cargo.write_text('[package]\nname = "x"\nversion = "3.5.1"\n', encoding="utf-8")
        subprocess.run(["git", "add", "-A"], cwd=str(tmp_path), capture_output=True)
        subprocess.run(
            ["git", "commit", "-m", "bump"],
            cwd=str(tmp_path),
            capture_output=True,
            env={**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"},
        )

        rc, out, err = self._run_script(["--diff", "HEAD~1..HEAD", "--rules", str(rules_dst), "--root", str(tmp_path)], tmp_path)
        assert rc == 0, f"expected exit 0, got {rc}; stdout={out}; stderr={err}"

    @staticmethod
    def _init_mini_repo(path: Path) -> None:
        subprocess.run(["git", "init", "-q"], cwd=str(path), capture_output=True)
        subprocess.run(["git", "config", "user.name", "t"], cwd=str(path), capture_output=True)
        subprocess.run(["git", "config", "user.email", "t@t"], cwd=str(path), capture_output=True)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])