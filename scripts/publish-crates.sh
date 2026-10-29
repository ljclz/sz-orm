#!/usr/bin/env bash
# publish-crates.sh — 全工作空间发布到 crates.io（按依赖拓扑排序）
# ================================================================
# 2026-10-25 产品评测修复（M6）：crates.io 版本长期滞后于仓库
# （sz-orm-core 1.0.0 vs 当前 10.12.0），导致用户按 README 装到旧版。
#
# 功能：
#   1. 解析 packages/* 与 cli 的 path 依赖，生成拓扑排序（依赖优先发布）
#   2. 逐个 cargo publish，失败自动重试一次
#   3. 发布完成后运行 G18 --fix 同步 README 版本引用
#
# 用法：
#   bash scripts/publish-crates.sh            # 全量发布
#   bash scripts/publish-crates.sh --dry-run  # 仅打印发布顺序，不实际发布
#
# 前置条件：
#   - 已执行 cargo login（crates.io 凭证）
#   - 需要发布的包版本已更新到目标版本（workspace.package.version）
#
# 注意：发布是不可逆的对外操作，执行前确认版本号与依赖链就绪。
# ================================================================

set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DRY_RUN=0
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=1
fi

# 检测 Python 解释器（2026-10-28 P3-1 修复：Windows 的 python3 可能是
# Microsoft Store 安装桩——command -v 命中但无法执行，必须实际试跑验证）
PY=""
for cand in python3 python; do
  if command -v "$cand" >/dev/null 2>&1 && "$cand" -c "print(1)" >/dev/null 2>&1; then
    PY="$cand"
    break
  fi
done
if [[ -z "$PY" ]]; then
  echo "❌ 未找到可执行的 python3/python，无法解析依赖拓扑。"
  exit 1
fi

# ---------- 1. 拓扑排序（依赖优先） ----------
echo "==> 解析 workspace 包依赖拓扑..."
ORDER="$("$PY" - <<'PY'
import os, re

ROOT = "."
pkg_dirs = []
for d in sorted(os.listdir(os.path.join(ROOT, "packages"))):
    if d.startswith("sz-orm") and os.path.isfile(os.path.join(ROOT, "packages", d, "Cargo.toml")):
        pkg_dirs.append(os.path.join(ROOT, "packages", d))
for extra in ("cli",):
    if os.path.isfile(os.path.join(ROOT, extra, "Cargo.toml")):
        pkg_dirs.append(os.path.join(ROOT, extra))

def pkg_name(d):
    txt = open(os.path.join(d, "Cargo.toml"), encoding="utf-8", errors="replace").read()
    m = re.search(r'^name\s*=\s*"([^"]+)"', txt, re.M)
    return m.group(1) if m else None

name2dir = {}
for d in pkg_dirs:
    n = pkg_name(d)
    if n:
        name2dir[n] = d

pkgs = {}
for d in pkg_dirs:
    n = pkg_name(d)
    if not n:
        continue
    txt = open(os.path.join(d, "Cargo.toml"), encoding="utf-8", errors="replace").read()
    deps = set()
    for p in re.findall(r'path\s*=\s*"([^"]+)"', txt):
        cand = os.path.normpath(os.path.join(d, p))
        for other, od in name2dir.items():
            if other != n and os.path.normpath(od) == cand:
                deps.add(other)
    pkgs[n] = deps

visited, order = set(), []
def visit(n):
    if n in visited:
        return
    visited.add(n)
    for dep in sorted(pkgs.get(n, ())):
        if dep in pkgs:
            visit(dep)
    order.append(n)

for n in sorted(pkgs):
    visit(n)
print("\n".join(order))
PY
)"

echo "发布顺序（依赖优先）："
echo "$ORDER" | sed 's/^/  - /'

if [[ "$DRY_RUN" == "1" ]]; then
  echo "==> --dry-run 模式，不实际发布。"
  exit 0
fi

# ---------- 2. 逐个发布（失败重试一次） ----------
LOG_DIR="$ROOT/logs"
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/publish-$(date +%Y%m%d-%H%M%S).log"
echo "==> 发布日志：$LOG"

for pkg in $ORDER; do
  echo "==> cargo publish -p $pkg"
  if cargo publish -p "$pkg" --allow-dirty 2>>"$LOG"; then
    echo "    ✅ $pkg 发布成功"
  else
    echo "    ⚠️ $pkg 首次失败，重试一次..."
    if cargo publish -p "$pkg" --allow-dirty 2>>"$LOG"; then
      echo "    ✅ $pkg 重试成功"
    else
      echo "    ❌ $pkg 发布失败（见 $LOG），停止后续发布。"
      exit 1
    fi
  fi
done

# ---------- 3. 发布后同步 README 版本引用 ----------
echo "==> 运行 G18 --fix 同步 README 数字声称..."
python scripts/check-metrics-real.py --fix

echo "==> 全部发布完成。"