#!/usr/bin/env python3
"""sz-orm 锁中毒（lock poisoning）风险盘点脚本（2026-10-24 深度评测新增）

背景：`std::sync::Mutex`/`std::sync::RwLock` 在线程 panic 时会进入 poisoned 状态，
后续 `.lock().unwrap()`/`.read().unwrap()`/`.write().unwrap()` 会再次 panic 造成级联故障。
连接池等关键路径已改用 `parking_lot`（非中毒）规避，但全项目仍有大量 std 锁调用点。

本脚本**盘点**生产代码中 std 锁 + unwrap 的风险面，输出基线供后续迁移跟踪：
- 每个文件的 `.lock()/.read()/.write().unwrap()` 命中数
- 是否同时存在 poison-tolerant 模式（`unwrap_or_else` / `into_inner`）
- `--strict` 时任一命中即失败（默认只盘点，exit 0）

用法:
    python scripts/check-panic-safety.py [--strict]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGES = os.path.join(ROOT, "packages")

_UNWRAP = re.compile(r"\.(?:lock|read|write)\(\)\.unwrap\(\)")
_TOLERANT = re.compile(r"unwrap_or_else|into_inner")


def main():
    strict = "--strict" in sys.argv
    total = 0
    files = []
    for dirpath, dirnames, filenames in os.walk(PACKAGES):
        if "target" in dirpath or os.sep + "tests" in dirpath:
            continue
        for fn in filenames:
            if not fn.endswith(".rs"):
                continue
            p = os.path.join(dirpath, fn)
            if os.sep + "src" + os.sep not in p:
                continue
            with open(p, encoding="utf-8", errors="replace") as f:
                content = f.read()
            if "std::sync::" not in content:
                continue
            n = len(_UNWRAP.findall(content))
            if n == 0:
                continue
            tolerant = bool(_TOLERANT.search(content))
            total += n
            files.append((os.path.relpath(p, ROOT).replace("\\", "/"), n, tolerant))

    print("=" * 60)
    print("  sz-orm 锁中毒风险盘点（std::sync 锁 + unwrap）")
    print("=" * 60)
    print(f"  涉及文件数: {len(files)}")
    print(f"  unwrap 调用总数: {total}")
    print(f"  poison-tolerant（unwrap_or_else/into_inner）文件数: "
          f"{sum(1 for _, _, t in files if t)}")
    print("\n  按命中数排序 TOP 15：")
    for rel, n, tol in sorted(files, key=lambda x: -x[1])[:15]:
        print(f"    {rel}: {n} 次（tolerant={'是' if tol else '否'}）")

    if strict and total > 0:
        print(f"\n  --strict：存在 {total} 处 std 锁 unwrap 调用 → ❌ 失败")
        return 1
    print("\n  结果: ✅ 盘点完成（基线输出，非阻断；建议逐步迁移到 parking_lot 或 "
          "unwrap_or_else(|e| e.into_inner())）")
    return 0


if __name__ == "__main__":
    sys.exit(main())