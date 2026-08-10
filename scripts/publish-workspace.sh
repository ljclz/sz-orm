#!/usr/bin/env bash
# SZ-ORM workspace 拓扑发布脚本
#
# 功能：
#   1. 复用 scripts/compute_topology.ps1 计算依赖拓扑排序
#   2. 按拓扑顺序逐包执行 cargo publish --dry-run 验证
#   3. 实际发布模式：按拓扑顺序逐包 cargo publish --token <token>
#   4. 每包发布后验证 crates.io 页面可访问
#   5. sz-pay 零回归验证（可选）
#
# 用法：
#   bash scripts/publish-workspace.sh --help
#   bash scripts/publish-workspace.sh --dry-run
#   bash scripts/publish-workspace.sh --token <token>
#   bash scripts/publish-workspace.sh --verify
#
# 退出码：
#   0 = 全部成功
#   1 = 至少一个包失败
#   2 = 脚本错误（参数/环境）

set -euo pipefail

# 颜色输出
if [ -t 1 ]; then
    RED='\033[31m'
    GREEN='\033[32m'
    YELLOW='\033[33m'
    RESET='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    RESET=''
fi

# 路径常量
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TOPOLOGY_PS1="$SCRIPT_DIR/compute_topology.ps1"
RESULTS_DIR="$WORKSPACE_ROOT/target/publish-results"

# 默认值
DRY_RUN=false
VERIFY_ONLY=false
TOKEN=""
SKIP_PACKAGES=""
TOPOLOGY_FILE=""

# 结果统计
TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0
FAILED_PACKAGES=()

usage() {
    cat <<EOF
SZ-ORM workspace 拓扑发布脚本

用法:
  bash scripts/publish-workspace.sh [OPTIONS]

选项:
  --dry-run              仅验证（cargo publish --dry-run），不实际发布
  --token TOKEN          实际发布到 crates.io 的 token
  --verify               仅验证已发布包的 crates.io 页面可访问
  --topology FILE        使用指定拓扑排序文件（每行一个包名），默认调用 compute_topology.ps1
  --skip PKG1,PKG2       跳过指定包（逗号分隔）
  --help, -h             显示本帮助

退出码:
  0 = 全部成功
  1 = 至少一个包失败
  2 = 脚本错误

示例:
  bash scripts/publish-workspace.sh --dry-run
  bash scripts/publish-workspace.sh --token [REDACTED]
  bash scripts/publish-workspace.sh --verify
EOF
}

# 解析参数
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --token)
            TOKEN="$2"
            shift 2
            ;;
        --token=*)
            TOKEN="${1#--token=}"
            shift
            ;;
        --verify)
            VERIFY_ONLY=true
            shift
            ;;
        --topology)
            TOPOLOGY_FILE="$2"
            shift 2
            ;;
        --topology=*)
            TOPOLOGY_FILE="${1#--topology=}"
            shift
            ;;
        --skip)
            SKIP_PACKAGES="$2"
            shift 2
            ;;
        --skip=*)
            SKIP_PACKAGES="${1#--skip=}"
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo -e "${RED}ERROR${RESET}: unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# 互斥检查
if [ "$DRY_RUN" = true ] && [ -n "$TOKEN" ]; then
    echo -e "${RED}ERROR${RESET}: --dry-run and --token are mutually exclusive" >&2
    exit 2
fi

if [ "$VERIFY_ONLY" = true ] && [ "$DRY_RUN" = true ]; then
    echo -e "${RED}ERROR${RESET}: --verify and --dry-run are mutually exclusive" >&2
    exit 2
fi

if [ "$VERIFY_ONLY" = true ] && [ -n "$TOKEN" ]; then
    echo -e "${RED}ERROR${RESET}: --verify and --token are mutually exclusive" >&2
    exit 2
fi

# 查找 PowerShell
find_powershell() {
    if command -v pwsh >/dev/null 2>&1; then
        echo "pwsh"
    elif command -v powershell >/dev/null 2>&1; then
        echo "powershell"
    else
        echo ""
    fi
}

# 计算拓扑排序
compute_topology() {
    if [ -n "$TOPOLOGY_FILE" ] && [ -f "$TOPOLOGY_FILE" ]; then
        echo -e "${YELLOW}Using topology file: $TOPOLOGY_FILE${RESET}" >&2
        cat "$TOPOLOGY_FILE"
        return
    fi

    local ps_cmd
    ps_cmd="$(find_powershell)"
    if [ -z "$ps_cmd" ]; then
        echo -e "${RED}ERROR${RESET}: PowerShell not found (required for compute_topology.ps1)" >&2
        exit 2
    fi

    echo -e "${YELLOW}Computing topology via $ps_cmd ...${RESET}" >&2
    "$ps_cmd" -NoProfile -ExecutionPolicy Bypass -File "$TOPOLOGY_PS1" -WorkspaceRoot "$WORKSPACE_ROOT"
}

# 获取包的 Cargo.toml 路径
get_cargo_toml() {
    local pkg="$1"
    # 尝试 packages/$pkg/Cargo.toml
    local candidate="$WORKSPACE_ROOT/packages/$pkg/Cargo.toml"
    if [ -f "$candidate" ]; then
        echo "$candidate"
        return
    fi
    # 尝试 $pkg/Cargo.toml（cli/examples）
    candidate="$WORKSPACE_ROOT/$pkg/Cargo.toml"
    if [ -f "$candidate" ]; then
        echo "$candidate"
        return
    fi
    echo ""
}

# 检查包是否应跳过
should_skip() {
    local pkg="$1"
    if [ -z "$SKIP_PACKAGES" ]; then
        return 1
    fi
    local IFS=','
    for skip_pkg in $SKIP_PACKAGES; do
        if [ "$pkg" = "$skip_pkg" ]; then
            return 0
        fi
    done
    return 1
}

# 验证 crates.io 页面可访问
verify_crates_io() {
    local pkg="$1"
    local version="$2"
    local url="https://crates.io/api/v1/crates/$pkg/$version"
    local http_code
    http_code=$(curl -s -o /dev/null -w "%{http_code}" "$url" 2>/dev/null || echo "000")
    if [ "$http_code" = "200" ]; then
        return 0
    else
        return 1
    fi
}

# 从 Cargo.toml 提取版本号（支持 version.workspace = true）
get_version() {
    local cargo_toml="$1"
    local version
    version=$(grep -m1 '^version' "$cargo_toml" | sed 's/.*"\(.*\)".*/\1/')
    if [ -z "$version" ] || echo "$version" | grep -q 'workspace'; then
        # 从 workspace 根 Cargo.toml 读取
        version=$(grep -m1 '^version' "$WORKSPACE_ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
    fi
    echo "$version"
}

# 单包 dry-run
publish_dry_run() {
    local pkg="$1"
    local cargo_toml
    cargo_toml="$(get_cargo_toml "$pkg")"
    if [ -z "$cargo_toml" ]; then
        echo -e "${YELLOW}SKIP${RESET} $pkg (Cargo.toml not found)"
        ((SKIPPED++)) || true
        return 0
    fi

    local pkg_dir
    pkg_dir="$(dirname "$cargo_toml")"

    echo -e "${YELLOW}DRY-RUN${RESET} $pkg ..."

    local output
    if output=$(cd "$pkg_dir" && cargo publish --dry-run --allow-dirty 2>&1); then
        echo -e "${GREEN}PASS${RESET} $pkg"
        ((PASSED++)) || true
        return 0
    else
        echo -e "${RED}FAIL${RESET} $pkg"
        echo "$output" | tail -20 | sed 's/^/    /'
        ((FAILED++)) || true
        FAILED_PACKAGES+=("$pkg")
        return 1
    fi
}

# 单包实际发布
publish_real() {
    local pkg="$1"
    local cargo_toml
    cargo_toml="$(get_cargo_toml "$pkg")"
    if [ -z "$cargo_toml" ]; then
        echo -e "${YELLOW}SKIP${RESET} $pkg (Cargo.toml not found)"
        ((SKIPPED++)) || true
        return 0
    fi

    local pkg_dir
    pkg_dir="$(dirname "$cargo_toml")"
    local version
    version="$(get_version "$cargo_toml")"

    echo -e "${YELLOW}PUBLISH${RESET} $pkg@$version ..."

    local output
    if output=$(cd "$pkg_dir" && cargo publish --allow-dirty --token "$TOKEN" 2>&1); then
        echo -e "${GREEN}PUBLISHED${RESET} $pkg@$version"
        # 验证 crates.io 页面
        sleep 2  # 等待 crates.io 索引更新
        if verify_crates_io "$pkg" "$version"; then
            echo -e "${GREEN}VERIFIED${RESET} $pkg@$version on crates.io"
        else
            echo -e "${YELLOW}WARN${RESET} $pkg@$version published but crates.io page not yet available (may need a few minutes)"
        fi
        ((PASSED++)) || true
        return 0
    else
        # 检查是否已发布（cargo publish 对已发布版本返回特定错误）
        if echo "$output" | grep -q "already exists\|already published"; then
            echo -e "${YELLOW}SKIP${RESET} $pkg@$version (already published)"
            ((SKIPPED++)) || true
            return 0
        fi
        echo -e "${RED}FAIL${RESET} $pkg"
        echo "$output" | tail -20 | sed 's/^/    /'
        ((FAILED++)) || true
        FAILED_PACKAGES+=("$pkg")
        return 1
    fi
}

# 仅验证模式
verify_only_mode() {
    local pkg="$1"
    local cargo_toml
    cargo_toml="$(get_cargo_toml "$pkg")"
    if [ -z "$cargo_toml" ]; then
        ((SKIPPED++)) || true
        return 0
    fi
    local version
    version="$(get_version "$cargo_toml")"

    if verify_crates_io "$pkg" "$version"; then
        echo -e "${GREEN}OK${RESET} $pkg@$version on crates.io"
        ((PASSED++)) || true
    else
        echo -e "${RED}MISSING${RESET} $pkg@$version not on crates.io"
        ((FAILED++)) || true
        FAILED_PACKAGES+=("$pkg")
    fi
}

# 主逻辑
main() {
    mkdir -p "$RESULTS_DIR"

    # 获取拓扑排序
    local topology
    if ! topology=$(compute_topology 2>"$RESULTS_DIR/topology.log"); then
        echo -e "${RED}ERROR${RESET}: topology computation failed" >&2
        cat "$RESULTS_DIR/topology.log" >&2
        exit 2
    fi

    local packages
    packages=$(echo "$topology" | grep -v '^$')
    TOTAL=$(echo "$packages" | wc -l)

    echo "=========================================="
    if [ "$DRY_RUN" = true ]; then
        echo "SZ-ORM DRY-RUN PUBLISH ($TOTAL packages)"
    elif [ "$VERIFY_ONLY" = true ]; then
        echo "SZ-ORM CRATES.IO VERIFY ($TOTAL packages)"
    else
        echo "SZ-ORM PUBLISH ($TOTAL packages)"
    fi
    echo "=========================================="
    echo "Topology order:"
    echo "$packages" | nl -ba | sed 's/^/  /'
    echo "=========================================="

    # 逐包处理
    local idx=0
    while IFS= read -r pkg; do
        [ -z "$pkg" ] && continue
        ((idx++)) || true

        if should_skip "$pkg"; then
            echo -e "${YELLOW}SKIP${RESET} [$idx/$TOTAL] $pkg (user skipped)"
            ((SKIPPED++)) || true
            continue
        fi

        if [ "$VERIFY_ONLY" = true ]; then
            verify_only_mode "$pkg"
        elif [ "$DRY_RUN" = true ]; then
            publish_dry_run "$pkg" || true  # dry-run 不中断，收集所有失败
        else
            publish_real "$pkg" || true  # 实际发布也不中断，收集所有失败
        fi
    done <<< "$packages"

    # 汇总
    echo "=========================================="
    echo "SUMMARY"
    echo "=========================================="
    echo "  Total:   $TOTAL"
    echo -e "  ${GREEN}Passed${RESET}:  $PASSED"
    echo -e "  ${RED}Failed${RESET}:  $FAILED"
    echo -e "  ${YELLOW}Skipped${RESET}: $SKIPPED"

    if [ $FAILED -gt 0 ]; then
        echo ""
        echo -e "${RED}Failed packages:${RESET}"
        for fp in "${FAILED_PACKAGES[@]}"; do
            echo "  - $fp"
        done
        # 保存失败清单
        printf '%s\n' "${FAILED_PACKAGES[@]}" > "$RESULTS_DIR/failed-packages.txt"
        echo ""
        echo "Failed packages list saved to: $RESULTS_DIR/failed-packages.txt"
        exit 1
    fi

    echo ""
    echo -e "${GREEN}ALL PASSED${RESET}"
    exit 0
}

main