#!/usr/bin/env python3
"""
check-phantom-tests.py 自测（2026-10-27 SDET 评测门禁 26 整改）

验收原则：每条规则的验收标准 = 能命中历史真阳性样本（本会话评测发现并已修复的
幻影断言），且对当前已修复代码零误报。

历史样本（2026-10-27 SDET 评测发现）：
- 永真断言 `assert!(x.is_empty() || !x.is_empty())`（原 dialect_advanced_test.rs:90）
- 双字面量比较 `assert_ne!("tenant_a", "tenant_b")`（原 multi_tenant_security_test.rs:9）
- 零断言测试函数（原 dialect_advanced_test.rs:178 test_saphana_supports_if）
- contains 兜底或 `sql.contains("active") || sql.is_empty()`（原 dialect_advanced_test.rs:110）
- 名义差分（原 smallstring_differential.rs 全文件子串包含）
- 测标准库（原 connection_tenant_security_test.rs 对字面量调 str::replace）

运行：
    python -m pytest tests/test_check_phantom_tests.py -v
"""

from __future__ import annotations

import importlib.util as _ilu
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent

_spec = _ilu.spec_from_file_location("check_phantom_tests", ROOT / "scripts" / "check-phantom-tests.py")
cpt = _ilu.module_from_spec(_spec)
sys.modules["check_phantom_tests"] = cpt
_spec.loader.exec_module(cpt)


# ========== 历史样本 fixture（修复前代码片段） ==========

TAUTOLOGY_SAMPLE = """
#[test]
fn test_fts() {
    let non_empty = d.full_text_search(&["title", "body"], "keyword");
    let empty = d.full_text_search(&[], "keyword");
    assert!(non_empty.is_empty() || !non_empty.is_empty());
    assert!(empty.is_empty() || !empty.is_empty());
}
"""

LITERAL_SAMPLE = """
#[tokio::test]
async fn test_multi_tenant_basic_isolation() {
    let tenant_a = "tenant_a";
    let tenant_b = "tenant_b";
    assert_ne!(tenant_a, tenant_b, "不同租户应隔离");
}
"""

TWO_LITERALS_SAMPLE = """
#[test]
fn test_compare_constants() {
    assert_eq!("a", "b", "两个编译期常量比较恒为假，无测试价值");
}
"""

VAR_VS_LITERAL_SAMPLE = """
#[tokio::test]
async fn test_empty_tenant_id() {
    let tenant = "";
    assert_eq!(tenant, "", "空租户 ID 应正确识别");
}
"""

ZERO_ASSERT_SAMPLE = """
#[test]
fn test_saphana_supports_if() {
    let d = SapHanaDialect;
    let _ = d.supports_if_exists();
    let _ = d.supports_if_not_exists();
}
"""

WEAK_OR_SAMPLE = """
#[test]
fn test_bool_to_int() {
    let sql = d.bool_to_int("active");
    assert!(sql.contains("active") || sql.is_empty());
}
"""

STD_LIB_SAMPLE = """
#[tokio::test]
async fn test_connection_tenant_sql_injection_single_quote_escaped() {
    let malicious_tenant = "tenant'; DROP TABLE--";
    let escaped = malicious_tenant.replace('\\'', "''");
    assert!(escaped.contains("''"), "单引号应被转义为两个单引号");
}
"""


def _scan_text(tmp_path, text, filename="sample_test.rs"):
    """把文本写入临时测试文件并扫描，返回 (rule, line, msg) 列表。"""
    p = tmp_path / "packages" / "sz-orm-core" / "tests" / filename
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")
    return cpt.scan_file(str(p))


# ========== P1 同表达式互斥或 ==========


class TestP1Tautology:
    def test_historical_sample_is_caught(self, tmp_path):
        hits = _scan_text(tmp_path, TAUTOLOGY_SAMPLE)
        assert any(r == "P1" for r, _, _ in hits)

    def test_normal_assert_not_flagged(self, tmp_path):
        text = 'assert!(a && b, "both must hold");\n'
        hits = _scan_text(tmp_path, text)
        assert all(r != "P1" for r, _, _ in hits)

    def test_current_dialect_file_zero_hits(self):
        hits = cpt.scan_file(str(ROOT / "packages" / "sz-orm-core" / "tests" / "dialect_advanced_test.rs"))
        assert all(r != "P1" for r, _, _ in hits), "修复后的 dialect_advanced_test 不应再命中 P1"


# ========== P2 双字面量断言 ==========


class TestP2LiteralCompare:
    def test_two_literals_is_caught(self, tmp_path):
        hits = _scan_text(tmp_path, TWO_LITERALS_SAMPLE)
        assert any(r == "P2" for r, _, _ in hits)

    def test_variable_compare_not_flagged(self, tmp_path):
        text = 'assert_eq!(config.max_size, 100, "builder 产物");\n'
        hits = _scan_text(tmp_path, text)
        assert all(r != "P2" for r, _, _ in hits)

    def test_current_multi_tenant_file_zero_hits(self):
        hits = cpt.scan_file(str(ROOT / "packages" / "sz-orm-core" / "tests" / "multi_tenant_security_test.rs"))
        assert all(r != "P2" for r, _, _ in hits)


# ========== P2b 变量 vs 字面量断言（软） ==========


class TestP2bVariableVsLiteral:
    def test_variable_vs_literal_is_caught(self, tmp_path):
        hits = _scan_text(tmp_path, VAR_VS_LITERAL_SAMPLE)
        assert any(r == "P2b" for r, _, _ in hits)

    def test_meaningful_comparison_not_flagged(self, tmp_path):
        # 复杂表达式（非裸标识符）不应被 P2b 误报
        text = 'assert_eq!(result.len(), "expected", "行为结果");\n'
        hits = _scan_text(tmp_path, text)
        assert all(r != "P2b" for r, _, _ in hits)


# ========== P3 测试函数零断言 ==========


class TestP3ZeroAssert:
    def test_historical_sample_is_caught(self, tmp_path):
        hits = _scan_text(tmp_path, ZERO_ASSERT_SAMPLE)
        assert any(r == "P3" for r, _, _ in hits)

    def test_function_with_assert_not_flagged(self, tmp_path):
        text = '#[test]\nfn ok() { let x = f(); assert!(x > 0); }\n'
        hits = _scan_text(tmp_path, text)
        assert all(r != "P3" for r, _, _ in hits)

    def test_pure_helper_test_is_allowed(self, tmp_path):
        # helper 函数（非 #[test]）不应被误判
        text = 'fn helper() { let _ = f(); }\n#[test]\nfn ok() { assert!(helper()); }\n'
        hits = _scan_text(tmp_path, text)
        assert all(r != "P3" for r, _, _ in hits)


# ========== P4 contains 兜底或（软） ==========


class TestP4WeakOr:
    def test_historical_sample_is_caught(self, tmp_path):
        hits = _scan_text(tmp_path, WEAK_OR_SAMPLE)
        assert any(r == "P4" for r, _, _ in hits)

    def test_normal_contains_not_flagged(self, tmp_path):
        text = 'assert!(sql.contains("SELECT"));\n'
        hits = _scan_text(tmp_path, text)
        assert all(r != "P4" for r, _, _ in hits)


# ========== P5 差分命名规范 ==========


class TestP5DifferentialNaming:
    def test_historical_nominal_differential_is_caught(self, tmp_path):
        # 原 smallstring_differential.rs：名为差分实为 contains
        text = (
            "//! SmallString 差分测试\n"
            "#[test]\nfn diff_select_basic() {\n"
            "    let (sql, _) = make_query().build_select();\n"
            "    assert!(sql.contains(\"SELECT\"));\n"
            "}\n"
        )
        hits = _scan_text(tmp_path, text, filename="smallstring_differential.rs")
        assert any(r == "P5" for r, _, _ in hits)

    def test_real_differential_not_flagged(self, tmp_path):
        text = (
            "#[test]\nfn diff() {\n"
            "    let a = f(); let b = g();\n"
            "    assert_eq!(a, b, \"对拍\");\n"
            "}\n"
        )
        hits = _scan_text(tmp_path, text, filename="plan_cache_differential.rs")
        assert all(r != "P5" for r, _, _ in hits)

    def test_literal_expected_value_is_valid_assertion(self, tmp_path):
        # typed_ast_zst_diff_test 用 SQL 字面量作期望值，属有效差分断言，不应误报
        text = (
            "#[test]\nfn diff() {\n"
            "    let sql = build();\n"
            "    assert_eq!(sql, \"ROW_NUMBER() OVER ()\");\n"
            "}\n"
        )
        hits = _scan_text(tmp_path, text, filename="typed_ast_zst_diff_test.rs")
        assert all(r != "P5" for r, _, _ in hits)

    def test_current_smallstring_file_zero_hits(self):
        hits = cpt.scan_file(str(ROOT / "packages" / "sz-orm-core" / "tests" / "smallstring_differential.rs"))
        assert all(r != "P5" for r, _, _ in hits), "修复后的 smallstring 不应再命中 P5"

    def test_non_differential_diff_name_not_triggered(self, tmp_path):
        # perf_diff / schema_diff / mock_real_diff 等"差异对比"语义文件不应触发 P5
        text = '#[test]\nfn diff() { assert!(sql.contains("x")); }\n'
        for fname in ("perf_diff.rs", "schema_diff_v2.rs", "mock_real_diff.rs"):
            hits = _scan_text(tmp_path, text, filename=fname)
            assert all(r != "P5" for r, _, _ in hits), f"{fname} 不应触发 P5"


# ========== P6 集成测试未引用产品 crate（软） ==========


class TestP6NoProductSymbols:
    def test_historical_std_lib_sample_is_caught(self, tmp_path):
        hits = _scan_text(tmp_path, STD_LIB_SAMPLE)
        assert any(r == "P6" for r, _, _ in hits)

    def test_historical_literal_variable_sample_is_caught(self, tmp_path):
        hits = _scan_text(tmp_path, LITERAL_SAMPLE)
        assert any(r == "P6" for r, _, _ in hits)

    def test_other_product_crate_not_flagged(self, tmp_path):
        # 引用 sz_orm_advisor 等其他产品包的集成测试不应被 P6 误报
        text = (
            "use sz_orm_advisor::{AutoTuningGateway, OptimizationAdvisor};\n"
            "#[test]\nfn test_gateway() {\n"
            "    let gw = AutoTuningGateway::new(advisor());\n"
            "    assert!(gw.run_loop(\"q1\", None, None, 1.0, 2.0).is_ok());\n"
            "}\n"
        )
        hits = _scan_text(tmp_path, text)
        assert all(r != "P6" for r, _, _ in hits)

    def test_current_connection_tenant_file_zero_hits(self):
        hits = cpt.scan_file(str(ROOT / "packages" / "sz-orm-core" / "tests" / "connection_tenant_security_test.rs"))
        assert all(r != "P6" for r, _, _ in hits), "修复后的 connection_tenant 测试应引用产品 crate"


# ========== 规则元数据一致性 ==========


class TestRuleRegistry:
    def test_all_rules_have_hard_flag(self):
        for rule_id, meta in cpt.RULES.items():
            assert "hard" in meta and isinstance(meta["hard"], bool)

    def test_only_supported_rules(self):
        assert set(cpt.RULES) == {"P1", "P2", "P3", "P5", "P2b", "P4", "P6"}