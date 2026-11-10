#!/bin/bash
# 批量跑非 cargo 门禁脚本
# 兼容两种调用方式：直接用 wsl -d Ubuntu -u root 时 cwd 已是仓库根；容器内挂载到 /w 时用 $ROOT
ROOT="${1:-$(pwd)}"
cd "$ROOT" 2>/dev/null || cd /mnt/d/www/test/鲜视达/rust/sz-orm 2>/dev/null || exit 1
echo "运行目录: $(pwd)"

scripts=(
  check-sql-injection-auto.py
  check-secrets.py
  check-phantom-delivery.py
  check-semantic-patterns.py
  check-architecture.py
  check-metrics-real.py
  check-publish-consistency.py
  check-doc-consistency.py
  check-doc-sync.py
  check-examples-readme.py
  check-doc-code-compiles.py
  check-ci-deny-sync.py
  check-gate-scripts.py
  check-security-doc-version.py
  check-test-symbols.py
  check-phantom-tests.py
  check-ci-blocking.py
  check-filename-encoding.py
  check-doc-version-consistency.py
  check-generated-code-compiles.py
  check-doc-links.py
  check-changelog-consistency.py
)

pass=0
fail=0
for script in "${scripts[@]}"; do
  if [ -f "scripts/$script" ]; then
    python3 "scripts/$script" >/dev/null 2>&1
    rc=$?
    if [ $rc -eq 0 ]; then
      echo "✅ $script"
      pass=$((pass+1))
    else
      echo "❌ $script (exit=$rc)"
      fail=$((fail+1))
    fi
  else
    echo "⚠️  $script not found"
  fi
done

echo ""
echo "总计: $pass 通过, $fail 失败"