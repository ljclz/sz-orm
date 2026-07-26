# Self-Hosted Runner 配置指南

本仓库的 24 小时 Soak Test（`.github/workflows/soak-self-hosted.yml`）必须运行在 self-hosted runner 上，因为 GitHub Actions 托管 runner 有 6 小时 job 时间限制，无法完成 24h 测试。

本文档说明 self-hosted runner 的注册、环境要求、标签配置、监控与故障恢复。

---

## 一、环境要求

| 项目 | 最低要求 | 推荐配置 | 说明 |
| --- | --- | --- | --- |
| 操作系统 | Ubuntu 20.04+ / Windows Server 2019+ | Ubuntu 22.04 LTS | Linux 推荐（soak.rs 的 RSS/fd 监控依赖 `/proc`） |
| Rust | stable 1.75+ | stable 最新 | 由 `dtolnay/rust-toolchain@stable` 自动安装 |
| CPU | 2 核 | 4 核 | cargo build + soak 并发 |
| 内存 | 4 GB | 8 GB | 24h 运行 + cargo 编译 |
| 磁盘 | 20 GB 可用 | 50 GB SSD | cargo 缓存 + target 产物 + 日志 |
| 网络 | 可访问 github.com | 稳定带宽 | runner 需轮询 GitHub API |
| Git | 2.40+ | 最新 | self-hosted runner 必备 |
| Bash | 4.0+ | 5.0+ | workflow 脚本依赖 bash（Windows 用 Git Bash） |

> ⚠️ **Linux 优先**：soak 测试的 `SoakMonitor` 通过 `/proc/self/status` 和 `/proc/self/fd` 采集 RSS、fd_count、thread_count。Windows 平台这些指标返回占位值（0），无法有效检测内存/句柄泄漏。**生产 24h soak test 必须在 Linux runner 上运行。**

---

## 二、注册 Self-Hosted Runner

### 2.1 获取注册 Token

1. 打开仓库主页 → **Settings** → **Actions** → **Runners** → **New self-hosted runner**
2. 选择操作系统（Linux / Windows / macOS）
3. 页面会显示 `REGISTRATION_TOKEN`（有效期约 1 小时）

### 2.2 Linux 注册步骤

```bash
# 1. 创建 runner 专用目录
mkdir -p ~/actions-runner && cd ~/actions-runner

# 2. 下载最新 runner（以 x64 为例，URL 从 GitHub 页面复制）
curl -o actions-runner-linux-x64-2.317.0.tar.gz -L \
  https://github.com/actions/runner/releases/download/v2.317.0/actions-runner-linux-x64-2.317.0.tar.gz

# 3. 解压
tar xzf actions-runner-linux-x64-2.317.0.tar.gz

# 4. 注册（替换 TOKEN 为步骤 2.1 获取的 token，URL 为仓库地址）
./config.sh --url https://github.com/<ORG>/<REPO> \
  --token <REGISTRATION_TOKEN> \
  --name sz-orm-soak-runner \
  --labels self-hosted,linux,soak \
  --unattended

# 5. 安装为系统服务（推荐，开机自启）
sudo ./svc.sh install
sudo ./svc.sh start

# 6. 验证状态
sudo ./svc.sh status
```

### 2.3 Windows 注册步骤

```powershell
# 1. 创建 runner 目录
mkdir C:\actions-runner ; cd C:\actions-runner

# 2. 下载（URL 从 GitHub 页面复制）
Invoke-WebRequest -Uri https://github.com/actions/runner/releases/download/v2.317.0/actions-runner-win-x64-2.317.0.zip -OutFile actions-runner-win-x64-2.317.0.zip

# 3. 解压
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory("C:\actions-runner\actions-runner-win-x64-2.317.0.zip", "C:\actions-runner")

# 4. 注册（在 PowerShell 中执行，替换 TOKEN 和 URL）
.\config.cmd --url https://github.com/<ORG>/<REPO> `
  --token <REGISTRATION_TOKEN> `
  --name sz-orm-soak-runner-win `
  --labels self-hosted,windows,soak `
  --unattended

# 5. 安装为 Windows 服务（需管理员权限）
.\svc.cmd install
.\svc.cmd start

# 6. 验证状态
Get-Service -Name "actions.runner.*"
```

> ℹ️ Windows runner 仅用于功能冒烟，**不用于生产 24h soak test**（见上文 Linux 优先说明）。

---

## 三、Runner 标签配置

本仓库 workflow 使用 `runs-on: self-hosted`，因此 runner 必须带 `self-hosted` 标签。注册时已通过 `--labels self-hosted,linux,soak` 配置。

### 标签说明

| 标签 | 用途 |
| --- | --- |
| `self-hosted` | 所有自托管 runner 的默认标签，workflow `runs-on` 匹配项 |
| `linux` | Linux 平台标识，便于按平台调度 |
| `soak` | 专用于 soak test 的 runner 标识（可用于未来精细化调度） |

### 追加/修改标签

注册后可在 GitHub UI 修改：**Settings** → **Actions** → **Runners** → 选中 runner → **Labels** → 编辑。

或通过命令行：
```bash
cd ~/actions-runner
./config.sh --labels self-hosted,linux,soak --unattended --url https://github.com/<ORG>/<REPO> --token <TOKEN>
```

> ⚠️ 标签变更需重启 runner 服务：`sudo ./svc.sh stop && sudo ./svc.sh start`（Linux）或 `.\svc.cmd stop && .\svc.cmd start`（Windows）。

---

## 四、监控与日志查看

### 4.1 Runner 服务状态

**Linux (systemd)：**
```bash
# 状态
sudo systemctl status actions.runner.<ORG>-<REPO>.<RUNNER_NAME>.service

# 实时日志
sudo journalctl -u actions.runner.<ORG>-<REPO>.<RUNNER_NAME>.service -f

# 最近 200 行
sudo journalctl -u actions.runner.<ORG>-<REPO>.<RUNNER_NAME>.service -n 200
```

**Windows (服务)：**
```powershell
Get-Service -Name "actions.runner.*"
Get-EventLog -LogName Application -Source "actions.runner.*" -Newest 100
```

### 4.2 Runner 工作目录日志

Runner 自身日志位于 `_diag/` 目录：
```bash
cd ~/actions-runner/_diag
ls -lt *.log | head        # 最新日志文件
tail -f Runner_*.log       # 实时跟踪
```

### 4.3 Job 运行日志

- **GitHub UI**：仓库 → **Actions** → 选择对应 run → 查看 step 日志
- **本地工作目录**：`~/actions-runner/_work/<REPO>/<REPO>/`（每次 job 的 checkout 与产物所在）
- **Soak CSV 报告**：job 结束后通过 artifact `soak-results-24h` 下载，或本地路径 `~/actions-runner/_work/<REPO>/<REPO>/target/soak-report.csv`

### 4.4 运行时资源监控

soak test 运行期间建议另开终端监控：
```bash
# 进程资源（替换 PID）
top -p $(pgrep -f "soak")

# 内存与 fd
watch -n 60 'cat /proc/$(pgrep -f soak)/status | grep -E "VmRSS|Threads" ; ls /proc/$(pgrep -f soak)/fd | wc -l'
```

### 4.5 GitHub API 查询 runner 状态

```bash
# 列出仓库 runner（需 GH_TOKEN）
gh api repos/<ORG>/<REPO>/actions/runners --jq '.runners[] | {name, status, busy, labels: [.labels[].name]}'
```

---

## 五、故障恢复指南

### 5.1 Runner 离线

**现象**：GitHub UI 显示 runner 为 "Offline"。

**排查步骤：**
1. 检查服务：`sudo systemctl status actions.runner.*`（Linux）/ `Get-Service actions.runner.*`（Windows）
2. 若服务停止：`sudo ./svc.sh start` / `.\svc.cmd start`
3. 检查网络：`curl -I https://github.com` 是否可达
4. 查看日志：`sudo journalctl -u actions.runner.* -n 100`，常见错误：
   - `Token expired`：重新执行 `config.sh` 注册
   - `Connection refused`：检查代理/防火墙
5. 重启服务后，runner 自动恢复 "Idle" 状态

### 5.2 Soak Job 卡住

**现象**：job 运行超过 25 小时仍未结束。

**处理：**
1. GitHub UI → 该 run → **Cancel workflow**
2. 若 cancel 无效，登录 runner 主机：`pkill -f "cargo test.*soak"`（Linux）/ `Stop-Process -Name "cargo*" -Force`（Windows）
3. 检查 runner 工作目录是否残留进程：`ps aux | grep soak`
4. 重启 runner 服务：`sudo ./svc.sh stop && sudo ./svc.sh start`
5. 查看 `target/soak-report.csv` 是否已生成，分析卡住时的最后几个采样点

### 5.3 Soak Test 失败（退化检测触发）

**现象**：job 状态为 failure，日志显示 "Soak test 检测到退化"。

**处理流程：**
1. 下载 artifact `soak-results-24h`，打开 `soak-report.csv`
2. 对照退化标准定位问题：
   | 指标 | 阈值 | 含义 |
   | --- | --- | --- |
   | RSS 增长 | > 50 MB | 内存泄漏 |
   | fd_count 增长 | > 10 | 句柄泄漏 |
   | pool_active ≠ pool_idle | 终态 | 连接池泄漏 |
   | ops_per_sec 衰减 | > 10% | 性能退化 |
   | p99_latency 增长 | > 2x | 慢退化 |
   | error_count | > 0 | 偶发错误 |
3. 根据 CSV 中的 `elapsed_secs` 列定位退化发生的时间窗口
4. 结合 commit 历史（`${GITHUB_SHA}`）定位引入问题的变更
5. 修复后重新触发 workflow_dispatch 验证

### 5.4 Runner 磁盘满

**现象**：`No space left on device`。

**处理：**
```bash
# 清理 cargo 缓存中的旧版本
cargo cache --autoclean

# 清理 runner 历史 _work 目录（保留最新一次）
cd ~/actions-runner/_work/<REPO>/<REPO>
ls -dt */ | tail -n +2 | xargs rm -rf

# 清理旧 artifact（GitHub UI: Settings → Actions → 旧 run → Delete artifacts）
```

### 5.5 重装 Runner

若配置损坏无法恢复：
```bash
# 1. 停止并卸载服务
sudo ./svc.sh stop
sudo ./svc.sh uninstall

# 2. 删除 runner 目录
cd ~ && rm -rf actions-runner

# 3. 在 GitHub UI 移除旧 runner：Settings → Actions → Runners → 选中 → Remove

# 4. 重新按「二、注册 Self-Hosted Runner」流程注册
```

---

## 六、安全注意事项

1. **Runner 隔离**：self-hosted runner 不应在多租户环境运行，仅用于可信仓库
2. **Token 保管**：`REGISTRATION_TOKEN` 有效期短，不要提交到代码库
3. **服务账户**：runner 应以非 root 用户运行（默认 `runner` 用户）
4. **网络限制**：生产环境建议 runner 仅出站访问 `github.com` 与 `*.githubusercontent.com`
5. **依赖审计**：定期更新 runner 二进制（GitHub 会发布安全补丁）

---

## 七、相关文件

- `.github/workflows/soak-self-hosted.yml`：24h soak test workflow（本文档配套）
- `.github/workflows/soak.yml`：GitHub 托管 runner 的短时 soak（6h 限制）
- `packages/sz-orm-core/tests/soak.rs`：soak test 入口
- `packages/sz-orm-core/tests/common/soak.rs`：监控与退化检测实现
- `docs/adr/`：架构决策记录
