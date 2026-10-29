<#
.SYNOPSIS
    SZ-ORM 集成层强制门禁脚本（Windows PowerShell 版）

.DESCRIPTION
    在 push / 合并 / 部署前强制执行全项目集成验证。
    任何一步失败立即停止并返回非零退出码。

    包含关卡 1~13 与 24~34（编号与 scripts/run-gates-v10120-server.sh 一致）：
      1. cargo fmt --check        格式检查
      2. cargo check --workspace  全项目编译检查（含 all-features）
      3. cargo clippy             严格模式（-D warnings）
      4. cargo test --workspace   全项目测试
      5. cargo doc                文档构建（捕获断裂的 doc 链接）
      6. API 变更扫描             检测公共 API 签名变化（如果有的话）
      7. 契约测试                 单独跑契约测试套件
      8. 禁止占位实现检查         扫描 todo!/unimplemented!/unreachable! + unsafe 审计
      9. SQL 注入扫描             扫描 SQL 拼接模式 + 凭据扫描 + 扫描器自测
      10. Feature 全组合编译      cargo check --all-features
      11. ADR-0001 上游未修改检查  核心包修改必须附带文档更新
      12. 文档一致性检查          验证文档数据与实际代码一致
      13. 审计证据验证            验证审计报告中 file:line 证据真实性
      24. 示例清单同步检查        README 示例表格 ↔ examples/src/bin 双向一致
      25. 文档代码可编译性检查    文档示例引用的 API/宏必须真实存在
      26. CI 与 deny.toml ignore 一致性
      27. 门禁脚本自检
      28. 安全文档版本一致性
      29. 幻影测试引用检测
      30. 幻影测试扫描            永真断言/字面量断言/零断言函数
      31. CI 阻断语义审计         continue-on-error job 必须登记豁免表
      32. 文件名编码合法性
      33. 文档版本声明一致性
      34. 代码生成器产物编译验证

.EXAMPLE
    ./scripts/gate.ps1
    ./scripts/gate.ps1 -SkipTests   # 跳过测试（紧急修复时用）
    ./scripts/gate.ps1 -Fast        # 只跑前 3 关（最快验证）

.NOTES
    门禁失败时返回非零退出码，pre-push hook 会阻止推送。
#>

[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$Fast
)

$ErrorActionPreference = "Stop"

# 切换到项目根（scripts/ 的父目录）
# 注：$PSScriptRoot 在某些 PowerShell 调用方式下可能为空，使用 $MyInvocation 作为后备
$ScriptDir = $PSScriptRoot
if (-not $ScriptDir) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$ProjectRoot = Split-Path -Parent $ScriptDir
if (-not $ProjectRoot) {
    $ProjectRoot = (Get-Location).Path
}
Set-Location $ProjectRoot

$StartTime = Get-Date
$FailedStep = $null
$StepCount = 0

function Write-Step($name) {
    $script:StepCount++
    $line = "[$($script:StepCount)] $name"
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host $line -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
}

function Write-Ok($msg) {
    Write-Host "[OK] $msg" -ForegroundColor Green
}

function Write-Fail($msg) {
    Write-Host "[FAIL] $msg" -ForegroundColor Red
    $script:FailedStep = $msg
}

function Invoke-Step($name, $scriptBlock) {
    Write-Step $name
    $stepStart = Get-Date
    try {
        & $scriptBlock
        if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne $null) {
            Write-Fail "$name 失败 (exit=$LASTEXITCODE)"
            return $false
        }
        $elapsed = ((Get-Date) - $stepStart).TotalSeconds
        Write-Ok "$name 通过 (${elapsed}s)"
        return $true
    } catch {
        Write-Fail "$name 异常: $_"
        return $false
    }
}

# ============================================================================
# 关卡 1: 格式检查
# ============================================================================
# 注意：一次性 `cargo fmt --all -- --check` 在 70+ 包 workspace 下会触发
# Windows 命令行长度上限（os error 206 文件名或扩展名太长），必须分包执行
# （2026-09-14 修复，与门禁 G1 的验证方式一致）
$ok = Invoke-Step "格式检查 (cargo fmt --check, 分包)" {
    $fail = 0
    # 不用 cargo metadata + ConvertFrom-Json（PS 5.1 对 315KB JSON 解析失败），
    # 直接从 workspace members 读路径，cargo fmt --manifest-path 逐包检查
    # （一次性 --all 在 70+ 包下触发 os error 206 命令行超长）
    $toml = Get-Content (Join-Path $PSScriptRoot "..\Cargo.toml") -Raw
    if ($toml -notmatch '(?s)members\s*=\s*\[([^\]]*)\]') { throw "Cargo.toml members 解析失败" }
    $members = [regex]::Matches($Matches[1], '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
    $root = Resolve-Path (Join-Path $PSScriptRoot "..")
    foreach ($m in $members) {
        $manifest = Join-Path $root "$m\Cargo.toml"
        if (-not (Test-Path $manifest)) { continue }
        cargo fmt --manifest-path $manifest -- --check 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Host "  FMT_FAIL: $m"
            $fail = 1
        }
    }
    if ($fail -ne 0) { exit 1 }
}
if (-not $ok) {
    Write-Host ""
    Write-Host "提示: 运行 'cargo fmt --all' 自动修复格式（或按上方 FMT_FAIL 分包修复）" -ForegroundColor Yellow
    exit 1
}

# ============================================================================
# 关卡 2: 全项目编译检查（含 all-features）
# ============================================================================
$ok = Invoke-Step "全项目编译检查 (cargo check --workspace --all-features)" {
    cargo check --workspace --all-features --all-targets
}
if (-not $ok) { exit 2 }

# ============================================================================
# 关卡 3: clippy 严格模式（零警告）
# ============================================================================
$ok = Invoke-Step "Clippy 严格检查 (-- -D warnings)" {
    cargo clippy --workspace --all-targets --all-features -- -D warnings
}
if (-not $ok) {
    Write-Host ""
    Write-Host "提示: 运行 'cargo clippy --fix --workspace --all-targets --all-features' 自动修复" -ForegroundColor Yellow
    exit 3
}

if ($Fast) {
    Write-Host ""
    Write-Host "[Fast 模式] 跳过后续测试/文档/契约关卡" -ForegroundColor Yellow
    $TotalElapsed = ((Get-Date) - $StartTime).TotalSeconds
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "  门禁通过（Fast 模式，3 关）— ${TotalElapsed}s" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
    exit 0
}

# ============================================================================
# 关卡 4: 全项目测试
# ============================================================================
if (-not $SkipTests) {
    $ok = Invoke-Step "全项目测试 (cargo test --workspace)" {
        cargo test --workspace
    }
    if (-not $ok) { exit 4 }
}

# ============================================================================
# 关卡 5: 文档构建（捕获断裂的 doc 链接）
# ============================================================================
$ok = Invoke-Step "文档构建 (cargo doc)" {
    $env:RUSTDOCFLAGS = "-D warnings"
    cargo doc --workspace --no-deps --all-features
}
if (-not $ok) { exit 5 }

# ============================================================================
# 关卡 6: API 变更扫描（如果存在 audit 脚本且有 git diff）
# ============================================================================
if (Test-Path "scripts/audit-api-changes.ps1") {
    Write-Step "API 变更扫描"
    $auditStart = Get-Date
    try {
        & "$PSScriptRoot/audit-api-changes.ps1" -Quiet
        $elapsed = ((Get-Date) - $auditStart).TotalSeconds
        Write-Ok "API 变更扫描通过 (${elapsed}s)"
    } catch {
        Write-Fail "API 变更扫描失败: $_"
        exit 6
    }
}

# ============================================================================
# 关卡 7: 契约测试套件
# ============================================================================
if (-not $SkipTests) {
    $ok = Invoke-Step "契约测试 (cargo test --test contracts)" {
        cargo test -p sz-orm-core --test contracts
    }
    if (-not $ok) { exit 7 }
}

# ============================================================================
# 关卡 8: 禁止占位实现检查 + unsafe 合规审计
# ============================================================================
# v10.13.0 (2026-10-27): 并入 check-unsafe-audit.py（L-3 双口径 SAFETY 审计）
$ok = Invoke-Step "禁止占位实现检查" {
    $matches = Select-String -Path (Get-ChildItem -Recurse "*.rs" -Exclude "*target*").FullName -Pattern '\b(todo!|unimplemented!|unreachable!)\b'
    if ($matches) {
        Write-Host "发现 $($matches.Count) 处占位实现：" -ForegroundColor Red
        $matches | ForEach-Object { Write-Host "  $($_.Path):$($_.LineNumber) — $($_.Line.Trim())" }
        exit 8
    }
}
if ($ok) {
    $ok = Invoke-Step "unsafe 合规审计 (check-unsafe-audit.py)" {
        python "$PSScriptRoot/check-unsafe-audit.py"
    }
}
if (-not $ok) { exit 8 }

# ============================================================================
# 关卡 9: SQL 注入扫描 + secrets 扫描 + G9 扫描器自测
# ============================================================================
# v8.5.0: 自动化审查（check-sql-injection-auto.py）替代手动审查
# v10.13.0 (2026-10-27): 关卡 9 并入 check-secrets.py 凭据扫描（H-3 整改）与
#   test_check_sql_injection_auto.py 扫描器自测（M-3 教训：自测必须接入门禁），全部通过才 PASS
$ok = Invoke-Step "SQL 注入自动化审查 (check-sql-injection-auto.py)" {
    python "$PSScriptRoot/check-sql-injection-auto.py" --workspace "$PSScriptRoot/.." --strict
}
if ($ok) {
    $ok = Invoke-Step "G9 扫描器自测 (test_check_sql_injection_auto.py)" {
        python "$PSScriptRoot/test_check_sql_injection_auto.py"
    }
}
if ($ok) {
    $ok = Invoke-Step "凭据泄露扫描 (check-secrets.py)" {
        python "$PSScriptRoot/check-secrets.py"
    }
}
if (-not $ok) { exit 9 }

# ============================================================================
# 关卡 10: Feature 全组合编译
# ============================================================================
$ok = Invoke-Step "Feature 全组合编译 (cargo check --all-features)" {
    cargo check --workspace --all-targets --all-features
}
if (-not $ok) { exit 10 }

# ============================================================================
# 关卡 11: ADR-0001 上游未修改检查
# ============================================================================
$ok = Invoke-Step "ADR-0001 上游未修改检查" {
    & "$PSScriptRoot/check-upstream-unmodified.ps1"
}
if (-not $ok) { exit 11 }

# ============================================================================
# 关卡 12: 文档一致性检查
# ============================================================================
$ok = Invoke-Step "文档一致性检查" {
    python "$PSScriptRoot/check-doc-consistency.py"
}
if (-not $ok) { exit 12 }

# ============================================================================
# 关卡 13: 审计证据验证（如果存在审计报告）
# ============================================================================
$auditReports = Get-ChildItem -Recurse "docs/audit/*.md" -ErrorAction SilentlyContinue
if ($auditReports) {
    $latestReport = $auditReports | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($latestReport) {
        $ok = Invoke-Step "审计证据验证 ($($latestReport.Name))" {
            & "$PSScriptRoot/audit-verify.ps1" $latestReport.FullName
        }
        if (-not $ok) { exit 13 }
    }
}

# ============================================================================
# 关卡 24: 示例清单同步（README Examples 表格 ↔ examples/src/bin 双向一致）
# ============================================================================
$ok = Invoke-Step "示例清单同步检查 (check-examples-readme.py)" {
    python "$PSScriptRoot/check-examples-readme.py"
}
if (-not $ok) { exit 24 }

# ============================================================================
# 关卡 25: 文档代码可编译性（文档示例引用的 API/宏必须真实存在）
# ============================================================================
$ok = Invoke-Step "文档代码可编译性检查 (check-doc-code-compiles.py)" {
    python "$PSScriptRoot/check-doc-code-compiles.py"
}
if (-not $ok) { exit 25 }

# ============================================================================
# 关卡 26: CI 与 deny.toml ignore 一致性（v10.13.0 / M-1 防复发）
# ============================================================================
$ok = Invoke-Step "CI 与 deny.toml ignore 一致性 (check-ci-deny-sync.py)" {
    python "$PSScriptRoot/check-ci-deny-sync.py"
}
if (-not $ok) { exit 26 }

# ============================================================================
# 关卡 27: 门禁脚本自检（v10.13.0 / M-4 防复发）
# ============================================================================
$ok = Invoke-Step "门禁脚本自检 (check-gate-scripts.py)" {
    python "$PSScriptRoot/check-gate-scripts.py"
}
if (-not $ok) { exit 27 }

# ============================================================================
# 关卡 28: 安全文档版本一致性（v10.13.0 / M-5 防复发）
# ============================================================================
$ok = Invoke-Step "安全文档版本一致性 (check-security-doc-version.py)" {
    python "$PSScriptRoot/check-security-doc-version.py"
}
if (-not $ok) { exit 28 }

# ============================================================================
# 关卡 29: 幻影测试引用检测（v10.13.0 / H-2 防复发，警告级）
# ============================================================================
$ok = Invoke-Step "幻影测试引用检测 (check-test-symbols.py)" {
    python "$PSScriptRoot/check-test-symbols.py"
}
if (-not $ok) { exit 29 }

# ============================================================================
# 关卡 30: 幻影测试扫描（永真断言 / 字面量断言 / 零断言函数 / 差分命名）
# ============================================================================
$ok = Invoke-Step "幻影测试扫描 (check-phantom-tests.py)" {
    python "$PSScriptRoot/check-phantom-tests.py"
}
if (-not $ok) { exit 30 }

# ============================================================================
# 关卡 31: CI 阻断语义审计（continue-on-error job 必须登记豁免表）
# ============================================================================
$ok = Invoke-Step "CI 阻断语义审计 (check-ci-blocking.py)" {
    python "$PSScriptRoot/check-ci-blocking.py"
}
if (-not $ok) { exit 31 }

# ============================================================================
# 关卡 32: 文件名编码合法性（2026-10-28 编号统一：此前与关卡 26 重号）
# ============================================================================
$ok = Invoke-Step "文件名编码合法性 (check-filename-encoding.py)" {
    python "$PSScriptRoot/check-filename-encoding.py"
}
if (-not $ok) { exit 32 }

# ============================================================================
# 关卡 33: 文档版本声明一致性（2026-10-28 编号统一：此前与关卡 27 重号）
# ============================================================================
$ok = Invoke-Step "文档版本声明一致性 (check-doc-version-consistency.py)" {
    python "$PSScriptRoot/check-doc-version-consistency.py"
}
if (-not $ok) { exit 33 }

# ============================================================================
# 关卡 34: 代码生成器产物编译验证（2026-10-28 编号统一：此前与关卡 28 重号）
# ============================================================================
$ok = Invoke-Step "代码生成器产物编译验证 (check-generated-code-compiles.py)" {
    python "$PSScriptRoot/check-generated-code-compiles.py"
}
if (-not $ok) { exit 34 }

# ============================================================================
# 2026-10-28 P2-3：补齐本地缺失的 G14-G19/G21/G23（V3-24 遗留——
# 此前 gate.ps1 只有 26 步，本地全绿不代表服务器 34 道全绿）
# ============================================================================
$ok = Invoke-Step "G14 文档同步更新检查 (check-doc-sync.py --diff HEAD)" {
    python "$PSScriptRoot/check-doc-sync.py" --diff HEAD
}
if (-not $ok) { exit 14 }

$ok = Invoke-Step "G15 幻影交付检查 (check-phantom-delivery.py)" {
    python "$PSScriptRoot/check-phantom-delivery.py"
}
if (-not $ok) { exit 15 }

$ok = Invoke-Step "G16 语义反模式扫描 (check-semantic-patterns.py)" {
    python "$PSScriptRoot/check-semantic-patterns.py"
}
if (-not $ok) { exit 16 }

$ok = Invoke-Step "G17 架构一致性扫描 (check-architecture.py)" {
    python "$PSScriptRoot/check-architecture.py"
}
if (-not $ok) { exit 17 }

$ok = Invoke-Step "G18 度量真实性扫描 (check-metrics-real.py)" {
    python "$PSScriptRoot/check-metrics-real.py"
}
if (-not $ok) { exit 18 }

$ok = Invoke-Step "G19 发布一致性扫描 (check-publish-consistency.py)" {
    python "$PSScriptRoot/check-publish-consistency.py"
}
if (-not $ok) { exit 19 }

$ok = Invoke-Step "G21 安全攻击测试 (auth security_attacks + crypto kat)" {
    cargo test -p sz-orm-auth --test security_attacks
    if ($LASTEXITCODE -ne 0) { return $false }
    cargo test -p sz-orm-crypto --test kat
    if ($LASTEXITCODE -ne 0) { return $false }
    cargo test -p sz-orm-core --features multi-tenant-enhanced --test security_attacks
    return ($LASTEXITCODE -eq 0)
}
if (-not $ok) { exit 21 }

$ok = Invoke-Step "G23 未用依赖扫描 (check-unused-deps.py)" {
    python "$PSScriptRoot/check-unused-deps.py"
}
if (-not $ok) { exit 23 }

# ============================================================================
# 汇总
# ============================================================================
$TotalElapsed = ((Get-Date) - $StartTime).TotalSeconds

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  门禁全部通过 ($StepCount 关)" -ForegroundColor Green
Write-Host "  总耗时: ${TotalElapsed}s" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green

exit 0
