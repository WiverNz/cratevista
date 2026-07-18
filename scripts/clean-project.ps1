param(
    [switch]$Apply
)

$ErrorActionPreference = "Stop"

function Format-Bytes {
    param([UInt64]$Bytes)

    if ($Bytes -ge 1GB) {
        return "{0:N2} GiB" -f ($Bytes / 1GB)
    }
    if ($Bytes -ge 1MB) {
        return "{0:N2} MiB" -f ($Bytes / 1MB)
    }
    if ($Bytes -ge 1KB) {
        return "{0:N2} KiB" -f ($Bytes / 1KB)
    }
    return "$Bytes B"
}

function Convert-ToDisplayPath {
    param(
        [string]$Root,
        [string]$Path
    )

    $relative = [System.IO.Path]::GetRelativePath($Root, $Path).Replace("\", "/")
    if ($relative -eq ".") {
        return "."
    }
    return $relative
}

function Test-IsUnderRoot {
    param(
        [string]$Root,
        [string]$Path
    )

    $relative = [System.IO.Path]::GetRelativePath($Root, $Path)
    return $relative -ne "." -and
        -not [System.IO.Path]::IsPathFullyQualified($relative) -and
        -not $relative.StartsWith("..$([System.IO.Path]::DirectorySeparatorChar)") -and
        -not $relative.StartsWith("..$([System.IO.Path]::AltDirectorySeparatorChar)") -and
        $relative -ne ".."
}

function Resolve-AllowlistEntry {
    param(
        [string]$Root,
        [string]$Entry,
        [int]$LineNumber
    )

    if ($Entry -ne $Entry.Trim()) {
        throw "Malformed allowlist entry at line ${LineNumber}: leading or trailing whitespace is not allowed."
    }
    if ($Entry.Length -eq 0) {
        throw "Malformed allowlist entry at line ${LineNumber}: empty entries are not allowed."
    }
    if ($Entry -match "^[A-Za-z]:[/\\]" -or $Entry.StartsWith("/") -or $Entry.StartsWith("\\")) {
        throw "Malformed allowlist entry at line ${LineNumber}: paths must be repository-relative."
    }
    if ($Entry.Contains("`0") -or $Entry.Contains("*") -or $Entry.Contains("?")) {
        throw "Malformed allowlist entry at line ${LineNumber}: wildcards and NUL bytes are not allowed."
    }
    $normalizedEntry = $Entry.TrimEnd("/", "\")
    if ($normalizedEntry.Length -eq 0) {
        throw "Malformed allowlist entry at line ${LineNumber}: empty entries are not allowed."
    }
    $parts = $normalizedEntry -split "[/\\]+"
    foreach ($part in $parts) {
        if ($part.Length -eq 0 -or $part -eq "." -or $part -eq "..") {
            throw "Malformed allowlist entry at line ${LineNumber}: empty, '.', and '..' path segments are not allowed."
        }
    }

    $full = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($Root, $normalizedEntry))
    if ($full -eq $Root) {
        throw "Refusing allowlist entry at line ${LineNumber}: repository root cannot be removed."
    }
    if (-not (Test-IsUnderRoot -Root $Root -Path $full)) {
        throw "Refusing allowlist entry at line ${LineNumber}: path escapes the repository."
    }
    return $full
}

function Resolve-ReparseTarget {
    param([System.IO.FileSystemInfo]$Item)

    if (($Item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) {
        return $null
    }

    try {
        $target = $Item.ResolveLinkTarget($true)
    } catch {
        return $null
    }
    if ($null -eq $target) {
        return $null
    }
    return [System.IO.Path]::GetFullPath($target.FullName)
}

function Assert-NoEscapingReparsePoints {
    param(
        [string]$Root,
        [string]$Path
    )

    $rootItem = Get-Item -LiteralPath $Path -Force
    $items = @($rootItem)
    if ($rootItem.PSIsContainer -and (($rootItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0)) {
        $items += Get-ChildItem -LiteralPath $Path -Force -Recurse -ErrorAction Stop
    }

    foreach ($item in $items) {
        $target = Resolve-ReparseTarget -Item $item
        if ($null -ne $target -and -not (Test-IsUnderRoot -Root $Root -Path $target)) {
            $display = Convert-ToDisplayPath -Root $Root -Path $item.FullName
            throw "Refusing to remove ${display}: reparse point resolves outside the repository to $target."
        }
    }
}

function Measure-PathSize {
    param([string]$Path)

    $item = Get-Item -LiteralPath $Path -Force
    if (-not $item.PSIsContainer) {
        return [UInt64]$item.Length
    }
    if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        return 0
    }

    [UInt64]$total = 0
    foreach ($child in Get-ChildItem -LiteralPath $Path -Force -Recurse -File -Attributes !ReparsePoint -ErrorAction Stop) {
        $total += [UInt64]$child.Length
    }
    return $total
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = [System.IO.Path]::GetFullPath((Join-Path $scriptDir ".."))
$allowlist = Join-Path $scriptDir "clean-project-paths.txt"

if (-not (Test-Path -LiteralPath $allowlist -PathType Leaf)) {
    throw "Missing cleanup allowlist: $allowlist"
}

$targets = New-Object System.Collections.Generic.List[string]
$seen = New-Object "System.Collections.Generic.HashSet[string]" ([System.StringComparer]::OrdinalIgnoreCase)
$lineNumber = 0
foreach ($rawLine in [System.IO.File]::ReadLines($allowlist)) {
    $lineNumber += 1
    if ($rawLine.StartsWith("#")) {
        continue
    }
    $target = Resolve-AllowlistEntry -Root $root -Entry $rawLine -LineNumber $lineNumber
    if ($seen.Add($target)) {
        $targets.Add($target)
    }
}

if ($targets.Count -eq 0) {
    throw "Cleanup allowlist contains no entries."
}

$existing = New-Object System.Collections.Generic.List[string]
[UInt64]$total = 0
foreach ($target in $targets) {
    if (-not (Test-Path -LiteralPath $target)) {
        continue
    }
    Assert-NoEscapingReparsePoints -Root $root -Path $target
    $existing.Add($target)
    $total += Measure-PathSize -Path $target
}

$mode = if ($Apply) { "apply" } else { "dry-run" }
Write-Output "CrateVista cleanup ($mode)"
if ($existing.Count -eq 0) {
    Write-Output "No allowlisted cleanup paths exist."
} else {
    Write-Output "Paths:"
    foreach ($path in $existing) {
        Write-Output "  $(Convert-ToDisplayPath -Root $root -Path $path)"
    }
}
Write-Output "Total removable size: $(Format-Bytes -Bytes $total)"

if ($Apply) {
    foreach ($path in $existing) {
        Remove-Item -LiteralPath $path -Force -Recurse
    }
    Write-Output "Removed $($existing.Count) allowlisted path(s)."
} else {
    Write-Output "Dry run only. Re-run with -Apply to delete these paths."
}

Write-Output ""
Write-Output "git status --short:"
git -C $root status --short
