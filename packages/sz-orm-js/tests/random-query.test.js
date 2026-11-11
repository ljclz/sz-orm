// M5.2: JS jest 随机生成器套件（L3 批②）
// 递归生成随机 QueryBuilder 配置，同一生成物喂同步构建与异步构建两条路径，
// 产物逐字节对比（SQL 文本 + params JSON）。
//
// 生成纪律：
// - 白名单递归组合（只生成双方已验证支持的语法/结构）
// - RNG 用 mulberry32 手写 seeded（可复现、跨 fast-check 版本稳定）；
//   不用 fc 的 arb.generate()（3.x/4.x API 不兼容）
// - 失败收集不早退：跑完全部轮数，汇总全部反例
// - 归一化口径先行：params JSON 序列化前排序键；undefined 键剔除
//   （JSON.stringify 丢 undefined 但 Object.keys 不丢——行对比假差异源头）
//
// 运行：node node_modules/jest/bin/jest.js packages/sz-orm-js/tests/random-query.test.js

const fc = require('fast-check');

// mulberry32 手写 seeded RNG（可复现，不依赖 fc 内部 API）
function mulberry32(seed) {
    let a = seed >>> 0;
    return function () {
        a |= 0;
        a = (a + 0x6d2b79f5) | 0;
        let t = Math.imul(a ^ (a >>> 15), 1 | a);
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}

const COLS = ['id', 'name', 'age', 'ok'];
const TABLES = ['users', 'orders', 'items'];

// 白名单递归生成器：查询规格（表 + 列集 + 条件树）
// 已知差异面在数据池规避：不生成 NaN/±Infinity（params JSON 化语义未定）
function genCond(rng, depth) {
    const r = rng();
    if (depth === 0 || r < 0.4) {
        const kindRoll = rng();
        const col = COLS[Math.floor(rng() * COLS.length)];
        if (kindRoll < 0.3) return { kind: 'i64', col, v: Math.floor(rng() * 200) - 100 };
        if (kindRoll < 0.55) return { kind: 'str', col, v: 'u' + Math.floor(rng() * 20) };
        if (kindRoll < 0.75) return { kind: 'bool', col, v: rng() < 0.5 };
        return { kind: 'f64', col, v: Math.round((rng() * 200 - 100) * 8) / 8 };
    }
    const n = 2 + Math.floor(rng() * 2);
    const conds = [];
    for (let i = 0; i < n; i++) conds.push(genCond(rng, depth - 1));
    return { kind: rng() < 0.5 ? 'and' : 'or', conds };
}

function genSpec(rng) {
    const cols = [];
    const colCount = Math.floor(rng() * 4);
    for (let i = 0; i < colCount; i++) cols.push(COLS[Math.floor(rng() * COLS.length)]);
    const conds = [];
    const condCount = Math.floor(rng() * 5);
    for (let i = 0; i < condCount; i++) conds.push(genCond(rng, 2));
    return {
        table: TABLES[Math.floor(rng() * TABLES.length)],
        cols: cols.length ? cols : ['id'],
        conds,
    };
}

function applySpec(qb, spec) {
    qb.setTable(spec.table);
    qb.setSelect(spec.cols);
    for (const c of spec.conds) applyCond(qb, c);
}

function applyCond(qb, c) {
    if (c.kind === 'i64') qb.whereEqI64(c.col, c.v);
    else if (c.kind === 'str') qb.whereEqStr(c.col, c.v);
    else if (c.kind === 'bool') qb.whereEqBool(c.col, c.v);
    else if (c.kind === 'f64') qb.whereEqF64(c.col, c.v);
    else if (c.kind === 'and' || c.kind === 'or') for (const s of c.conds) applyCond(qb, s);
}

// 归一化：undefined 键剔除 + 键排序（行对比假差异源头剔除）
function normalizeSqlWithParams(result) {
    const sql = typeof result === 'string' ? result : result.sql;
    const params = (result && result.params ? result.params : []).map((p) => {
        if (p && typeof p === 'object' && !Array.isArray(p)) {
            const clean = {};
            for (const k of Object.keys(p).sort()) {
                if (p[k] !== undefined) clean[k] = p[k];
            }
            return clean;
        }
        return p;
    });
    return JSON.stringify({ sql, params });
}

describe('L3 随机生成器：QueryBuilder 同步 vs 异步构建差分（3 类 × 300 轮）', () => {
    let szOrm;

    beforeAll(() => {
        szOrm = require('../index.js');
    });

    test('同步确定性 + 同步/异步对拍，失败收集不早退', async () => {
        const counterexamples = [];
        let executed = 0;

        // 3 类生成域（不同 seed 段：纯等值 / 嵌套布尔 / 混合边界值）
        const classes = [
            { seed: 0x20260001, depth: 0 },
            { seed: 0x20260002, depth: 2 },
            { seed: 0x20260003, depth: 3 },
        ];

        for (const cls of classes) {
            const rng = mulberry32(cls.seed);
            for (let round = 0; round < 300; round++) {
                const spec = genSpec(rng);
                // 边界值注入：首轮固定边界集
                if (round === 0) {
                    spec.conds = [
                        { kind: 'i64', col: 'id', v: 9007199254740991 },
                        { kind: 'str', col: 'name', v: "it's \"q\" \\" },
                        { kind: 'f64', col: 'age', v: 1.5 },
                    ];
                }

                const s1 = new szOrm.QueryBuilder('mysql');
                applySpec(s1, spec);
                const sync1 = s1.buildSelect();
                const s2 = new szOrm.QueryBuilder('mysql');
                applySpec(s2, spec);
                const sync2 = s2.buildSelect();

                executed++;
                if (normalizeSqlWithParams(sync1) !== normalizeSqlWithParams(sync2)) {
                    counterexamples.push(
                        `类${cls.seed} 轮${round} 同步两次构建不一致: ${JSON.stringify(spec)}`
                    );
                    continue;
                }

                const a = new szOrm.QueryBuilder('mysql');
                applySpec(a, spec);
                const async1 = await a.asyncBuildSelect();
                if (normalizeSqlWithParams(sync1) !== normalizeSqlWithParams(async1)) {
                    counterexamples.push(
                        `类${cls.seed} 轮${round} 同步 vs 异步不一致: ${JSON.stringify(spec)}\n` +
                            `  sync:  ${normalizeSqlWithParams(sync1)}\n` +
                            `  async: ${normalizeSqlWithParams(async1)}`
                    );
                }
            }
        }

        // 汇总全部反例（不早退）
        expect(counterexamples).toEqual([]);
        expect(executed).toBe(900);
    });
});
