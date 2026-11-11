// M5.1: JS jest 路径对标套件（L2 批①）
// 对每对"快路径 vs 普通路径"，同一输入双跑对比输出。
// 靶面：napi 绑定层 QueryBuilder 同步构建 vs 异步构建（asyncBuildSelect/asyncBuildDelete）
//       ——两条代码路径必须产出完全一致的 {sql, params}。
// 对比分级：SQL 文本 + params 数组 → 逐字节严格（无浮点域）。
// 数据面 = 固定边界集 + fast-check PBT（seeded，可复现）。
//
// 运行：node node_modules/jest/bin/jest.js packages/sz-orm-js/tests/path-parity.test.js

const fc = require('fast-check');

function extractSql(result) {
    if (typeof result === 'string') return result;
    if (result && typeof result === 'object' && 'sql' in result) return result.sql;
    return String(result);
}

function extractParams(result) {
    if (result && typeof result === 'object' && 'params' in result) return result.params;
    return [];
}

// 固定边界集：空 / 单条件 / 多条件 / 特殊字符标识符与值 / 极值整数
const BOUNDARY_CASES = [
    { table: 'users', cols: ['id', 'name'], conds: [] },
    { table: 'users', cols: ['id'], conds: [{ kind: 'i64', col: 'id', v: 1 }] },
    {
        table: 't',
        cols: ['a', 'b', 'c'],
        conds: [
            { kind: 'i64', col: 'id', v: 9007199254740991 },
            { kind: 'str', col: 'name', v: "it's \"q\" \\ end" },
            { kind: 'bool', col: 'ok', v: true },
            { kind: 'f64', col: 'score', v: 1.5 },
        ],
    },
    { table: 'or"der', cols: ['back`tick'], conds: [{ kind: 'str', col: 'c', v: '中文🚀' }] },
    { table: 't', cols: [], conds: [{ kind: 'i64', col: 'id', v: -1 }] },
];

describe('L2 路径对标：QueryBuilder 同步 vs 异步构建', () => {
    let szOrm;

    beforeAll(() => {
        szOrm = require('../index.js');
    });

    function buildBoth(dbType, spec) {
        const sync = new szOrm.QueryBuilder(dbType);
        sync.setTable(spec.table);
        sync.setSelect(spec.cols);
        for (const c of spec.conds) {
            if (c.kind === 'i64') sync.whereEqI64(c.col, c.v);
            else if (c.kind === 'str') sync.whereEqStr(c.col, c.v);
            else if (c.kind === 'bool') sync.whereEqBool(c.col, c.v);
            else if (c.kind === 'f64') sync.whereEqF64(c.col, c.v);
        }
        return sync.buildSelect();
    }

    test('固定边界集：buildSelect 与 asyncBuildSelect 逐字节一致', async () => {
        for (const db of ['mysql', 'sqlite', 'postgresql']) {
            for (const spec of BOUNDARY_CASES) {
                const syncResult = buildBoth(db, spec);
                const qbAsync = new szOrm.QueryBuilder(db);
                qbAsync.setTable(spec.table);
                qbAsync.setSelect(spec.cols);
                for (const c of spec.conds) {
                    if (c.kind === 'i64') qbAsync.whereEqI64(c.col, c.v);
                    else if (c.kind === 'str') qbAsync.whereEqStr(c.col, c.v);
                    else if (c.kind === 'bool') qbAsync.whereEqBool(c.col, c.v);
                    else if (c.kind === 'f64') qbAsync.whereEqF64(c.col, c.v);
                }
                const asyncResult = await qbAsync.asyncBuildSelect();
                expect(extractSql(syncResult)).toBe(extractSql(asyncResult));
                expect(JSON.stringify(extractParams(syncResult))).toBe(
                    JSON.stringify(extractParams(asyncResult))
                );
            }
        }
    });

    test('buildDelete 与 asyncBuildDelete 逐字节一致（边界集）', async () => {
        for (const db of ['mysql', 'sqlite']) {
            const sync = new szOrm.QueryBuilder(db);
            sync.setTable('users');
            sync.whereEqI64('id', 1);
            const qbAsync = new szOrm.QueryBuilder(db);
            qbAsync.setTable('users');
            qbAsync.whereEqI64('id', 1);
            const a = sync.buildDelete();
            const b = await qbAsync.asyncBuildDelete();
            expect(extractSql(a)).toBe(extractSql(b));
            expect(JSON.stringify(extractParams(a))).toBe(JSON.stringify(extractParams(b)));
        }
    });

    test('PBT：随机条件序列，同步/异步产物一致（300 轮，seeded）', async () => {
        const colArb = fc.constantFrom('id', 'name', 'age', 'ok');
        const condArb = fc.oneof(
            fc.tuple(colArb, fc.integer({ min: -2147483648, max: 2147483647 })).map(([col, v]) => ({ kind: 'i64', col, v })),
            fc.tuple(colArb, fc.string({ maxLength: 16 })).map(([col, v]) => ({ kind: 'str', col, v })),
            fc.tuple(colArb, fc.boolean()).map(([col, v]) => ({ kind: 'bool', col, v })),
            fc.tuple(colArb, fc.double({ noNaN: true, noDefaultInfinity: true })).map(([col, v]) => ({ kind: 'f64', col, v })),
        );
        const specArb = fc.record({
            table: fc.constantFrom('users', 'orders', 'items'),
            cols: fc.array(fc.constantFrom('id', 'name', 'age', 'ok'), { maxLength: 4 }),
            conds: fc.array(condArb, { maxLength: 6 }),
        });
        await fc.assert(
            fc.asyncProperty(specArb, async (spec) => {
                const build = (qb) => {
                    qb.setTable(spec.table);
                    qb.setSelect(spec.cols);
                    for (const c of spec.conds) {
                        if (c.kind === 'i64') qb.whereEqI64(c.col, c.v);
                        else if (c.kind === 'str') qb.whereEqStr(c.col, c.v);
                        else if (c.kind === 'bool') qb.whereEqBool(c.col, c.v);
                        else if (c.kind === 'f64') qb.whereEqF64(c.col, c.v);
                    }
                    return qb.buildSelect();
                };
                const s = build(new szOrm.QueryBuilder('mysql'));
                const a = build(new szOrm.QueryBuilder('mysql'));
                // 同步路径两次构建也应一致（确定性）
                expect(extractSql(s)).toBe(extractSql(a));
                expect(JSON.stringify(extractParams(s))).toBe(JSON.stringify(extractParams(a)));
                // 异步路径对拍
                const buildAsync = async (qb) => {
                    qb.setTable(spec.table);
                    qb.setSelect(spec.cols);
                    for (const c of spec.conds) {
                        if (c.kind === 'i64') await qb.whereEqI64(c.col, c.v);
                        else if (c.kind === 'str') await qb.whereEqStr(c.col, c.v);
                        else if (c.kind === 'bool') await qb.whereEqBool(c.col, c.v);
                        else if (c.kind === 'f64') await qb.whereEqF64(c.col, c.v);
                    }
                    return await qb.asyncBuildSelect();
                };
                const asyncResult = await buildAsync(new szOrm.QueryBuilder('mysql'));
                expect(extractSql(asyncResult)).toBe(extractSql(s));
                expect(JSON.stringify(extractParams(asyncResult))).toBe(
                    JSON.stringify(extractParams(s))
                );
            }),
            { numRuns: 300, seed: 20261111 },
        );
    });
});
