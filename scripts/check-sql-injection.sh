#!/bin/bash
# SQL Injection Risk Scanner (Gate 9)
# Scans workspace for potential SQL injection risks.

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEPRECATED=0
CONCAT=0

echo "=== Gate 9: SQL Injection Scan ==="

# Check deprecated where_cond / or_where usage in non-test code
while IFS=: read -r file line content; do
    if [[ ! "$file" =~ test ]]; then
        echo "[DEPRECATED] $file:$line: $content"
        DEPRECATED=$((DEPRECATED + 1))
    fi
done < <(grep -rn '\.where_cond(\|\.or_where(' --include='*.rs' "$ROOT/packages" "$ROOT/examples" "$ROOT/cli" 2>/dev/null | grep -v '///\|//!')

# Check format! with WHERE + variable interpolation
while IFS=: read -r file line content; do
    # Skip if it uses array indexing (columns[0], conditions[0], etc.)
    if ! echo "$content" | grep -qE 'columns\[|conditions\[|tables\[|updates\['; then
        echo "[REVIEW] $file:$line: $content"
        CONCAT=$((CONCAT + 1))
    fi
done < <(grep -rn 'format!.*WHERE.*{' --include='*.rs' "$ROOT/packages" "$ROOT/examples" "$ROOT/cli" 2>/dev/null | grep -v '///\|//!')

TOTAL=$((DEPRECATED + CONCAT))
if [ "$TOTAL" -eq 0 ]; then
    echo "Gate 9 PASSED: no SQL injection risks found"
    exit 0
else
    echo "Gate 9: $TOTAL item(s) found (non-blocking, manual review recommended)"
    exit 0
fi
