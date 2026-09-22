#!/usr/bin/env node
/**
 * v8.3.0: SSH 端口探测脚本 — 探测第二台服务器 PostgreSQL 实际端口
 *
 * 使用 Node.js ssh2 包连接 121.204.253.75:32，执行 `ss -tlnp | grep postgres`
 * 探测 PostgreSQL 实际端口，输出端口号到 stdout。
 *
 * 认证方式（按优先级）：
 *   1. 密钥认证（内置第二台服务器 root 密钥）
 *   2. 密码认证（SZ_ORM_E2E_SSH_PASSWORD 环境变量）
 *
 * 用法：
 *   node scripts/e2e_ssh_probe.js
 *
 * 输出：
 *   成功：端口号（如 5432）
 *   失败：E2E_SSH_UNREACHABLE + 错误信息
 */

const { Client } = require('ssh2');

const SSH_HOST = '121.204.253.75';
const SSH_PORT = 32;
const SSH_USER = 'root';
const CANDIDATE_PORTS = [5432, 5433, 5434, 5435, 5436];

const SSH_KEY = `-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACAZ7aSyL1OtDzpB6g9DOlXEkbW/8m7PoABKI6F+HPYClAAAAJDU+jBX1Pow
VwAAAAtzc2gtZWQyNTUxOQAAACAZ7aSyL1OtDzpB6g9DOlXEkbW/8m7PoABKI6F+HPYClA
AAAECTym3MgA4KhTkqlXmGdgDivtIyVsDqegfqguAUJhi9mxntpLIvU60POkHqD0M6VcSR
tb/ybs+gAEojoX4c9gKUAAAADHJvb3RAUzI1My03NQE=
-----END OPENSSH PRIVATE KEY-----`;

const sshPassword = process.env.SZ_ORM_E2E_SSH_PASSWORD;

const conn = new Client();

conn.on('ready', () => {
    conn.exec('ss -tlnp | grep postgres', (err, stream) => {
        if (err) {
            console.error(`E2E_SSH_UNREACHABLE: 执行命令失败: ${err.message}`);
            conn.end();
            process.exit(1);
        }

        let stdout = '';
        let stderr = '';

        stream.on('close', () => {
            if (stdout.trim()) {
                const portMatch = stdout.match(/:(\d+)\s/);
                if (portMatch) {
                    console.log(portMatch[1]);
                    conn.end();
                    process.exit(0);
                }
            }

            console.log(`SSH 连接成功，但未找到 PostgreSQL 进程，尝试候选端口: ${CANDIDATE_PORTS.join(', ')}`);
            probeCandidatePorts(conn, 0);
        });

        stream.on('data', (data) => { stdout += data.toString(); });
        stream.stderr.on('data', (data) => { stderr += data.toString(); });
    });
});

conn.on('error', (err) => {
    console.error(`E2E_SSH_UNREACHABLE: SSH 连接失败 (${SSH_HOST}:${SSH_PORT}): ${err.message}`);
    console.error('请确认第二台服务器可达 + SSH 密钥/密码正确');
    process.exit(1);
});

function probeCandidatePorts(conn, index) {
    if (index >= CANDIDATE_PORTS.length) {
        console.error('E2E_SSH_UNREACHABLE: 所有候选端口均不可达，请手动确认 PostgreSQL 端口');
        conn.end();
        process.exit(1);
    }

    const port = CANDIDATE_PORTS[index];
    conn.exec(`ss -tln | grep ':${port} '`, (err, stream) => {
        if (err) {
            probeCandidatePorts(conn, index + 1);
            return;
        }

        let stdout = '';
        stream.on('close', () => {
            if (stdout.trim()) {
                console.log(port);
                conn.end();
                process.exit(0);
            } else {
                probeCandidatePorts(conn, index + 1);
            }
        });
        stream.on('data', (data) => { stdout += data.toString(); });
        stream.stderr.on('data', () => { });
    });
}

const connectOpts = {
    host: SSH_HOST,
    port: SSH_PORT,
    username: SSH_USER,
    readyTimeout: 10000,
};

if (sshPassword) {
    connectOpts.password = sshPassword;
} else {
    connectOpts.privateKey = SSH_KEY;
}

conn.connect(connectOpts);
