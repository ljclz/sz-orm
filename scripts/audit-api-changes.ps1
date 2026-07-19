<#
.SYNOPSIS
    SZ-ORM public API behavior change audit script (Windows PowerShell)

.DESCRIPTION
    Detect public API signature/behavior changes based on git diff:
      1. pub fn / pub async fn signature changes
      2. pub struct / pub enum field changes
      3. pub trait method signature changes
      4. Return type changes (Result<...>, Box<dyn ...>, etc.)
      5. Error type changes (PoolError, TxError, DbError variants)
      6. panic! / unwrap / expect behavior-related changes

    When changes are detected:
      - List all changed APIs
      - Automatically grep the workspace to find all callers
      - Prompt user to check if tests are synchronized
      - With -Strict switch, return non-zero exit code if callers not covered

.EXAMPLE
    ./scripts/audit-api-changes.ps1                   # diff against HEAD~1
    ./scripts/audit-api-changes.ps1 -Base main        # diff against main branch
    ./scripts/audit-api-changes.ps1 -Quiet            # quiet mode (warn only, no block)
    ./scripts/audit-api-changes.ps1 -Strict           # strict mode (exit 1 if not covered)
#>

[CmdletBinding()]
param(
    [string]$Base = "HEAD~1",
    [switch]$Quiet,
    [switch]$Strict
)

$ErrorActionPreference = "Continue"

$ScriptDir = $PSScriptRoot
if (-not $ScriptDir) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$ProjectRoot = Split-Path -Parent $ScriptDir
if (-not $ProjectRoot) {
    $ProjectRoot = (Get-Location).Path
}
Set-Location $ProjectRoot

# ============================================================================
# 1. Check git repository
# ============================================================================
try {
    $null = git rev-parse --is-inside-work-tree 2>&1
} catch {
    Write-Host "[ERROR] Not inside a git repository" -ForegroundColor Red
    exit 1
}

# ============================================================================
# 2. Collect API changes
# ============================================================================
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Public API behavior change audit" -ForegroundColor Cyan
Write-Host "  Base: $Base" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Detect changed lines of pub fn / pub async fn / pub struct / pub enum / pub trait
$DiffOutput = git diff "$Base" --unified=0 -- "packages/*/src/*.rs" 2>&1

if (-not $DiffOutput) {
    Write-Host "[OK] No code changes detected" -ForegroundColor Green
    exit 0
}

# Extract public API change lines (+/- prefix, contains pub keyword)
$ApiChangePatterns = @(
    '^[+-]\s*pub\s+(async\s+)?fn\s+(\w+)',          # pub fn / pub async fn
    '^[+-]\s*pub\s+struct\s+(\w+)',                   # pub struct
    '^[+-]\s*pub\s+enum\s+(\w+)',                     # pub enum
    '^[+-]\s*pub\s+trait\s+(\w+)',                    # pub trait
    '^[+-]\s*pub\s+type\s+(\w+)',                     # pub type
    '^[+-]\s*pub\s+const\s+(\w+)',                    # pub const
    '^[+-]\s*pub\s+mod\s+(\w+)'                       # pub mod
)

$AddedApis = @{}
$RemovedApis = @{}
$ChangedFiles = @{}

$currentFile = $null
foreach ($line in $DiffOutput -split "`n") {
    # Track current file
    if ($line -match '^\+\+\+\s+b?/(.+)$') {
        $currentFile = $Matches[1]
        continue
    }
    if ($line -match '^---\s+a?/(.+)$') {
        continue
    }

    # Detect API changes
    foreach ($pattern in $ApiChangePatterns) {
        if ($line -match $pattern) {
            $sign = $line.Substring(0, 1)
            $apiName = $Matches[1] + " (" + $Matches[2] + ")"
            if ($sign -eq "+") {
                if (-not $AddedApis.ContainsKey($apiName)) {
                    $AddedApis[$apiName] = @()
                }
                $AddedApis[$apiName] += $currentFile
            } elseif ($sign -eq "-") {
                if (-not $RemovedApis.ContainsKey($apiName)) {
                    $RemovedApis[$apiName] = @()
                }
                $RemovedApis[$apiName] += $currentFile
            }
            if (-not $ChangedFiles.ContainsKey($currentFile)) {
                $ChangedFiles[$currentFile] = 0
            }
            $ChangedFiles[$currentFile]++
            break
        }
    }
}

# ============================================================================
# 3. Detect behavior changes (panic/unwrap/Result return types)
# ============================================================================
$BehaviorPatterns = @(
    @{ Name = "panic!"; Pattern = '^[+-].*\bpanic!\(' },
    @{ Name = "unwrap()"; Pattern = '^[+-].*\.unwrap\(\)' },
    @{ Name = "expect()"; Pattern = '^[+-].*\.expect\(' },
    @{ Name = "Result return"; Pattern = '^[+-].*->\s*Result<' },
    @{ Name = "Option return"; Pattern = '^[+-].*->\s*Option<' },
    @{ Name = "Error variant"; Pattern = '^[+-].*(PoolError|TxError|DbError|CacheError)::\w+' }
)

$BehaviorChanges = @()
foreach ($line in $DiffOutput -split "`n") {
    foreach ($bp in $BehaviorPatterns) {
        if ($line -match $bp.Pattern) {
            $sign = if ($line.StartsWith("+")) { "+" } else { "-" }
            $BehaviorChanges += [PSCustomObject]@{
                Type = $bp.Name
                Sign = $sign
                Line = $line.TrimStart("+-").Trim()
            }
            break
        }
    }
}

# ============================================================================
# 4. Summary output
# ============================================================================
$HasChanges = ($AddedApis.Count -gt 0) -or ($RemovedApis.Count -gt 0) -or ($BehaviorChanges.Count -gt 0)

if (-not $HasChanges) {
    Write-Host "[OK] No public API or behavior changes detected" -ForegroundColor Green
    exit 0
}

Write-Host ""
Write-Host "--- Added public APIs ---" -ForegroundColor Green
foreach ($api in $AddedApis.Keys | Sort-Object) {
    $files = $AddedApis[$api] | Select-Object -Unique
    Write-Host "  + $api" -ForegroundColor Green
    foreach ($f in $files) {
        Write-Host "      @ $f" -ForegroundColor DarkGray
    }
}

Write-Host ""
Write-Host "--- Removed/Modified public APIs ---" -ForegroundColor Red
foreach ($api in $RemovedApis.Keys | Sort-Object) {
    $files = $RemovedApis[$api] | Select-Object -Unique
    Write-Host "  - $api" -ForegroundColor Red
    foreach ($f in $files) {
        Write-Host "      @ $f" -ForegroundColor DarkGray
    }
}

Write-Host ""
Write-Host "--- Behavior changes ---" -ForegroundColor Yellow
$BehaviorChanges | Group-Object Type | ForEach-Object {
    Write-Host "  [$($_.Name)] $($_.Count) changes" -ForegroundColor Yellow
    foreach ($c in $_.Group | Select-Object -First 3) {
        Write-Host "      $($c.Sign) $($c.Line)" -ForegroundColor DarkGray
    }
    if ($_.Count -gt 3) {
        Write-Host "      ... and $($_.Count - 3) more" -ForegroundColor DarkGray
    }
}

# ============================================================================
# 5. Find affected callers
# ============================================================================
Write-Host ""
Write-Host "--- Affected callers ---" -ForegroundColor Cyan

$ChangedApiNames = @()
$ChangedApiNames += $AddedApis.Keys | ForEach-Object { ($_ -split ' \(')[0] }
$ChangedApiNames += $RemovedApis.Keys | ForEach-Object { ($_ -split ' \(')[0] }
$ChangedApiNames = $ChangedApiNames | Select-Object -Unique

$CallersByApi = @{}
foreach ($apiName in $ChangedApiNames) {
    # Skip overly generic names (avoid false positives)
    if ($apiName.Length -lt 4) { continue }
    if (@("new", "get", "set", "len", "is_", "with", "build", "run") -contains $apiName) { continue }

    # Search callers in workspace (excluding the definition itself)
    $searchResults = Get-ChildItem -Path "packages", "cli", "examples" -Recurse -Include "*.rs" -ErrorAction SilentlyContinue |
        Select-String -Pattern "\b$([regex]::Escape($apiName))\b" -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -notmatch "\\src\\.*\.rs$" -or $_.Path -match "\\tests\\|\\benches\\" }

    if ($searchResults) {
        $CallersByApi[$apiName] = $searchResults | ForEach-Object { $_.Path } | Select-Object -Unique
    }
}

foreach ($api in $CallersByApi.Keys | Sort-Object) {
    $callers = $CallersByApi[$api]
    Write-Host "  [$api] $($callers.Count) callers:" -ForegroundColor Cyan
    foreach ($c in $callers | Select-Object -First 5) {
        Write-Host "      - $c" -ForegroundColor DarkGray
    }
    if ($callers.Count -gt 5) {
        Write-Host "      ... and $($callers.Count - 5) more" -ForegroundColor DarkGray
    }
}

# ============================================================================
# 6. Check test synchronization
# ============================================================================
Write-Host ""
Write-Host "--- Test sync check ---" -ForegroundColor Cyan

$TestFilesChanged = $ChangedFiles.Keys | Where-Object { $_ -match "tests[/\\]" }
$SrcFilesChanged = $ChangedFiles.Keys | Where-Object { $_ -match "src[/\\]" }

if ($SrcFilesChanged -and -not $TestFilesChanged -and ($AddedApis.Count -gt 0 -or $RemovedApis.Count -gt 0)) {
    Write-Host "  [WARN] src/ has API changes but tests/ not modified" -ForegroundColor Yellow
    Write-Host "  Please confirm if tests need updating" -ForegroundColor Yellow
} else {
    Write-Host "  [OK] src/ and tests/ changes synchronized" -ForegroundColor Green
}

# ============================================================================
# 7. Audit suggestions
# ============================================================================
Write-Host ""
Write-Host "--- Audit suggestions ---" -ForegroundColor Cyan
Write-Host "  1. Confirm all new/modified APIs are registered in docs/api-contracts.md" -ForegroundColor White
Write-Host "  2. Confirm all callers have adapted to the new API signature" -ForegroundColor White
Write-Host "  3. Confirm behavior changes (panic/Result/error types) are synced to contract tests" -ForegroundColor White
Write-Host "  4. Run 'cargo test -p sz-orm-core --test contracts' to verify contracts" -ForegroundColor White

# ============================================================================
# 8. Strict mode exit code
# ============================================================================
if ($Strict) {
    # Strict mode: if API changes detected but tests not synced, return non-zero
    if ($SrcFilesChanged -and -not $TestFilesChanged -and ($AddedApis.Count -gt 0 -or $RemovedApis.Count -gt 0)) {
        Write-Host ""
        Write-Host "[STRICT] API changes detected but tests not synced, exit 1" -ForegroundColor Red
        exit 1
    }
}

# Quiet mode: always return 0 (warn only, no block)
if ($Quiet) {
    exit 0
}

# Default: changes detected but reviewed, return 0
exit 0
