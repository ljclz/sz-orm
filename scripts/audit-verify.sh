#!/usr/bin/env bash
# audit-verify.sh — 审计证据自动验证脚本（sz-orm 版）
# 用法: bash scripts/audit-verify.sh <审计报告.md>
#
# 验证审计报告中的每项结论是否附带真实的 file:line 证据，
# 并检查该证据在代码中是否实际存在。
#
# 这是门禁 13（审计合规硬约束）的自动执行脚本。
#
# v2（2026-10-27 整改 P1-2）相对 v1 的强化：
# 1. 一行内的**全部**引用逐个校验（v1 只校验每行第一个匹配）；
# 2. 物理行计数（wc -l + 末行无换行补偿），与编辑器行号一致；
# 3. **被引行非空检查**：行号落在空行上判定失败（v1 从不读取被引行）；
# 4. file:/// Unix 绝对路径不再被错误剥掉首斜杠（v1 会把 /home/x 变成 home/x）。
# 已知边界：本脚本仍不能证明"该行内容支撑该结论"（语义层验证需人工），
# 但保证引用的文件存在、行号在物理行范围内、且该行不是空行。

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

# 物理行数（wc -l 数换行符；末行无换行时补 1）
physical_lines() {
  local total
  total=$(wc -l < "$1")
  if [[ -n "$(tail -c 1 "$1")" ]]; then
    total=$((total + 1))
  fi
  echo "$total"
}

# verify_file <path> <start> <end>
verify_file() {
  local path="$1" lineno="$2" endline="$3"
  if [[ ! -f "$path" ]]; then
    echo "❌ $path:$lineno — 文件不存在"
    FAIL=$((FAIL + 1))
    return
  fi
  local total
  total=$(physical_lines "$path")
  if (( lineno > total )); then
    echo "❌ $path:$lineno — 行号超出范围（文件共 $total 物理行）"
    FAIL=$((FAIL + 1))
    return
  fi
  if (( endline > total )); then
    echo "❌ $path:$lineno-L$endline — 区间终点超出范围（文件共 $total 物理行）"
    FAIL=$((FAIL + 1))
    return
  fi
  if (( endline < lineno )); then
    echo "❌ $path:$lineno-L$endline — 区间终点小于起点"
    FAIL=$((FAIL + 1))
    return
  fi
  # 被引行（区间起点）必须是非空行
  local content
  content=$(sed -n "${lineno}p" "$path")
  if [[ -z "${content//[[:space:]]/}" ]]; then
    echo "❌ $path:$lineno — 被引行为空行（无法承载证据）"
    FAIL=$((FAIL + 1))
    return
  fi
  echo "✅ $path:$lineno (共 $total 物理行)"
  PASS=$((PASS + 1))
}

# 逐行扫描；对每行循环提取"最早出现的"引用并消费，直到该行无引用。
while IFS= read -r line || [[ -n "$line" ]]; do
  [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue

  while [[ -n "$line" ]]; do
    if [[ "$line" =~ file://([^[:space:]#]+)#L([0-9]+)(-L([0-9]+))? ]]; then
      local_path="${BASH_REMATCH[1]}"
      lineno="${BASH_REMATCH[2]}"
      endline="${BASH_REMATCH[4]:-$lineno}"
      # Windows: file:///E:/... → E:/...；Unix 绝对路径保留原样
      if [[ "$local_path" =~ ^/([A-Za-z]:/.*)$ ]]; then
        local_path="${BASH_REMATCH[1]}"
      fi
      m="${BASH_REMATCH[0]}"
      verify_file "$local_path" "$lineno" "$endline"
      line="${line#*"$m"}"

    elif [[ "$line" =~ packages/([a-zA-Z0-9_-]+/[^:[:space:]]+\.rs):([0-9]+) ]]; then
      m="${BASH_REMATCH[0]}"
      verify_file "packages/${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[2]}"
      line="${line#*"$m"}"

    elif [[ "$line" =~ (^|[^A-Za-z0-9_.-])(deny\.toml|Cargo\.toml|Cargo\.lock|AGENTS\.md|rust-toolchain\.toml|CHANGELOG\.md):([0-9]+) ]]; then
      m="${BASH_REMATCH[0]}"
      verify_file "${BASH_REMATCH[2]}" "${BASH_REMATCH[3]}" "${BASH_REMATCH[3]}"
      line="${line#*"$m"}"

    elif [[ "$line" =~ src/([a-zA-Z0-9_/-]+\.rs):([0-9]+) ]]; then
      # 简写映射到 sz-orm-core
      rel_path="packages/sz-orm-core/src/${BASH_REMATCH[1]}"
      lineno="${BASH_REMATCH[2]}"
      m="${BASH_REMATCH[0]}"
      if [[ -f "$rel_path" ]]; then
        verify_file "$rel_path" "$lineno" "$lineno"
      else
        echo "⚠️  src/${BASH_REMATCH[1]}:$lineno — 未找到对应文件（跳过）"
        WARN=$((WARN + 1))
      fi
      line="${line#*"$m"}"

    else
      break
    fi
  done
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
