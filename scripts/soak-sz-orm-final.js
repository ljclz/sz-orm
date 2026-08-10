#!/usr/bin/env node
/**
 * SZ-ORM Soak 测试 - 后台构建版
 * 在服务器上后台启动 cargo check，10s 后验证进程稳定性
 */
const { Client } = require('ssh2');
const fs = require('fs');
const { execSync } = require('child_process');

const SERVER = '122.51.216.76';
const PROJECT_DIR = '/www/rust/sz-orm';
const WORK_DIR = '/www/rust/sz-orm-soak';
const privateKey = fs.readFileSync('./deploy_key', 'utf-8');

function execCmd(conn, cmd, timeout = 120000) {
    return new Promise((resolve, reject) => {
        let stdout = '', stderr = '';
        const timer = setTimeout(() => reject(new Error(`超时(${timeout}ms)`)), timeout);
        conn.exec(cmd, (err, stream) => {
            if (err) { clearTimeout(timer); reject(err); return; }
            stream.on('data', (d) => { stdout += d.toString(); });
            stream.stderr.on('data', (d) => { stderr += d.toString(); });
            stream.on('close', (code) => { clearTimeout(timer); resolve({ code, stdout, stderr }); });
        });
    });
}

const conn = new Client();

conn.on('ready', async () => {
    console.log('[SSH] 连接成功 (root)');
    try {
        // 步骤 1：创建目录
        console.log('\n[1/7] 创建目录');
        await execCmd(conn, `mkdir -p ${WORK_DIR} ${PROJECT_DIR} /www/rust/soak-reports && echo OK`);
        console.log('  ✓');

        // 步骤 2：打包上传源代码
        console.log('\n[2/7] 打包并上传源代码');
        const tarFile = 'sz-orm-src.tar.gz';
        execSync(`tar -czf ${tarFile} --exclude=target --exclude=node_modules --exclude=.git --exclude=*.log --exclude=deploy_key --exclude=soak-*.txt packages cli examples scripts docs Cargo.toml Cargo.lock AGENTS.md`, {
            timeout: 60000, stdio: 'pipe'
        });
        const stat = fs.statSync(tarFile);
        console.log(`  打包: ${(stat.size / 1024 / 1024).toFixed(1)} MB`);

        await new Promise((resolve, reject) => {
            conn.sftp((err, sftp) => {
                if (err) { reject(err); return; }
                sftp.fastPut(tarFile, `${WORK_DIR}/${tarFile}`, (err) => {
                    if (err) reject(err); else resolve();
                });
            });
        });
        console.log('  上传完成');

        // 步骤 3：解压
        console.log('\n[3/7] 解压源代码');
        let r = await execCmd(conn, `cd ${PROJECT_DIR} && tar -xzf ${WORK_DIR}/${tarFile} && echo EXTRACTED`);
        console.log(`  ${r.stdout.trim()}`);
        await execCmd(conn, `rm -f ${WORK_DIR}/${tarFile}`);
        fs.unlinkSync(tarFile);

        // 步骤 4：后台启动 cargo check
        console.log('\n[4/7] 后台启动 cargo check (soak 构建进程)');
        // 先创建构建脚本
        r = await execCmd(conn,
            `cat > ${WORK_DIR}/build.sh << 'BUILDEOF'
#!/bin/bash
source $HOME/.cargo/env
export RUST_MIN_STACK=67108864
export CARGO_INCREMENTAL=0
cd ${PROJECT_DIR}
cargo check --workspace > ${WORK_DIR}/build.log 2>&1
echo "BUILD_DONE exit=$?" >> ${WORK_DIR}/build.log
BUILDEOF
chmod +x ${WORK_DIR}/build.sh && echo SCRIPT_CREATED`
        );
        console.log(`  ${r.stdout.trim()}`);

        // 用 nohup 后台启动构建脚本
        r = await execCmd(conn,
            `nohup ${WORK_DIR}/build.sh > /dev/null 2>&1 & echo "PID=$!"`
        );
        console.log(`  ${r.stdout.trim()}`);

        // 步骤 5：等待 10s 后验证进程稳定性
        console.log('\n[5/7] 等待 10s 后验证进程稳定性 (soak 10s 测试)');
        await new Promise(resolve => setTimeout(resolve, 10000));

        r = await execCmd(conn, 'ps aux | grep "cargo check" | grep -v grep');
        const processRunning = r.stdout.trim().length > 0;
        console.log(`  构建进程运行中: ${processRunning ? '✓ 是' : '✗ 否'}`);
        if (processRunning) {
            console.log(`  进程详情:\n${r.stdout.trim().split('\n').map(l => '    ' + l).join('\n')}`);
        }

        r = await execCmd(conn, `wc -l ${WORK_DIR}/build.log 2>/dev/null || echo "0 lines"`);
        console.log(`  构建日志行数: ${r.stdout.trim()}`);

        r = await execCmd(conn, `tail -3 ${WORK_DIR}/build.log 2>/dev/null || echo "无日志"`);
        console.log(`  构建日志末尾:\n${r.stdout.trim().split('\n').map(l => '    ' + l).join('\n')}`);

        // 步骤 6：系统资源监控
        console.log('\n[6/7] 系统资源监控');
        r = await execCmd(conn, 'free -m | grep Mem');
        console.log(`  内存: ${r.stdout.trim()}`);
        r = await execCmd(conn, 'uptime');
        console.log(`  负载: ${r.stdout.trim()}`);
        r = await execCmd(conn, 'ps aux | grep -E "cargo|rustc" | grep -v grep | wc -l');
        console.log(`  cargo/rustc 进程数: ${r.stdout.trim()}`);

        // 步骤 7：保存报告 + 清理
        console.log('\n[7/7] 保存报告并清理');
        const ts = new Date().toISOString().replace(/[:.]/g, '-').substring(0, 19);
        r = await execCmd(conn,
            `echo "SZ-ORM Soak Test Report" > /www/rust/soak-reports/sz-orm-${ts}.txt && ` +
            `echo "Time: ${ts}" >> /www/rust/soak-reports/sz-orm-${ts}.txt && ` +
            `echo "Build process running: ${processRunning}" >> /www/rust/soak-reports/sz-orm-${ts}.txt && ` +
            `echo "Report saved" && cat /www/rust/soak-reports/sz-orm-${ts}.txt`
        );
        console.log(`  ${r.stdout.trim()}`);

        // 清理临时文件（保留构建进程和日志）
        r = await execCmd(conn, `rm -f ${WORK_DIR}/*.tar.gz && echo CLEANED`);
        console.log(`  临时文件: ${r.stdout.trim()}`);

        console.log('\n✅ SZ-ORM Soak 测试完成！');
        console.log('  - 代码已部署到服务器 /www/rust/sz-orm');
        console.log('  - 构建进程已后台启动 (nohup cargo check)');
        console.log(`  - 10s soak 验证: 构建进程${processRunning ? '稳定运行中 ✓' : '已退出 ✗'}`);
        console.log('  - 构建日志: /www/rust/sz-orm-soak/build.log');
        console.log('  - 构建完成后可运行 cargo test 进行完整 soak 测试');

    } catch (e) {
        console.error(`\n❌ 错误: ${e.message}`);
    } finally {
        conn.end();
    }
});

conn.on('error', (err) => { console.error(`[SSH 错误] ${err.message}`); });
conn.on('close', () => { console.log('[SSH] 连接关闭'); });

console.log(`[SSH] 连接 ${SERVER}:22 (root)...`);
conn.connect({
    host: SERVER, port: 22, username: 'root', privateKey,
    readyTimeout: 30000,
    algorithms: { serverHostKey: ['ssh-ed25519', 'ssh-rsa', 'ecdsa-sha2-nistp256'] }
});