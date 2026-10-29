#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""G25 门禁：文档代码可编译性检查（2026-10-27 新增，UXR 评测 R1 落地；v2 修复误报）

背景：
  README / quickstart / 使用指南 / 迁移指南中的代码块曾存在多处"照抄必编译失败"的示例：
  - README.md Quick Start §3 直接链式调用 .select(...) 后的 .where_eq（select 返回 Result）
  - 使用指南 §2.3 fn main() 无 Result 返回却使用 `?`
  - 迁移指南使用当前代码库中不存在的 API（User::query / pool.begin / Migrator::run 等）
  传统门禁（G18 度量真实性 / G24 示例清单）只检查数字与文件，不检查代码块，导致上述问题漏网。

本脚本做**静态启发式检查**（不编译）：
  1. 仅检查"sz-orm 侧"代码块（代码块含 sz_orm_/QueryBuilder/get_dialect/Pool 等特征），
     迁移指南中的"原框架侧"（SQLx/Diesel/SeaORM）对照代码不检查；
  2. 符号索引扫描**全部 packages/sz-orm-*** 的 pub fn / trait fn / 派生宏，
     支持泛型 trait 方法签名（fn name<'a>(...)）；
  3. 检查常见"照抄编译不过"陷阱：
     - fn main() 无 Result 返回却使用 `?`
     - .select() 返回 Result 未解包即链式调用
     - 引用 sz-orm 中不存在的符号 / 派生宏；
  4. crates.io 备选版本不得高于仓库版本（提示性）。

用法（Windows/Linux 通用）：
    python scripts/check-doc-code-compiles.py            # 只检查
    python scripts/check-doc-code-compiles.py --strict  # 所有警告按错误处理

退出码：0 = 通过；1 = 存在错误；2 = 脚本自身错误。
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PACKAGES_SRC = ROOT / "packages"

DOC_FILES = [
    "README.md",
    "README.zh.md",
    "docs/quickstart.md",
    "docs/sz-orm使用指南.md",
    "docs/sz-orm学习路线图.md",
    "docs/sz-orm架构设计.md",
    "docs/sz-ormAPI参考.md",
    "docs/sz-orm与同类产品对比分析.md",
    "docs/custom-diagnostic-guide.md",
    "docs/glossary-zh-en.md",
    "docs/migration-guide-sqlx.md",
    "docs/migration-guide-diesel.md",
    "docs/migration-guide-seaorm.md",
    "docs/query_builder_selection_guide.md",
]


def load_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def extract_rust_blocks(text: str):
    """提取所有 ```rust / ```rust,no_run 代码块的文本。"""
    pattern = re.compile(r"```rust(?:,no_run)?\s*\n(.*?)```", re.DOTALL)
    return pattern.findall(text)


# ---------------------------------------------------------------- 符号索引

def build_symbol_index() -> dict[str, set[str]]:
    """扫描全部 packages/sz-orm-* 源码，收集：
       pub fn / trait fn 方法名（支持泛型签名）、派生宏名、公开结构/枚举/trait 名。
    返回 {类别: 符号集合}。
    """
    index: dict[str, set[str]] = {
        "methods": set(),
        "derive_macros": set(),
        "items": set(),
    }
    fn_re = re.compile(r"\bfn\s+(\w+)\s*[<(]")
    pub_fn_re = re.compile(r"\bpub\s+(?:async\s+)?fn\s+(\w+)\s*[<(]")
    derive_re = re.compile(r"proc_macro_derive\(\s*([A-Za-z_]\w*)")
    item_re = re.compile(
        r"\bpub\s+(?:struct|enum|trait|macro_rules!|type|const)\s+(\w+)"
    )
    if PACKAGES_SRC.exists():
        for p in PACKAGES_SRC.rglob("src/**/*.rs"):
            text = load_text(p)
            index["methods"].update(fn_re.findall(text))
            index["methods"].update(pub_fn_re.findall(text))
            index["derive_macros"].update(derive_re.findall(text))
            index["items"].update(item_re.findall(text))
    return index


# ---------------------------------------------------------------- 检查项

# 常见 Rust 标准库 / 宏 / 属性 / 局部惯用名（不可能是 sz-orm 方法）
STD_IGNORED = {
    # 标准库方法
    "new", "default", "clone", "to_string", "as_str", "as_i64", "as_i32", "as_u32", "as_u64",
    "as_f64", "as_bool", "as_bytes", "into", "from", "iter", "collect", "map", "unwrap",
    "unwrap_or_default", "expect", "is_empty", "is_none", "is_some", "len", "push", "insert",
    "contains", "join", "format", "println", "print", "match", "split_once", "split", "trim",
    "to_lowercase", "parse", "from_secs", "from_millis", "now", "and", "retry", "primary",
    "references", "to_owned", "as_ref", "as_mut", "borrow", "take", "replace", "clear",
    "remove", "pop", "first", "last", "get", "get_mut", "extend", "sort", "sort_by", "max",
    "min", "sum", "count", "any", "all", "find", "position", "enumerate", "zip", "rev",
    "skip", "take_while", "filter", "filter_map", "flat_map", "fold", "reduce", "cycle",
    "chars", "bytes", "lines", "split_whitespace", "starts_with", "ends_with", "cmp",
    "partial_cmp", "eq", "ne", "lt", "le", "gt", "ge", "hash", "write", "flush", "read",
    "read_to_string", "read_line", "sleep", "spawn", "join_handle", "try_recv", "recv",
    "send", "sync", "lock", "unlock", "wait", "notify", "notify_waiters", "try_lock",
    "from_raw_parts", "slice", "chunks", "windows", "copy_within", "resize", "reserve",
    "capacity", "shrink_to_fit", "swap", "rotate", "dedup", "binary_search", "partition",
    "to_vec", "into_vec", "to_string_lossy", "to_owned", "is_ok", "is_err", "ok", "err",
    "transpose", "flatten", "unwrap_or", "unwrap_or_else", "ok_or", "ok_or_else",
    "and_then", "or_else", "map_or", "map_or_else", "expect_err", "unwrap_err", "as_deref",
    "cloned", "copied", "as_slice", "as_mut_slice", "into_iter", "to_ascii_lowercase",
    "to_ascii_uppercase", "char", "digit", "to_digit", "from_digit", "max_by", "min_by",
    "clamp", "abs", "signum", "pow", "sqrt", "floor", "ceil", "round", "trunc", "rem",
    "div_euclid", "rem_euclid", "checked_add", "checked_sub", "checked_mul", "checked_div",
    "saturating_add", "saturating_sub", "wrapping_add", "wrapping_sub", "overflowing_add",
    # std::boxed::Box::pin 等标准库
    "pin", "forget", "drop", "size_of", "align_of", "transmute", "manually_drop",
    # 属性/宏
    "derive", "allow", "deny", "cfg", "test", "tokio", "main", "macro_use", "macro_export",
    "inline", "deprecated", "doc", "repr", "non_exhaustive", "must_use", "async", "await",
    # chrono / std Duration / Atomic 方法（第三方或标准库）
    "minutes", "seconds", "hours", "days", "weeks", "milliseconds", "microseconds",
    "nanoseconds", "fetch_add", "fetch_sub", "fetch_update", "compare_exchange",
    "compare_exchange_weak", "load", "store", "swap", "get_mut", "get_many_mut", "split_at",
    "split_at_mut", "chunks_exact", "chunks_mut", "windows_mut", "copy_from_slice",
    "swap_with_slice", "reverse", "rotate_left", "rotate_right", "fill", "fill_with",
    # 类型/关键字/常用命名
    "Some", "None", "Ok", "Err", "Box", "Result", "Option", "String", "Vec", "HashMap",
    "BTreeMap", "HashSet", "BTreeSet", "VecDeque", "Arc", "Rc", "Mutex", "RwLock", "AtomicU32",
    "Duration", "PathBuf", "Path", "usize", "u8", "u16", "u32", "u64", "i8", "i16", "i32",
    "i64", "f32", "f64", "bool", "char", "Self", "self", "super", "std", "core", "alloc",
    "dyn", "trait", "const", "static", "where", "impl", "struct", "enum", "use", "mod", "pub",
    "fn", "let", "mut", "return", "break", "continue", "loop", "while", "for", "if", "else",
    "match", "ref", "move", "as", "in", "unsafe", "extern", "crate", "type", "async_trait",
    "main", "connect", "begin_transaction", "query_with_params", "execute", "query", "acquire",
    "release", "commit", "rollback", "savepoint", "release_savepoint", "rollback_to_savepoint",
    # 原框架侧常见符号（迁移指南对照代码，不检查）
    "sqlx", "diesel", "sea_orm", "users", "posts", "insert_into", "values", "execute", "load",
    "filter", "eq", "and", "on", "left_join", "find", "find_by_id", "one", "all", "paginate",
    "fetch_page", "num_items", "insert", "update", "delete", "delete_by_id", "exec", "set",
    "save", "find_with_related", "with", "into_active_model", "transaction", "migrate",
    "run", "Migrate", "migrate", "migrate!", "query_as", "query", "bind", "fetch_all",
    "fetch_one", "fetch_optional", "try_from", "try_into", "from_row", "from_rows",
}

# 标准 derive 宏（不是 sz-orm 提供）
STD_DERIVES = {
    "Debug", "Clone", "Default", "PartialEq", "Eq", "Hash", "PartialOrd", "Ord", "Copy",
    "Serialize", "Deserialize", "Debug2", "Send", "Sync", "Getters", "Setters", "From",
    "TryFrom", "Into", "TryInto", "Display", "Error", "AsRef", "Deref", "DerefMut",
    "FromPrimitive", "ToPrimitive", "EnumIter", "DeriveEntityModel", "DeriveRelation",
    "sea_orm", "table", "column", "primary_key", "DeriveSchema", "Queryable", "FromRow",
    "FromQueryResult", "Schema", "Model", "ActiveModel", "Relation", "GraphQLModel",
    "Builder", "ColumnEnum", "RelationTrait", "Validate", "Governed", "SqlType", "Entity",
}


def is_szorm_side(code: str) -> bool:
    """判断代码块是否属于"sz-orm 侧"（需要检查符号）。"""
    markers = (
        "sz_orm_", "sz-orm", "QueryBuilder", "get_dialect", "PoolConfigBuilder",
        "SqlxMySqlConnectionFactory", "SqlxPgConnectionFactory", "SqlxSqliteConnectionFactory",
        "MySqlPoolHandle", "PgPoolHandle", "SqlitePoolHandle", "dialect::get_dialect",
        "MigrationContext", "FileMigrationResolver", "sql_string", "build_select_with_params",
    )
    return any(m in code for m in markers)


def _strip_string_literals(code: str) -> str:
    """移除 Rust 代码中的字符串字面量（"..."、r"..."、r#"..."#、b"..."、字符 'c'），
    避免 SQL/DSL 字符串中的单词被误判为符号调用。"""
    # 原始字符串 r#"..."# / r"..." / r##"..."##
    code = re.sub(r'r#+"[^"]*"#+', '""', code)
    code = re.sub(r'r"[^"]*"', '""', code)
    # 普通字符串（含转义）
    code = re.sub(r'b?"(?:[^"\\]|\\.)*"', '""', code)
    # 字符字面量
    code = re.sub(r"'(?:[^'\\]|\\.)*'", "''", code)
    return code


def _strip_comments(code: str) -> list[str]:
    """返回去掉注释与字符串后的有效代码行；属性行仅保留 `#[derive(...)]`。"""
    body_lines = []
    in_block_comment = False
    for ln in code.splitlines():
        stripped = ln.strip()
        if in_block_comment:
            if '*/' in ln:
                in_block_comment = False
            continue
        if stripped.startswith('/*'):
            if '*/' not in ln:
                in_block_comment = True
            continue
        if stripped.startswith('//'):
            continue
        if stripped.startswith('#['):
            # 仅保留 #[derive(...)] 行供派生宏检测；其余属性行（#[table]/#[column]/#[cfg] 等）跳过
            if re.match(r'#\[derive\s*\(', stripped):
                body_lines.append(ln.split('//', 1)[0])
            continue
        if stripped.startswith('#!'):
            continue
        ln_clean = ln.split('//', 1)[0]
        ln_clean = _strip_string_literals(ln_clean)
        body_lines.append(ln_clean)
    return body_lines


def check_symbols_in_blocks(text: str, file_label: str, index: dict[str, set[str]],
                              issues: list[str]) -> None:
    """在 sz-orm 侧代码块中查找不存在的符号 / 派生宏。"""
    known_methods = index["methods"] | index["items"]
    for block_no, code in enumerate(extract_rust_blocks(text), 1):
        if not is_szorm_side(code):
            continue
        body = "\n".join(_strip_comments(code))
        # 1) 方法调用符号：name(
        for m in re.finditer(r"\b([a-z_][a-z0-9_]*)\s*\(", body):
            name = m.group(1)
            if name in STD_IGNORED or name in known_methods:
                continue
            # 局部变量/参数/函数定义
            if re.search(rf"\blet\s+{re.escape(name)}\b|\bfn\s+{re.escape(name)}\b", body):
                continue
            issues.append(
                f"[{file_label}] rust 块#{block_no} 调用了 sz-orm 中不存在的符号 `{name}()`"
            )
        # 2) 派生宏：#[derive(...)]
        for m in re.finditer(r"#\[\s*derive\s*\(\s*([^)]+)\)", body):
            for dm in m.group(1).split(","):
                dm = dm.strip().split("(")[0]
                if not dm or dm in STD_DERIVES or dm in index["derive_macros"]:
                    continue
                issues.append(f"[{file_label}] rust 块#{block_no} 引用了不存在的派生宏 `{dm}`")


def check_common_traps(text: str, file_label: str, issues: list[str]) -> None:
    """启发式检查常见"照抄编译不过"模式（仅 sz-orm 侧代码块）。"""
    for block_no, code in enumerate(extract_rust_blocks(text), 1):
        if not is_szorm_side(code):
            continue
        # A. fn main() { ... 使用 ?（main 无 Result 返回）
        if re.search(r"\bfn\s+main\s*\(\)\s*\{[^}]*\?", code, re.DOTALL) and "Result" not in code:
            issues.append(
                f"[{file_label}] rust 块#{block_no} `fn main()` 无 Result 返回却使用了 `?`（编译失败）"
            )
        # B. .select(...) 后未处理 Result 就直接链式调用
        if re.search(r"\.select\([^)]*\)\s*\n?\s*\.(where_|or_where|order_|limit|page)", code):
            if ".expect(" not in code and ".unwrap()" not in code and "?" not in code:
                issues.append(
                    f"[{file_label}] rust 块#{block_no} `.select()` 返回 Result，未用 expect/unwrap/? 解包就链式调用"
                )


def check_version_consistency(text: str, file_label: str, ws_version: str, issues: list[str]) -> None:
    """文档中引用的 crates.io 版本不得高于仓库版本（提示性）。"""
    for m in re.finditer(r'sz-orm-(?:core|sqlx)\s*=\s*"(\d+\.\d+)', text):
        ver = m.group(1)
        try:
            major, minor = int(ver.split(".")[0]), int(ver.split(".")[1])
            ws_major, ws_minor = (int(x) for x in ws_version.split(".")[:2])
            if (major, minor) > (ws_major, ws_minor):
                issues.append(
                    f"[{file_label}] crates.io 备选版本 `{ver}` 高于仓库版本 {ws_version}，发布滞后时用户会装错"
                )
        except ValueError:
            pass


# ---------------------------------------------------------------- 主流程

def main() -> int:
    parser = argparse.ArgumentParser(description="G25 文档代码可编译性检查")
    parser.add_argument("--fix", action="store_true", help="自动修复（当前无可自动修复项）")
    parser.add_argument("--strict", action="store_true", help="所有警告按错误处理")
    args = parser.parse_args()

    ws_version = load_text(ROOT / "Cargo.toml")
    m = re.search(r'^version\s*=\s*"([^"]+)"', ws_version, re.M)
    if not m:
        print("错误: 无法从 Cargo.toml 读取 workspace 版本")
        return 2
    ws_version = m.group(1)

    index = build_symbol_index()
    issues: list[str] = []

    for rel in DOC_FILES:
        p = ROOT / rel
        if not p.exists():
            issues.append(f"[{rel}] 文件不存在")
            continue
        text = load_text(p)
        check_symbols_in_blocks(text, rel, index, issues)
        check_common_traps(text, rel, issues)
        check_version_consistency(text, rel, ws_version, issues)

    if issues:
        print(f"G25 文档代码可编译性检查: {len(issues)} 个问题")
        for it in issues:
            print(f"  - {it}")
        return 1

    print("G25 文档代码可编译性检查: 通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())