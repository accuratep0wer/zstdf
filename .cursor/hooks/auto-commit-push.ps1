# Auto-commit and push after an agent run finishes (stop hook).
# Fail-open: empty stdout JSON; never block the agent on git issues.
$ErrorActionPreference = "Continue"
try {
    $input | Out-Null
    $root = (Get-Location).Path
    $safe = @("-c", "safe.directory=$root")

    function Git([Parameter(ValueFromRemainingArguments = $true)]$Args) {
        & git @safe @Args 2>&1
    }

    $status = Git status --porcelain
    if (-not $status) {
        Write-Output "{}"
        exit 0
    }

    # Stage tracked updates and known source/docs; respect .gitignore
    Git add -A | Out-Null
    $staged = Git diff --cached --name-only
    if (-not $staged) {
        Write-Output "{}"
        exit 0
    }

    $stamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss zzz"
    $msg = @"
chore: agent session checkpoint

Automated stop-hook commit ($stamp).
"@
    Git commit -m $msg | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Output "{}"
        exit 0
    }

    Git push -u origin HEAD | Out-Null
    Write-Output "{}"
    exit 0
}
catch {
    Write-Output "{}"
    exit 0
}
