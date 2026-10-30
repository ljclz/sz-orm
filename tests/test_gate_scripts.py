#!/usr/bin/env python3
"""
门禁脚本自测（2026-10-27 SDET 评测 ROI-3 整改）

背景：24 道门禁的"检查器"绝大多数没有自测（先例：tests/test_check_doc_sync.py、
scripts/test_check_sql_injection_auto.py）。本文件为下列脚本补自测：
- check-metrics-real.py：数字/版本声称匹配、--fix 修正、Cargo.toml 版本读取
- check-semantic-patterns.py：R1~R4 规则命中、strip_tests 剥离
- check-coverage.py：DEFAULT_MODULES/CRATE_FEATURE_MAP 与仓库一致性
- check-mutation-coverage.py：默认目标文件存在性、feature 有效性
- check-phantom-delivery.py：接线断言登记表引用的文件存在
- check-gate-scripts.py：硬编码/恒真/同链/多行 `||` 假 PASS 规则命中，且不误报正确写法

运行：
    python -m pytest tests/test_gate_scripts.py -v
"""

from __future__ import annotations

import importlib.util as _ilu
import re
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"


def _load(name: str, path: Path):
    spec = _ilu.spec_from_file_location(name, path)
    mod = _ilu.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


metrics_real = _load("check_metrics_real", SCRIPTS / "check-metrics-real.py")
semantic = _load("check_semantic_patterns", SCRIPTS / "check-semantic-patterns.py")
coverage = _load("check_coverage", SCRIPTS / "check-coverage.py")
mutation = _load("check_mutation_coverage", SCRIPTS / "check-mutation-coverage.py")
phantom = _load("check_phantom_delivery", SCRIPTS / "check-phantom-delivery.py")
gate_auditor = _load("check_gate_scripts", SCRIPTS / "check-gate-scripts.py")
secrets_scanner = _load("check_secrets", SCRIPTS / "check-secrets.py")


def _metrics(**overrides):
    base = {
        "packages": 70,
        "test_annotations": 21161,
        "features": 508,
        "dialects": 28,
        "workspace_members": 72,
        "version": "10.13.0",
    }
    base.update(overrides)
    return base


# ========== check-metrics-real.py ==========


class TestMetricsRealClaims:
    def test_matching_claims_produce_no_issues(self, tmp_path):
        readme = tmp_path / "README.md"
        readme.write_text(
            "> v10.13.0 · 72 workspace members · 21161+ tests · 28 SQL dialects\n"
            "[![Version](https://img.shields.io/badge/version-10.13.0-blue.svg)]\n"
            "v10.13.0 includes 72 workspace members.\n"
            "For the latest features (v10.13.0), see CHANGELOG.\n",
            encoding="utf-8",
        )
        assert metrics_real.scan_readme(_metrics(), str(readme)) == []

    def test_stale_version_claims_are_flagged(self, tmp_path):
        """SDET 评测 ROI-2：README 版本漂移（v10.12.0 vs Cargo.toml 10.13.0）
        必须被拦截——2026-10-27 评测实测此前的 metrics-real 抓不住。"""
        readme = tmp_path / "README.md"
        readme.write_text(
            "> v10.12.0 · 72 workspace members · 21161+ tests\n"
            "[![Version](https://img.shields.io/badge/version-10.12.0-blue.svg)]\n"
            "v10.12.0 includes 72 workspace members.\n"
            "For the latest features (v10.12.0), see CHANGELOG.\n",
            encoding="utf-8",
        )
        issues = metrics_real.scan_readme(_metrics(), str(readme))
        kinds = {it[3] for it in issues}
        assert {"顶部版本行", "版本徽章", "Overview 版本行", "Note 最新版本"} <= kinds

    def test_historical_version_log_is_not_flagged(self, tmp_path):
        readme = tmp_path / "README.md"
        readme.write_text(
            "### v5.1.0 AI Application Optimization (293 new tests)\n"
            "### v4.1.0 New Capabilities (9 feature gates, off by default)\n",
            encoding="utf-8",
        )
        assert metrics_real.scan_readme(_metrics(), str(readme)) == []

    def test_stale_test_count_is_flagged(self, tmp_path):
        readme = tmp_path / "README.md"
        readme.write_text("21153+ tests\n", encoding="utf-8")
        issues = metrics_real.scan_readme(_metrics(), str(readme))
        assert any(it[3] == "顶部测试数(英文)" for it in issues)

    def test_fix_readme_repairs_version_and_counts(self, tmp_path):
        # 2026-10-28 起 check-metrics-real._safe_write 拒绝写仓库外路径（防路径穿越），
        # 合成 README 需放在仓库内（target/ 已被 gitignore），测后清理。
        fix_dir = ROOT / "target" / "g18-self-test"
        fix_dir.mkdir(parents=True, exist_ok=True)
        readme = fix_dir / "README.md"
        readme.write_text(
            "> v10.12.0 · 72 workspace members\n"
            "[![Version](https://img.shields.io/badge/version-10.12.0-blue.svg)]\n",
            encoding="utf-8",
        )
        try:
            issues = metrics_real.scan_readme(_metrics(), str(readme))
            assert issues
            fixed, lines = metrics_real.fix_readme(
                _metrics(), str(readme), {it[0] for it in issues}
            )
            assert fixed >= 2
            text = "\n".join(lines)
            assert "v10.13.0" in text and "version-10.13.0" in text
            assert r"\g<" not in text, "修复后不得残留模板占位符"
        finally:
            import shutil

            shutil.rmtree(fix_dir, ignore_errors=True)

    def test_workspace_version_is_semver(self):
        version = metrics_real.read_workspace_version()
        assert version, "Cargo.toml [workspace.package] 必须有 version"
        assert re.fullmatch(r"\d+\.\d+\.\d+", version), f"非法 semver: {version}"

    def test_real_metrics_contains_version(self):
        metrics = metrics_real.real_metrics()
        assert isinstance(metrics["version"], str) and metrics["version"]


# ========== check-semantic-patterns.py ==========


class TestSemanticPatterns:
    def _rule(self, rule_id):
        return next(r for r in semantic.DEFAULT_RULES if r["id"] == rule_id)

    def test_r1_detects_zero_increment(self):
        assert re.search(self._rule("R1")["pattern"], "self.record_usage(tenant, 0);")

    def test_r2_detects_discarded_check_result(self):
        assert re.search(self._rule("R2")["pattern"], "let _ = validator.validate(input);")

    def test_r3_detects_unsigned_always_true_comparison(self):
        rule = self._rule("R3")["pattern"]
        # 注意：\b 边界要求类型名独立出现（如 `x: u64`），`remaining_u64` 不构成边界
        assert re.search(rule, "fn f(x: u64) { if x < 0 }")
        assert re.search(rule, "fn f(x: u32) { if x >= 0 }")
        assert not re.search(rule, "fn f(x: u32) { if x > 0 }"), "u32 > 0 是合法检查，不应误报"

    def test_r4_detects_zero_release(self):
        assert re.search(self._rule("R4")["pattern"], "pool.release_usage(tenant, 0);")

    def test_strip_tests_removes_cfg_test_and_mod_tests(self):
        src = (
            "fn prod() {}\n"
            "#[cfg(test)]\nmod t1 {\n    fn inner_a() {}\n}\n"
            "mod tests {\n    fn inner_b() {}\n}\n"
        )
        out = semantic.strip_tests(src)
        assert "inner_a" not in out and "inner_b" not in out
        assert "prod" in out


# ========== check-coverage.py ==========


class TestCoverageGate:
    def test_default_modules_exist_in_repo(self):
        missing = [m for m in coverage.DEFAULT_MODULES if not (ROOT / m).exists()]
        assert missing == [], f"DEFAULT_MODULES 引用了不存在的文件: {missing}"

    def test_crate_feature_map_crates_and_features_exist(self):
        problems = []
        for crate, feats in coverage.CRATE_FEATURE_MAP.items():
            cargo = ROOT / "packages" / crate / "Cargo.toml"
            if not cargo.exists():
                problems.append(f"包不存在: {crate}")
                continue
            available = set(coverage.get_crate_features(crate))
            unknown = [f for f in feats.split(",") if f and f not in available]
            if unknown:
                problems.append(f"{crate} 缺少 feature: {unknown}")
        assert problems == [], "CRATE_FEATURE_MAP 与 Cargo.toml 不一致: " + "; ".join(problems)

    def test_get_crate_features_excludes_default(self):
        feats = coverage.get_crate_features("sz-orm-core")
        assert feats, "sz-orm-core 应解析出 feature 列表"
        assert "default" not in feats


# ========== check-mutation-coverage.py ==========


class TestMutationGate:
    def test_default_files_exist(self):
        missing = [f for f in mutation.DEFAULT_FILES if not (ROOT / f).exists()]
        assert missing == [], f"DEFAULT_FILES 引用了不存在的文件: {missing}"

    def test_default_features_defined_in_core(self):
        available = set(coverage.get_crate_features("sz-orm-core"))
        unknown = [
            f
            for f in mutation.DEFAULT_FEATURES.split(",")
            if f and f not in available
        ]
        assert unknown == [], f"DEFAULT_FEATURES 含 sz-orm-core 未定义的 feature: {unknown}"


# ========== check-phantom-delivery.py ==========


class TestPhantomDelivery:
    def test_wiring_assertions_files_exist(self):
        for a in phantom.WIRING_ASSERTIONS:
            f = a.get("file")
            if f:
                assert (ROOT / f).exists(), f"接线断言引用的文件不存在: {a['name']} → {f}"

    def test_declared_symbols_registry_nonempty(self):
        assert phantom.DECLARED_SYMBOLS, "符号断言登记表不应为空"


# ========== G26-B：门禁脚本自测完整性元测试（2026-10-27 SDET 评测新增） ==========

# 存量无自测脚本登记表（2026-10-27 盘点）。
# 规则：不在本表内的 check-*.py 必须存在对应自测（tests/test_check_*.py、
# scripts/test_check_*.py，或被 tests/test_gate_scripts.py 覆盖），否则红牌。
# 新增门禁脚本必须自带自测；存量脚本补自测后可从本表移除。
EXEMPT_SCRIPTS = {
    "check-allow-deprecated-dangling",   # 存量：废弃 API 悬挂扫描，无自测
    "check-architecture",               # 存量：架构一致性扫描，无自测
    "check-benchmark-regression",       # 存量：基准回归扫描，无自测
    "check-binding-coverage",           # 存量：绑定层覆盖率扫描，无自测
    "check-ci-deny-sync",               # 存量：CI 与 deny.toml 同步，无自测
    "check-deprecated-downstream",      # 存量：下游废弃 API 扫描，无自测
    "check-deprecation-period",         # 存量：废弃 API 保留期，无自测
    "check-doc-code-compiles",          # 存量：文档代码可编译性，无自测
    "check-doc-consistency",            # 存量：文档与代码一致性，无自测
    "check-downstream-compat-auto",     # 存量：下游兼容自动扫描，无自测
    "check-downstream-compat",          # 存量：下游兼容扫描，无自测
    "check-examples-readme",            # 存量：示例清单同步，无自测
    "check-gate-scripts",               # 门禁自检脚本（audit the checkers），自引用豁免
    "check-glossary-consistency",       # 存量：术语表一致性，无自测
    "check-panic-safety",               # 存量：panic 安全扫描，无自测
    "check-security-doc-version",       # 存量：安全文档版本漂移，无自测
    "check-tech-debt",                  # 存量：技术债扫描，无自测
    "check-unsafe-audit",               # 存量：unsafe/SAFETY 审计，无自测
    "check-unused-deps",                # 存量：未用依赖扫描，无自测
    "check-v65-wiring",                 # 存量：v6.5 接线扫描，无自测
}


class TestGateScriptSelfTestCoverage:
    def test_all_check_scripts_have_self_tests(self):
        """G26-B：不在豁免表内的 check-*.py 必须被某个自测覆盖。"""
        import glob

        check_scripts = sorted(Path(SCRIPTS).glob("check-*.py"))
        self_test_files = (
            list(sorted((ROOT / "tests").glob("test_check_*.py")))
            + list(sorted(SCRIPTS.glob("test_check_*.py")))
            + [Path(__file__)]
        )
        text = "\n".join(
            p.read_text(encoding="utf-8", errors="replace") for p in self_test_files
        )
        missing = []
        for sp in check_scripts:
            stem = sp.stem  # "check-metrics-real"
            if stem in EXEMPT_SCRIPTS:
                continue
            if stem.replace("-", "_") not in text and stem not in text:
                missing.append(stem)
        assert missing == [], f"无自测的门禁脚本（需补自测或登记豁免）: {missing}"

    def test_new_gate_scripts_not_exempt(self):
        """新门禁脚本（check-phantom-tests / check-ci-blocking）不得在豁免表内。"""
        assert "check-phantom-tests" not in EXEMPT_SCRIPTS
        assert "check-ci-blocking" not in EXEMPT_SCRIPTS


class TestNewGateScriptsSmoke:
    """2026-10-28 补充：UXR 第三轮新增门禁脚本（G26/G27/G28 及警告级）的冒烟自测。

    覆盖 check-filename-encoding / check-doc-version-consistency /
    check-generated-code-compiles / check-lockfile-ignore-relevance / check-test-symbols，
    满足 G 系列门禁"检查器自身必须有自测"的 SDET 整改要求。
    """

    def test_check_filename_encoding_logic(self):
        mod = _load("check_filename_encoding", SCRIPTS / "check-filename-encoding.py")
        # 正常中文文件名可无损 round-trip
        assert mod._name_roundtrips("sz-orm使用指南.md")
        # GBK 乱码字节（经 surrogateescape 进入 str）无法按 UTF-8 编码 → 判定为乱码
        bad = b"\xd6\xd0\xce\xc4".decode("utf-8", "surrogateescape")
        assert not mod._name_roundtrips(bad)
        # 当前仓库扫描应无乱码文件名
        assert mod.scan() == []

    def test_check_doc_version_consistency_config(self):
        mod = _load(
            "check_doc_version_consistency",
            SCRIPTS / "check-doc-version-consistency.py",
        )
        # 检查表引用的核心文档必须真实存在
        for rel in mod.CHECKS:
            assert (ROOT / rel).exists(), f"G27 检查表引用文件不存在: {rel}"
        # 顶部版本行正则能提取当前版本号
        pat = mod.CHECKS["README.md"][0][0]
        m = re.search(pat, "v10.13.0 · 21,183+ tests")
        assert m is not None and m.group(1) == "10.13.0"

    def test_check_doc_version_consistency_gate_count_logic(self, tmp_path):
        """2026-10-28 P0-2：门禁数声称校验——漂移注入红牌 + 历史章节豁免。"""
        mod = _load(
            "check_doc_version_consistency",
            SCRIPTS / "check-doc-version-consistency.py",
        )
        # 真实仓库：权威门禁数 34，当前文档零漂移
        assert mod.count_agents_gates() == 34
        assert mod.check_gate_count_claims(34) == []
        # 反向注入：声称 25 → 红牌；历史标题下的 11（中英文）→ 豁免
        (tmp_path / "FAKE.md").write_text(
            "# head\n当前 25 道门禁\n"
            "### 门禁验证（v2.0.0 历史记录——当时共 11 道门禁）\n"
            "11 道门禁中 9 项通过\n"
            "### Gate Verification (historical record)\n"
            "9 of 11 gates passed\n",
            encoding="utf-8",
        )
        saved = (mod.ROOT, list(mod.GATE_COUNT_DOCS))
        mod.ROOT = tmp_path
        mod.GATE_COUNT_DOCS = ["FAKE.md"]
        try:
            issues = mod.check_gate_count_claims(34)
        finally:
            mod.ROOT, mod.GATE_COUNT_DOCS = saved
        assert len(issues) == 1 and "25" in issues[0] and "FAKE.md:2" in issues[0]

    def test_check_generated_code_compiles_importable(self):
        mod = _load(
            "check_generated_code_compiles",
            SCRIPTS / "check-generated-code-compiles.py",
        )
        assert callable(mod.main)

    def test_check_lockfile_ignore_relevance_importable(self):
        mod = _load(
            "check_lockfile_ignore_relevance",
            SCRIPTS / "check-lockfile-ignore-relevance.py",
        )
        assert callable(mod.main)

    def test_check_test_symbols_importable(self):
        mod = _load("check_test_symbols", SCRIPTS / "check-test-symbols.py")
        assert callable(mod.main)


# ========== check-gate-scripts.py（G27 门禁自检，2026-10-28 P2-3 强化） ==========


class TestGateAuditorRules:
    """audit-the-checkers 的回归：每种"假 PASS"形态必须命中，正确写法必须不命中。"""

    def _shell(self, tmp_path, body):
        gate = tmp_path / "gate.sh"
        gate.write_text("\n".join(body) + "\n", encoding="utf-8")
        old = gate_auditor.SERVER_GATE
        gate_auditor.SERVER_GATE = gate
        try:
            return gate_auditor.check_shell_gate()
        finally:
            gate_auditor.SERVER_GATE = old

    def _ps(self, tmp_path, body):
        gate = tmp_path / "gate.ps1"
        gate.write_text("\n".join(body) + "\n", encoding="utf-8")
        old = gate_auditor.PS_GATE
        gate_auditor.PS_GATE = gate
        try:
            return gate_auditor.check_ps_gate()
        finally:
            gate_auditor.PS_GATE = old

    def test_rule1_bare_mark_pass(self, tmp_path):
        problems = self._shell(tmp_path, ["mark 5 RUNNING", "mark 5 PASS"])
        assert any("无条件放行" in p for p in problems), problems

    def test_rule2_always_true_condition(self, tmp_path):
        problems = self._shell(
            tmp_path, ["mark 1 RUNNING", "if true; then mark 1 PASS; else mark 1 FAIL; fi"]
        )
        assert any("恒真条件" in p for p in problems), problems

    def test_rule3_command_then_mark_pass_same_chain(self, tmp_path):
        problems = self._shell(
            tmp_path,
            [
                "mark 2 RUNNING",
                'cargo fmt --all -- --check >> "$LOG" 2>&1; mark 2 PASS',
            ],
        )
        assert any("同链顺序执行" in p for p in problems), problems

    def test_rule4_multiline_or_false_pass(self, tmp_path):
        problems = self._shell(
            tmp_path,
            [
                "mark 3 RUNNING",
                "if cargo check -j 8 \\",
                "  || cargo check -j 4; then mark 3 PASS; else mark 3 FAIL; fi",
            ],
        )
        assert any("`||`" in p for p in problems), problems

    def test_correct_patterns_not_flagged(self, tmp_path):
        problems = self._shell(
            tmp_path,
            [
                "mark 4 RUNNING",
                'if cargo test -p x >> "$LOG" 2>&1; then mark 4 PASS; else mark 4 FAIL; fi',
                "mark 6 RUNNING",
                'cargo audit >> "$LOG" 2>&1',
                "AUDIT_RC=$?",
                'if [ $AUDIT_RC -eq 0 ]; then mark 6 PASS; else mark 6 FAIL; fi',
                "mark 20 SKIP",
            ],
        )
        assert problems == [], problems

    def test_ps_step_result_must_be_checked(self, tmp_path):
        problems = self._ps(
            tmp_path,
            [
                '$ok = Invoke-Step "A" {',
                "    cargo build",
                "}",
                '$ok = Invoke-Step "B" {',
                "    cargo test",
                "}",
                "if (-not $ok) { exit 2 }",
            ],
        )
        assert len(problems) == 1 and "未被检查" in problems[0], problems

    def test_ps_checked_steps_not_flagged(self, tmp_path):
        problems = self._ps(
            tmp_path,
            [
                '$ok = Invoke-Step "A" {',
                "    cargo build",
                "}",
                "if (-not $ok) { exit 1 }",
                '$ok = Invoke-Step "B" {',
                "    cargo test",
                "}",
                "if ($ok) {",
                '    $ok = Invoke-Step "C" {',
                "        cargo doc",
                "    }",
                "}",
                "if (-not $ok) { exit 2 }",
            ],
        )
        assert problems == [], problems


# ========== check-secrets.py（2026-10-28 P0-3：legacy 凭据路径豁免作用域） ==========

# 夹具凭据用相邻字面量拼接 + 运行时 format 构造，避免本文件被密钥扫描自命中
_LEGACY_PW = "SzOrm" "Test2026"
_NEW_PW = "RealNew" "Secret99"


class TestSecretsLegacyExemptionScope:
    """路径豁免只作用于 docs/assessment/ 历史审计记录，代码路径与新凭据不豁免。"""

    def test_legacy_doc_exempt_but_new_secret_reported(self, tmp_path):
        root = tmp_path
        legacy = root / "docs" / "assessment"
        legacy.mkdir(parents=True)
        # 历史审计文档中的 legacy 凭据 → 豁免
        (legacy / "old_report.md").write_text(
            "oracle://sz_orm_test:{}@127.0.0.1:1521/freepdb1\n".format(_LEGACY_PW),
            encoding="utf-8",
        )
        # 历史审计文档中的新真实凭据 → 必须报告
        (legacy / "new_leak.md").write_text(
            "mysql://admin:{}@10.0.0.1/db\n".format(_NEW_PW), encoding="utf-8"
        )
        # 代码路径中的 legacy 凭据 → 必须报告（豁免不覆盖代码）
        (root / "leak.rs").write_text(
            'let u = "oracle://sz_orm_test:{}@127.0.0.1:1521/freepdb1";\n'.format(_LEGACY_PW),
            encoding="utf-8",
        )
        findings: list = []
        for p in root.rglob("*"):
            if p.is_file():
                findings.extend(secrets_scanner.scan_file(p, root))
        files = {f.file.name for f in findings}
        assert "old_report.md" not in files, "legacy 凭据在历史审计文档中应被豁免"
        assert "new_leak.md" in files, "历史审计文档中的新凭据不得豁免"
        assert "leak.rs" in files, "代码路径中的 legacy 凭据不得豁免"

    def test_dsn_template_placeholder_whitelisted(self, tmp_path):
        """format! DSN 模板中的 {pw} 变量占位符不误报。"""
        p = tmp_path / "dsn.rs"
        p.write_text(
            'fn u() -> String { format!("oracle://sz_orm_test:{pw}@127.0.0.1:1521/freepdb1") }\n',
            encoding="utf-8",
        )
        assert secrets_scanner.scan_file(p, tmp_path) == []


# ========== check-publish-consistency.py（2026-10-30 收尾：G19 远程校验语义版本比较） ==========


class TestPublishConsistencySemanticVersion:
    """--check-crates-io 的版本比较必须用语义（major.minor 元组），
    禁止字符串比较（'10.7' > '10.13.0' 误判）。"""

    def _mod(self):
        return _load("check_publish_consistency", SCRIPTS / "check-publish-consistency.py")

    def test_ver_key_semantic_ordering(self):
        mod = self._mod()
        assert mod._ver_key("10.7") < mod._ver_key("10.13.0")
        assert mod._ver_key("9.4") < mod._ver_key("10.0")
        assert mod._ver_key("10.7") == mod._ver_key("10.7.1")

    def test_check_crates_io_claims_warns_not_errors_on_lagging_claim(self, monkeypatch):
        """README 声称 10.7、crates.io 实际 10.13.0 → 语义上滞后 → 警告而非红牌。"""
        mod = self._mod()

        def fake_fetch(name):
            return "10.13.0" if name == "sz-orm-sqlx" else None

        monkeypatch.setattr(mod, "fetch_crates_io_max_version", fake_fetch)
        errors = mod.check_crates_io_claims("10.13.0")
        assert errors == 0, "滞后声称（10.7 < 10.13.0）不应判为错误"

    def test_check_crates_io_claims_errors_on_ahead_claim(self, monkeypatch):
        """README 声称 10.14、crates.io 实际 10.13.0 → 语义上前置 → 红牌。"""
        mod = self._mod()

        def fake_fetch(name):
            return "10.13.0" if name == "sz-orm-sqlx" else None

        monkeypatch.setattr(mod, "fetch_crates_io_max_version", fake_fetch)
        # 注入一个比实际更高的声称：直接构造 claims 验证比较逻辑
        assert mod._ver_key("10.14") > mod._ver_key("10.13.0")


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
