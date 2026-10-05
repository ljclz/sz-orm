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
const data = fs.readFileSync(localFile);
const b64 = data.toString('base64');
const chunkSize = 60000;
const conn = new Client();
let sent = 0;
conn.on('ready', () => {
    conn.exec(`rm -f /tmp/upload.b64`, (err, stream) => {
        if (err) { console.error(err); process.exit(1); }
        stream.on('close', () => {
            sendChunk();
        });
    });
});
function sendChunk() {
    if (sent >= b64.length) {
        conn.exec(`base64 -d /tmp/upload.b64 > ${remoteFile} && wc -l ${remoteFile}`, (err, stream) => {
            if (err) { console.error(err); process.exit(1); }
            stream.on('close', () => { conn.end(); process.exit(0); });
            stream.stdout.on('data', (d) => process.stdout.write(d));
            stream.stderr.on('data', (d) => process.stderr.write(d));
        });
        return;
    }
    const chunk = b64.slice(sent, sent + chunkSize);
    conn.exec(`echo -n '${chunk}' >> /tmp/upload.b64`, (err, stream) => {
        if (err) { console.error(err); process.exit(1); }
        stream.on('close', () => {
            sent += chunkSize;
            process.stdout.write('.');
            sendChunk();
        });
    });
}
conn.on('error', (e) => { console.error('SSH error:', e.message); process.exit(1); });
setTimeout(() => { console.error('\n[TIMEOUT]'); process.exit(124); }, 120000);
conn.connect({ host: '121.204.253.75', port: 32, username: 'root', privateKey: key });