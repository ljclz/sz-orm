#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check-csharp-abi-surface.py — C# 绑定与 Rust 导出/ABI 头文件的三方对齐校验器

背景（2026-10-28 v3 评测）：C# 绑定用 P/Invoke 直接按符号名调用 Rust 导出，
`csharp/SzOrm.cs`、`include/sz_orm_cabi.h`、`src/lib.rs` 三处必须逐符号一致，
且字符串编解码必须遵守头文件里的 UTF-8 契约。历史上出过两类缺陷：

  * R5：Rust 侧 UTF-8，C# 侧却用 `LPStr` / `PtrToStringAnsi`（ANSI 代码页），
        非 ASCII 数据必然乱码——本机无 .NET SDK，测试跑不到，只能靠静态校验；
  * 覆盖面口径漂移：README 宣称"封装 N / 共 M 个导出"，M 随导出增删而变，
    文档不会自己更新。

本脚本做**不需要 .NET SDK** 的静态一致性校验（fail-closed）：

  R1  C# 每条 DllImport/LibraryImport 的符号必须存在于 lib.rs 的
      `#[no_mangle] extern \"C\"` 导出集合 —— 否则运行时 EntryPointNotFound。
  R2  C# 每条导入符号必须在 include 头文件中声明 —— 头文件是 ABI 单一声明源。
  R3  C# 侧禁止 ANSI/UTF-16 编解码标记（LPStr / PtrToStringAnsi / LPWStr /
      PtrToStringUni / StringAnsi），必须与 UTF-8 契约一致。
  R4  头文件必须携带 ABI 契约横幅（UTF-8 字符串编码 + 仅 SQLite DSN）。
  R5  README.md / README.zh.md 宣称的 C ABI 导出总数必须等于 lib.rs 实测数量。

用法：
    python scripts/check-csharp-abi-surface.py            # 读工作区
    python scripts/check-csharp-abi-surface.py --at-head  # 读 HEAD 提交内容

退出码：0 通过；1 存在违规；2 用法/输入不可读（fail-closed）。
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT_FILES = {
    "lib.rs": "packages/sz-orm-cabi/src/lib.rs",
    "header": "packages/sz-orm-cabi/include/sz_orm_cabi.h",
    "csharp": "packages/sz-orm-cabi/csharp/SzOrm.cs",
    "readme": "README.md",
    "readme_zh": "README.zh.md",
}

# 禁止的 C# 非 UTF-8 编解码标记（R3）
BAD_MARSHAL = [
    ("LPStr", "ANSI 代码页入参（应为 UnmanagedType.LPUTF8Str）"),
    ("LPWStr", "UTF-16 入参（应为 UnmanagedType.LPUTF8Str）"),
    ("PtrToStringAnsi", "ANSI 解码出参（应为 Marshal.PtrToStringUTF8）"),
    ("PtrToStringUni", "UTF-16 解码出参（应为 Marshal.PtrToStringUTF8）"),
    ("StringAnsi", "ANSI 字符串封送（契约要求 UTF-8）"),
]

RE_EXPORT = re.compile(r'#\[no_mangle\]\s*(?:pub\s+)?(?:unsafe\s+)?extern\s+"C"\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)')
RE_EXPORT_INLINE = re.compile(r'extern\s+"C"\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)')
RE_HEADER_DECL = re.compile(r'\b(sz_orm_[A-Za-z0-9_]+)\s*\(')
RE_DLLIMPORT = re.compile(r'\[(?:DllImport|LibraryImport)\b[^\]]*\]\s*(?:private\s+|internal\s+|public\s+)?static\s+(?:unsafe\s+)?(?:extern\s+)?[\w\*\[\]<>,\s\?]+?\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(')
RE_README_TOTAL = re.compile(
    r'(\d+)\s*(?:个|of the)\s*(?:C\s*ABI|ABI\s*导出)'   # "37 个 C ABI" / "37 of the C ABI"
    r'|of the\s+(\d+)\s+C\s*ABI'                        # "9 of the 37 C ABI exports"
    r'|(\d+)\s+C\s*ABI'                                 # "37 C ABI exports"
    r'|(?:C ABI\s*导出|ABI 导出)[^\d]{0,12}(\d+)\s*个'   # "C ABI 导出 37 个"
)


def read_text(root: Path, rel: str, at_head: bool) -> str:
    """读取仓库内文本；--at-head 时取提交内容，避免工作区在制品干扰判定。"""
    if at_head:
        proc = subprocess.run(
            ["git", "-C", str(root), "show", f"HEAD:{rel}"],
            capture_output=True,
            timeout=60,
        )
        if proc.returncode != 0:
            raise IOError(f"git show HEAD:{rel} 失败")
        return proc.stdout.decode("utf-8", "replace")
    p = root / rel
    if not p.is_file():
        raise IOError(f"文件不存在: {rel}")
    return p.read_text(encoding="utf-8", errors="replace")


def rust_exports(text: str) -> set:
    """`#[no_mangle] extern \"C\" fn NAME` 导出集合。

    属性与函数签名可能跨行，先做换行归一再匹配；两种形态都覆盖。
    """
    joined = re.sub(r"\s+", " ", text)
    names = set(RE_EXPORT.findall(joined))
    # 兜底：属性与 fn 之间夹了其他属性（#[cfg(...)]）的情形
    for m in RE_EXPORT_INLINE.finditer(text):
        names.add(m.group(1))
    return names


def header_decls(text: str) -> set:
    return set(RE_HEADER_DECL.findall(text))


def csharp_imports(text: str) -> set:
    joined = re.sub(r"[ \t]+", " ", text)
    names = set()
    for m in RE_DLLIMPORT.finditer(joined):
        fn = m.group(1)
        if fn.startswith("sz_orm"):
            names.add(fn)
    return names


def readme_totals(text: str) -> set:
    nums = set()
    for m in RE_README_TOTAL.finditer(text):
        for g in m.groups():
            if g and g.isdigit():
                nums.add(int(g))
    return nums


def main() -> int:
    ap = argparse.ArgumentParser(description="C# 绑定与 Rust 导出/ABI 头文件三方对齐校验")
    ap.add_argument("--root", default=".", help="仓库根（默认当前目录）")
    ap.add_argument("--at-head", action="store_true", help="以 HEAD 提交内容为准")
    args = ap.parse_args()

    root = Path(args.root).resolve()
    try:
        src = {k: read_text(root, rel, args.at_head) for k, rel in ROOT_FILES.items()}
    except (IOError, OSError, subprocess.SubprocessError) as exc:
        print("FATAL: 输入不可读（fail-closed）: %s" % exc)
        return 2

    source = "HEAD" if args.at_head else "工作区"
    print("=== C# ABI 表面对齐校验（读取来源：%s）===" % source)

    exports = rust_exports(src["lib.rs"])
    decls = header_decls(src["header"])
    imports = csharp_imports(src["csharp"])

    violations = []
    warnings = []

    if not exports:
        violations.append("lib.rs 未解析到任何 #[no_mangle] extern \"C\" 导出（解析失效即红牌，禁止静默通过）")
    if not imports:
        violations.append("SzOrm.cs 未解析到任何 DllImport/LibraryImport 符号（解析失效即红牌）")
    if not decls:
        violations.append("sz_orm_cabi.h 未解析到任何 sz_orm_* 声明（解析失效即红牌）")

    # R1 C# → Rust 导出
    for name in sorted(imports - exports):
        violations.append("R1 C# 导入的符号在 lib.rs 无导出：sz_orm 入口 '%s'（运行时 EntryPointNotFound）" % name)
    # R2 C# → 头文件声明
    for name in sorted(imports - decls):
        violations.append("R2 C# 导入的符号未在 ABI 头文件声明：'%s'" % name)

    # R3 编码契约
    for token, why in BAD_MARSHAL:
        hits = [i + 1 for i, line in enumerate(src["csharp"].splitlines()) if token in line]
        if hits:
            violations.append("R3 C# 侧存在非 UTF-8 编解码 '%s'（%s）行号 %s" % (token, why, ",".join(map(str, hits))))

    # R4 头文件契约横幅
    header_low = src["header"].lower()
    if "utf-8" not in header_low:
        violations.append("R4 头文件缺少 UTF-8 字符串编码契约声明")
    if "sqlite" not in header_low:
        violations.append("R4 头文件缺少\"仅 SQLite DSN\"限制声明")

    # R5 README 宣称的导出总数
    total = len(exports)
    for label, key in (("README.md", "readme"), ("README.zh.md", "readme_zh")):
        nums = readme_totals(src[key])
        if not nums:
            warnings.append("R5 %s 未宣称 C ABI 导出总数（跳过）" % label)
            continue
        if total not in nums:
            violations.append(
                "R5 %s 宣称导出总数 %s 与 lib.rs 实测 %d 不一致" % (label, sorted(nums), total)
            )

    # 覆盖面提示（仅警告，不作红牌）：未被 C# 封装的导出
    uncovered = sorted(exports - imports)
    if uncovered:
        warnings.append(
            "覆盖面：%d 个导出已被 C# 封装，%d 个未封装（如 %s）"
            % (len(imports & exports), len(uncovered), ", ".join(uncovered[:5]))
        )

    # SafeHandle 落地情况（P1-5 记账项，警告级：裸 IntPtr 依赖手工 Dispose/终结器）
    if "SafeHandle" not in src["csharp"]:
        warnings.append(
            "句柄形态：C# 侧仍为裸 IntPtr + 手工 Dispose/终结器，未使用 SafeHandle（P1-5 遗留）"
        )

    for w in warnings:
        print("  WARN  %s" % w)
    for v in violations:
        print("  RED   %s" % v)

    print("实测：Rust 导出 %d，头文件声明 %d，C# 导入 %d" % (len(exports), len(decls), len(imports)))
    if violations:
        print("FAIL: %d 项违规" % len(violations))
        return 1
    print("OK: C# 绑定与 Rust 导出/头文件契约一致")
    return 0


if __name__ == "__main__":
    sys.exit(main())
