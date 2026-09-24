#!/usr/bin/env node
/**
 * v8.7.0: SFTP 下载文件脚本 — 从第二台服务器下载指定文件
 */

const { Client } = require('ssh2');
const fs = require('fs');
const path = require('path');

const SSH_HOST = '121.204.253.75';
const SSH_PORT = 32;
const SSH_USER = 'root';

const SSH_KEY = `-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACAZ7aSyL1OtDzpB6g9DOlXEkbW/8m7PoABKI6F+HPYClAAAAJDU+jBX1Pow
VwAAAAtzc2gtZWQyNTUxOQAAACAZ7aSyL1OtDzpB6g9DOlXEkbW/8m7PoABKI6F+HPYClA
AAAECTym3MgA4KhTkqlXmGdgDivtIyVsDqegfqguAUJhi9mxntpLIvU60POkHqD0M6VcSR
tb/ybs+gAEojoX4c9gKUAAAADHJvb3RAUzI1My03NQE=
-----END OPENSSH PRIVATE KEY-----`;

const remoteBase = '/www/rust/sz-orm/';
const localBase = 'E:/vue/test/鲜视达/rust/sz-orm/';

const files = [
    'packages/sz-orm-bench/src/lib.rs',
    'packages/sz-orm-bench/src/real_db.rs',
    'packages/sz-orm-bench/tests/real_db_bench.rs',
    'packages/sz-orm-core/src/access_control.rs',
    'packages/sz-orm-core/src/accessors.rs',
    'packages/sz-orm-core/src/active_model.rs',
    'packages/sz-orm-core/src/api_coverage.rs',
    'packages/sz-orm-core/tests/e2e_mysql_returning_alt.rs',
    'packages/sz-orm-core/tests/e2e_real_db_cache.rs',
    'packages/sz-orm-core/tests/e2e_real_db_pool.rs',
    'packages/sz-orm-core/tests/e2e_real_db_query.rs',
    'packages/sz-orm-core/tests/e2e_real_db_returning.rs',
];

const conn = new Client();

conn.on('ready', () => {
    conn.sftp((err, sftp) => {
        if (err) {
            console.error(`SFTP_ERROR: ${err.message}`);
            conn.end();
            process.exit(1);
        }

        let completed = 0;
        const total = files.length;

        files.forEach(f => {
            const remote = remoteBase + f;
            const local = localBase + f.replace(/\//g, path.sep);

            sftp.readFile(remote, (err, data) => {
                if (err) {
                    console.error(`DOWNLOAD_FAILED: ${f}: ${err.message}`);
                } else {
                    fs.writeFileSync(local, data);
                    console.log(`DOWNLOAD_OK: ${f}`);
                }
                completed++;
                if (completed === total) {
                    sftp.end();
                    conn.end();
                }
            });
        });
    });
});

conn.on('error', (err) => {
    console.error(`SSH_ERROR: ${err.message}`);
    process.exit(1);
});

conn.connect({
    host: SSH_HOST,
    port: SSH_PORT,
    username: SSH_USER,
    privateKey: SSH_KEY,
    readyTimeout: 30000,
});