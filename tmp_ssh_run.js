const { Client } = require('ssh2');
const fs = require('fs');

const privateKey = fs.readFileSync(__dirname + '/tmp_ssh_key2', 'utf8');
const cmd = process.argv[2] || 'echo connected && rustc --version && cargo --version && ls /www/rust/ 2>/dev/null || echo no_rust_dir';

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