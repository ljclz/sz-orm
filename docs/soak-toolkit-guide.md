# 通用 Soak 自托管工具使用指南

> 版本：2.0
> 适用：任何需要长时间稳定性测试的 Web 项目
> 服务器：122.51.216.76（Ubuntu 24.04，8 核 15GB）
> 工具部署位置：`/www/rust/soak-toolkit/`

---

## 一、前置条件

1. 服务器已部署通用 Soak 工具到 `/www/rust/soak-toolkit/`
2. 你的项目已在服务器上运行，监听某个端口（如 8300）
3. SSH 密钥已配置（`deploy_key` 文件）

---

## 二、30 秒快速开始

### 方式 1：命令行参数（推荐）

```bash
# 在本地执行（需 Node.js + ssh2 包）
node soak-trigger.js \
  --duration 10s \
  --project my-project \
  --port 9000 \
  --work-dir /www/rust/my-project-soak \
  --report-dir /www/rust/soak-reports \
  --soak-ports 9001-9005 \
  --protected-process my-process-name \
  --restart-script /www/rust/my-project-soak/restart.sh \
  --cron-marker "# my-project-soak" \
  --key-path ./deploy_key
```

### 方式 2：服务器直接执行

```bash
# SSH 登录服务器后执行
bash /www/rust/soak-toolkit/soak-runner.sh \
  --duration 10s \
  --trigger manual \
  --project my-project \
  --protected-port 9000 \
  --work-dir /www/rust/my-project-soak \
  --report-dir /www/rust/soak-reports \
  --soak-ports 9001-9005 \
  --protected-process my-process-name \
  --restart-script /www/rust/my-project-soak/restart.sh \
  --cron-marker "# my-project-soak"
```

---

## 三、参数清单

| 参数 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `--duration` | 是 | 10s | 测试持续时间（支持 s/m/h，如 10s、5m、6h） |
| `--trigger` | 否 | manual | 触发方式（manual/cron/nightly） |
| `--project` | 是 | sz-rust | 项目名（小写字母/数字/连字符） |
| `--port` / `--protected-port` | 是 | 8300 | 被保护进程的监听端口 |
| `--protected-process` | 否 | sz-rust-sz300 | 被保护进程名（用于日志提示） |
| `--work-dir` | 是 | /www/rust/sz-rust-soak | 工作目录（存放临时文件） |
| `--report-dir` | 否 | /www/rust/soak-reports | 归档目录（所有项目共享） |
| `--soak-ports` | 否 | 8401-8405 | 采样端口范围（避免与被保护端口冲突） |
| `--restart-script` | 否 | - | 被保护进程死亡时的重启脚本 |
| `--cron-marker` | 否 | # sz-rust-soak | cron 条目标记（用于区分不同项目） |
| `--key-path` | 否 | ../../deploy_key | SSH 密钥路径（仅 soak-trigger.js） |

---

## 四、完整接入步骤

### 步骤 1：创建工作目录和重启脚本

```bash
# SSH 登录服务器
ssh root@122.51.216.76

# 创建工作目录
mkdir -p /www/rust/my-project-soak

# 创建重启脚本（根据你的项目修改）
cat > /www/rust/my-project-soak/restart.sh << 'EOF'
#!/bin/bash
# 重启 my-project 服务
cd /www/wwwroot/my-project
# 例如：systemctl restart my-project
# 或：pm2 restart my-project
# 或：bash start.sh
echo "my-project 已重启"
EOF
chmod +x /www/rust/my-project-soak/restart.sh
```

### 步骤 2：执行 Soak 测试

```bash
# 10 秒冒烟测试
bash /www/rust/soak-toolkit/soak-runner.sh \
  --duration 10s \
  --project my-project \
  --protected-port 9000 \
  --work-dir /www/rust/my-project-soak \
  --protected-process my-process-name \
  --restart-script /www/rust/my-project-soak/restart.sh \
  --cron-marker "# my-project-soak"

# 1 小时稳定性测试
bash /www/rust/soak-toolkit/soak-runner.sh \
  --duration 1h \
  --trigger manual \
  --project my-project \
  --protected-port 9000 \
  --work-dir /www/rust/my-project-soak \
  --protected-process my-process-name \
  --restart-script /www/rust/my-project-soak/restart.sh \
  --cron-marker "# my-project-soak"

# 6 小时长时间 Soak 测试
bash /www/rust/soak-toolkit/soak-runner.sh \
  --duration 6h \
  --trigger manual \
  --project my-project \
  --protected-port 9000 \
  --work-dir /www/rust/my-project-soak \
  --protected-process my-process-name \
  --restart-script /www/rust/my-project-soak/restart.sh \
  --cron-marker "# my-project-soak"
```

### 步骤 3：配置 cron 自动调度

```bash
# 配置 cron（每周日 00:00 UTC + 每日 18:00 UTC 自动执行 6h soak）
bash /www/rust/soak-toolkit/soak-cron-setup.sh setup \
  --cron-marker "# my-project-soak" \
  --soak-runner /www/rust/soak-toolkit/soak-runner.sh \
  --work-dir /www/rust/my-project-soak

# 验证 cron 配置
crontab -l | grep "my-project-soak"

# 移除 cron 配置
bash /www/rust/soak-toolkit/soak-cron-setup.sh remove \
  --cron-marker "# my-project-soak"
```

### 步骤 4：查看归档报告

```bash
# 查看所有归档记录
cat /www/rust/soak-reports/index.csv

# 查询特定项目的记录
bash /www/rust/soak-toolkit/soak-archive.sh --query "my-project"

# 查看某次 soak 的详细报告
cat /www/rust/soak-reports/2026-08-09/soak-report-*.csv

# 清理 30 天前的归档
bash /www/rust/soak-toolkit/soak-archive.sh --cleanup-expired --retention-days 30
```

### 步骤 5：验证 6h soak 结果

```bash
bash /www/rust/soak-toolkit/verify-6h-soak.sh \
  --report-dir /www/rust/soak-reports \
  --protected-port 9000 \
  --soak-ports 9001-9005 \
  --cron-marker "# my-project-soak"
```

---

## 五、从本地触发（无需 SSH 登录服务器）

### 前置条件

```bash
# 安装 ssh2 包
npm install ssh2
```

### 执行

```bash
# 将 soak-trigger.js 复制到你的项目
cp /path/to/sz-rust/scripts/soak-self-hosted/soak-trigger.js ./soak-trigger.js

# 执行 10s 冒烟测试
node soak-trigger.js \
  --duration 10s \
  --project my-project \
  --port 9000 \
  --work-dir /www/rust/my-project-soak \
  --soak-ports 9001-9005 \
  --protected-process my-process-name \
  --restart-script /www/rust/my-project-soak/restart.sh \
  --cron-marker "# my-project-soak" \
  --key-path ./deploy_key
```

`soak-trigger.js` 会自动：
1. 上传最新脚本到 `/www/rust/soak-toolkit/`
2. 在服务器执行 soak 测试
3. 验证归档报告
4. 验证被保护进程存活

---

## 六、多项目共存

多个项目可以使用同一份工具，通过参数区分：

| 项目 | 端口 | 工作目录 | 采样端口 | cron 标记 |
|------|------|---------|---------|----------|
| sz-rust | 8300 | /www/rust/sz-rust-soak | 8401-8405 | # sz-rust-soak |
| sz-pay | 8301 | /www/rust/sz-pay-soak | 8406-8410 | # sz-pay-soak |
| my-project | 9000 | /www/rust/my-project-soak | 9001-9005 | # my-project-soak |

所有项目的归档报告共享 `/www/rust/soak-reports/` 目录，通过 `run_id` 和 `trigger` 区分。

---

## 七、输出文件说明

### 归档目录结构

```
/www/rust/soak-reports/
├── index.csv                          # 索引文件（所有记录）
└── 2026-08-09/                        # 按日期分目录
    ├── soak-report-20260809-161838-manual.csv  # 单次 soak 报告
    └── soak-report-20260809-181312-manual.csv
```

### index.csv 列结构

```
run_id,trigger,trigger_time,set_duration,actual_duration,status,report_path,rust_version,retention_days
```

### soak-report-*.csv 列结构

```
timestamp,rss_mb,fd_count,ops_per_sec,p99_latency_ms,error_count
```

---

## 八、常见问题

### Q: 被保护进程不在运行怎么办？

A: Soak 工具会输出警告 `⚠️ 端口 XXXX 无进程监听`，但不会中断测试。测试结束后会检查被保护进程是否存活，如果死亡会执行重启脚本。

### Q: 如何修改采样间隔？

A: 默认 60 秒采样一次。修改 `soak-runner.sh` 中的 `SAMPLE_INTERVAL=60`。

### Q: 如何查看 6h soak 是否在运行？

A: `screen -ls | grep soak-6h` 检查 screen 会话，或 `tail -f /www/rust/my-project-soak/6h-soak-manual.log` 查看日志。

### Q: 端口冲突怎么办？

A: 确保不同项目使用不同的 `--soak-ports` 范围，避免与被保护端口冲突。

---

## 九、工具文件清单

| 文件 | 功能 | 部署位置 |
|------|------|---------|
| `config-defaults.sh` | 默认值定义 | /www/rust/soak-toolkit/ |
| `process-guard.sh` | 进程隔离保护 | /www/rust/soak-toolkit/ |
| `soak-archive.sh` | 报告归档 | /www/rust/soak-toolkit/ |
| `soak-runner.sh` | Soak 执行 | /www/rust/soak-toolkit/ |
| `soak-cron-setup.sh` | cron 配置 | /www/rust/soak-toolkit/ |
| `verify-6h-soak.sh` | 6h 验证 | /www/rust/soak-toolkit/ |
| `soak-trigger.js` | 本地触发器 | 你的项目本地 |