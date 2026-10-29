#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""门禁：门禁脚本自检（audit the checkers）。

背景：2026-10-27 安全评测 M-4 发现：
- `run-gates-v10120-server.sh` Gate 1 硬编码 `mark 1 PASS`（无实际 fmt 检查）
- Gate 9 未用 --strict，Unsafe>0 时脚本仍退出 0 导致假 PASS
- Gate 21 未纳入 OWASP 套件
- 2026-10-24 深度评测还发现过 Gate 6 用 `||` 导致"任一成功即 PASS"

本门禁扫描门禁脚本自身，防止上述缺陷回归：
1. 硬编码 PASS：`mark N PASS` 出现在无条件执行行（非 `if ... then mark N PASS`）
2. 假 PASS 模式：`if A || B; then mark N PASS`（任一成功即放行，含反斜杠续行）
3. 恒真条件：`if true / if [ true ] / if :; then mark N PASS`
4. 顺序链 PASS：`some_cmd; mark N PASS`（命令失败也会 PASS）
5. gate.ps1 关卡结果 `$ok` 未被检查（失败不会阻断）
6. 关键安全辅助脚本是否被接入（G9 自测 / secrets / unsafe 审计 / G24-G34 增强门禁）
7. CI 工作流假 PASS：`run:` 命令里的 `|| true` / `|| :` 必须登记豁免（2026-10-29 P2-3 后半）
8. 接入源可见性：登记源扩展为 服务器门禁 + gate.ps1 + `.github/workflows/*.yml`。
   只接入 CI 的检查器必须登记在 `CI_ONLY_EXEMPT` 并写明理由，否则红牌。
   （2026-10-28 全栈评测 §8.4 的盲区：本门禁历史上只拼 `run-gates-v10120-server.sh`
   与 `gate.ps1` 两个脚本文本，删掉 CI 里的步骤不会有任何反应。）
   `continue-on-error` 不在本规则范围内，由门禁 31（check-ci-blocking.py）负责。
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
SERVER_GATE = SCRIPTS / "run-gates-v10120-server.sh"
PS_GATE = SCRIPTS / "gate.ps1"
WORKFLOWS = ROOT / ".github" / "workflows"

# 关键安全辅助脚本（必须出现在门禁脚本中，否则安全门禁不完整）
REQUIRED_CHECKS = [
    "check-sql-injection-auto.py",      # G9 SQL 注入扫描（--strict）
    "test_check_sql_injection_auto.py", # G9 扫描器自测（M-3 教训：自测必须接入）
    "check-secrets.py",                 # 凭据泄露扫描（H-3）
    "check-unsafe-audit.py",            # unsafe/SAFETY 审计（L-3）
    "check-ci-deny-sync.py",            # CI 与 deny.toml ignore 一致性（M-1）
    "check-gate-scripts.py",            # 门禁自检（本脚本，豁免自引用）
    "check-security-doc-version.py",    # 安全文档版本漂移（M-5）
    "check-phantom-tests.py",          # G30 幻影测试扫描（2026-10-27 SDET 评测新增）
    "check-ci-blocking.py",            # G31 CI 阻断语义审计（2026-10-27 SDET 评测新增）
    "check-examples-readme.py",        # G24 示例清单同步（2026-10-28 P1-3 接线）
    "check-doc-code-compiles.py",      # G25 文档代码可编译性（2026-10-28 P1-3 接线）
    "check-filename-encoding.py",      # G32 文件名编码合法性（2026-10-28 编号统一）
    "check-doc-version-consistency.py",# G33 文档版本声明一致性（2026-10-28 编号统一）
    "check-generated-code-compiles.py",# G34 生成器产物编译验证（2026-10-28 编号统一）
    "check-csharp-abi-surface.py",     # C# 绑定/Rust 导出/ABI 头三方对齐（2026-10-29 P2-3 纳入登记源）
]

# CI 工作流里允许出现的 `|| true` / `|| :`（吞掉失败）站点，必须逐条登记理由。
# 匹配方式：行内容（去空白）包含下列片段即视为已豁免；未登记的命中一律红牌。
# 为什么不是"禁止一切 || true"：这些命令的失败本身不是被审查的属性
# （条件依赖工具、清理进程、网络差异），但"静默把失败改成成功"必须显式记账。
CI_FAIL_EXEMPT = {
    "cargo +nightly udeps": "cargo-udeps 需 nightly，非 nightly 环境安装失败不应阻断主构建（警告级，与门禁 23 同口径）",
    "pkill -f cargo-mutants": "收尾清理：没有残留进程时 pkill 返回 1，属于期望行为",
    'git diff "$BASE"...HEAD': "增量变异的前置 diff：BASE 取不到时降级为空 diff，由后续 mutation 步骤自行失败",
    "cargo build --workspace --keep-going": "CodeQL 前的取型构建，真实检查由 CodeQL 分析承担（--keep-going 已尽量多编译）",
    "packages/ || true": "semgrep 规则集首次拉取/网络失败不应阻断 CI，扫描结果仍以实际报告为准",
}

# 只接入 CI、未接入本地门禁脚本的检查器：必须在此登记理由，否则视为漏接。
# 判据来自 check_required_scripts() 的登记源归属（服务器门禁 / gate.ps1 / CI 工作流）。
CI_ONLY_EXEMPT = {
    "check-csharp-abi-surface.py": "依赖 .NET/PowerShell 绑定源码树，本地门禁脚本无该运行时；CI 的 csharp-abi job 每次 PR 阻断执行",
}


def _logical_lines(lines):
    """合并反斜杠续行，使跨行的 `if A \\\n|| B; then mark N PASS` 也能被规则命中。"""
    merged = []
    buf = ""
    for raw in lines:
        stripped = raw.rstrip()
        if stripped.endswith("\\"):
            buf += stripped[:-1] + " "
            continue
        merged.append((buf + stripped).strip())
        buf = ""
    if buf:
        merged.append(buf.strip())
    return merged


def check_shell_gate():
    """检查 run-gates-v10120-server.sh。"""
    problems = []
    if not SERVER_GATE.exists():
        return ["缺失 run-gates-v10120-server.sh"]
    lines = SERVER_GATE.read_text(encoding="utf-8", errors="replace").splitlines()
    for i, stripped in enumerate(_logical_lines(lines), 1):
        # 1) 硬编码 PASS：行本身即 `mark N PASS`，且不是 if/then 分支体
        m = re.match(r"mark\s+(\d+)\s+PASS\s*(?:#.*)?$", stripped)
        if m:
            problems.append(
                f"{SERVER_GATE.name}:{i} 硬编码 `mark {m.group(1)} PASS`（无条件放行）"
            )
            continue
        # 2) 假 PASS：if A || B; then mark N PASS（任一成功即放行）
        if re.search(r"if\s+.*\|\|.*;\s*then\s+mark\s+\d+\s+PASS", stripped):
            problems.append(f"{SERVER_GATE.name}:{i} 使用 `||` 连接检查（任一成功即 PASS）")
        # 3) 恒真条件：`if true / if [ true ] / if :; then mark N PASS`
        if re.search(r"if\s+(?:true|\[\s*true\s*\]|:)\s*;\s*then\s+mark\s+\d+\s+PASS", stripped):
            problems.append(f"{SERVER_GATE.name}:{i} 恒真条件直接 mark PASS（未执行任何检查）")
        # 4) 顺序执行后无条件 PASS：`some_cmd; mark N PASS`（既非 if 也非 then 分支）
        if re.match(r"^(?!if\b|elif\b|then\b)[^;]+;\s*mark\s+\d+\s+PASS", stripped) and " then " not in stripped:
            problems.append(f"{SERVER_GATE.name}:{i} 命令与 `mark N PASS` 同链顺序执行（命令失败也 PASS）")
    return problems


def check_ps_gate():
    """检查 gate.ps1（PowerShell 版）。

    gate.ps1 的关卡模式是 `$ok = Invoke-Step ...` + `if (-not $ok) { exit N }`；
    只检查前者有后者才计入 PASS。缺少结果检查的关卡等于无条件放行。
    """
    problems = []
    if not PS_GATE.exists():
        return ["缺失 gate.ps1"]
    lines = PS_GATE.read_text(encoding="utf-8", errors="replace").splitlines()
    step_idx = [i for i, l in enumerate(lines) if l.strip().startswith("$ok = Invoke-Step")]
    for pos, i in enumerate(step_idx):
        # 本关卡的判定窗口：到下一个 Invoke-Step（或文件末尾）为止
        end = step_idx[pos + 1] if pos + 1 < len(step_idx) else len(lines)
        window = "\n".join(lines[i + 1:end])
        if not re.search(r"if\s*\(\s*(?:-not\s+)?\$ok", window):
            problems.append(f"{PS_GATE.name}:{i + 1} 关卡结果 $ok 未被检查（失败不会阻断）")
    for i, line in enumerate(lines, 1):
        stripped = line.strip()
        # 防御性：检测类似 `Write-Host "... PASS"` 而无检查条件的情况（启发式）
        if re.search(r'Write-Host\s+.*PASS', stripped) and "if " not in stripped:
            problems.append(f"{PS_GATE.name}:{i} 疑似无条件输出 PASS")
    return problems


def _disp(path):
    """仓库内显示相对路径，仓库外（如自测夹具的临时目录）显示绝对路径。"""
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return str(path)


def workflow_files():
    """列出 `.github/workflows/*.yml|yaml`（排序保证输出稳定）。"""
    if not WORKFLOWS.exists():
        return []
    return sorted(p for p in WORKFLOWS.iterdir()
                  if p.is_file() and p.suffix in (".yml", ".yaml"))


def check_ci_gate():
    """检查 CI 工作流里的 `|| true` / `|| :`（吞掉失败 = 假 PASS）。

    与门禁 31（check-ci-blocking.py）分工：那道管 `continue-on-error`，
    本道管命令行内把退出码改写成 0 的写法——两者都是"失败不阻断"，但机制不同。
    fail-closed：workflows 目录读不到时直接红牌，不允许"没有文件所以通过"。
    """
    if not WORKFLOWS.exists():
        return [f"缺失 CI 工作流目录 {_disp(WORKFLOWS)}（无法审计，按失败处理）"]
    files = workflow_files()
    if not files:
        return [f"{_disp(WORKFLOWS)} 下没有任何 .yml/.yaml（无法审计，按失败处理）"]
    problems = []
    for p in files:
        text = p.read_text(encoding="utf-8", errors="replace")
        # 不合并续行：YAML 块里的行号要能直接 goto，报告引用才有意义。
        for i, line in enumerate((l.strip() for l in text.splitlines()), 1):
            if not re.search(r"\|\|\s*(?:true\b|:(?=$|\s|[;&|)]))", line):
                continue
            if any(frag in line for frag in CI_FAIL_EXEMPT):
                continue
            problems.append(
                f"{_disp(p)}:{i} 用 `|| true`/`|| :` 吞掉失败且未登记豁免: {line[:120]}"
            )
    return problems


def check_required_scripts():
    """检查关键安全辅助脚本是否被门禁脚本引用，并校验登记源归属。

    登记源共三处：服务器门禁脚本、gate.ps1、`.github/workflows/*.yml`。
    只出现在 CI 的检查器必须登记在 CI_ONLY_EXEMPT（写明"为什么本地门禁不跑"），
    否则等于"本地 34 关全绿但检查器从未执行"的盲区。
    返回 (未接入清单, 归属问题清单)。
    """
    sources = {"run-gates-v10120-server.sh": SERVER_GATE, "gate.ps1": PS_GATE}
    for p in workflow_files():
        sources[p.name] = p
    texts = {
        name: p.read_text(encoding="utf-8", errors="replace")
        for name, p in sources.items() if p.exists()
    }
    missing = []
    ownership = []
    for script in REQUIRED_CHECKS:
        if script == "check-gate-scripts.py":
            continue  # 自引用豁免
        hit = sorted(n for n, t in texts.items() if script in t)
        local = [n for n in hit if n in ("run-gates-v10120-server.sh", "gate.ps1")]
        if not hit:
            missing.append(script)
        elif not local:
            if script not in CI_ONLY_EXEMPT:
                ownership.append(
                    f"{script} 仅接入 {', '.join(hit)}（未接入本地门禁且未登记 CI_ONLY_EXEMPT 理由）"
                )
    return missing, ownership


def main():
    print("=== 门禁：门禁脚本自检（audit the checkers）===")
    problems = []
    problems += check_shell_gate()
    problems += check_ps_gate()
    problems += check_ci_gate()
    missing, ownership = check_required_scripts()
    if missing:
        problems.append("关键安全辅助脚本未接入任何门禁脚本: " + ", ".join(missing))
    problems += ownership

    if problems:
        print("  ❌ 发现以下问题：")
        for p in problems:
            print(f"    - {p}")
        print("  ❌ FAIL")
        return 1
    print("  ✅ PASS：门禁脚本无硬编码 PASS、无 `||` 假 PASS，CI 无未登记的 `|| true`，关键安全脚本均已接入")
    print(f"  记账：CI `|| true` 豁免 {len(CI_FAIL_EXEMPT)} 条 / 仅 CI 接入的检查器 {len(CI_ONLY_EXEMPT)} 条 "
          f"（理由见脚本内登记表，登记即理由，未登记即红牌）")
    return 0


if __name__ == "__main__":
    sys.exit(main())