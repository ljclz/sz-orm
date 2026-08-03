# SQL Injection Risk Scanner (Gate 9)
# Scans workspace for potential SQL injection risks.

$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path | Split-Path -Parent
$rsFiles = Get-ChildItem -Path "$root\packages", "$root\examples", "$root\cli" -Recurse -Filter "*.rs" -ErrorAction SilentlyContinue

$deprecatedHits = @()
$concatHits = @()

foreach ($f in $rsFiles) {
    $lines = Get-Content $f.FullName -Encoding UTF8
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        if ($line -match '^\s*//' -or $line -match '^\s*!') { continue }

        if ($line -match '\.where_cond\(|\.or_where\(' -and $f.Name -notmatch 'test') {
            $deprecatedHits += "$($f.FullName):$($i+1): $line"
        }

        if ($line -match "format!.*WHERE.*\{.*\}" -and $line -notmatch "columns\[|conditions\[|tables\[|updates\[") {
            $concatHits += "$($f.FullName):$($i+1): $line"
        }
    }
}

$exit = 0
if ($deprecatedHits.Count -gt 0) {
    Write-Host "=== deprecated where_cond/or_where usage (migrate to where_eq/or_where_eq) ===" -ForegroundColor Yellow
    $deprecatedHits | ForEach-Object { Write-Host $_ }
    Write-Host ""
}

if ($concatHits.Count -gt 0) {
    Write-Host "=== potential SQL string concatenation (manual review needed) ===" -ForegroundColor Yellow
    $concatHits | ForEach-Object { Write-Host $_ }
    Write-Host ""
}

$total = $deprecatedHits.Count + $concatHits.Count
if ($total -eq 0) {
    Write-Host "Gate 9 PASSED: no SQL injection risks found" -ForegroundColor Green
} else {
    Write-Host "Gate 9: $total item(s) found (non-blocking, manual review recommended)" -ForegroundColor Yellow
}

exit $exit
