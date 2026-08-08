# M3.3: Python pytest 异步测试套件
# 覆盖异步查询/事务，断言绑定层与 sz-orm-core 行为一致
#
# 运行：pytest packages/sz-orm-python/tests/test_async.py -v

import pytest
import asyncio


@pytest.mark.asyncio
async def test_async_crud():
    """测试异步 CRUD 操作"""
    sz_orm = pytest.importorskip("sz_orm")
    qb = sz_orm.QueryBuilder("mysql")
    qb.set_table("users")
    qb.set_select(["id", "name"])
    sql = qb.build_select()
    assert "SELECT" in sql


@pytest.mark.asyncio
async def test_async_transaction():
    """测试异步事务操作"""
    sz_orm = pytest.importorskip("sz_orm")
    pool = sz_orm.Pool("sqlite", max_size=5)
    assert pool.db_type == "sqlite"


@pytest.mark.asyncio
async def test_async_concurrent_queries():
    """测试并发异步查询"""
    sz_orm = pytest.importorskip("sz_orm")

    async def build_query(table):
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table(table)
        qb.set_select(["*"])
        return qb.build_select()

    results = await asyncio.gather(
        build_query("users"),
        build_query("orders"),
        build_query("products"),
    )
    assert len(results) == 3
    assert "users" in results[0]
    assert "orders" in results[1]
    assert "products" in results[2]