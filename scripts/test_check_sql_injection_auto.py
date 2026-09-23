#!/usr/bin/env python3
"""Tests for SQL Injection Automated Review Engine."""

import sys
import os
import importlib.util

sys.path.insert(0, os.path.dirname(__file__))
_spec = importlib.util.spec_from_file_location(
    "check_sql_injection_auto",
    os.path.join(os.path.dirname(__file__), "check-sql-injection-auto.py"),
)
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)
judge_injection_risk = _mod.judge_injection_risk
JudgeResult = _mod.JudgeResult
scan_workspace = _mod.scan_workspace


def test_r3_question_placeholder():
    """R3: ? placeholder -> Safe."""
    result, reason = judge_injection_risk(
        "packages/sz-orm-core/src/test.rs", 1,
        'let sql = format!("SELECT * FROM t WHERE id = ?", );',
        "format_concat",
    )
    assert result == JudgeResult.SAFE, f"Expected SAFE, got {result}: {reason}"
    assert "R3" in reason


def test_r3_dollar_placeholder():
    """R3: $1 named param -> Safe."""
    result, reason = judge_injection_risk(
        "packages/sz-orm-core/src/test.rs", 1,
        'let sql = format!("SELECT * FROM users WHERE id = $1", );',
        "format_concat",
    )
    assert result == JudgeResult.SAFE, f"Expected SAFE, got {result}: {reason}"


def test_r2_table_name():
    """R2: table name placeholder -> Safe."""
    result, reason = judge_injection_risk(
        "packages/sz-orm-cabi/src/lib.rs", 1,
        'format!("DELETE FROM {} WHERE {}", table, where_clause)',
        "format_concat",
    )
    assert result == JudgeResult.SAFE, f"Expected SAFE, got {result}: {reason}"


def test_r4_test_file():
    """R4: test file -> Safe."""
    result, reason = judge_injection_risk(
        "packages/sz-orm-ai/tests/ai_agent_test.rs", 1,
        'Ok(format!("SELECT * FROM data WHERE task = \'{}\'", query))',
        "format_concat",
    )
    assert result == JudgeResult.SAFE, f"Expected SAFE, got {result}: {reason}"


def test_r4_safety_comment():
    """R4: SAFETY comment -> Safe."""
    result, reason = judge_injection_risk(
        "packages/sz-orm-wasm/src/lib.rs", 1,
        'return Err(format!("unsupported WHERE clause: {}", where_clause)); // SAFETY: 错误消息拼接，非 SQL 执行',
        "format_concat",
    )
    assert result == JudgeResult.SAFE, f"Expected SAFE, got {result}: {reason}"


def test_r4_security_module():
    """R4: security module -> Safe."""
    result, reason = judge_injection_risk(
        "packages/sz-orm-ai/src/sql_sanitizer.rs", 1,
        'let sql = format!("SELECT * FROM data WHERE value = \'{}\'", long_token);',
        "format_concat",
    )
    assert result == JudgeResult.SAFE, f"Expected SAFE, got {result}: {reason}"


def test_r2_join_pattern():
    """R2: join(" AND ") -> Safe."""
    result, reason = judge_injection_risk(
        "packages/sz-orm-core/src/query.rs", 1,
        'format!(" WHERE {}", group_strs.join(" AND "))',
        "format_concat",
    )
    assert result == JudgeResult.SAFE, f"Expected SAFE, got {result}: {reason}"


def test_r2_rownum():
    """R2: ROWNUM -> Safe."""
    result, reason = judge_injection_risk(
        "packages/sz-orm-nl-query/src/dialect_renderer.rs", 1,
        'format!("{} WHERE ROWNUM <= {}", base, n)',
        "format_concat",
    )
    assert result == JudgeResult.SAFE, f"Expected SAFE, got {result}: {reason}"


def test_workspace_scan_all_safe():
    """Workspace scan: all 66 items should be Safe."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    report = scan_workspace(root)
    assert report.total > 0, "Should find items"
    assert report.unsafe == 0, f"Should have 0 Unsafe, got {report.unsafe}"
    assert report.manual_review == 0, f"Should have 0 ManualReview, got {report.manual_review}"


def main():
    tests = [
        test_r3_question_placeholder,
        test_r3_dollar_placeholder,
        test_r2_table_name,
        test_r4_test_file,
        test_r4_safety_comment,
        test_r4_security_module,
        test_r2_join_pattern,
        test_r2_rownum,
        test_workspace_scan_all_safe,
    ]
    passed = 0
    failed = 0
    for test in tests:
        try:
            test()
            print(f"  PASS: {test.__name__}")
            passed += 1
        except Exception as e:
            print(f"  FAIL: {test.__name__}: {e}")
            failed += 1
    print(f"\n{passed} passed; {failed} failed; {len(tests)} total")
    if failed > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()