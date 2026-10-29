#!/usr/bin/env bash
# ============================================================================
# 门禁 1 格式检查（Git Bash / Linux 跨平台版）— check-fmt-all.sh
#
# 背景：`cargo fmt --all -- --check` 在 70+ 包 workspace 下会触发 Windows
#       CreateProcessW 命令行 32767 字符上限（os error 206），必须逐包执行。
#       本脚本从 workspace members 读取清单，逐包运行 cargo fmt --check。
#
# 用法：
#   bash scripts/check-fmt-all.sh [--fix]
#     --fix  实际格式化（默认仅检查）
#
# 退出码：0 = 全部通过；1 = 存在格式漂移
# ============================================================================

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FIX=0
[ "${1:-}" = "--fix" ] && FIX=1

# 自动定位 cargo（Windows Git Bash 的 cargo 可能不在 PATH）
find_cargo() {
    if command -v cargo >/dev/null 2>&1; then
        echo "cargo"
        return
    fi
    for cand in "$HOME/.cargo/bin/cargo" "$HOME/.cargo/bin/cargo.exe" "/c/Users/Administrator/.cargo/bin/cargo"; do
        [ -f "$cand" ] && echo "$cand" && return
    done
    echo ""
}
CARGO_BIN="$(find_cargo)"
if [ -z "$CARGO_BIN" ]; then
    echo "ERROR: 未找到 cargo，请将 cargo 加入 PATH 或设置 CARGO_HOME"
    exit 1
fi

# 2026-10-28 P3-1 修复：Windows 的 python3 可能是 Microsoft Store 安装桩
# （command -v 命中但无法执行），必须实际试跑验证
PY_CMD=""
for cand in python3 python; do
    if command -v "$cand" >/dev/null 2>&1 && "$cand" -c "print(1)" >/dev/null 2>&1; then
        PY_CMD="$cand"
        break
    fi
done
if [[ -z "$PY_CMD" ]]; then
    echo "ERROR: python3/python 不可执行（需要解析 Cargo.toml members）"
    exit 1
fi
MEMBERS=$("$PY_CMD" - <<'PY'
import re, pathlib, sys
t = pathlib.Path('Cargo.toml').read_text(encoding='utf-8')
m = re.search(r'members\s*=\s*\[([^\]]*)\]', t)
if not m:
    sys.exit(1)
print('\n'.join(re.findall(r'"([^"]+)"', m.group(1))))
PY
) || { echo "ERROR: 无法解析 Cargo.toml members"; exit 1; }
# Windows Python 的 stdout 会输出 CRLF，去掉 \r 避免路径带回车符
MEMBERS=$(printf '%s' "$MEMBERS" | tr -d '\r')

echo "=== 门禁 1 格式检查（逐包，${FIX:-0}=检查模式）==="
FAIL=0
COUNT=0

while IFS= read -r member; do
    manifest="$member/Cargo.toml"
    [ -f "$manifest" ] || continue
    COUNT=$((COUNT + 1))
    if [ "$FIX" -eq 1 ]; then
        "$CARGO_BIN" fmt --manifest-path "$manifest" >/dev/null 2>&1
    else
        "$CARGO_BIN" fmt --manifest-path "$manifest" -- --check >/dev/null 2>&1
    fi
    if [ $? -ne 0 ]; then
        echo "  FMT_FAIL: $member"
        FAIL=1
    fi
done <<< "$MEMBERS"

if [ "$FAIL" -ne 0 ]; then
    echo "=== 格式检查未通过（$COUNT 包中有格式漂移）==="
    echo "  修复: bash scripts/check-fmt-all.sh --fix"
    exit 1
fi
echo "=== 格式检查通过（$COUNT 包）==="
exit 0