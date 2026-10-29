#!/bin/bash
# v10.12.0 全量门禁执行脚本（服务器后台运行版）
# 状态文件: gates-status/gate_N.status (PASS/FAIL/SKIP/RUNNING)
# 日志文件: gates-status/gates.log
# 完成标记: gates-status/ALL_DONE

cd /www/rust/sz-orm-v10120 || exit 1

STATUS_DIR="gates-status"
LOG="$STATUS_DIR/gates.log"
mkdir -p "$STATUS_DIR"
rm -f "$STATUS_DIR/ALL_DONE"

# 环境变量
export RUST_MIN_STACK=67108864
export CARGO_INCREMENTAL=0
export PATH="/root/.cargo/bin:$PATH"
export RUSTC_WRAPPER=""

log() { echo "[$(date '+%H:%M:%S')] $*" | tee -a "$LOG"; }
mark() { echo "$2" > "$STATUS_DIR/gate_$1.status"; log "Gate $1: $2"; }

log "===== v10.12.0 全量门禁开始 ====="

# Gate 1: fmt（v10.13.0 / M-4 整改：改为真实执行，不再硬编码 PASS）
mark 1 RUNNING
if cargo fmt --all -- --check >> "$LOG" 2>&1; then mark 1 PASS; else mark 1 FAIL; fi

# Gate 2: check
mark 2 RUNNING
if cargo check --workspace --all-targets -j 8 >> "$LOG" 2>&1; then mark 2 PASS; else mark 2 FAIL; fi

# Gate 3: clippy
mark 3 RUNNING
if cargo clippy --workspace --all-targets -j 8 -- -D warnings >> "$LOG" 2>&1; then mark 3 PASS; else mark 3 FAIL; fi

# Gate 4: test
mark 4 RUNNING
if cargo test --workspace -j 8 >> "$LOG" 2>&1; then mark 4 PASS; else mark 4 FAIL; fi

# Gate 5: doc
mark 5 RUNNING
if cargo doc --workspace --no-deps --all-features -j 8 >> "$LOG" 2>&1; then mark 5 PASS; else mark 5 FAIL; fi

# Gate 6: audit
mark 6 RUNNING
cargo audit >> "$LOG" 2>&1
AUDIT_RC=$?
cargo deny check advisories >> "$LOG" 2>&1
DENY_RC=$?
if [ $AUDIT_RC -eq 0 ] && [ $DENY_RC -eq 0 ]; then mark 6 PASS; else mark 6 FAIL; fi
log "Gate 6 audit rc=$AUDIT_RC deny rc=$DENY_RC"

# Gate 7: integration（跳过 clickhouse/mssql）
mark 7 RUNNING
if cargo test --workspace --tests -j 8 -- --ignored --skip clickhouse --skip mssql >> "$LOG" 2>&1; then mark 7 PASS; else mark 7 FAIL; fi

# Gate 8: 禁止占位实现 + unsafe 合规审计
# v10.13.0 (2026-10-27): 并入 check-unsafe-audit.py（L-3 裁决：块级/函数级双口径 SAFETY 覆盖）
mark 8 RUNNING
G8_OK=1
PLACEHOLDERS=$(grep -rn 'todo!\|unimplemented!\|unreachable!' --include='*.rs' packages/ | grep -v 'tests/' | grep -v '\.md' | wc -l)
if [ "$PLACEHOLDERS" -ne 0 ]; then G8_OK=0; fi
if ! python3 scripts/check-unsafe-audit.py >> "$LOG" 2>&1; then G8_OK=0; fi
if [ $G8_OK -eq 1 ]; then mark 8 PASS; else mark 8 FAIL; fi
log "Gate 8 placeholders found: $PLACEHOLDERS"

# Gate 9: SQL 注入扫描 + secrets 扫描 + G9 扫描器自测
# v10.13.0 (2026-10-27): 并入 check-secrets.py（H-3 整改），加 --strict（M-4 整改：
# 原 Gate 9 未用 --strict，Unsafe>0 时脚本退出码仍为 0 导致假 PASS）
# v10.13.0 (2026-10-27): 并入 test_check_sql_injection_auto.py（M-3 教训：扫描器自测必须接入门禁）
mark 9 RUNNING
G9_OK=1
if ! python3 scripts/check-sql-injection-auto.py --strict >> "$LOG" 2>&1; then G9_OK=0; fi
if ! python3 scripts/test_check_sql_injection_auto.py >> "$LOG" 2>&1; then G9_OK=0; fi
if ! python3 scripts/check-secrets.py >> "$LOG" 2>&1; then G9_OK=0; fi
if [ $G9_OK -eq 1 ]; then mark 9 PASS; else mark 9 FAIL; fi

# Gate 10: Feature 全组合编译（可能因 protoc 缺失失败）
mark 10 RUNNING
if cargo check --workspace --all-targets --all-features -j 8 >> "$LOG" 2>&1; then mark 10 PASS; else mark 10 FAIL; fi

# Gate 11: ADR-0001 上游仓库未修改
# 下游项目严禁修改 sz-orm 上游代码；检查 packages/ 与工作区清单是否有改动。
# （untracked 的 docs/、gates-status/ 等交付物不视为对上游代码的修改）
mark 11 RUNNING
G11_MODIFIED=$(git diff --name-only HEAD -- packages/ Cargo.toml Cargo.lock 2>/dev/null | grep -v '^$' | wc -l)
if [ "$G11_MODIFIED" -eq 0 ]; then mark 11 PASS; else mark 11 FAIL; fi
log "Gate 11 modified upstream files: $G11_MODIFIED"

# Gate 12: 文档与代码一致性
mark 12 RUNNING
if python3 scripts/check-doc-consistency.py >> "$LOG" 2>&1; then mark 12 PASS; else mark 12 FAIL; fi

# Gate 13: 审计证据验证
mark 13 RUNNING
if [ -f "docs/assessment/2026-10-20-comprehensive-audit-report.md" ]; then
  if bash scripts/audit-verify.sh docs/assessment/2026-10-20-comprehensive-audit-report.md >> "$LOG" 2>&1; then mark 13 PASS; else mark 13 FAIL; fi
else
  mark 13 SKIP
fi

# Gate 14: 文档同步更新检查
mark 14 RUNNING
if python3 scripts/check-doc-sync.py --diff HEAD >> "$LOG" 2>&1; then mark 14 PASS; else mark 14 FAIL; fi

# Gate 15: 幻影交付检查
mark 15 RUNNING
if python3 scripts/check-phantom-delivery.py >> "$LOG" 2>&1; then mark 15 PASS; else mark 15 FAIL; fi

# Gate 16: 语义反模式扫描
mark 16 RUNNING
if python3 scripts/check-semantic-patterns.py >> "$LOG" 2>&1; then mark 16 PASS; else mark 16 FAIL; fi

# Gate 17: 架构一致性扫描
mark 17 RUNNING
if python3 scripts/check-architecture.py >> "$LOG" 2>&1; then mark 17 PASS; else mark 17 FAIL; fi

# Gate 18: 度量真实性扫描
mark 18 RUNNING
if python3 scripts/check-metrics-real.py >> "$LOG" 2>&1; then mark 18 PASS; else mark 18 FAIL; fi

# Gate 19: 发布一致性扫描
mark 19 RUNNING
if python3 scripts/check-publish-consistency.py >> "$LOG" 2>&1; then mark 19 PASS; else mark 19 FAIL; fi

# Gate 20: 变异测试（跳过，单独执行）
mark 20 SKIP

# Gate 21: 安全攻击测试
# v10.13.0 (M-4 整改)：在 3 个核心攻击测试之外，纳入 owasp-pentest-suite 门控套件
# （12 个定义该 feature 的包逐一执行，对齐 docs/sz-orm-review-runbook.md §4.1）
mark 21 RUNNING
SEC_PASS=0
SEC_TOTAL=0
for t in "cargo test -p sz-orm-auth --test security_attacks -j 8" \
         "cargo test -p sz-orm-crypto --test kat -j 8" \
         "cargo test -p sz-orm-core --features multi-tenant-enhanced --test security_attacks -j 8"; do
  SEC_TOTAL=$((SEC_TOTAL+1))
  if $t >> "$LOG" 2>&1; then SEC_PASS=$((SEC_PASS+1)); fi
done
for p in sz-orm-audit sz-orm-auth sz-orm-config sz-orm-core sz-orm-crypto \
         sz-orm-dtx sz-orm-grpc sz-orm-lc sz-orm-masking sz-orm-storage \
         sz-orm-swagger sz-orm-wasm; do
  SEC_TOTAL=$((SEC_TOTAL+1))
  if cargo test -p "$p" --features owasp-pentest-suite -j 8 >> "$LOG" 2>&1; then
    SEC_PASS=$((SEC_PASS+1))
  else
    log "Gate 21 owasp suite failed: $p"
  fi
done
if [ $SEC_PASS -eq $SEC_TOTAL ]; then mark 21 PASS; else mark 21 FAIL; fi
log "Gate 21 security tests: $SEC_PASS/$SEC_TOTAL passed"

# Gate 22: 覆盖率（跳过，单独执行）
mark 22 SKIP

# Gate 23: 未用依赖扫描
mark 23 RUNNING
if python3 scripts/check-unused-deps.py >> "$LOG" 2>&1; then mark 23 PASS; else mark 23 FAIL; fi

# Gate 24: 示例清单同步（README Examples 表格 ↔ examples/src/bin 双向一致）
mark 24 RUNNING
if python3 scripts/check-examples-readme.py >> "$LOG" 2>&1; then mark 24 PASS; else mark 24 FAIL; fi

# Gate 25: 文档代码可编译性（文档示例引用的 API/宏必须真实存在）
mark 25 RUNNING
if python3 scripts/check-doc-code-compiles.py >> "$LOG" 2>&1; then mark 25 PASS; else mark 25 FAIL; fi

# Gate 26: CI 与 deny.toml ignore 一致性（v10.13.0 / M-1 防复发）
mark 26 RUNNING
if python3 scripts/check-ci-deny-sync.py >> "$LOG" 2>&1; then mark 26 PASS; else mark 26 FAIL; fi

# Gate 27: 门禁脚本自检（v10.13.0 / M-4 防复发：硬编码 PASS / || 假 PASS / 关键脚本接入）
mark 27 RUNNING
if python3 scripts/check-gate-scripts.py >> "$LOG" 2>&1; then mark 27 PASS; else mark 27 FAIL; fi

# Gate 28: 安全文档版本一致性（v10.13.0 / M-5 防复发：Security.md 与 workspace 版本漂移）
mark 28 RUNNING
if python3 scripts/check-security-doc-version.py >> "$LOG" 2>&1; then mark 28 PASS; else mark 28 FAIL; fi

# Gate 29: 幻影测试引用检测（v10.13.0 / H-2 防复发，警告级，strict 时阻断）
mark 29 RUNNING
if python3 scripts/check-test-symbols.py >> "$LOG" 2>&1; then mark 29 PASS; else mark 29 FAIL; fi

# Gate 30: 幻影测试扫描（v10.13.0 / 2026-10-27 SDET 评测新增：永真断言/字面量断言/
# 零断言测试函数/差分命名规范）
mark 30 RUNNING
if python3 scripts/check-phantom-tests.py >> "$LOG" 2>&1; then mark 30 PASS; else mark 30 FAIL; fi

# Gate 31: CI 阻断语义审计（v10.13.0 / 2026-10-27 SDET 评测新增：continue-on-error
# job 必须登记豁免表，否则 fail-closed）
mark 31 RUNNING
if python3 scripts/check-ci-blocking.py >> "$LOG" 2>&1; then mark 31 PASS; else mark 31 FAIL; fi

# Gate 32: 文件名编码合法性（2026-10-28 编号统一：此前与 Gate 26 重号）
mark 32 RUNNING
if python3 scripts/check-filename-encoding.py >> "$LOG" 2>&1; then mark 32 PASS; else mark 32 FAIL; fi

# Gate 33: 文档版本声明一致性（2026-10-28 编号统一：此前与 Gate 27 重号）
mark 33 RUNNING
if python3 scripts/check-doc-version-consistency.py >> "$LOG" 2>&1; then mark 33 PASS; else mark 33 FAIL; fi

# Gate 34: 代码生成器产物编译验证（2026-10-28 编号统一：此前与 Gate 28 重号；
# cargo 缺失时脚本内部跳过）
mark 34 RUNNING
if python3 scripts/check-generated-code-compiles.py >> "$LOG" 2>&1; then mark 34 PASS; else mark 34 FAIL; fi

log "===== 全量门禁结束 ====="

# 汇总
PASS_COUNT=$(grep -l PASS "$STATUS_DIR"/gate_*.status 2>/dev/null | wc -l)
FAIL_COUNT=$(grep -l FAIL "$STATUS_DIR"/gate_*.status 2>/dev/null | wc -l)
SKIP_COUNT=$(grep -l SKIP "$STATUS_DIR"/gate_*.status 2>/dev/null | wc -l)
log "Summary: PASS=$PASS_COUNT FAIL=$FAIL_COUNT SKIP=$SKIP_COUNT"

echo "DONE PASS=$PASS_COUNT FAIL=$FAIL_COUNT SKIP=$SKIP_COUNT" > "$STATUS_DIR/ALL_DONE"