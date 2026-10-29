# 本机 WSL2 + Docker 容器化 CI 通用指南（跨项目）

> **触发词**：`容器化 CI` / `Linux 容器跑门禁` / `WSL 跑门禁` / `docker 门禁` / `容器内执行门禁脚本`
>
> **通用用法**：在任何项目的 AI 会话中说出触发词，并**给出仓库在 Windows 上的绝对路径**（例如
> `D:\www\test\鲜视达\rust\sz-orm`），AI 按本文档执行。
>
> **一句话**：本机已具备 WSL2 Ubuntu 22.04 + Docker Engine（无需 Docker Desktop），可在 Linux
> 容器内挂载任意 Windows 仓库目录运行 sh / cargo 门禁脚本。

## 一、环境现状（2026-10-27 实测就绪）

| 组件 | 状态 |
|------|------|
| WSL2 发行版 | Ubuntu 22.04.5 LTS（`wsl -d Ubuntu` 进入，默认 root；数据目录 `D:\WSL\Ubuntu\ext4.vhdx`，**勿删**） |
| Docker Engine | WSL 内 `docker.io` 29.1.3（systemd 管理，`systemctl start docker` 启动） |
| 镜像加速器 | `/etc/docker/daemon.json` 已配置 `https://docker.m.daocloud.io`（国内可拉镜像，实测 hello-world/alpine 成功） |
| 已验证 | alpine 容器挂载仓库运行 sh 门禁（audit-verify.sh 63 通过 / 0 失败） |

## 二、标准执行流程（AI 照抄，`<REPO_PATH>` 替换为仓库 Windows 绝对路径）

### 1. 启动 Docker（若未运行）

```bash
wsl -d Ubuntu -u root -- systemctl start docker
```

### 2. 容器内运行 sh 门禁脚本（无需 rust 镜像）

把 `<REPO_PATH>` 转为 WSL 挂载路径：`/mnt/<盘符小写>/<路径>`（例如 `D:\www\test\foo` →
`/mnt/d/www/test/foo`）。

```bash
wsl -d Ubuntu -u root -- bash -c 'docker run --rm \
  -v "/mnt/<盘符>/<路径>:/workspace" -w /workspace \
  alpine:3.20 sh -c "apk add --no-cache bash >/dev/null 2>&1; bash <门禁脚本相对路径>"'
```

示例（跑 audit-verify）：`bash scripts/audit-verify.sh docs/assessment/xxx.md`
Python 门禁：`apk add --no-cache bash python3`（或换 `python:3.12-slim` 镜像）；脚本依赖 PyYAML 时再装 `py3-yaml`（如 sz-orm 的 G14/G31）。

**国内网络提速（2026-10-28 实测）**：默认 `apk add` 访问 `dl-cdn.alpinelinux.org` 国内可能极慢（索引拉取数分钟无进展）。可在宿主机建一个只含阿里云源的仓库文件（如 `_tmp_apk_repos`，两行：`https://mirrors.aliyun.com/alpine/v3.20/main` 与 `https://mirrors.aliyun.com/alpine/v3.20/community`），挂载为只读 `/etc/apk/repositories` 后安装立即提速：

```bash
wsl -d Ubuntu -u root -- bash -c 'docker run --rm \
  -v "/mnt/<盘符>/<路径>/_tmp_apk_repos:/etc/apk/repositories:ro" \
  -v "/mnt/<盘符>/<路径>:/workspace" -w /workspace \
  alpine:3.20 sh -c "apk add --no-cache bash python3 py3-yaml git >/dev/null 2>&1; bash <门禁脚本相对路径>"'
```

临时文件（`_tmp_apk_repos` 等）用完后请删除。

### 3. 容器内运行 cargo 门禁（需要 rust 镜像，约 1GB，按仓库工具链版本拉取）

```bash
wsl -d Ubuntu -u root -- bash -c 'docker pull rust:1.99'   # 版本按 rust-toolchain.toml

wsl -d Ubuntu -u root -- bash -c 'docker run --rm \
  -v "/mnt/<盘符>/<路径>:/workspace" -w /workspace \
  rust:1.99 bash -c "cargo clippy --workspace --all-targets -- -D warnings"'
```

### 4. 维护命令

```bash
wsl -l -v                  # 查看发行版
wsl -d Ubuntu              # 进入 Ubuntu shell
wsl -d Ubuntu -u root -- docker ps   # 查看容器
wsl --shutdown             # 停止全部 WSL（释放内存）
```

## 三、注意事项

1. **不要删除 `D:\WSL\Ubuntu`**——它是 Ubuntu 发行版磁盘镜像（ext4.vhdx 约 2GB）。
2. **wsl.exe 的控制台消息可能是 UTF-16**，PowerShell 直接显示乱码；但 `wsl -d Ubuntu -u root -- bash -c '...'` 中 Linux 命令的 stdout 通常是 UTF-8，直接读取即可，不要再 pipe 给 `iconv -f UTF-16LE`（会把正常 UTF-8 转成乱码）。需要读取 wsl.exe 自身错误消息时用 Python `subprocess` + `decode('utf-16')`。
3. **Git Bash 调用 PowerShell 时 `$x` 会被 bash 展开**——PowerShell 脚本写成 `.ps1` 文件再执行。
4. Windows 路径含中文/空格时，WSL 侧路径 `/mnt/d/...` 原样可用，容器挂载加引号。
5. 宿主机另有 Docker Desktop（`D:\Docker`，服务停止），与本 WSL 内 docker.io 独立，不冲突。
6. 大仓库/大量编译建议直接 `wsl -d Ubuntu` 进入后 `git clone` 到 Linux 侧再跑（`/mnt/d` 走 9P 协议较慢，适合少量 sh 门禁；cargo 全量编译在 Linux 侧本地盘更快）。