# Explicit coordinator publish to an agent/* branch.
# Not invoked by the stop hook. Never pushes main or master.
# See docs/development/agent-checkpoint.md.
param(
    [Parameter(Mandatory = $true)]
    [string]$Branch
)

$ErrorActionPreference = "Stop"
$root = (Get-Location).Path
$safe = @("-c", "safe.directory=$root")
$gitExe = @(Get-Command git -CommandType Application -ErrorAction Stop)[0].Source

function Invoke-Git([Parameter(ValueFromRemainingArguments = $true)][string[]]$GitArgs) {
    & $gitExe @safe @GitArgs
    if ($LASTEXITCODE -ne 0) {
        throw "git failed: git $($GitArgs -join ' ')"
    }
}

function Denied([string]$Path) {
    $norm = $Path.Replace('\', '/').ToLowerInvariant()
    if ($norm.StartsWith("examples/public-stdf/downloads/")) { return $true }
    if ($norm.StartsWith("examples/detail data views")) { return $true }
    foreach ($suffix in @(".stdf", ".stdf.gz", ".std", ".std.gz")) {
        if ($norm.EndsWith($suffix)) { return $true }
    }
    return $false
}

if ($Branch -notmatch '^agent/[a-z0-9][a-z0-9._/-]{0,120}$' -or $Branch -match '\.\.' -or $Branch -match '/$' -or $Branch -match '^agent/(main|master)(/|$)') {
    Write-Error "destination must be agent/<lowercase-name>, not '$Branch'"
    exit 2
}

$current = (& $gitExe @safe rev-parse --abbrev-ref HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw "not a git checkout" }

Write-Output "checks: cargo fmt"
& cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { Write-Error "cargo fmt failed; not publishing"; exit 1 }

Write-Output "checks: cargo clippy"
& cargo clippy --workspace --locked --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { Write-Error "cargo clippy failed; not publishing"; exit 1 }

Write-Output "checks: cargo test"
& cargo test --workspace --locked
if ($LASTEXITCODE -ne 0) { Write-Error "cargo test failed; not publishing"; exit 1 }

$dirty = & $gitExe @safe status --porcelain
if ($LASTEXITCODE -ne 0) { throw "git status failed" }

if ($dirty) {
    if ($current -eq "main" -or $current -eq "master") {
        Invoke-Git switch -c $Branch
        $current = $Branch
    }
    elseif ($current -ne $Branch) {
        Write-Error "refusing to commit on '$current'. Switch to '$Branch' or start from main/master."
        exit 2
    }

    Invoke-Git add -u
    $untracked = & $gitExe @safe ls-files --others --exclude-standard
    foreach ($path in @($untracked)) {
        if (-not $path) { continue }
        if (Denied $path) {
            Write-Output "skipping untracked sample path: $path"
            continue
        }
        Invoke-Git add -- $path
    }
    $staged = & $gitExe @safe diff --cached --name-only
    if ($staged) {
        Invoke-Git commit -m "chore: coordinator checkpoint on $Branch"
    }
}

$head = (& $gitExe @safe rev-parse --abbrev-ref HEAD).Trim()
if ($head -eq "main" -or $head -eq "master") {
    Write-Error "refusing to push while HEAD is $head"
    exit 2
}

Write-Output "push: HEAD -> refs/heads/$Branch"
& $gitExe @safe push -u origin "HEAD:refs/heads/$Branch"
if ($LASTEXITCODE -ne 0) { throw "git push failed" }
Write-Output "published refs/heads/$Branch"
exit 0
