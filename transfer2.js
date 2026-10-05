const { Client } = require('ssh2');
const fs = require('fs');
const key = `-----BEGIN OPENSSH PRIVATE KEY-----
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
        if (err) { console.error(err); process.exit(1); }
        const data = fs.readFileSync(localFile);
        const ws = sftp.createWriteStream(remoteFile);
        ws.on('close', () => {
            console.log('Uploaded', localFile, '->', remoteFile, '(' + data.length + ' bytes)');
            conn.end();
            process.exit(0);
        });
        ws.on('error', (e) => { console.error('Write error:', e.message); process.exit(1); });
        ws.write(data);
        ws.end();
    });
});
conn.on('error', (e) => { console.error('SSH error:', e.message); process.exit(1); });
setTimeout(() => { console.error('\n[TIMEOUT]'); process.exit(124); }, 60000);
conn.connect({ host: '121.204.253.75', port: 32, username: 'root', privateKey: key });