const { Client } = require('ssh2');
const fs = require('fs');

const privateKey = `-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACDU29lusYU9XtBPEOoNY45s9/8FyBUvqZ71S3M+/cunEAAAAJDSsSHU0rEh
1AAAAAtzc2gtZWQyNTUxOQAAACDU29nusYU9XtBPEOoNY45s9/8FyBUvqZ71S3M+/cunEA
AAAEB+NB6sHDsFL6+xDJci9+NbBCvTdlTIgGLLOM2mZPsNPdTb2W6xhT1e0E8Q6g1jjmz3
/wXIFS+pnvVLcz79y6cQAAAADHJvb3RAUzI1My03NQE=
-----END OPENSSH PRIVATE KEY-----`;

const cmd = process.argv[2] || 'echo "connected" && rustc --version && cargo --version && ls /www/rust/ 2>/dev/null || echo "no /www/rust"';

const conn = new Client();
conn.on('ready', () => {
    conn.exec(cmd, (err, stream) => {
        if (err) { console.error('exec error:', err); process.exit(1); }
        let stdout = '', stderr = '';
        stream.on('data', (d) => stdout += d);
        stream.stderr.on('data', (d) => stderr += d);
        stream.on('close', () => {
            if (stdout) console.log(stdout.trim());
            if (stderr) console.error(stderr.trim());
            conn.end();
        });
    });
}).on('error', (err) => {
    console.error('SSH error:', err.message);
    process.exit(1);
}).connect({
    host: '121.204.253.75',
    port: 32,
    username: 'root',
    privateKey,
    readyTimeout: 15000,
});