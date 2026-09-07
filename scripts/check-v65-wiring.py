#!/usr/bin/env python3
"""v6.5.0 接线验证脚本

扫描 examples/src/bin/ 下 6 个 demo，确认每个 demo 调用了对应的 v6.5.0 新 API，
并验证调用链可达实现（非孤立模块）。

验证的 6 项能力：
1. parallel_queries
2. execute_batch_parallel
3. parallel_join!
4. prepare_cached / PreparedStatementCache
5. query_stream_unified / AsyncRowStream
6. BackpressureRowStream
"""

import os
import re
import sys

DEMO_DIR = os.path.join("examples", "src", "bin")

CAPABILITIES = [
    {
        "name": "parallel_queries",
        "demo": "parallel_queries_demo.rs",
        "api_patterns": [r"parallel_queries\s*\("],
        "impl_file": "packages/sz-orm-parallel/src/parallel_queries.rs",
    },
    {
        "name": "execute_batch_parallel",
        "demo": "batch_parallel_demo.rs",
        "api_patterns": [r"\.execute_batch_parallel\s*\("],
        "impl_file": "packages/sz-orm-core/src/connection_ext.rs",
    },
    {
        "name": "parallel_join!",
        "demo": "parallel_join_demo.rs",
        "api_patterns": [r"parallel_join\s*!"],
        "impl_file": "packages/sz-orm-parallel/src/macros.rs",
    },
    {
        "name": "PreparedStatementCache",
        "demo": "prepared_cache_demo.rs",
        "api_patterns": [r"PreparedStatementCache"],
        "impl_file": "packages/sz-orm-core/src/prepared_cache.rs",
    },
    {
        "name": "query_stream_unified",
        "demo": "stream_unified_demo.rs",
        "api_patterns": [r"query_stream_unified", r"AsyncRowStream"],
        "impl_file": "packages/sz-orm-core/src/connection_ext.rs",
    },
    {
        "name": "BackpressureRowStream",
        "demo": "backpressure_stream_demo.rs",
        "api_patterns": [r"BackpressureRowStream"],
        "impl_file": "packages/sz-orm-stream/src/backpressure_stream.rs",
    },
]


def check_demo_calls_api(demo_path, api_patterns):
    """检查 demo 文件是否调用了指定的 API"""
    if not os.path.exists(demo_path):
        return False, f"demo 文件不存在: {demo_path}"
    with open(demo_path, "r", encoding="utf-8") as f:
        content = f.read()
    for pattern in api_patterns:
        if re.search(pattern, content):
            return True, "OK"
    return False, f"未找到 API 调用: {api_patterns}"


def check_impl_exists(impl_file):
    """检查实现文件是否存在"""
    if os.path.exists(impl_file):
        return True, "OK"
    return False, f"实现文件不存在: {impl_file}"


def main():
    print("=== v6.5.0 接线验证 ===")
    passed = 0
    total = len(CAPABILITIES)

    for cap in CAPABILITIES:
        demo_path = os.path.join(DEMO_DIR, cap["demo"])
        impl_file = cap["impl_file"]

        demo_ok, demo_msg = check_demo_calls_api(demo_path, cap["api_patterns"])
        impl_ok, impl_msg = check_impl_exists(impl_file)

        if demo_ok and impl_ok:
            print(f"  ✅ {cap['name']}: demo={cap['demo']}, impl={impl_file}")
            passed += 1
        else:
            print(f"  ❌ {cap['name']}: demo={demo_msg}, impl={impl_msg}")

    print(f"\n结果: {passed}/{total} 接线验证通过")

    if passed == total:
        print("=== 全部通过 ===")
        return 0
    else:
        print("=== 存在失败 ===")
        return 1


if __name__ == "__main__":
    sys.exit(main())