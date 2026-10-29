# sz-orm 本机 WSL2 + Docker 容器化 CI 使用指南

> **触发词**：`容器化 CI`、`Linux 容器跑门禁`、`WSL 跑门禁`、`docker 门禁`、`容器内执行门禁脚本`
> 当项目 AI 收到这些指令（或类似措辞）时，按本文档执行。
>
> **一句话**：本机已具备 WSL2 Ubuntu 22.04 + Docker Engine，可在 Linux 容器内挂载仓库运行 sz-orm 的
> 门禁脚本（sh 门禁已验证；cargo 门禁需拉取 rust 镜像）。无需宿主机 Docker Desktop。

## 一、环境现状（2026-10-27 实测就绪）

| 组件 | 状态 |
|------|------|
| WSL2 发行版 | Ubuntu 22.04.5 LTS（`wsl -d Ubuntu` 进入，默认 root；数据目录 `D:\WSL\Ubuntu\ext4.vhdx`，**勿删**） |
| Docker Engine | WSL 内 `docker.io` 29.1.3（systemd 管理，`systemctl start docker` 启动） |
| 镜像加速器 | `/etc/docker/daemon.json` 已配置 `https://docker.m.daocloud.io`（国内可拉镜像，实测 hello-world/alpine 成功） |
| 已验证 | `docker run --rm alpine:3.20` 成功；挂载仓库容器内跑 `scripts/audit-verify.sh` → **63 通过 / 0 失败**；2026-10-28 补跑全部非 cargo 门禁 → **16/16 PASS**（G11 反映工作区未提交改动；G23 需 rust 镜像） |

## 二、标准执行流程（项目 AI 照抄）

### 1. 启动 Docker（若未运行）

```bash
wsl -d Ubuntu -u root -- systemctl start docker
```

### 2. 容器内运行 sh 门禁脚本（无需 rust 镜像）

仓库经 `/mnt/d/www/test/鲜视达/rust/sz-orm` 挂载进 alpine 容器，示例跑 audit-verify：

```bash
wsl -d Ubuntu -u root -- bash -c 'docker run --rm \
  -v "/mnt/d/www/test/鲜视达/rust/sz-orm:/workspace" -w /workspace \
  alpine:3.20 sh -c "apk add --no-cache bash >/dev/null 2>&1; bash scripts/audit-verify.sh docs/assessment/2026-10-26-fullstack-review.md"'
```

- 其他 sh 门禁同理替换最后一条命令，例如：`bash scripts/run-gates-v10120-server.sh`（需仓库位于 `/www/rust/sz-orm-v10120`，改挂载路径或用 git clone）。
- Python 门禁（`scripts/check-metrics-real.py` 等）需在容器内装 python3：`apk add --no-cache bash python3`（或换 `python:3.12-slim` 镜像）。
- 门禁 G14（`scripts/check-doc-sync.py`）与 G31（`scripts/check-ci-blocking.py`）额外依赖 PyYAML，需再装 `py3-yaml`：`apk add --no-cache bash python3 py3-yaml`。
- 门禁 G23（`scripts/check-unused-deps.py`）依赖 `cargo machete`，纯 alpine 容器无法运行，需按第三节用 rust 镜像。
- **国内网络提速（2026-10-28 实测）**：默认 `apk add` 会访问 `dl-cdn.alpinelinux.org`，国内可能极慢（索引拉取数分钟无进展）。可在宿主机建一个只含阿里云源的仓库文件（例如 `_tmp_apk_repos`，内容两行：`https://mirrors.aliyun.com/alpine/v3.20/main` 与 `https://mirrors.aliyun.com/alpine/v3.20/community`），挂载为只读 `/etc/apk/repositories` 后安装立即提速：

  ```bash
  wsl -d Ubuntu -u root -- bash -c 'docker run --rm \
    -v "/mnt/d/www/test/鲜视达/rust/sz-orm/_tmp_apk_repos:/etc/apk/repositories:ro" \
    -v "/mnt/d/www/test/鲜视达/rust/sz-orm:/workspace" -w /workspace \
    alpine:3.20 sh -c "apk add --no-cache bash python3 py3-yaml git >/dev/null 2>&1; bash <门禁脚本相对路径>"'
  ```
  临时文件（`_tmp_apk_repos` 等）用完后请删除。

### 3. 容器内运行 cargo 门禁（需要 rust 镜像，较大约 1GB）

```bash
# 首次拉取 rust 镜像（匹配仓库 rust-toolchain.toml 1.99.1）
wsl -d Ubuntu -u root -- bash -c 'docker pull rust:1.99'

# 挂载仓库跑 clippy / test（示例）
wsl -d Ubuntu -u root -- bash -c 'docker run --rm \
  -v "/mnt/d/www/test/鲜视达/rust/sz-orm:/workspace" -w /workspace \
  rust:1.99 bash -c "cargo clippy -p sz-orm-cabi --all-targets -- -D warnings"'
```

- rust 镜像内自带 bash/cargo/rustc，无需再装；如需 protoc 等额外依赖再 `apt-get install`。

### 4. 维护命令

```bash
wsl -l -v                    # 查看发行版（Ubuntu / docker-desktop）
wsl -d Ubuntu                # 进入 Ubuntu shell
wsl -d Ubuntu -u root -- docker ps   # 查看容器
wsl --shutdown               # 停止全部 WSL（释放内存）
```

## 三、注意事项

1. **不要删除 `D:\WSL\Ubuntu`**——它是 Ubuntu 发行版磁盘镜像（ext4.vhdx 约 2GB）。安装期下载缓存（rootfs tar / MSI / 日志）已清理。
2. **wsl.exe 的控制台消息可能是 UTF-16**，PowerShell 直接显示乱码；但 `wsl -d Ubuntu -u root -- bash -c '...'` 中 Linux 命令的 stdout 通常是 UTF-8，直接读取即可，不要再 pipe 给 `iconv -f UTF-16LE`（会把正常 UTF-8 转成乱码）。需要读取 wsl.exe 自身错误消息时用 Python `subprocess` + `decode('utf-16')`。
3. **Git Bash 调用 PowerShell 时变量 `$x` 会被 bash 展开**——PowerShell 脚本请写成 `.ps1` 文件再执行，避免命令行内嵌变量。
4. 仓库路径含中文（`鲜视达`），WSL 侧是 `/mnt/d/www/test/鲜视达/rust/sz-orm`，容器挂载用该路径。
5. 宿主机另有 Docker Desktop（`D:\Docker`，服务停止、WSL 集成未开），与本 WSL 内 docker.io 独立，不影响本文档流程。
6. 如需在本机跑全部 Linux 门禁（G1-G23），建议直接 `wsl -d Ubuntu` 进入后 `git clone https://github.com/ljclz/sz-orm` 到 Linux 侧再跑 `bash scripts/run-gates-v10120-server.sh`（挂载 `/mnt/d` 走 9P 协议较慢，仅适合少量 sh 门禁）。