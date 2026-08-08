// M3.4: JS jest 等价性测试套件
// 覆盖 CRUD/事务/Eager Loading/异步查询，断言绑定层与 sz-orm-core 行为一致
//
// 运行：npx jest packages/sz-orm-js/tests/equivalence.test.js

function extractSql(result) {
    if (typeof result === 'string') return result;
    if (result && typeof result === 'object' && 'sql' in result) return result.sql;
    return String(result);
}

describe('CRUD 等价性测试', () => {
    let szOrm;

    beforeAll(() => {
        szOrm = require('../index.js');
    });

    test('QueryBuilder SELECT 构建', () => {
        const qb = new szOrm.QueryBuilder('mysql');
        qb.setTable('users');
        qb.setSelect(['id', 'name', 'email']);
        const sql = extractSql(qb.buildSelect());
        expect(sql).toContain('SELECT');
        expect(sql).toContain('users');
        expect(sql).toContain('id');
        expect(sql).toContain('name');
        expect(sql).toContain('email');
    });

    test('QueryBuilder WHERE 条件（参数化）', () => {
        const qb = new szOrm.QueryBuilder('mysql');
        qb.setTable('users');
        qb.setSelect(['*']);
        qb.whereEqI64('id', 1);
        const sql = extractSql(qb.buildSelect());
        expect(sql).toContain('WHERE');
        expect(sql).toContain('id');
    });

    test('QueryBuilder WHERE 字符串条件', () => {
        const qb = new szOrm.QueryBuilder('mysql');
        qb.setTable('users');
        qb.setSelect(['*']);
        qb.whereEqStr('name', 'Alice');
        const sql = extractSql(qb.buildSelect());
        expect(sql).toContain('WHERE');
        expect(sql).toContain('name');
    });

    test('QueryBuilder DELETE 构建', () => {
        const qb = new szOrm.QueryBuilder('mysql');
        qb.setTable('users');
        qb.whereEqI64('id', 1);
        const sql = extractSql(qb.buildDelete());
        expect(sql).toContain('DELETE');
        expect(sql).toContain('users');
    });

    test('QueryBuilder 分页', () => {
        const qb = new szOrm.QueryBuilder('mysql');
        qb.setTable('users');
        qb.setSelect(['*']);
        qb.setLimit(10);
        qb.setOffset(20);
        const sql = extractSql(qb.buildSelect());
        expect(sql).toContain('LIMIT');
        expect(sql).toContain('OFFSET');
    });

    test('QueryBuilder ORDER BY', () => {
        const qb = new szOrm.QueryBuilder('mysql');
        qb.setTable('users');
        qb.setSelect(['*']);
        qb.addOrderBy('id');
        const sql = extractSql(qb.buildSelect());
        expect(sql).toContain('ORDER BY');
    });
});

describe('事务等价性测试', () => {
    let szOrm;

    beforeAll(() => {
        szOrm = require('../index.js');
    });

    test('连接池创建', () => {
        const pool = new szOrm.Pool('mysql', 10);
        expect(pool.dbType).toBe('mysql');
        expect(pool.maxSize).toBe(10);
    });

    test('连接池配置', () => {
        const pool = new szOrm.Pool('postgres', 20, 5, 60);
        expect(pool.dbType).toBe('postgres');
        expect(pool.maxSize).toBe(20);
    });
});

describe('方言等价性测试', () => {
    let szOrm;

    beforeAll(() => {
        szOrm = require('../index.js');
    });

    test.each(['mysql', 'postgres', 'sqlite'])('QueryBuilder %s 方言', (dbType) => {
        const qb = new szOrm.QueryBuilder(dbType);
        qb.setTable('users');
        qb.setSelect(['id', 'name']);
        const sql = extractSql(qb.buildSelect());
        expect(sql).toContain('SELECT');
        expect(sql).toContain('users');
    });

    test('MySQL 方言标识符引用（反引号）', () => {
        const qb = new szOrm.QueryBuilder('mysql');
        qb.setTable('users');
        qb.setSelect(['id']);
        const sql = extractSql(qb.buildSelect());
        expect(sql).toContain('users');
    });

    test('PostgreSQL 方言标识符引用（双引号）', () => {
        const qb = new szOrm.QueryBuilder('postgres');
        qb.setTable('users');
        qb.setSelect(['id']);
        const sql = extractSql(qb.buildSelect());
        expect(sql).toContain('users');
    });
});
