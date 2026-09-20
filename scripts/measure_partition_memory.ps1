param(
    [string]$CliPath = (Join-Path $PSScriptRoot '..\target\debug\zstdf-cli.exe'),
    [int]$SmallParts = 2000,
    [int]$LargeParts = 20000
)
$ErrorActionPreference = 'Stop'
if ($SmallParts -lt 1 -or $LargeParts -le $SmallParts) { throw 'Require 0 < SmallParts < LargeParts' }
$cli = (Resolve-Path -LiteralPath $CliPath).Path
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ('zstdf_memory_' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporaryRoot | Out-Null

function Write-Sample([string]$Path, [int]$Count, [bool]$Incomplete) {
    $file = [IO.File]::Create($Path)
    $gzip = [IO.Compression.GZipStream]::new($file, [IO.Compression.CompressionLevel]::Fastest)
    $writer = [IO.BinaryWriter]::new($gzip)
    try {
        $writer.Write([byte[]]@(2, 0, 0, 10, 2, 4))
        $pir = [byte[]]@(2, 0, 5, 10, 1, 0)
        $ptr = [byte[]]@(12, 0, 15, 10, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 128, 63)
        $prr = [byte[]]@(9, 0, 5, 20, 1, 0, 0, 1, 0, 1, 0, 1, 0)
        for ($index = 0; $index -lt $Count; $index++) {
            if (-not $Incomplete) { $writer.Write($pir) }
            $writer.Write($ptr)
            if (-not $Incomplete) { $writer.Write($prr) }
        }
    } finally { $writer.Dispose(); $gzip.Dispose(); $file.Dispose() }
}

function Measure-Conversion([string]$Name, [int]$Count, [bool]$Incomplete) {
    $inputPath = Join-Path $temporaryRoot ($Name + '.stdf.gz')
    $outputPath = Join-Path $temporaryRoot ($Name + '-out')
    $stdoutPath = Join-Path $temporaryRoot ($Name + '.stdout')
    $stderrPath = Join-Path $temporaryRoot ($Name + '.stderr')
    Write-Sample $inputPath $Count $Incomplete
    $pendingLimit = if ($Incomplete) { 128 } else { 1000 }
    $commandArgs = @('convert', '--layout', 'catalog', '--output-dir', ('"' + $outputPath + '"'), '--memory-limit-mib', '16', '--max-output-files', '1000', '--max-open-writers', '2', '--row-group-rows', '1024', '--max-pending-tests', $pendingLimit, ('"' + $inputPath + '"'))
    $process = Start-Process -FilePath $cli -ArgumentList $commandArgs -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    $peak = 0L
    try {
        while (-not $process.HasExited) {
            $process.Refresh()
            $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
            $null = $process.WaitForExit(10)
        }
        $process.WaitForExit()
        $exitCode = $process.ExitCode
    } finally { $process.Dispose() }
    $stdout = Get-Content -LiteralPath $stdoutPath -Raw
    $stderr = Get-Content -LiteralPath $stderrPath -Raw
    if ($Incomplete) {
        if ($exitCode -eq 0 -or $stderr -notmatch 'pending tests') { throw "Expected bounded pending-test failure: $stderr" }
        $catalog = Get-Content -LiteralPath (Join-Path $outputPath '_catalog.json') -Raw | ConvertFrom-Json
        if ($catalog.run.status -ne 'failed' -or @($catalog.sources.PSObject.Properties).Count -ne 0) { throw 'Incomplete input published a source' }
        if (Get-ChildItem -LiteralPath $outputPath -Filter '*.parquet' -Recurse) { throw 'Incomplete input left Parquet fragments' }
    } elseif ($exitCode -ne 0 -or $stdout -notmatch "(?m)^rows=$Count\r?`$" ) {
        throw "Unexpected conversion result: $stdout $stderr"
    }
    [pscustomobject]@{ Name = $Name; Records = $Count; PeakRssMiB = [Math]::Round($peak / 1MB, 2); ExitCode = $exitCode }
}

try {
    $small = Measure-Conversion 'small' $SmallParts $false
    $large = Measure-Conversion 'large' $LargeParts $false
    $incomplete = Measure-Conversion 'missing-prr' 1000000 $true
    @($small, $large, $incomplete) | Format-Table -AutoSize
    if ($small.PeakRssMiB -le 0 -or $large.PeakRssMiB -le 0) { throw 'No usable RSS measurements captured' }
    # Allow 16 MiB for the configured budget plus 64 MiB runtime/allocator variation.
    if ($large.PeakRssMiB -gt $small.PeakRssMiB + 80 -or $incomplete.PeakRssMiB -gt $small.PeakRssMiB + 80) { throw 'RSS regression exceeds the documented allowance' }
    'PASS: row totals, bounded missing-PRR failure, and RSS growth allowance.'
} finally {
    $resolved = (Resolve-Path -LiteralPath $temporaryRoot).Path
    $tempPrefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolved.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path $resolved -Leaf) -notlike 'zstdf_memory_*') { throw 'Unexpected cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
