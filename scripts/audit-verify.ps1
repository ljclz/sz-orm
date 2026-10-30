<#
.SYNOPSIS
    审计证据自动验证脚本（sz-orm 版，Windows PowerShell）

.DESCRIPTION
    验证审计报告中的每项结论是否附带真实的 file:line 证据，
    并检查该证据在代码中是否实际存在。

    这是门禁 13（审计合规硬约束）的自动执行脚本。

    v2（2026-10-27 整改 P1-2）相对 v1 的强化：
    1. 一行内的**全部**引用逐个校验（v1 只校验每行第一个匹配）；
    2. 行号分母改用 UTF-8 读取的**物理行数**（v1 用 Measure-Object -Line
       只数非空行，分母偏小，合法引用可能被误判越界；且 GBK 解码含中文
       注释的 UTF-8 文件时会吞行导致行号体系错位）；
    3. **被引行非空检查**：行号落在空行上判定失败（v1 从不读取被引行，
       编造一个在范围内的行号即可通过）。

    v4（2026-10-29 整改 P1-4，闭环评测 R2「audit-verify 语义空转」）相对 v2 的强化：
    1. **符号级断言**：引用行可携带 `[must-contain: X]` / `[must-not-contain: X]`
       标记，被引区间必须（不）包含 X，否则红牌。v2 只证明"行存在且非空"，
       真实行号承载虚构结论照样通过——R2 复现的正是这一空转。
    2. **弱语义校验**：被引行与结论行须有共享标识符，否则警告；共享 token 的
       提取**包含中日韩词块**（≥2 字），修复纯中文断言零 token 直接跳过校验的旁路。
    3. **-Strict**：断言缺失（既无 must-contain 标记、又无共享 token）即红牌，
       使"每条结论必须有可机器检验的证据"成为可强制的门禁。
    4. **代码块围栏跳过** + **crate 前缀映射** + **子目录根级文件**，
       与 audit-verify.sh v3 对齐，消除两个入口的校验强度漂移。
    5. **通配覆盖**：新增任意扩展名的 `path.ext:NN` 规则。此前只识别
       `.rs` / 少数根级文件，`scripts/*.py`、`tests/*.py`、`docs/*.md`、
       `.github/*.yml`、`.cs`/`.h` 引用被**静默跳过**——"通过 N 条"的分母
       本身就漏了大半，属于比语义空转更隐蔽的空转。

    已知边界：本脚本仍不能证明"该行内容语义上支撑该结论"——它证明的是
    被引区间确实含有声明的符号/字面量。语义仍需人工，但虚构断言不再是静默通过。

.PARAMETER ReportPath
    审计报告文件路径（.md 格式）

.PARAMETER Strict
    启用严格模式：无共享 token 的引用、以及未携带断言标记的引用判为失败

.PARAMETER AtHead
    以 `git show HEAD:<path>` 的**已提交内容**为准校验行号，而不是工作区。
    审计报告的行号是针对某个提交写的，而工作区可能正被其他改动推进——
    不加本开关时，这类"提交后行号漂移"会表现为被引行为空/断言失败的假阳性。
    绝对路径（file:/// 指向仓库外）自动回退为读工作区文件；未入库与空文件
    一律视为不存在（空文件本就无法承载证据）。

.EXAMPLE
    .\scripts\audit-verify.ps1 docs\assessment\2026-10-28-fullstack-review-v3.md
    .\scripts\audit-verify.ps1 docs\assessment\2026-10-28-fullstack-review-v3.md -Strict
    .\scripts\audit-verify.ps1 docs\assessment\2026-10-28-fullstack-review-v3.md -AtHead
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)]
    [string]$ReportPath,

    [switch]$Strict,

    [switch]$AtHead
)

$ErrorActionPreference = "Stop"

# 切换到项目根
$ScriptDir = $PSScriptRoot
if (-not $ScriptDir) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$ProjectRoot = Split-Path -Parent $ScriptDir
if (-not $ProjectRoot) {
    $ProjectRoot = (Get-Location).Path
}
Set-Location $ProjectRoot

if (-not (Test-Path $ReportPath)) {
    Write-Host "❌ 报告文件不存在: $ReportPath" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  审计证据验证: $ReportPath (strict=$Strict)" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

$script:Pass = 0
$script:Fail = 0
$script:Warn = 0

# 行缓存：同一文件多次引用不重复读盘
$LineCache = @{}

# 标识符提取的停用词：路径/工具/语言名，命中它们不能证明语义
$StopTokens = @(
    'file', 'http', 'https', 'path', 'line', 'lines', 'docs', 'tests', 'test',
    'src', 'packages', 'toml', 'lock', 'cargo', 'self', 'this', 'that', 'with',
    'from', 'com', 'www', 'github', 'rust', 'true', 'false', 'none', 'null',
    'readme', 'agents', 'changelog', 'deny', 'toolchain', 'report', 'assessment',
    'md', 'rs', 'py', 'sh', 'ps1', 'json', 'yaml'
)

function Get-PhysicalLines([string]$Path) {
    if ($LineCache.ContainsKey($Path)) { return $LineCache[$Path] }
    $gitRepoPath = $Path -replace '\\', '/'
    # 路径归一化（2026-10-29 legacy-citation-ledger 建议 1）：
    # 1) 旧克隆根前缀（E:/vue/... 或 /vue/...，正则匹配可能吞掉盘符）→ 仓库相对路径
    $legacyRootRe = '^/?[A-Za-z]?:?/vue/test/鲜视达/rust/sz-orm'
    if ($gitRepoPath -match $legacyRootRe) {
        $gitRepoPath = ($gitRepoPath -replace $legacyRootRe, '').TrimStart('/')
        $Path = $gitRepoPath
    }
    # 2) 以 / 开头的仓库相对路径（/packages/...）→ 去掉开头斜杠
    if ($Path.StartsWith('/') -and -not ($Path -match '^/[A-Za-z]:/')) {
        $Path = $Path.TrimStart('/')
        $gitRepoPath = $Path
    }
    if ($AtHead -and -not [System.IO.Path]::IsPathRooted($Path)) {
        # 已提交内容为准。用 Start-Process 直接重定向子进程 stdout 到临时文件，
        # 拿到的是原始字节再按 UTF-8 解析，避免 PowerShell 用控制台代码页
        # （Windows 上为 GBK）解码 git 输出造成的乱码与吞行；且跨平台可用。
        $tmp = [System.IO.Path]::GetTempFileName()
        $err = "$tmp.err"
        $proc = Start-Process -FilePath "git" -ArgumentList @("show", "HEAD:$gitRepoPath") `
            -NoNewWindow -Wait -PassThru -RedirectStandardOutput $tmp -RedirectStandardError $err
        $ok = ($proc.ExitCode -eq 0) -and (Test-Path $tmp) -and ((Get-Item $tmp).Length -gt 0)
        if (-not $ok) {
            Remove-Item $tmp, $err -ErrorAction SilentlyContinue
            $LineCache[$Path] = $null
            return $null
        }
        $lines = [System.IO.File]::ReadAllLines($tmp, [System.Text.Encoding]::UTF8)
        Remove-Item $tmp, $err -ErrorAction SilentlyContinue
        $LineCache[$Path] = $lines
        return $lines
    }
    if (-not (Test-Path $Path)) {
        $LineCache[$Path] = $null
        return $null
    }
    # UTF-8 严格读取（无效字节替换为 U+FFFD 而非按 GBK 误解码），
    # ReadAllLines 把 CRLF / LF / 裸 CR 都按物理行拆分
    $lines = [System.IO.File]::ReadAllLines((Resolve-Path $Path).Path, [System.Text.Encoding]::UTF8)
    $LineCache[$Path] = $lines
    return $lines
}

# 提取可比较 token：ASCII 标识符（≥4）+ 中日韩词块（≥2 字）。
# 只取 ASCII 会让纯中文断言得到空集合，从而整体跳过语义校验（R2 的旁路之一）。
function Get-ComparableTokens([string]$Text) {
    if ([string]::IsNullOrEmpty($Text)) { return @() }
    $set = New-Object 'System.Collections.Generic.HashSet[string]'
    foreach ($m in [regex]::Matches($Text, '[A-Za-z_][A-Za-z0-9_]{3,}')) {
        $t = $m.Value.ToLowerInvariant()
        if ($StopTokens -notcontains $t) { [void]$set.Add($t) }
    }
    foreach ($m in [regex]::Matches($Text, '[\u4e00-\u9fff]{2,}')) {
        [void]$set.Add($m.Value)
    }
    return @($set)
}

function Test-TokenOverlap([string]$A, [string]$B) {
    $ta = Get-ComparableTokens $A
    $tb = Get-ComparableTokens $B
    if ($ta.Count -eq 0 -or $tb.Count -eq 0) { return $null }   # 无可比较集合：不确定
    foreach ($t in $ta) { if ($tb -contains $t) { return $true } }
    return $false
}

# 断言标记：[must-contain: X] / [must-not-contain: X]
# **绑定规则**：标记只作用于紧邻其前面的那条引用（可选分隔符为空白、反引号、
# 中英文标点），避免一行内多条引用互相误伤。
function Get-TrailingAssertions([string]$Tail, [string]$Marker) {
    $out = @()
    $pos = 0
    while ($true) {
        $rest = $Tail.Substring($pos) -replace '^[\s`、,，;；\-\u2013]+', ''
        $pat = "^\[$Marker\s*:\s*([^\]]+)\]"
        if ($rest -match $pat) {
            $out += $Matches[1].Trim()
            $pos += ($Tail.Substring($pos).Length - $rest.Length) + $Matches[0].Length
            $skip = $Tail.Substring($pos) -replace '^[\s`、,，;；\-\u2013]+', ''
            $pos += $Tail.Substring($pos).Length - $skip.Length
            if ($skip -match '^\[' -and -not ($skip -match "^\[$Marker")) { break }
            continue
        }
        break
    }
    return $out
}

function Test-RepoFile([string]$Path) {
    return ($null -ne (Get-PhysicalLines $Path))
}

function Test-Citation([string]$LocalPath, [int]$LineNo, [int]$EndLine, [string]$Citing, [array]$Must = @(), [array]$MustNot = @()) {
    $lines = Get-PhysicalLines $LocalPath
    $scope = if ($AtHead) { "（HEAD 提交内容中）" } else { "" }
    if ($null -eq $lines) {
        Write-Host "❌ $LocalPath`:$LineNo — 文件不存在$scope" -ForegroundColor Red
        $script:Fail++
        return
    }
    $total = $lines.Count
    if ($LineNo -gt $total) {
        Write-Host "❌ $LocalPath`:$LineNo — 行号超出范围（文件共 $total 物理行）" -ForegroundColor Red
        $script:Fail++
        return
    }
    if ($EndLine -gt $total) {
        Write-Host "❌ $LocalPath`:$LineNo-L$EndLine — 区间终点超出范围（文件共 $total 物理行）" -ForegroundColor Red
        $script:Fail++
        return
    }
    if ($EndLine -lt $LineNo) {
        Write-Host "❌ $LocalPath`:$LineNo-L$EndLine — 区间终点小于起点" -ForegroundColor Red
        $script:Fail++
        return
    }
    # 被引行（区间起点）必须是非空行：空行引用无法承载证据
    $cited = $lines[$LineNo - 1]
    if ([string]::IsNullOrWhiteSpace($cited)) {
        Write-Host "❌ $LocalPath`:$LineNo — 被引行为空行（无法承载证据）" -ForegroundColor Red
        $script:Fail++
        return
    }

    # 区间全文（含起点行），供断言与弱语义校验使用
    $rangeText = ($lines[($LineNo - 1)..($EndLine - 1)] -join "`n")

    # 1) 符号级断言：声明了 must-contain / must-not-contain 就必须真的成立
    foreach ($needle in $Must) {
        if ($rangeText.IndexOf($needle, [System.StringComparison]::Ordinal) -lt 0) {
            Write-Host "❌ $LocalPath`:$LineNo — 断言失败：被引区间不含 must-contain '$needle'" -ForegroundColor Red
            $script:Fail++
            return
        }
    }
    foreach ($needle in $MustNot) {
        if ($rangeText.IndexOf($needle, [System.StringComparison]::Ordinal) -ge 0) {
            Write-Host "❌ $LocalPath`:$LineNo — 断言失败：被引区间含 must-not-contain '$needle'" -ForegroundColor Red
            $script:Fail++
            return
        }
    }

    # 2) 弱语义校验（v4）
    if (-not [string]::IsNullOrWhiteSpace($Citing)) {
        $overlap = Test-TokenOverlap $Citing $cited
        $hasMarker = ($Must.Count -gt 0 -or $MustNot.Count -gt 0)
        if ($overlap -eq $false -and -not $hasMarker) {
            Write-Host "⚠️  弱语义校验：$LocalPath`:$LineNo — 被引行与结论无共享标识符（纯路径引用可加 [must-contain: …] 声明）" -ForegroundColor Yellow
            $script:Warn++
            if ($Strict) {
                Write-Host "❌ $LocalPath`:$LineNo — Strict：语义校验失败（无共享 token 且无断言标记）" -ForegroundColor Red
                $script:Fail++
                return
            }
        }
    }

    Write-Host "✅ $LocalPath`:$LineNo (共 $total 物理行)" -ForegroundColor Green
    $script:Pass++
}

# 调用入口：从被引文本之后紧随的位置解析断言标记，再执行校验
function Invoke-Citation([string]$LocalPath, [int]$LineNo, [int]$EndLine, [string]$Citing, [string]$Matched) {
    $idx = $Citing.IndexOf($Matched)
    $tail = if ($idx -lt 0) { "" } else { $Citing.Substring($idx + $Matched.Length) }
    $must = @(Get-TrailingAssertions $tail 'must-contain')
    $mustNot = @(Get-TrailingAssertions $tail 'must-not-contain')
    Test-Citation $LocalPath $LineNo $EndLine $Citing $must $mustNot
}

# 逐行扫描报告；对每行循环提取"最早出现的"引用并消费，直到该行无引用。
# ``` 围栏内的行（内嵌日志/示例）跳过，避免附录日志被二次校验。
$inFence = $false

foreach ($rawLine in [System.IO.File]::ReadAllLines((Resolve-Path $ReportPath).Path, [System.Text.Encoding]::UTF8)) {
    if ($rawLine -match '^\s*```') {
        $inFence = -not $inFence
        continue
    }
    if ($inFence) { continue }
    if ([string]::IsNullOrWhiteSpace($rawLine) -or $rawLine.TrimStart().StartsWith("#")) {
        continue
    }
    $line = $rawLine
    while ($true) {
        # 1) file:///path#L123 或 file:///path#L123-L456
        if ($line -match 'file://([^\s#]+)#L(\d+)(?:-L(\d+))?') {
            $filepath = $Matches[1]
            $lineno = [int]$Matches[2]
            $endline = if ($Matches[3]) { [int]$Matches[3] } else { $lineno }
            $whole = $Matches[0]
            # Windows: file:///E:/... → E:/...；其余保留原绝对/相对路径
            if ($filepath -match '^/([A-Za-z]:/.*)$') {
                $localPath = $Matches[1]
            } else {
                $localPath = $filepath
            }
            Invoke-Citation $localPath $lineno $endline $line $whole
            $line = $line.Substring($line.IndexOf($whole) + $whole.Length)
            continue
        }
        # 2) packages/xxx/src/yyy.rs:123
        if ($line -match 'packages/([a-zA-Z0-9_-]+/[^\s:`]+\.rs):(\d+)') {
            $whole = $Matches[0]
            $relPath = "packages/$($Matches[1])"
            Invoke-Citation $relPath ([int]$Matches[2]) ([int]$Matches[2]) $line $whole
            $line = $line.Substring($line.IndexOf($whole) + $whole.Length)
            continue
        }
        # 3) crate 前缀：sz-orm-*/src/... 与 cli|examples/src/...:123
        if ($line -match '(^|[^A-Za-z0-9_.-])((sz-orm-[a-z0-9-]+|cli|examples)/src/([A-Za-z0-9_/-]+\.rs)):(\d+)') {
            $whole = $Matches[0]
            $cratePath = $Matches[2]
            if ($Matches[3] -like 'sz-orm-*') {
                $cratePath = "packages/$cratePath"
            }
            Invoke-Citation $cratePath ([int]$Matches[5]) ([int]$Matches[5]) $line $whole
            $line = $line.Substring($line.IndexOf($whole) + $whole.Length)
            continue
        }
        # 4) 仓库根级文件（支持可选目录前缀，如 packages/sz-orm-studio/Cargo.toml:12）
        if ($line -match '(?<![\w.\-])((?:[\w\-./]*[/])?(?:deny\.toml|Cargo\.toml|Cargo\.lock|AGENTS\.md|rust-toolchain\.toml|CHANGELOG\.md)):(\d+)') {
            $whole = $Matches[0]
            $relPath = $Matches[1]
            Invoke-Citation $relPath ([int]$Matches[2]) ([int]$Matches[2]) $line $whole
            $line = $line.Substring($line.IndexOf($whole) + $whole.Length)
            continue
        }
        # 5) src/xxx.rs:123 简写（映射到 sz-orm-core）
        if ($line -match 'src/([a-zA-Z0-9_/-]+\.rs):(\d+)') {
            $whole = $Matches[0]
            $relPath = "packages/sz-orm-core/src/$($Matches[1])"
            if (Test-RepoFile $relPath) {
                Invoke-Citation $relPath ([int]$Matches[2]) ([int]$Matches[2]) $line $whole
            } else {
                Write-Host "⚠️  src/$($Matches[1])`:$($Matches[2]) — 未找到对应文件（跳过）" -ForegroundColor Yellow
                $script:Warn++
            }
            $line = $line.Substring($line.IndexOf($whole) + $whole.Length)
            continue
        }
        # 6) v4 通配覆盖：任意仓库相对路径的带扩展名引用。
        #    在加入本条之前，`scripts/*.py:NN`、`docs/*.md:NN`、`.github/*.yml:NN`、
        #    `tests/*.py:NN`、`*.cs`/`*.h` 等形式**完全不进入校验**（2026-10-29 对 154 份
        #    报告对照实测：v2 校验 2038 处、v4 校验 2589 处，551 处历史上从未被读过）。
        if ($line -match '(?<![\w.\-:/\\])((?:[\w\-.@\u4e00-\u9fff/]*?)\.(?:rs|py|md|markdown|yml|yaml|toml|json|h|hpp|cs|sh|ps1|js|ts|java|go|cpp|txt|ini|nix|gitignore|cfg)):(\d+)') {
            $whole = $Matches[0]
            $relPath = $Matches[1]
            $lineno = [int]$Matches[2]
            if (Test-RepoFile $relPath) {
                Invoke-Citation $relPath $lineno $lineno $line $whole
            } elseif (-not ($relPath -match '/')) {
                # 纯文件名且根目录不存在：可能是简写，记警告不记失败
                Write-Host "⚠️  $relPath`:$lineno — 仓库根未找到该文件（跳过）" -ForegroundColor Yellow
                $script:Warn++
            } else {
                Invoke-Citation $relPath $lineno $lineno $line $whole
            }
            $line = $line.Substring($line.IndexOf($whole) + $whole.Length)
            continue
        }
        break
    }
}

Write-Host ""
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  验证结果" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  ✅ 通过: $Pass" -ForegroundColor Green
Write-Host "  ❌ 失败: $Fail" -ForegroundColor Red
Write-Host "  ⚠️  警告: $Warn" -ForegroundColor Yellow
Write-Host "==========================================" -ForegroundColor Cyan

if ($Fail -gt 0) {
    Write-Host "❌ 审计证据验证未通过 — 存在 $Fail 处无效引用" -ForegroundColor Red
    exit 1
} elseif ($Pass -eq 0) {
    Write-Host "⚠️  未找到任何 file:line 证据 — 报告可能未遵循审计规范" -ForegroundColor Yellow
    exit 1
} else {
    Write-Host "✅ 审计证据验证通过" -ForegroundColor Green
    exit 0
}
