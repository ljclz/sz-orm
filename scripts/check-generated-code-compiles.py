#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""G34 门禁：代码生成器产物编译验证（2026-10-27 新增，UXR 评测第三轮落地）

背景：
  - designer code_gen 曾生成 `#[derive(sz_orm_core::Model)]`（宏当时不存在）；
  - CLI `make:model --pk-type i32/u32/u64` 曾生成引用不存在 `Value::as_i32/as_u32/as_u64`
    的坏代码；
  - CLI 模板曾使用私有模块路径 `sz_orm_core::model::` / `sz_orm_core::value::`（模块私有）。
  现有单元测试只断言生成字符串包含某些片段，不编译产物，导致坏代码漏网。

检查方式：
  1. 在 `target/g28-gen-check/` 创建临时 crate（path 依赖 sz-orm-core）；
  2. 调用 `cargo run -p sz-orm-cli -- make:model` 生成 5 种主键类型（i64/u32/u64/String/uuid）
     且带业务字段（--fields）的真实产物；
  3. 将产物作为模块引入临时 crate，`cargo check` 编译；
  4. 编译失败 → 红牌。

说明：需要 cargo 可用；cargo 不在 PATH 时自动跳过（返回 0，与 G7/G19 网络类门禁一致）。
用法：
    python scripts/check-generated-code-compiles.py [--strict]
退出码：0 = 通过（或跳过）；1 = 生成产物无法编译。
"""
from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TMP = ROOT / "target" / "g28-gen-check"

MODELS = [
    ("UserI64", "i64"),
    ("UserU32", "u32"),
    ("UserU64", "u64"),
    ("UserString", "String"),
    ("UserUuid", "uuid"),
]
FIELDS = "name:String,age:i32"


def to_snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def run(cmd: list[str], cwd: Path | None = None, env: dict | None = None):
    # 列表形式调用 subprocess（shell=False 默认），cmd 来自脚本内硬编码参数，非用户输入
    print("  $ ", *map(str, cmd))
    return subprocess.run(cmd, cwd=str(cwd) if cwd else None, env=env)


def run_with_retry(cmd: list[str], retries: int = 1, cwd: Path | None = None, env: dict | None = None):
    """复用 run() 执行命令，瞬时失败（退出码非 0）时重试。

    v10.13.0 (2026-10-28 架构评测 P0-5 门禁确定性治理)：共享 target 目录下被强杀的
    并行 cargo 进程可能残留陈旧增量产物，导致门禁偶发瞬时红；隔离 target + 单次重试
    将此类基础设施竞态与真实回归区分开。
    """
    r = None
    for attempt in range(retries + 1):
        r = run(cmd, cwd=cwd, env=env)
        if r.returncode == 0:
            return r
        if attempt < retries:
            print(f"    ↳ 第 {attempt + 1} 次失败（退出码 {r.returncode}），重试一次（瞬时 cargo 构建竞态缓解）")
    return r


def main() -> int:
    parser = argparse.ArgumentParser(description="G28 生成器产物编译验证")
    parser.add_argument("--strict", action="store_true",
                        help="cargo 不可用时视为失败（默认跳过）")
    args = parser.parse_args()

    cargo = shutil.which("cargo")
    if not cargo:
        print("  ℹ️  cargo 不在 PATH，跳过生成器产物编译验证（G28）")
        return 1 if args.strict else 0

    print("=" * 60)
    print("  代码生成器产物编译验证（门禁 34）")
    print("=" * 60)

    # 1. 准备临时 crate
    if TMP.exists():
        shutil.rmtree(TMP)
    (TMP / "src").mkdir(parents=True)
    (TMP / "Cargo.toml").write_text(
        f"""[package]
name = "gen-check"
version = "0.1.0"
edition = "2021"

[dependencies]
sz-orm-core = {{ path = "{ROOT.as_posix()}/packages/sz-orm-core" }}

# 脱离父 workspace（临时验证 crate 不属于 sz-orm workspace）
[workspace]
""",
        encoding="utf-8",
    )

    # 2. 生成 5 种主键类型模型
    # v10.13.0 (2026-10-28 P0-5)：整条流水线使用独立 CARGO_TARGET_DIR（target/g28-target），
    # 与工作空间构建、并行评测会话的 cargo 进程完全隔离——共享 target 目录下不同
    # feature 统一化的交错构建曾产生陈旧增量产物，导致本门禁瞬时红（E0308）。
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(ROOT / "target" / "g28-target")
    gen_ok = True
    for name, pk in MODELS:
        r = run_with_retry(
            [cargo, "run", "-p", "sz-orm-cli", "--", "make:model", name,
             "--pk-type", pk, "--fields", FIELDS, "--output", str(TMP / "src")],
            cwd=ROOT,
            env=env,
        )
        if r.returncode != 0:
            print(f"  ❌ 生成 {name} (pk={pk}) 失败")
            gen_ok = False

    if not gen_ok:
        print("\n  结果: ❌ 生成模型代码失败")
        return 1

    # 3. 写 lib.rs 引入所有生成模块（文件名 = snake_case(模型名)）
    mods = "\n".join(f"pub mod {to_snake(name)};" for name, _ in MODELS)
    (TMP / "src" / "lib.rs").write_text(
        "//! G28 生成器产物编译验证（自动生成，勿手改）\n" + mods + "\n",
        encoding="utf-8",
    )

    # 4. cargo check 编译生成产物
    r = run([cargo, "check"], cwd=TMP, env=env)
    if r.returncode != 0:
        print("\n  结果: ❌ 生成产物无法编译（红牌）")
        return 1

    print("\n  结果: ✅ 5 种主键类型的 make:model 产物均可编译")
    return 0


if __name__ == "__main__":
    sys.exit(main())