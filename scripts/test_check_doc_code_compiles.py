#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""G25 门禁脚本自测（2026-10-27，与 check-doc-code-compiles.py 配套）

运行：python scripts/test_check_doc_code_compiles.py
验证：符号存在性检测 / fn main 用 ? / select 链式 / 派生宏 / 版本一致性 / 原框架侧过滤。
"""
import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location(
    "g25", ROOT / "scripts" / "check-doc-code-compiles.py")
g25 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g25)

failures = []


def check(name: str, cond: bool) -> None:
    print(("PASS" if cond else "FAIL") + ": " + name)
    if not cond:
        failures.append(name)


# 1. 符号检测：能抓出不存在的 sz-orm 符号，且不误报真实符号
index = {"methods": {"real_method"}, "derive_macros": {"RealDerive"}, "items": set()}
issues = []
g25.check_symbols_in_blocks(
    "```rust\nuse sz_orm_core::x;\nlet _ = real_method();\nphantom_call();\n```",
    "test", index, issues)
check("检测不存在的符号 phantom_call", any("phantom_call" in i for i in issues))
check("真实符号 real_method 不误报", not any("real_method" in i for i in issues))

# 2. fn main() 无 Result 却使用 ?
issues = []
g25.check_common_traps(
    "```rust\nuse sz_orm_core::x;\nfn main() { let _ = x()?; }\n```", "test", issues)
check("检测 fn main 用 ? 无 Result", any("fn main()" in i and "?" in i for i in issues))

# 3. .select() 返回 Result 未解包就链式调用
issues = []
g25.check_common_traps(
    '```rust\nlet sql = QueryBuilder::new(d).select(vec!["a"]).where_eq("a", v);\n```',
    "test", issues)
check("检测 select 链式未解包", any("select()" in i for i in issues))

# 4. 不存在的派生宏
issues = []
g25.check_symbols_in_blocks(
    "```rust\nuse sz_orm_core::x;\n#[derive(PhantomDerive)]\nstruct X;\n```", "test", index, issues)
check("检测不存在的派生宏 PhantomDerive", any("PhantomDerive" in i for i in issues))

# 5. 版本一致性：crates.io 备选版本高于仓库版本
issues = []
g25.check_version_consistency('sz-orm-core = "99.0"', "test", "10.13.0", issues)
check("检测备选版本高于仓库", any("99.0" in i for i in issues))

# 6. 原框架侧代码块不进入符号检查
check("原框架侧代码不被判定为 sz-orm 侧",
      not g25.is_szorm_side('```rust\nlet x = sqlx::query("SELECT 1");\n```'))

# 7. 注释/字符串不产生符号误报
issues = []
g25.check_symbols_in_blocks(
    '```rust\n// fake_symbol_in_comment()\nlet sql = "SELECT * FROM t WHERE id IN (1)";\n```',
    "test", index, issues)
check("注释与字符串中的词不误报", not issues)

print()
if failures:
    print("FAILED: " + ", ".join(failures))
    sys.exit(1)
print("ALL PASS")