#!/usr/bin/env node
/**
 * v8.7.0: SFTP 上传单个文件到服务器
 */
const { Client } = require('ssh2');
const fs = require('fs');

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

const localFile = process.argv[2];
const remoteFile = process.argv[3];

const conn = new Client();

conn.on('ready', () => {
    conn.sftp((err, sftp) => {
        if (err) {
            console.error(`SFTP_ERROR: ${err.message}`);
            conn.end();
            process.exit(1);
        }

        const data = fs.readFileSync(localFile);
        const stream = sftp.createWriteStream(remoteFile);
        stream.on('error', (err) => {
            console.error(`UPLOAD_FAILED: ${err.message}`);
            sftp.end();
            conn.end();
        });
        stream.on('close', () => {
            console.log(`UPLOAD_OK: ${localFile} -> ${remoteFile}`);
            sftp.end();
            conn.end();
        });
        stream.end(data);
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