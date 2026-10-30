#!/usr/bin/env bash
# ============================================================================
# 门禁 10 Feature 全组合编译（Git Bash 版）— check-all-features.sh
#
# 与 scripts/check-all-features.ps1 同语义：逐包运行 --all-features，
# 但对 sz-orm-queue 排除 kafka/pulsar feature（Windows 无 librdkafka C 库 /
# pulsar 需要 protoc），改用 all-real-no-native，避免已知环境问题。
#
# 用法：
#   bash scripts/check-all-features.sh [--package <pkg>]
#
# 退出码：0 = 全部通过；1 = 存在失败
# ============================================================================

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# 自动定位 cargo（Windows Git Bash 的 cargo 可能不在 PATH）
find_cargo() {
    if command -v cargo >/dev/null 2>&1; then
        echo "cargo"
        return
    fi
    for cand in "$HOME/.cargo/bin/cargo" "$HOME/.cargo/bin/cargo.exe"; do
        [ -f "$cand" ] && echo "$cand" && return
    done
    echo ""
}
CARGO_BIN="$(find_cargo)"
if [ -z "$CARGO_BIN" ]; then
    echo "ERROR: 未找到 cargo，请将 cargo 加入 PATH"
    exit 1
fi

if ! command -v python3 >/dev/null 2>&1 && ! command -v python >/dev/null 2>&1; then
    echo "ERROR: python3/python 未安装（需要解析 Cargo.toml members）"
    exit 1
fi
PY_CMD=$(command -v python3 || command -v python)

# 从 Cargo.toml 提取 workspace members（与 check-fmt-all.sh 同口径）
MEMBERS=$("$PY_CMD" - <<'PY'
import re, pathlib, sys
t = pathlib.Path('Cargo.toml').read_text(encoding='utf-8')
m = re.search(r'members\s*=\s*\[([^\]]*)\]', t)
if not m:
    sys.exit(1)
print('\n'.join(re.findall(r'"([^"]+)"', m.group(1))))
PY
) || { echo "ERROR: 无法解析 Cargo.toml members"; exit 1; }
MEMBERS=$(printf '%s' "$MEMBERS" | tr -d '\r')

# 可选：只检查指定包
TARGET=""
if [ "${1:-}" = "--package" ]; then
    TARGET="$2"
fi

echo "=== 门禁 10 Feature 全组合编译（逐包）==="
FAIL=0
COUNT=0

while IFS= read -r member; do
    [ -z "$member" ] && continue
    manifest="$member/Cargo.toml"
    [ -f "$manifest" ] || continue
    pkg=$(basename "$member")
    if [ -n "$TARGET" ] && [ "$pkg" != "$TARGET" ]; then
        continue
    fi
    COUNT=$((COUNT + 1))
    if [ "$pkg" = "sz-orm-queue" ]; then
        # rdkafka cmake-build 在 Windows 上栈溢出，pulsar 需要 protoc → 用 all-real-no-native
        echo "  [CHECK] $pkg --features all-real-no-native (kafka/pulsar excluded)"
        "$CARGO_BIN" check --manifest-path "$manifest" --all-targets --features all-real-no-native >/tmp/all-features-$pkg.log 2>&1
    else
        echo "  [CHECK] $pkg --all-features"
        "$CARGO_BIN" check --manifest-path "$manifest" --all-targets --all-features >/tmp/all-features-$pkg.log 2>&1
    fi
    if [ $? -ne 0 ]; then
        echo "  [FAIL] $pkg"
        tail -15 /tmp/all-features-$pkg.log
        FAIL=1
    fi
done <<< "$MEMBERS"

echo ""
if [ "$FAIL" -ne 0 ]; then
    echo "=== FAILED（$COUNT 包中存在失败）==="
    exit 1
fi
echo "=== ALL PASSED（$COUNT 包）==="
exit 0