# Local checkpoint after an agent run finishes (stop hook).
# Does not stage, commit, or push. Fail-open: empty stdout JSON.
# See docs/development/agent-checkpoint.md.
$ErrorActionPreference = "Continue"
try {
    $input | Out-Null
    $root = (Get-Location).Path
    $safe = @("-c", "safe.directory=$root")
    $gitExe = @(Get-Command git -CommandType Application -ErrorAction Stop)[0].Source

    function Invoke-Git([Parameter(ValueFromRemainingArguments = $true)][string[]]$GitArgs) {
        & $gitExe @safe @GitArgs 2>&1
    }

    $status = Invoke-Git status --porcelain
    if (-not $status) {
        Write-Output "{}"
        exit 0
    }

    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $dir = Join-Path $root "target/agent-work/checkpoints/$stamp"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null

    $tracked = Invoke-Git diff HEAD
    $staged = Invoke-Git diff --cached
    $untracked = Invoke-Git ls-files --others --exclude-standard
    $status | Out-File -FilePath (Join-Path $dir "status.txt") -Encoding utf8
    $tracked | Out-File -FilePath (Join-Path $dir "tracked.diff") -Encoding utf8
    $staged | Out-File -FilePath (Join-Path $dir "staged.diff") -Encoding utf8
    $untracked | Out-File -FilePath (Join-Path $dir "untracked.txt") -Encoding utf8

    $branch = Invoke-Git rev-parse --abbrev-ref HEAD
    if ($branch -match '^(main|master)$') {
        [Console]::Error.WriteLine("checkpoint: refusing to publish; HEAD is $branch")
    }
    if ($env:ZSTDF_AGENT_PUBLISH -eq "1") {
        [Console]::Error.WriteLine("ZSTDF_AGENT_PUBLISH is ignored by the stop hook. It never pushes. Run scripts/publish_agent_branch.ps1 explicitly.")
    }

    $python = $null
    foreach ($name in @("python", "python3", "py")) {
        $cmd = Get-Command $name -ErrorAction SilentlyContinue
        if ($cmd) { $python = $cmd.Source; break }
    }
    if ($python) {
        & $python "scripts/check_pr_size.py" --worktree 2>&1 | ForEach-Object {
            [Console]::Error.WriteLine($_)
        }
    }

    [Console]::Error.WriteLine("checkpoint: $dir")
    Write-Output "{}"
    exit 0
}
catch {
    Write-Output "{}"
    exit 0
}
