#!/usr/bin/env bash
# audit-verify.sh — 审计证据自动验证脚本（sz-orm 版）
# 用法: bash scripts/audit-verify.sh <审计报告.md> [--strict]
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
#
# v3（2026-10-28 架构评测 P2-4，V3-2/PERFV2-17 修复）相对 v2 的强化：
# 1. **子目录根级文件**：`packages/*/Cargo.toml:NN` 不再被误解析为根 Cargo.toml
#    （根级文件名规则支持可选路径前缀）；
# 2. **crate 前缀映射**：`sz-orm-*/src/xxx.rs:NN` 映射到对应包（不再一律映射 core）；
#    `cli/src/xxx.rs:NN` 映射到 cli；
# 3. **代码块跳过**：``` 围栏内的行（内嵌日志/示例）不再被二次校验——修复
#    "附录内嵌日志二次解析误报"（PERFV2-17）；
# 4. **弱语义校验**：被引行与引用行无共享标识符 token → 警告（--strict 下红牌）。
#    纯路径引用（如引用 Cargo.toml:6 看版本号）允许零共享，属已知边界。
# 已知边界：本脚本仍不能证明"该行内容**支撑**该结论"（语义层验证需人工），
# 但保证引用的文件存在、行号在物理行范围内、行非空，且暴露无共享 token 的可疑引用。

set -euo pipefail

STRICT=0
REPORT=""
for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=1 ;;
    *) REPORT="$arg" ;;
  esac
done

if [[ -z "$REPORT" ]]; then
  echo "用法: $0 <审计报告.md> [--strict]"
  exit 2
fi

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

# 提取标识符 token（≥4 字符，小写化），排除通用停用词
ident_tokens() {
  printf '%s' "$1" | grep -oE '[A-Za-z_][A-Za-z0-9_]{3,}' \
    | tr '[:upper:]' '[:lower:]' \
    | grep -vE '^(file|http|https|path|line|lines|docs|tests|test|src|packages|toml|lock|cargo|self|this|that|with|from|com|www|github|rust|true|false|none|null|readme|agents|changelog|deny|toolchain|report|assessment)$' \
    | sort -u || true
}

# verify_file <path> <start> <end> <citing line>
verify_file() {
  local path="$1" lineno="$2" endline="$3" citing="${4:-}"
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

  # v3 弱语义校验：被引行与引用行有共享标识符才视为"可信引用"
  # （纯路径引用如 `Cargo.toml:6` 看版本号，允许零共享——仅警告，--strict 才红牌）
  if [[ -n "$citing" ]]; then
    local citing_tokens cited_tokens shared
    citing_tokens=$(ident_tokens "$citing")
    cited_tokens=$(ident_tokens "$content")
    if [[ -n "$citing_tokens" && -n "$cited_tokens" ]]; then
      shared=$(comm -12 <(printf '%s\n' "$citing_tokens") <(printf '%s\n' "$cited_tokens") || true)
      if [[ -z "${shared//[[:space:]]/}" ]]; then
        echo "⚠️  弱语义校验：$path:$lineno — 被引行与结论无共享标识符（若为纯路径引用可忽略）"
        WARN=$((WARN + 1))
        if (( STRICT )); then
          echo "❌ $path:$lineno — --strict 模式：语义校验失败（无共享 token）"
          FAIL=$((FAIL + 1))
          return
        fi
      fi
    fi
  fi

  echo "✅ $path:$lineno (共 $total 物理行)"
  PASS=$((PASS + 1))
}

# 逐行扫描；对每行循环提取"最早出现的"引用并消费，直到该行无引用。
# v3：``` 围栏内（内嵌日志/示例代码块）跳过。
# 含反引号的正则必须放在单引号变量中（内联在 [[ =~ ]] 会触发命令替换）
re_fence='^[[:space:]]*```'
re_crate='(^|[^A-Za-z0-9_.-])((sz-orm-[a-z0-9-]+|cli|examples)/src/([a-zA-Z0-9_/-]+\.rs)):([0-9]+)'
re_root='(^|[^A-Za-z0-9_.-])(([^:[:space:]`]*\/)?(deny\.toml|Cargo\.toml|Cargo\.lock|AGENTS\.md|rust-toolchain\.toml|CHANGELOG\.md)):([0-9]+)'

in_fence=0
while IFS= read -r line || [[ -n "$line" ]]; do
  # 代码块围栏切换（行首 ```，允许缩进与语言标注）
  if [[ "$line" =~ $re_fence ]]; then
    in_fence=$((1 - in_fence))
    continue
  fi
  if (( in_fence )); then
    continue
  fi

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
      verify_file "$local_path" "$lineno" "$endline" "$line"
      line="${line#*"$m"}"

    elif [[ "$line" =~ packages/([a-zA-Z0-9_-]+/[^:[:space:]]+\.rs):([0-9]+) ]]; then
      m="${BASH_REMATCH[0]}"
      verify_file "packages/${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[2]}" "$line"
      line="${line#*"$m"}"

    elif [[ "$line" =~ $re_crate ]]; then
      m="${BASH_REMATCH[0]}"
      crate_path="${BASH_REMATCH[2]}"
      # sz-orm-* 包的真实位置在 packages/ 下；cli/examples 在仓库根
      if [[ "${BASH_REMATCH[3]}" == sz-orm-* ]]; then
        crate_path="packages/${crate_path}"
      fi
      verify_file "$crate_path" "${BASH_REMATCH[5]}" "${BASH_REMATCH[5]}" "$line"
      line="${line#*"$m"}"

    elif [[ "$line" =~ $re_root ]]; then
      m="${BASH_REMATCH[0]}"
      verify_file "${BASH_REMATCH[2]}" "${BASH_REMATCH[5]}" "${BASH_REMATCH[5]}" "$line"
      line="${line#*"$m"}"

    elif [[ "$line" =~ src/([a-zA-Z0-9_/-]+\.rs):([0-9]+) ]]; then
      # 简写映射到 sz-orm-core
      rel_path="packages/sz-orm-core/src/${BASH_REMATCH[1]}"
      lineno="${BASH_REMATCH[2]}"
      m="${BASH_REMATCH[0]}"
      if [[ -f "$rel_path" ]]; then
        verify_file "$rel_path" "$lineno" "$lineno" "$line"
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
