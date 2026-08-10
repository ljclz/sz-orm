# SZ-ORM Soak 测试使用指南

> 版本：1.0
> 适用：SZ-ORM 库项目长时间稳定性测试
> 服务器：122.51.216.76（Ubuntu 24.04，8 核 15GB）
> 工具部署位置：`/www/rust/soak-toolkit/`
> 项目工作目录：`/www/rust/sz-orm-soak`
> 日期：2026-08-09

---

## 一、项目特点

SZ-ORM 是 Rust ORM 库项目（非 Web 服务），soak 测试方式与 Web 服务不同：

| 对比项 | Web 服务 soak | SZ-ORM 库 soak |
|--------|--------------|----------------|
| 被监控对象 | HTTP 服务进程 | cargo test/build 进程 |
| 端口检查 | ✅ 有监听端口 | ❌ 无端口（库项目） |
| 稳定性指标 | 内存/fd/延迟/ops | 内存/fd/编译时间/测试通过率 |
| 测试方式 | 持续请求服务 | 循环运行测试套件 |

---

## 二、前置条件

1. 服务器已部署通用 Soak 工具到 `/www/rust/soak-toolkit/`
2. SSH 密钥已配置（`deploy_key` 文件）
3. 服务器已安装 Rust 工具链（rustup + cargo）
4. sz-orm 代码已上传到服务器 `/www/rust/sz-orm`

---

## 三、参数清单

| 参数 | 值 | 说明 |
|------|-----|------|
| `--project` | sz-orm | 项目名 |
| `--work-dir` | /www/rust/sz-orm-soak | 工作目录 |
| `--report-dir` | /www/rust/soak-reports | 归档目录 |
| `--soak-ports` | 8501-8505 | 采样端口范围 |
| `--cron-marker` | # sz-orm-soak | cron 标记 |
| `--duration` | 10s（冒烟）/ 1h（稳定） | 测试持续时间 |

---

## 四、执行步骤

### 步骤 1：SSH 连接服务器

```bash
# 服务器信息
# 地址：122.51.216.76
# 用户：ubuntu
# SSH 端口：22
# 认证：SSH 密钥（deploy_key）
```

### 步骤 2：创建工作目录

```bash
mkdir -p /www/rust/sz-orm-soak
mkdir -p /www/rust/soak-reports
```

### 步骤 3：上传 sz-orm 代码

```bash
# 方式 1：git clone（如果有远程仓库）
cd /www/rust && git clone <sz-orm-repo> sz-orm

# 方式 2：rsync 上传（从本地）
rsync -avz --exclude target --exclude node_modules \
  ./ ubuntu@122.51.216.76:/www/rust/sz-orm/
```

### 步骤 4：构建 sz-orm

```bash
cd /www/rust/sz-orm
export RUST_MIN_STACK=67108864
export CARGO_INCREMENTAL=0
cargo build --release --workspace 2>&1 | tee /www/rust/sz-orm-soak/build.log
```

### 步骤 5：运行 Soak 测试

```bash
# 10 秒冒烟测试（验证构建 + 测试可运行）
cd /www/rust/sz-orm
export RUST_MIN_STACK=67108864
export CARGO_INCREMENTAL=0
timeout 10 cargo test --workspace -j 2 --no-fail-fast 2>&1 | \
  tee /www/rust/sz-orm-soak/soak-test-10s.log

# 检查结果
grep -c "test result: ok" /www/rust/sz-orm-soak/soak-test-10s.log
```

### 步骤 6：监控进程稳定性

```bash
# 监控 cargo test 进程的内存和 fd
while true; do
  pid=$(pgrep -f "cargo test" | head -1)
  if [ -z "$pid" ]; then break; fi
  rss=$(ps -o rss= -p $pid 2>/dev/null || echo 0)
  fd=$(ls /proc/$pid/fd 2>/dev/null | wc -l)
  echo "$(date +%H:%M:%S),rss=${rss}kb,fd=${fd}" >> \
    /www/rust/sz-orm-soak/monitor.csv
  sleep 5
done
```

### 步骤 7：收集报告

```bash
# 归档报告
cp /www/rust/sz-orm-soak/build.log /www/rust/soak-reports/
cp /www/rust/sz-orm-soak/soak-test-*.log /www/rust/soak-reports/
cp /www/rust/sz-orm-soak/monitor.csv /www/rust/soak-reports/
```

---

## 五、从本地触发（使用 ssh2）

```bash
# 安装 ssh2 包
npm install ssh2

# 执行 soak 测试
node scripts/soak-sz-orm.js --duration 10s --key-path ./deploy_key
```

`soak-sz-orm.js` 会自动：
1. SSH 连接服务器
2. 创建工作目录
3. 上传 sz-orm 代码
4. 构建 sz-orm（cargo build --release）
5. 运行 soak 测试（cargo test）
6. 监控进程稳定性
7. 收集测试报告
8. 清理临时文件

---

## 六、多项目共存

| 项目 | 端口 | 工作目录 | 采样端口 | cron 标记 |
|------|------|---------|---------|----------|
| sz-rust | 8300 | /www/rust/sz-rust-soak | 8401-8405 | # sz-rust-soak |
| sz-pay | 8301 | /www/rust/sz-pay-soak | 8406-8410 | # sz-pay-soak |
| sz-orm | 无 | /www/rust/sz-orm-soak | 8501-8505 | # sz-orm-soak |

---

## 七、清理

```bash
# 测试完成后清理临时文件
rm -rf /www/rust/sz-orm-soak/*.log
rm -rf /www/rust/sz-orm-soak/monitor.csv

# 确保进程已释放
pkill -f "cargo test.*sz-orm" 2>/dev/null || true
```