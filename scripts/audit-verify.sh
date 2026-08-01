#!/usr/bin/env bash
# audit-verify.sh — 审计证据自动验证脚本（sz-orm 版）
# 用法: bash scripts/audit-verify.sh <审计报告.md>
#
# 验证审计报告中的每项结论是否附带真实的 file:line 证据，
# 并检查该证据在代码中是否实际存在。
#
# 这是门禁 13（审计合规硬约束）的自动执行脚本。

set -euo pipefail

REPORT="${1:?用法: $0 <审计报告.md>}"

if [[ ! -f "$REPORT" ]]; then
  echo "❌ 报告文件不存在: $REPORT"
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

echo "=========================================="
echo "  审计证据验证: $REPORT"
echo "=========================================="

PASS=0
FAIL=0
WARN=0

# 提取所有 file:line 或 file:line-line 形式的引用
# 支持格式: file:///path/to/file.rs#L123  或  src/xxx.rs:123  或  packages/xxx/src/yyy.rs:123
while IFS= read -r line; do
  # 跳过注释和空行
  [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue

  # 匹配 file:///path#L123 或 file:///path#L123-L456
  if [[ "$line" =~ file://([^[:space:]#]+)#L([0-9]+)(-L([0-9]+))? ]]; then
    filepath="${BASH_REMATCH[1]}"
    lineno="${BASH_REMATCH[2]}"
    endline="${BASH_REMATCH[4]:-$lineno}"

    # 将 file:// 路径转为本地路径
    local_path="${filepath#/}"
    # Windows: file:///E:/... → E:/...
    if [[ "$local_path" =~ ^[A-Za-z]:/ ]]; then
      local_path="$local_path"
    fi

    if [[ -f "$local_path" ]]; then
      total_lines=$(wc -l < "$local_path")
      if (( lineno <= total_lines )); then
        echo "✅ $local_path:$lineno (共 $total_lines 行)"
        PASS=$((PASS + 1))
      else
        echo "❌ $local_path:$lineno — 行号超出范围（文件共 $total_lines 行）"
        FAIL=$((FAIL + 1))
      fi
    else
      echo "❌ $local_path:$lineno — 文件不存在"
      FAIL=$((FAIL + 1))
    fi

  # 匹配 src/xxx.rs:123 形式（相对路径）
  elif [[ "$line" =~ packages/([a-zA-Z0-9_-]+/[^:[:space:]]+\.rs):([0-9]+) ]]; then
    rel_path="packages/${BASH_REMATCH[1]}"
    lineno="${BASH_REMATCH[2]}"

    if [[ -f "$rel_path" ]]; then
      total_lines=$(wc -l < "$rel_path")
      if (( lineno <= total_lines )); then
        echo "✅ $rel_path:$lineno (共 $total_lines 行)"
        PASS=$((PASS + 1))
      else
        echo "❌ $rel_path:$lineno — 行号超出范围（文件共 $total_lines 行）"
        FAIL=$((FAIL + 1))
      fi
    else
      echo "❌ $rel_path:$lineno — 文件不存在"
      FAIL=$((FAIL + 1))
    fi

  # 匹配 src/xxx.rs:123 形式（简写）
  elif [[ "$line" =~ src/([a-zA-Z0-9_/-]+\.rs):([0-9]+) ]]; then
    # 尝试在 sz-orm-core 中查找
    rel_path="packages/sz-orm-core/src/${BASH_REMATCH[1]}"
    lineno="${BASH_REMATCH[2]}"

    if [[ -f "$rel_path" ]]; then
      total_lines=$(wc -l < "$rel_path")
      if (( lineno <= total_lines )); then
        echo "✅ $rel_path:$lineno (共 $total_lines 行)"
        PASS=$((PASS + 1))
      else
        echo "❌ $rel_path:$lineno — 行号超出范围（文件共 $total_lines 行）"
        FAIL=$((FAIL + 1))
      fi
    else
      echo "⚠️  src/${BASH_REMATCH[1]}:$lineno — 未找到对应文件（跳过）"
      WARN=$((WARN + 1))
    fi
  fi
done < "$REPORT"

echo ""
echo "=========================================="
echo "  验证结果"
echo "=========================================="
echo "  ✅ 通过: $PASS"
echo "  ❌ 失败: $FAIL"
echo "  ⚠️  警告: $WARN"
echo "=========================================="

if (( FAIL > 0 )); then
  echo "❌ 审计证据验证未通过 — 存在 $FAIL 处无效引用"
  exit 1
elif (( PASS == 0 )); then
  echo "⚠️  未找到任何 file:line 证据 — 报告可能未遵循审计规范"
  exit 1
else
  echo "✅ 审计证据验证通过"
  exit 0
fi
