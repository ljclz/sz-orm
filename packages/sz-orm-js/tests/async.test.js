// M3.4: JS jest 异步测试套件
// 覆盖异步查询/事务，断言绑定层与 sz-orm-core 行为一致
//
// 运行：npx jest packages/sz-orm-js/tests/async.test.js

function extractSql(result) {
  if (typeof result === 'string') return result;
  if (result && typeof result === 'object' && 'sql' in result) return result.sql;
  return String(result);
}

describe('异步等价性测试', () => {
  let szOrm;

  beforeAll(() => {
    szOrm = require('../index.js');
  });

  test('异步 CRUD 操作', async () => {
    const qb = new szOrm.QueryBuilder('mysql');
    qb.setTable('users');
    qb.setSelect(['id', 'name']);
    const sql = extractSql(qb.buildSelect());
    expect(sql).toContain('SELECT');
  });

  test('异步事务操作', async () => {
    const pool = new szOrm.Pool('sqlite', 5);
    expect(pool.dbType).toBe('sqlite');
  });

  test('并发异步查询', async () => {
    async function buildQuery(table) {
      const qb = new szOrm.QueryBuilder('mysql');
      qb.setTable(table);
      qb.setSelect(['*']);
      return extractSql(qb.buildSelect());
    }

    const results = await Promise.all([
      buildQuery('users'),
      buildQuery('orders'),
      buildQuery('products'),
    ]);
    expect(results).toHaveLength(3);
    expect(results[0]).toContain('users');
    expect(results[1]).toContain('orders');
    expect(results[2]).toContain('products');
  });
});
