#!/usr/bin/env bash
# audit-verify.sh — 审计证据自动验证脚本（sz-orm 版）
# 用法: bash scripts/audit-verify.sh <审计报告.md> [--strict] [--at-head]
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
#
# v4（2026-10-29 §11.6 P1-4 后半）对齐 audit-verify.ps1 v4 的四项能力——
# 在此之前服务器侧门禁 13（本脚本）与本地侧（.ps1）是**两套口径**：.ps1 校验
# 2589 处引用、本脚本只校验 2038 处，且本脚本读不懂符号级断言。
# 1. **符号级断言**：引用后可跟 `[must-contain: X]` / `[must-not-contain: X]`，
#    被引区间必须真的包含/不包含该**字面子串**（逗号不是分隔符；多条需重复标记）。
#    标记只绑定紧邻其前的那一条引用，避免一行内多条引用互相误伤。
# 2. **--at-head**：被引文件按 `git show HEAD:<path>` 的**提交内容**取行，
#    规避"并行会话在工作区加行 → 提交后行号漂移"造成的假阳性。报告本身仍读工作区。
# 3. **通配覆盖**：新增任意扩展名的 `path.ext:NN` 规则（rs/py/md/yml/toml/cs/sh/…），
#    此前 `scripts/*.py:NN`、`docs/*.md:NN`、`.github/*.yml:NN`、`tests/*.py:NN`
#    等形式**完全不进入校验**。
# 4. **CJK token**：共享标识符判定纳入中日韩词块（≥2 字）。只取 ASCII 会让纯中文
#    断言得到空集合，从而整体跳过语义校验。
# 已知边界：本脚本仍不能证明"该行内容**支撑**该结论"（语义层验证需人工），
# 但保证引用的文件存在、行号在物理行范围内、行非空、声明的断言成立，
# 并暴露无共享 token 的可疑引用。

set -euo pipefail

STRICT=0
HEAD_MODE=0
REPORT=""
for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=1 ;;
    --at-head|--athead) HEAD_MODE=1 ;;
    *) REPORT="$arg" ;;
  esac
done

if [[ -z "$REPORT" ]]; then
  echo "用法: $0 <审计报告.md> [--strict] [--at-head]"
  exit 2
fi

if [[ ! -f "$REPORT" ]]; then
  echo "❌ 报告文件不存在: $REPORT"
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# --at-head 需要 git；不可用时 fail-closed（不静默退回工作区口径）
if (( HEAD_MODE )) && ! command -v git >/dev/null 2>&1; then
  echo "❌ --at-head 需要 git，当前环境未找到"
  exit 2
fi

# CJK 词块提取：优先 grep -P，老 grep / busybox 退回 C locale 字节规则
if printf '测' | grep -oP '[\x{4e00}-\x{9fff}]' >/dev/null 2>&1; then
  re_cjk() { grep -oP '[\x{4e00}-\x{9fff}]{2,}'; }
else
  re_cjk() { LC_ALL=C grep -oE '[^ -~]{6,}'; }
fi

echo "=========================================="
echo "  审计证据验证: $REPORT"
if (( HEAD_MODE )); then
  echo "  内容口径: HEAD 提交（--at-head）"
else
  echo "  内容口径: 工作区"
fi
echo "=========================================="

PASS=0
FAIL=0
WARN=0

declare -A RESOLVED=()
TMPD=""
# 注意：trap 里必须用 if，不能用 `[[ ... ]] && rm`——条件为假时 trap 的退出码
# 会覆盖脚本自身的 exit 0，导致所有"校验通过"的运行返回 1。
cleanup() {
  if [[ -n "$TMPD" ]]; then
    rm -rf "$TMPD"
  fi
}
trap cleanup EXIT
AV_IDX=0

# resolve_file <cited path> → 打印一个可直接 sed 的本地可读路径；不存在则返回 1
resolve_file() {
  local p="$1"
  if [[ -n "${RESOLVED[$p]+set}" ]]; then
    [[ -n "${RESOLVED[$p]}" ]] && printf '%s' "${RESOLVED[$p]}" && return 0
    return 1
  fi
  local out=""
  if (( HEAD_MODE )) && [[ "$p" != /* ]]; then
    [[ -z "$TMPD" ]] && TMPD="$(mktemp -d)"
    local cand="$TMPD/f$AV_IDX"
    AV_IDX=$((AV_IDX + 1))
    if git show "HEAD:$p" > "$cand" 2>/dev/null; then
      out="$cand"
    else
      rm -f "$cand"
    fi
  else
    [[ -f "$p" ]] && out="$p"
  fi
  RESOLVED["$p"]="$out"
  [[ -n "$out" ]] && { printf '%s' "$out"; return 0; }
  return 1
}

# 物理行数（wc -l 数换行符；末行无换行时补 1）
physical_lines() {
  local total
  total=$(wc -l < "$1")
  if [[ -n "$(tail -c 1 "$1")" ]]; then
    total=$((total + 1))
  fi
  echo "$total"
}

# 提取可比较 token：ASCII 标识符（≥4）+ 中日韩词块（≥2 字），排除停用词
# 两次 printf 分别喂给两个 grep：写在同一个 `{ }` 组里会让第二条命令读到 EOF
# （组内命令共享 stdin），CJK 分支静默失效、退化成纯 ASCII 比对。
ident_tokens() {
  {
    printf '%s' "$1" | grep -oE '[A-Za-z_][A-Za-z0-9_]{3,}' || true
    printf '%s' "$1" | re_cjk || true
  } |
    tr '[:upper:]' '[:lower:]' |
    grep -vE '^(file|http|https|path|line|lines|docs|tests|test|src|packages|toml|lock|cargo|self|this|that|with|from|com|www|github|rust|true|false|none|null|readme|agents|changelog|deny|toolchain|report|assessment|md|rs|py|sh|ps1|json|yaml)$' |
    sort -u || true
}

# 剥离引用与断言标记之间的分隔符（空白、反引号、中英文标点、破折号）
re_sep='^([[:space:]]|[`、，,;；:：—–\-])+'

# extract_assertions <tail> <marker> → 逐行打印 needle（只取紧邻的连续同类标记）
extract_assertions() {
  local rest="$1" marker="$2"
  local re
  re="^\\[${marker}[[:space:]]*:[[:space:]]*([^]]+)\\]"
  while [[ -n "$rest" ]]; do
    if [[ $rest =~ $re_sep ]]; then
      rest="${rest#"${BASH_REMATCH[0]}"}"
    fi
    if [[ $rest =~ $re ]]; then
      printf '%s\n' "${BASH_REMATCH[1]}"
      rest="${rest#"${BASH_REMATCH[0]}"}"
    else
      break
    fi
  done
}

# verify_file <path> <start> <end> <citing> [must] [mustnot]
# must / mustnot 为换行分隔的字面 needle 串（对齐 .ps1 的 Test-Citation 签名）
verify_file() {
  local path="$1" lineno="$2" endline="$3" citing="${4:-}"
  local must="${5:-}" mustnot="${6:-}"
  local real
  if ! real=$(resolve_file "$path"); then
    local scope=""
    (( HEAD_MODE )) && scope="（HEAD 提交内容中）"
    echo "❌ $path:$lineno — 文件不存在$scope"
    FAIL=$((FAIL + 1))
    return
  fi
  local total
  total=$(physical_lines "$real")
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
  local content range
  content=$(sed -n "${lineno}p" "$real")
  if [[ -z "${content//[[:space:]]/}" ]]; then
    echo "❌ $path:$lineno — 被引行为空行（无法承载证据）"
    FAIL=$((FAIL + 1))
    return
  fi
  # 区间全文（含起点行），供断言校验使用
  range=$(sed -n "${lineno},${endline}p" "$real")

  # v4 符号级断言：声明了 must-contain / must-not-contain 就必须真的成立
  local has_marker=0 needle
  while IFS= read -r needle; do
    [[ -z "$needle" ]] && continue
    has_marker=1
    case "$range" in
      *"$needle"*) ;;
      *)
        echo "❌ $path:$lineno — 断言失败：被引区间不含 must-contain '$needle'"
        FAIL=$((FAIL + 1))
        return
        ;;
    esac
  done <<< "$must"
  while IFS= read -r needle; do
    [[ -z "$needle" ]] && continue
    has_marker=1
    case "$range" in
      *"$needle"*)
        echo "❌ $path:$lineno — 断言失败：被引区间含 must-not-contain '$needle'"
        FAIL=$((FAIL + 1))
        return
        ;;
    esac
  done <<< "$mustnot"

  # v3 弱语义校验：被引行与引用行有共享标识符才视为"可信引用"
  # 已带断言标记的引用不再重复警告（标记本身就是更强的声明）。
  # 纯路径引用如 `Cargo.toml:6` 看版本号，允许零共享——仅警告，--strict 才红牌。
  if [[ -n "$citing" && $has_marker -eq 0 ]]; then
    local citing_tokens cited_tokens shared
    citing_tokens=$(ident_tokens "$citing")
    cited_tokens=$(ident_tokens "$content")
    if [[ -n "$citing_tokens" && -n "$cited_tokens" ]]; then
      shared=$(comm -12 <(printf '%s\n' "$citing_tokens") <(printf '%s\n' "$cited_tokens") || true)
      if [[ -z "${shared//[[:space:]]/}" ]]; then
        echo "⚠️  弱语义校验：$path:$lineno — 被引行与结论无共享标识符（纯路径引用可加 [must-contain: …] 声明）"
        WARN=$((WARN + 1))
        if (( STRICT )); then
          echo "❌ $path:$lineno — --strict 模式：语义校验失败（无共享 token 且无断言标记）"
          FAIL=$((FAIL + 1))
          return
        fi
      fi
    fi
  fi

  echo "✅ $path:$lineno (共 $total 物理行)"
  PASS=$((PASS + 1))
}

# check_cite <path> <start> <end> <line> <matched>
# 断言标记从"匹配文本之后"的尾部解析，因此只绑定紧邻的前一条引用；
# 弱语义校验用的是整行剩余文本（与 .ps1 的 Invoke-Citation → Test-Citation 一致）。
check_cite() {
  local path="$1" lineno="$2" endline="$3" citing="${4:-}" matched="${5:-}"
  local tail=""
  if [[ -n "$matched" && "$citing" == *"$matched"* ]]; then
    tail="${citing#*"$matched"}"
  fi
  verify_file "$path" "$lineno" "$endline" "$citing" \
    "$(extract_assertions "$tail" "must-contain")" \
    "$(extract_assertions "$tail" "must-not-contain")"
}

# 逐行扫描；对每行循环提取"最早出现的"引用并消费，直到该行无引用。
# v3：``` 围栏内（内嵌日志/示例代码块）跳过。
# 含反引号的正则必须放在单引号变量中（内联在 [[ =~ ]] 会触发命令替换）
re_fence='^[[:space:]]*```'
re_crate='(^|[^A-Za-z0-9_.-])((sz-orm-[a-z0-9-]+|cli|examples)/src/([a-zA-Z0-9_/-]+\.rs)):([0-9]+)'
re_root='(^|[^A-Za-z0-9_.-])(([^:[:space:]`]*\/)?(deny\.toml|Cargo\.toml|Cargo\.lock|AGENTS\.md|rust-toolchain\.toml|CHANGELOG\.md)):([0-9]+)'
# v4 通配覆盖：任意仓库相对路径 + 常见扩展名 + :行号
# 路径字符类用 *（不是 +）：`.gitignore:67` 这类点文件在扩展名前没有字符，
# 用 + 会让 sh 静默跳过而 .ps1 校验——正是本轮量到的 9 处口径差。
re_any='(^|[^A-Za-z0-9_.:/\\-])([A-Za-z0-9_./@一-鿿-]*\.(rs|py|md|markdown|yml|yaml|toml|json|h|hpp|cs|sh|ps1|js|ts|java|go|cpp|txt|ini|nix|cfg|gitignore))(:)([0-9]+)'

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
      check_cite "$local_path" "$lineno" "$endline" "$line" "$m"
      line="${line#*"$m"}"

    elif [[ "$line" =~ packages/([a-zA-Z0-9_-]+/[^:[:space:]]+\.rs):([0-9]+) ]]; then
      m="${BASH_REMATCH[0]}"
      check_cite "packages/${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[2]}" "$line" "$m"
      line="${line#*"$m"}"

    elif [[ "$line" =~ $re_crate ]]; then
      m="${BASH_REMATCH[0]}"
      crate_path="${BASH_REMATCH[2]}"
      # sz-orm-* 包的真实位置在 packages/ 下；cli/examples 在仓库根
      if [[ "${BASH_REMATCH[3]}" == sz-orm-* ]]; then
        crate_path="packages/${crate_path}"
      fi
      check_cite "$crate_path" "${BASH_REMATCH[5]}" "${BASH_REMATCH[5]}" "$line" "$m"
      line="${line#*"$m"}"

    elif [[ "$line" =~ $re_root ]]; then
      m="${BASH_REMATCH[0]}"
      check_cite "${BASH_REMATCH[2]}" "${BASH_REMATCH[5]}" "${BASH_REMATCH[5]}" "$line" "$m"
      line="${line#*"$m"}"

    elif [[ "$line" =~ src/([a-zA-Z0-9_/-]+\.rs):([0-9]+) ]]; then
      # 简写映射到 sz-orm-core
      rel_path="packages/sz-orm-core/src/${BASH_REMATCH[1]}"
      lineno="${BASH_REMATCH[2]}"
      m="${BASH_REMATCH[0]}"
      if resolve_file "$rel_path" >/dev/null; then
        check_cite "$rel_path" "$lineno" "$lineno" "$line" "$m"
      else
        echo "⚠️  src/${BASH_REMATCH[1]}:$lineno — 未找到对应文件（跳过）"
        WARN=$((WARN + 1))
      fi
      line="${line#*"$m"}"

    elif [[ "$line" =~ $re_any ]]; then
      # v4：规则 1~5 未覆盖的任意 path.ext:NN。
      m="${BASH_REMATCH[0]}"
      rel_path="${BASH_REMATCH[2]}"
      lineno="${BASH_REMATCH[5]}"
      if resolve_file "$rel_path" >/dev/null; then
        check_cite "$rel_path" "$lineno" "$lineno" "$line" "$m"
      elif [[ "$rel_path" != */* ]]; then
        # 纯文件名且仓库根不存在：可能是简写，记警告不记失败
        echo "⚠️  $rel_path:$lineno — 仓库根未找到该文件（跳过）"
        WARN=$((WARN + 1))
      else
        # 带目录的未知路径：交给 verify_file 判"文件不存在"（fail-closed）
        check_cite "$rel_path" "$lineno" "$lineno" "$line" "$m"
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
