#!/usr/bin/env node
/**
 * v8.3.0: 数据库可达性预检脚本
 *
 * 预检 3 个数据库可达性：
 *   - MySQL 121.204.253.75:8802（3 库 test/shop/njszjt）
 *   - PostgreSQL 121.204.253.75（端口由 SZ_ORM_E2E_PG_URL 或 SSH 探测确定）
 *   - Oracle 127.0.0.1:1521
 *
 * 输出 JSON 报告（含 host/port/database/reachable/error_code），不可达数据库输出明确提示 + 跳过建议。
 *
 * 环境变量：
 *   SZ_ORM_E2E_MYSQL_URL_TEST / SZ_ORM_E2E_MYSQL_URL_SHOP / SZ_ORM_E2E_MYSQL_URL_NJSZJT
 *   SZ_ORM_E2E_PG_URL
 *   SZ_ORM_E2E_ORACLE_USER / SZ_ORM_E2E_ORACLE_PASSWORD / SZ_ORM_E2E_ORACLE_CONNECT_STRING
 *
 * 用法：
 *   node scripts/e2e_db_precheck.js
 */

const net = require('net');

const results = [];

function checkTcp(host, port, database, label) {
    return new Promise((resolve) => {
        const socket = new net.Socket();
        socket.setTimeout(5000);
        socket.on('connect', () => {
            socket.destroy();
            results.push({ label, host, port, database, reachable: true });
            resolve();
        });
        socket.on('timeout', () => {
            socket.destroy();
            results.push({ label, host, port, database, reachable: false, error_code: 'TIMEOUT' });
            resolve();
        });
        socket.on('error', (err) => {
            results.push({ label, host, port, database, reachable: false, error_code: err.code || 'ERROR', error: err.message });
            resolve();
        });
        socket.connect(port, host);
    });
}

async function main() {
    const mysqlHost = '121.204.253.75';
    const mysqlPort = 8802;

    const mysqlUrls = {
        test: process.env.SZ_ORM_E2E_MYSQL_URL_TEST,
        shop: process.env.SZ_ORM_E2E_MYSQL_URL_SHOP,
        njszjt: process.env.SZ_ORM_E2E_MYSQL_URL_NJSZJT,
    };

    for (const [db, url] of Object.entries(mysqlUrls)) {
        if (url) {
            await checkTcp(mysqlHost, mysqlPort, db, `MySQL/${db}`);
        } else {
            results.push({
                label: `MySQL/${db}`,
                host: mysqlHost,
                port: mysqlPort,
                database: db,
                reachable: false,
                error_code: 'ENV_NOT_SET',
                error: `SZ_ORM_E2E_MYSQL_URL_${db.toUpperCase()} 未设置`,
            });
        }
    }

    const pgUrl = process.env.SZ_ORM_E2E_PG_URL || process.env.POSTGRES_URL;
    if (pgUrl) {
        const pgMatch = pgUrl.match(/@([^:]+):(\d+)/);
        if (pgMatch) {
            await checkTcp(pgMatch[1], parseInt(pgMatch[2]), 'lewuli', 'PostgreSQL');
        } else {
            await checkTcp('121.204.253.75', 5432, 'lewuli', 'PostgreSQL');
        }
    } else {
        results.push({
            label: 'PostgreSQL',
            host: '121.204.253.75',
            port: 'unknown',
            database: 'lewuli',
            reachable: false,
            error_code: 'ENV_NOT_SET',
            error: 'SZ_ORM_E2E_PG_URL / POSTGRES_URL 均未设置',
        });
    }

    const oracleUser = process.env.SZ_ORM_E2E_ORACLE_USER || process.env.SZ_ORM_ORACLE_USER || 'sys';
    const oracleConnStr = process.env.SZ_ORM_E2E_ORACLE_CONNECT_STRING
        || process.env.SZ_ORM_ORACLE_CONNECT_STRING
        || '127.0.0.1:1521/freepdb1.FALSE';
    const oracleHostMatch = oracleConnStr.match(/^([^:]+):(\d+)/);
    if (oracleHostMatch) {
        await checkTcp(oracleHostMatch[1], parseInt(oracleHostMatch[2]), 'freepdb1', `Oracle(${oracleUser})`);
    } else {
        await checkTcp('127.0.0.1', 1521, 'freepdb1', `Oracle(${oracleUser})`);
    }

    console.log(JSON.stringify({ results }, null, 2));

    const unreachable = results.filter(r => !r.reachable);
    if (unreachable.length > 0) {
        console.error('\n不可达数据库:');
        for (const r of unreachable) {
            console.error(`  ${r.label}: ${r.host}:${r.port} — ${r.error_code}${r.error ? ' (' + r.error + ')' : ''}`);
        }
        console.error('\n跳过建议: 不可达数据库的 e2e 测试将自动跳过（非 panic）');
    }
}

main();