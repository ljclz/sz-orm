#!/usr/bin/env node
/**
 * v8.7.0: 远程门禁执行脚本 — 在第二台服务器上执行门禁 1（fmt）+ 门禁 6（audit）
 */

const { Client } = require('ssh2');

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

const command = process.argv[2] || 'echo no command';
const timeoutMs = parseInt(process.argv[3] || '120000', 10);

const conn = new Client();

conn.on('ready', () => {
    conn.exec(command, (err, stream) => {
        if (err) {
            console.error(`SSH_EXEC_ERROR: ${err.message}`);
            conn.end();
            process.exit(1);
        }

        let stdout = '';
        let stderr = '';

        stream.on('close', () => {
            console.log(stdout);
            if (stderr) console.error(stderr);
            conn.end();
        });

        stream.on('data', (data) => { stdout += data.toString(); });
        stream.stderr.on('data', (data) => { stderr += data.toString(); });
    });
});

conn.on('error', (err) => {
    console.error(`SSH_ERROR: ${err.message}`);
    process.exit(1);
});

setTimeout(() => {
    console.error('SSH_TIMEOUT');
    conn.end();
    process.exit(2);
}, timeoutMs);

conn.connect({
    host: SSH_HOST,
    port: SSH_PORT,
    username: SSH_USER,
    privateKey: SSH_KEY,
    readyTimeout: 30000,
});