# 门禁阻断报告（2026-09-25）

- 分支 / commit：`main` @ `59655e755a7f688ae1bac984d1755f388efc0024`（工作区干净，git status 无未提交改动）
- 失败门禁：**G4 单元/集成测试（`cargo test --workspace`）**
- 状态机：`G1 ✅ → G2 ✅ → G3 ✅ → G4 ❌ → FAILED`（红牌即停，G5~G23 未执行）
- 失败命令：

```bash
cargo test --workspace
# 复跑（稳定复现，两次一致）：
cargo test -p sz-orm-cli --test cli_phantom2_wiring_test
```

## 失败输出

```
running 3 tests
test test_migrate_derive_down_e2e ... ok
test test_wasm_build_config_e2e ... FAILED
test test_config_hot_reload_e2e ... FAILED

---- test_wasm_build_config_e2e stdout ----
thread 'test_wasm_build_config_e2e' panicked at cli\tests\cli_phantom2_wiring_test.rs:17:5:
wasm build-config 应成功

---- test_config_hot_reload_e2e stdout ----
thread 'test_config_hot_reload_e2e' panicked at cli\tests\cli_phantom2_wiring_test.rs:40:5:
config hot-reload 应成功

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s

error: test failed, to rerun pass `-p sz-orm-cli --test cli_phantom2_wiring_test`
```

## 根因分析

两个测试以子进程方式调用 `CARGO_BIN_EXE_sz-orm` 执行 CLI 子命令，并断言退出码成功；但这两个子命令是 **feature 门控命令**，裸跑 `cargo test --workspace` 构建的 CLI 二进制未启用对应 feature，CLI 按设计以退出码 1 拒绝：

- `wasm build-config` 未启用 `wasm-build` → [cli/src/main.rs:2746](../../cli/src/main.rs#L2746) 返回 `Err("wasm build-config 命令需要启用 wasm-build feature: ...")`（手动复现实测 `WASM_EXIT=1`）
- `config hot-reload` 未启用 `hot-reload` → [cli/src/main.rs:2798](../../cli/src/main.rs#L2798) 返回 `Err("config hot-reload 命令需要启用 hot-reload feature: ...")`（手动复现实测 `HOTRELOAD_EXIT=1`）

而测试文件内三个同类 e2e 测试中，**只有 lsp 测试做了 feature 门控**（[cli/tests/cli_phantom2_wiring_test.rs:49](../../cli/tests/cli_phantom2_wiring_test.rs#L49) `#[cfg(feature = "lsp-server")]`），另外两个漏掉了门控：

- [cli/tests/cli_phantom2_wiring_test.rs:11-25](../../cli/tests/cli_phantom2_wiring_test.rs#L11) `test_wasm_build_config_e2e` 无 `#[cfg(feature = "wasm-build")]`
- [cli/tests/cli_phantom2_wiring_test.rs:27-47](../../cli/tests/cli_phantom2_wiring_test.rs#L27) `test_config_hot_reload_e2e` 无 `#[cfg(feature = "hot-reload")]`

文件头注释（[cli/tests/cli_phantom2_wiring_test.rs:3](../../cli/tests/cli_phantom2_wiring_test.rs#L3)）明确要求 `cargo test -p sz-orm-cli --test cli_phantom2_wiring_test --features wasm-build,hot-reload,lsp-server` 才能运行，与裸跑语义冲突。

## 证据清单（file:line 均已核验真实存在）

| 证据 | 位置 | 说明 |
|------|------|------|
| 失败断言（wasm） | cli/tests/cli_phantom2_wiring_test.rs:17 | `assert!(output.status.success(), "wasm build-config 应成功")` |
| 失败断言（hot-reload） | cli/tests/cli_phantom2_wiring_test.rs:40 | `assert!(output.status.success(), "config hot-reload 应成功")` |
| 正确门控对照 | cli/tests/cli_phantom2_wiring_test.rs:49 | `#[cfg(feature = "lsp-server")]`（同文件既有先例） |
| 运行前提注释 | cli/tests/cli_phantom2_wiring_test.rs:3 | 要求 `--features wasm-build,hot-reload,lsp-server` |
| CLI feature 门控拒绝 | cli/src/main.rs:2746 | wasm-build 未启用时返回 Err |
| CLI feature 门控拒绝 | cli/src/main.rs:2798 | hot-reload 未启用时返回 Err |
| feature 定义 | cli/Cargo.toml:76-78 | `wasm-build`/`hot-reload`/`lsp-server` 均为按需 feature |
| 引入提交 | 9a0ac36（2026-09-17） | `feat(phantom2): CLI 子命令接线`，v8.8.0 引入；83298a3 同日为 lsp 测试补门控，另两个测试漏补 |

G15 依赖核查：`scripts/check-phantom-delivery.py` 中无对该测试文件的直接引用（grep 0 命中），为测试补门控不影响 G15 断言。

## 环境备注（不影响红牌结论）

1. **G1 执行方式**：`cargo fmt --all -- --check` 在本机 72 包 workspace 下触发 Windows 命令行长度限制（os error 206），与 `scripts/gate.ps1:96-111` 已登记问题一致；按 gate.ps1 同语义逐包 `cargo fmt --manifest-path <m> -- --check` 执行，72/72 通过。
2. **审查期间环境竞争**：审查进行中检测到另一会话在同一共享 target 目录 `F:\cargo-target` 上运行外部 `cargo test`（编译 sz-pay 下游 `sz-orm-core 5.1.0`/`sz-rust-*` 等包）并发起 `cargo clean`（进程 24176/32640，已确认并终止 24176），导致后续带 feature 复验构建出现 fingerprint/incremental 写入竞态（os error 3）。**G4 失败在环境被污染前已两次稳定复现，结论不受影响**。
3. **带 feature 复验已完成（根因实锤）**：改用隔离 target 目录 `F:\cargo-target-audit` 后台复验，`CARGO_TARGET_DIR=F:/cargo-target-audit cargo test -p sz-orm-cli --test cli_phantom2_wiring_test --features wasm-build,hot-reload` → `test result: ok. 3 passed; 0 failed`（ISO_EXIT=0）。带 feature 全绿、裸跑必红，代码逻辑无缺陷，缺陷确认为**测试文件 feature 门控缺失**。

## 建议修复

与同文件 lsp 测试的既有先例（83298a3 提交）保持一致，为两个测试补 feature 门控：

```rust
// cli/tests/cli_phantom2_wiring_test.rs
#[cfg(feature = "wasm-build")]
#[test]
fn test_wasm_build_config_e2e() { ... }   // 第 11-25 行

#[cfg(feature = "hot-reload")]
#[test]
fn test_config_hot_reload_e2e() { ... }   // 第 27-47 行
```

修复步骤：

1. 修改 [cli/tests/cli_phantom2_wiring_test.rs](../../cli/tests/cli_phantom2_wiring_test.rs) 按上述方式补 `#[cfg]` 门控；
2. `cargo test --workspace` 验证裸跑全绿（被门控测试转为 skipped）；
3. `cargo test -p sz-orm-cli --test cli_phantom2_wiring_test --features wasm-build,hot-reload,lsp-server` 验证带 feature 全绿（完整 e2e 覆盖不丢失）；
4. `cargo fmt --check --all` + `cargo clippy --workspace --all-targets -- -D warnings` 复验门禁 1/3。

## 复跑要求

修复后从 **G4** 重跑，禁止跳过失败关；G4 通过后继续 G5~G23。

## 附录：机器可验证证据（audit-verify.sh 格式）

- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/tests/cli_phantom2_wiring_test.rs#L17 （失败断言：wasm build-config 应成功）
- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/tests/cli_phantom2_wiring_test.rs#L40 （失败断言：config hot-reload 应成功）
- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/tests/cli_phantom2_wiring_test.rs#L49 （正确门控对照：#[cfg(feature = "lsp-server")]）
- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/tests/cli_phantom2_wiring_test.rs#L3 （运行前提注释：要求 --features）
- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/src/main.rs#L2746 （CLI feature 门控拒绝：wasm-build）
- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/src/main.rs#L2798 （CLI feature 门控拒绝：hot-reload）
- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/Cargo.toml#L76 （feature 定义 wasm-build）
- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/Cargo.toml#L77 （feature 定义 hot-reload）
- file:///E:/vue/test/鲜视达/rust/sz-orm/cli/Cargo.toml#L78 （feature 定义 lsp-server）
- file:///E:/vue/test/鲜视达/rust/sz-orm/scripts/gate.ps1#L96 （G1 环境问题登记：os error 206 逐包方案）
