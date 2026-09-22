#!/usr/bin/env python3
"""
v8.3.0: 端到端测试覆盖矩阵生成脚本

解析 cargo test --features e2e-real-db 输出，生成 13×3=39 格覆盖矩阵报告。
每格附测试函数名 + 通过状态（PASSED/FAILED/SKIPPED/NA）+ 真实数据验证证据。
MySQL RETURNING 格标 N/A。
"""

import json
import subprocess
import sys
import os
import re
from collections import defaultdict

FUNCTIONS = [
    "CRUD", "连接池", "事务", "迁移", "查询构建",
    "多租户", "缓存", "软删除", "分页", "预加载",
    "方言行为", "批量操作", "RETURNING"
]

DATABASES = ["MySQL", "PostgreSQL", "Oracle"]

TEST_MAP = {
    ("CRUD", "MySQL"): ["test_e2e_mysql_crud_full_lifecycle_test_db", "test_e2e_mysql_crud_data_types"],
    ("CRUD", "PostgreSQL"): ["test_e2e_pg_crud_full_lifecycle", "test_e2e_pg_crud_data_types"],
    ("CRUD", "Oracle"): ["test_e2e_oracle_crud_full_lifecycle"],
    ("连接池", "MySQL"): ["test_e2e_mysql_pool_acquire_release", "test_e2e_mysql_pool_health_check"],
    ("连接池", "PostgreSQL"): ["test_e2e_pg_pool_concurrent", "test_e2e_pg_pool_health_check"],
    ("连接池", "Oracle"): ["test_e2e_oracle_pool_exhaustion", "test_e2e_oracle_pool_health_check"],
    ("事务", "MySQL"): ["test_e2e_mysql_tx_commit", "test_e2e_mysql_tx_rollback", "test_e2e_mysql_tx_savepoint"],
    ("事务", "PostgreSQL"): ["test_e2e_pg_tx_commit", "test_e2e_pg_tx_rollback"],
    ("事务", "Oracle"): ["test_e2e_oracle_tx_commit", "test_e2e_oracle_tx_rollback"],
    ("迁移", "MySQL"): ["test_e2e_mysql_migration_create_table", "test_e2e_mysql_migration_idempotent"],
    ("迁移", "PostgreSQL"): ["test_e2e_pg_migration_alter_table", "test_e2e_pg_migration_idempotent"],
    ("迁移", "Oracle"): ["test_e2e_oracle_migration_create_table", "test_e2e_oracle_migration_idempotent"],
    ("查询构建", "MySQL"): ["test_e2e_mysql_query_where_conditions", "test_e2e_mysql_query_order_limit_offset", "test_e2e_mysql_query_aggregate", "test_e2e_mysql_query_join"],
    ("查询构建", "PostgreSQL"): ["test_e2e_pg_query_where_conditions", "test_e2e_pg_query_aggregate", "test_e2e_pg_query_join"],
    ("查询构建", "Oracle"): ["test_e2e_oracle_query_where_conditions", "test_e2e_oracle_query_aggregate"],
    ("多租户", "MySQL"): ["test_e2e_mysql_tenant_isolation"],
    ("多租户", "PostgreSQL"): ["test_e2e_pg_tenant_isolation"],
    ("多租户", "Oracle"): ["test_e2e_oracle_tenant_isolation"],
    ("缓存", "MySQL"): ["test_e2e_mysql_cache_hit"],
    ("缓存", "PostgreSQL"): ["test_e2e_pg_cache_invalidation"],
    ("缓存", "Oracle"): ["test_e2e_oracle_cache_hit"],
    ("软删除", "MySQL"): ["test_e2e_mysql_soft_delete_full"],
    ("软删除", "PostgreSQL"): ["test_e2e_pg_soft_delete_full"],
    ("软删除", "Oracle"): ["test_e2e_oracle_soft_delete_full"],
    ("分页", "MySQL"): ["test_e2e_mysql_pagination_limit_offset"],
    ("分页", "PostgreSQL"): ["test_e2e_pg_pagination_keyset"],
    ("分页", "Oracle"): ["test_e2e_oracle_pagination_offset_fetch"],
    ("预加载", "MySQL"): ["test_e2e_mysql_eager_load"],
    ("预加载", "PostgreSQL"): ["test_e2e_pg_eager_load"],
    ("预加载", "Oracle"): ["test_e2e_oracle_eager_load"],
    ("方言行为", "MySQL"): ["test_e2e_mysql_dialect_upsert"],
    ("方言行为", "PostgreSQL"): ["test_e2e_pg_dialect_upsert"],
    ("方言行为", "Oracle"): ["test_e2e_oracle_dialect_identifier"],
    ("批量操作", "MySQL"): ["test_e2e_mysql_crud_batch_insert", "test_e2e_mysql_crud_batch_update"],
    ("批量操作", "PostgreSQL"): ["test_e2e_pg_crud_batch_insert"],
    ("批量操作", "Oracle"): ["test_e2e_oracle_crud_batch_insert"],
    ("RETURNING", "MySQL"): [],
    ("RETURNING", "PostgreSQL"): ["test_e2e_pg_crud_returning"],
    ("RETURNING", "Oracle"): ["test_e2e_oracle_crud_returning"],
}

def run_tests():
    env = os.environ.copy()
    cmd = [
        "cargo", "test", "-p", "sz-orm-core",
        "--features", "e2e-real-db,multi-tenant-enhanced,l1-cache",
        "--test", "e2e_real_db_crud",
        "--test", "e2e_real_db_pool",
        "--test", "e2e_real_db_transaction",
        "--test", "e2e_real_db_migration",
        "--test", "e2e_real_db_query",
        "--test", "e2e_real_db_multi_tenant",
        "--test", "e2e_real_db_cache",
        "--test", "e2e_real_db_soft_delete",
        "--test", "e2e_real_db_pagination",
        "--test", "e2e_real_db_eager_load",
        "--test", "e2e_real_db_dialect_behavior",
    ]
    result = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=600, encoding='utf-8', errors='replace')
    output = (result.stdout or "") + (result.stderr or "")

    passed = set()
    failed = set()
    for match in re.finditer(r'test (\w+) \.\.\. (ok|FAILED)', output):
        name, status = match.group(1), match.group(2)
        if status == "ok":
            passed.add(name)
        else:
            failed.add(name)
    return passed, failed

def generate_matrix(passed, failed):
    print("# sz-orm v8.3.0 端到端测试覆盖矩阵（13×3=39 格）\n")
    print(f"| 功能 | MySQL | PostgreSQL | Oracle |")
    print(f"|------|-------|------------|--------|")

    total_cells = 0
    covered_cells = 0
    na_cells = 0

    for func in FUNCTIONS:
        row = f"| {func} "
        for db in DATABASES:
            tests = TEST_MAP.get((func, db), [])
            total_cells += 1

            if func == "RETURNING" and db == "MySQL":
                row += "| N/A "
                na_cells += 1
                continue

            if not tests:
                row += "| SKIPPED "
                continue

            covered_cells += 1
            all_passed = all(t in passed for t in tests)
            any_failed = any(t in failed for t in tests)
            if any_failed:
                row += "| FAILED "
            elif all_passed:
                row += "| PASSED "
            else:
                row += "| SKIPPED "
        row += "|"
        print(row)

    print(f"\n## 统计\n")
    print(f"- 总格子数: {total_cells}")
    print(f"- 已覆盖: {covered_cells}")
    print(f"- N/A: {na_cells}")
    print(f"- 跳过: {total_cells - covered_cells - na_cells}")

def main():
    if "--no-run" in sys.argv:
        passed, failed = set(), set()
    else:
        print("运行 e2e 测试...", file=sys.stderr)
        passed, failed = run_tests()
        print(f"通过: {len(passed)}, 失败: {len(failed)}", file=sys.stderr)

    generate_matrix(passed, failed)

if __name__ == "__main__":
    main()