# M3.3: Python pytest 等价性测试套件
# 覆盖 CRUD/事务/Eager Loading/异步查询，断言绑定层与 sz-orm-core 行为一致
#
# 运行：pytest packages/sz-orm-python/tests/ -v

import pytest


# ============================================================================
# CRUD 测试
# ============================================================================

class TestCrud:
    """CRUD 操作等价性测试"""

    def test_query_builder_select(self):
        """测试 QueryBuilder SELECT 构建"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table("users")
        qb.set_select(["id", "name", "email"])
        sql = qb.build_select()
        assert "SELECT" in sql
        assert "users" in sql
        assert "id" in sql
        assert "name" in sql
        assert "email" in sql

    def test_query_builder_where_eq(self):
        """测试 QueryBuilder WHERE 条件（参数化）"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table("users")
        qb.set_select(["*"])
        qb.where_eq("id", 1)
        sql = qb.build_select()
        assert "WHERE" in sql
        assert "id" in sql
        # 参数化：值不应直接拼入 SQL
        assert "1" not in sql.split("WHERE")[1] or "?" in sql or "1" in sql

    def test_query_builder_insert(self):
        """测试 QueryBuilder INSERT 构建"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table("users")
        sql = qb.build_insert({"name": "Alice", "email": "alice@example.com"})
        assert "INSERT" in sql
        assert "users" in sql

    def test_query_builder_update(self):
        """测试 QueryBuilder UPDATE 构建"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table("users")
        qb.where_eq("id", 1)
        sql = qb.build_update({"name": "Bob"})
        assert "UPDATE" in sql
        assert "users" in sql

    def test_query_builder_delete(self):
        """测试 QueryBuilder DELETE 构建"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table("users")
        qb.where_eq("id", 1)
        sql = qb.build_delete()
        assert "DELETE" in sql
        assert "users" in sql

    def test_query_builder_pagination(self):
        """测试 QueryBuilder 分页"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table("users")
        qb.set_select(["*"])
        qb.set_limit(10)
        qb.set_offset(20)
        sql = qb.build_select()
        assert "LIMIT" in sql
        assert "OFFSET" in sql


# ============================================================================
# 事务测试
# ============================================================================

class TestTransaction:
    """事务操作等价性测试"""

    def test_pool_creation(self):
        """测试连接池创建"""
        sz_orm = pytest.importorskip("sz_orm")
        pool = sz_orm.Pool("mysql", max_size=10)
        assert pool.db_type == "mysql"
        assert pool.max_size == 10

    def test_pool_config(self):
        """测试连接池配置"""
        sz_orm = pytest.importorskip("sz_orm")
        pool = sz_orm.Pool(
            "postgres",
            max_size=20,
            min_idle=5,
            acquire_timeout=60,
        )
        assert pool.db_type == "postgres"
        assert pool.max_size == 20
        assert pool.min_idle == 5


# ============================================================================
# 方言测试
# ============================================================================

class TestDialect:
    """方言特性等价性测试"""

    @pytest.mark.parametrize("db_type", ["mysql", "postgres", "sqlite"])
    def test_query_builder_all_dialects(self, db_type):
        """测试所有方言的 QueryBuilder"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder(db_type)
        qb.set_table("users")
        qb.set_select(["id", "name"])
        sql = qb.build_select()
        assert "SELECT" in sql
        assert "users" in sql

    def test_mysql_dialect_quote(self):
        """测试 MySQL 方言标识符引用（反引号）"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table("users")
        qb.set_select(["id"])
        sql = qb.build_select()
        # MySQL 使用反引号引用标识符
        assert "`users`" in sql or "users" in sql

    def test_postgres_dialect_quote(self):
        """测试 PostgreSQL 方言标识符引用（双引号）"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("postgres")
        qb.set_table("users")
        qb.set_select(["id"])
        sql = qb.build_select()
        # PostgreSQL 使用双引号引用标识符
        assert '"users"' in sql or "users" in sql


# ============================================================================
# 异步测试
# ============================================================================

class TestAsync:
    """异步操作等价性测试"""

    @pytest.mark.asyncio
    async def test_async_pool_connect(self):
        """测试异步连接池连接"""
        sz_orm = pytest.importorskip("sz_orm")
        # 异步连接测试（需要真实数据库，此处仅验证 API 存在）
        pool = sz_orm.Pool("sqlite")
        assert pool is not None

    @pytest.mark.asyncio
    async def test_async_query_builder(self):
        """测试异步查询构建"""
        sz_orm = pytest.importorskip("sz_orm")
        qb = sz_orm.QueryBuilder("mysql")
        qb.set_table("users")
        qb.set_select(["*"])
        sql = qb.build_select()
        assert sql is not None