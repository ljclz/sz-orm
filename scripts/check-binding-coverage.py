#!/usr/bin/env python3
"""统计各绑定对 sz-orm-core 核心外部 API 的覆盖率。

用法：
    python scripts/check-binding-coverage.py \
        --core packages/sz-orm-core/src \
        --bindings cabi,java,python,go,cpp,js,wasm \
        --core-api-list scripts/core-api-list.json \
        --output docs/binding-coverage-report-v7.2.0.json \
        --threshold 0.9
"""

import argparse
import json
import re
import sys
from pathlib import Path


def extract_core_apis(core_dir: str) -> set[str]:
    """从 sz-orm-core/src/ 提取所有 pub API 符号。"""
    apis = set()
    core_path = Path(core_dir)

    patterns = [
        r"\bpub\s+fn\s+(\w+)",
        r"\bpub\s+async\s+fn\s+(\w+)",
        r"\bpub\s+struct\s+(\w+)",
        r"\bpub\s+enum\s+(\w+)",
        r"\bpub\s+trait\s+(\w+)",
        r"\bpub\s+const\s+(\w+)",
        r"\bpub\s+type\s+(\w+)",
    ]

    for rs_file in core_path.rglob("*.rs"):
        try:
            content = rs_file.read_text(encoding="utf-8", errors="ignore")
            for pattern in patterns:
                for match in re.finditer(pattern, content):
                    apis.add(match.group(1))
        except Exception:
            continue

    return apis


def load_core_api_list(path: str) -> list[dict]:
    """加载显式核心 API 清单。"""
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    return data["apis"]


def extract_binding_exports(binding_dir: str, binding_name: str) -> set[str]:
    """从绑定包提取导出符号。"""
    exports = set()
    binding_path = Path(binding_dir)

    if not binding_path.exists():
        return exports

    markers = {
        "cabi": ["#[no_mangle]"],
        "java": ["#[no_mangle]"],
        "go": ["#[no_mangle]"],
        "cpp": ["#[no_mangle]"],
        "python": ["#[pyfunction]", "#[pymethods]", "#[pyclass]"],
        "js": ["#[napi]"],
        "wasm": ["#[wasm_bindgen]"],
    }

    markers_list = markers.get(binding_name, ["#[no_mangle]"])
    fn_pattern = re.compile(r"\bfn\s+(\w+)")

    for rs_file in binding_path.rglob("*.rs"):
        if "target" in rs_file.parts:
            continue
        try:
            lines = rs_file.read_text(encoding="utf-8", errors="ignore").split("\n")
            for i, line in enumerate(lines):
                if any(marker in line for marker in markers_list):
                    search_end = min(i + 500, len(lines))
                    for j in range(i, search_end):
                        match = fn_pattern.search(lines[j])
                        if match:
                            exports.add(match.group(1))
        except Exception:
            continue

    return exports


def snake_to_camel(name: str) -> str:
    """snake_case → camelCase"""
    return re.sub(r"_([a-z])", lambda m: m.group(1).upper(), name)


def match_core_to_exports(core_apis: list[dict], exports: set[str], binding_name: str) -> tuple[list[str], list[dict]]:
    """将核心 API 清单匹配到绑定导出符号。

    cabi: sz_orm_ 前缀 + snake_case
    go: sz_orm_go_ 前缀 + snake_case
    cpp: sz_orm_cpp_ 前缀 + snake_case
    java: JNI 命名 Java_..._<MethodName>，方法名为 camelCase
    python/js/wasm: camelCase 或直接映射
    """
    prefix_map = {
        "cabi": "sz_orm_",
        "go": "sz_orm_go_",
        "cpp": "sz_orm_cpp_",
    }

    semantic_aliases = {
        "ping": ["ping", "async_ping", "health_check", "healthCheck"],
        "query": ["query", "async_query", "fetch", "fetch_all"],
        "execute": ["execute", "async_execute", "exec", "run"],
        "count": ["count", "async_count", "row_count"],
        "version": ["version", "get_version", "pkg_version", "crate_version"],
        "transaction_begin": ["beginTransaction", "txBegin", "begin_transaction", "begin", "async_begin"],
        "transaction_commit": ["commitTransaction", "txCommit", "commit_transaction", "commit", "async_commit"],
        "transaction_rollback": ["rollbackTransaction", "txRollback", "rollback_transaction", "rollback", "async_rollback"],
        "transaction_free": ["freeTransaction", "txFree", "free_transaction", "close", "drop"],
        "transaction_execute": ["transactionExecute", "txExecute", "execute_transaction", "async_execute_transaction"],
        "pool_new": ["poolNew", "new", "connect", "open", "constructor"],
        "pool_free": ["poolFree", "free", "close", "disconnect", "drop", "destroy"],
        "pool_stats": ["poolStats", "stats", "getStats", "status"],
        "pool_metrics": ["poolMetrics", "metrics", "getMetrics"],
        "query_one": ["queryOne", "findOne", "query_one", "fetchone", "fetch_one", "async_query_one"],
        "execute_batch": ["executeBatch", "batchExecute", "exec_batch", "execute_many", "async_execute_batch"],
        "query_result_free": ["queryResultFree", "resultFree", "freeResult"],
        "error_description": ["errorDescription", "errorMessage", "getErrorMessage", "error_message"],
        "string_free": ["stringFree", "freeString", "free_string"],
        "table_exists": ["tableExists", "hasTable", "table_exists"],
        "model_insert": ["modelInsert", "insert", "save", "create", "async_insert"],
        "model_update": ["modelUpdate", "update", "save_changes", "async_update"],
        "model_delete": ["modelDelete", "delete", "remove", "async_delete"],
        "model_find": ["modelFind", "find", "get", "fetch", "find_by_id", "async_find"],
        "model_insert_tx": ["modelInsertTx", "insertTx", "insert_tx", "async_insert_tx"],
        "model_update_tx": ["modelUpdateTx", "updateTx", "update_tx", "async_update_tx"],
        "model_delete_tx": ["modelDeleteTx", "deleteTx", "delete_tx", "async_delete_tx"],
        "model_find_tx": ["modelFindTx", "findTx", "find_tx", "async_find_tx"],
        "qb_new": ["qbNew", "new"],
        "qb_table": ["qbTable", "table", "set_table", "from"],
        "qb_where_eq": ["qbWhereEq", "whereEq", "where_eq", "where", "where_eq_str", "where_eq_i64", "where_eq_f64", "where_eq_bool"],
        "qb_order_by": ["qbOrderBy", "orderBy", "order_by", "add_order_by", "add_order_desc"],
        "qb_limit": ["qbLimit", "limit", "set_limit"],
        "qb_build": ["qbBuild", "build", "build_select", "build_insert", "build_update", "build_delete"],
        "qb_free": ["qbFree", "free", "close"],
    }

    covered = []
    gaps = []

    for api in core_apis:
        api_name = api["name"]
        camel = snake_to_camel(api_name)
        found = False

        candidates = {api_name, camel}
        candidates.update(semantic_aliases.get(api_name, []))

        if binding_name in prefix_map:
            prefix = prefix_map[binding_name]
            candidates.add(f"{prefix}{api_name}")
        elif binding_name == "java":
            for exp in exports:
                for cand in candidates:
                    if exp.endswith(f"_{cand}"):
                        found = True
                        break
                if found:
                    break

        if not found:
            found = bool(candidates & exports)

        if found:
            covered.append(api_name)
        else:
            gaps.append(api)

    return covered, gaps


def main():
    parser = argparse.ArgumentParser(description="绑定层 API 覆盖率审计")
    parser.add_argument("--core", required=True, help="sz-orm-core/src 路径")
    parser.add_argument("--bindings", required=True, help="逗号分隔的绑定名")
    parser.add_argument("--core-api-list", help="核心 API 清单 JSON（指定后用清单作为分母）")
    parser.add_argument("--output", required=True, help="输出 JSON 文件路径")
    parser.add_argument("--threshold", type=float, default=0.9, help="覆盖率阈值")
    args = parser.parse_args()

    use_list = args.core_api_list is not None

    if use_list:
        core_api_list = load_core_api_list(args.core_api_list)
        total_core = len(core_api_list)
        print(f"核心外部 API 清单: {total_core} 个（来源: {args.core_api_list}）")
    else:
        core_apis = extract_core_apis(args.core)
        total_core = len(core_apis)
        print(f"核心 API 总数（全部 pub 符号）: {total_core}")

    binding_names = [b.strip() for b in args.bindings.split(",")]
    report = []
    packages_dir = Path(args.core).parent.parent

    for binding_name in binding_names:
        binding_dir = packages_dir / f"sz-orm-{binding_name}" / "src"
        exports = extract_binding_exports(str(binding_dir), binding_name)

        if use_list:
            covered_list, gap_list = match_core_to_exports(core_api_list, exports, binding_name)
            covered_count = len(covered_list)
            coverage_ratio = covered_count / total_core if total_core > 0 else 0.0
            gap_api_list = [{"api_path": g["name"], "category": g["category"], "description": g["description"]} for g in gap_list]
        else:
            covered_count = len(exports)
            coverage_ratio = covered_count / total_core if total_core > 0 else 0.0
            gap_api_list = []

        report.append({
            "binding_name": binding_name,
            "total_core_api": total_core,
            "covered_api": covered_count,
            "coverage_ratio": round(coverage_ratio, 4),
            "meets_threshold": coverage_ratio >= args.threshold,
            "gap_api_count": total_core - covered_count,
            "gap_api_list": gap_api_list,
            "generated_by": "check-binding-coverage.py",
        })

        status = "达标" if coverage_ratio >= args.threshold else "不达标"
        print(f"  {binding_name}: {covered_count}/{total_core} = {coverage_ratio:.2%} [{status}]")

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)

    print(f"\n报告已写入: {args.output}")


if __name__ == "__main__":
    main()
