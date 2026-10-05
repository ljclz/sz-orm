const { Client } = require('ssh2');
const key = `-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACAZ7aSyL1OtDzpB6g9DOlXEkbW/8m7PoABKI6F+HPYClAAAAJDU+jBX1Pow
VwAAAAtzc2gtZWQyNTUxOQAAACAZ7aSyL1OtDzpB6g9DOlXEkbW/8m7PoABKI6F+HPYClA
AAAECTym3MgA4KhTkqlXmGdgDivtIyVsDqegfqguAUJhi9mxntpLIvU60POkHqD0M6VcSR
tb/ybs+gAEojoX4c9gKUAAAADHJvb3RAUzI1My03NQE=
-----END OPENSSH PRIVATE KEY-----`;
const cmd = process.argv[2];
const timeout = parseInt(process.argv[3] || '120000');
const conn = new Client();
conn.on('ready', () => {
    conn.exec(cmd, (err, stream) => {
        if (err) { console.error(err); process.exit(1); }
        stream.on('close', () => { conn.end(); });
        stream.stderr.on('data', (d) => { process.stderr.write(d); });
        stream.stdout.on('data', (d) => { process.stdout.write(d); });
    });
});
conn.on('error', (e) => { console.error('SSH error:', e.message); process.exit(1); });
setTimeout(() => { console.error('\n[TIMEOUT]'); process.exit(124); }, timeout);
conn.connect({ host: '121.204.253.75', port: 32, username: 'root', privateKey: key });