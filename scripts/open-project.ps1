<#
.SYNOPSIS
  Open a CrateVista interactive explorer for a local Rust project.

.DESCRIPTION
  Default (live): generate + serve + open the browser, with local source access and
  watch mode, bound to loopback, with CrateVista choosing an available port. The
  server keeps running in the foreground; Ctrl+C stops it.

  This script never modifies the target project or any Git state; CrateVista's only
  writes are under the project's own `target\cratevista\`.

.EXAMPLE
  ./scripts/open-project.ps1 D:\Projects\MyRustProject

.EXAMPLE
  ./scripts/open-project.ps1 D:\Projects\MyRustProject\Cargo.toml

.EXAMPLE
  ./scripts/open-project.ps1 D:\Projects\MyRustProject -Static
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$Project,
  [switch]$Static
)

$ErrorActionPreference = 'Stop'
$Nightly = 'nightly-2026-07-01'

# Write to stderr and exit with an explicit code. (Write-Error under
# `ErrorActionPreference = Stop` throws a terminating error before `exit` runs,
# which would collapse every failure to exit code 1.)
function Fail([string]$Message, [int]$Code = 2) {
  [Console]::Error.WriteLine("error: $Message")
  exit $Code
}

# Reject a URL argument; require a filesystem path.
if ($Project -match '://') {
  Fail "Expected a filesystem path, not a URL: $Project"
}
if (-not (Test-Path -LiteralPath $Project)) {
  Fail "Path does not exist: $Project"
}

# Resolve the manifest from either a directory or a direct Cargo.toml path.
$item = Get-Item -LiteralPath $Project
if ($item.PSIsContainer) {
  $manifest = Join-Path $item.FullName 'Cargo.toml'
} else {
  $manifest = $item.FullName
}
if ((-not (Test-Path -LiteralPath $manifest -PathType Leaf)) -or ((Split-Path $manifest -Leaf) -ne 'Cargo.toml')) {
  Fail "No Cargo.toml found (looked at: $manifest). Pass a workspace directory or a path to its Cargo.toml."
}
$manifest = (Resolve-Path -LiteralPath $manifest).Path
$projectRoot = Split-Path -Parent $manifest

# --- Command resolution -----------------------------------------------------
# 1) Prefer the installed `cargo cratevista` subcommand.
# 2) Otherwise, if run from the CrateVista repository, fall back to `cargo run`.
# 3) Otherwise fail with a clear instruction.
$repoRoot = Split-Path -Parent $PSScriptRoot
$installed = $false
try {
  & cargo cratevista --help *> $null
  $installed = ($LASTEXITCODE -eq 0)
} catch {
  $installed = $false
}

if ($installed) {
  $prefix = @('cratevista')
} elseif (Test-Path -LiteralPath (Join-Path $repoRoot 'crates\cargo-cratevista\Cargo.toml')) {
  Write-Warning "'cargo cratevista' is not installed; using the repository build."
  $prefix = @('run', '--quiet', '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'), '-p', 'cargo-cratevista', '--', 'cratevista')
} else {
  [Console]::Error.WriteLine("error: CrateVista is not available.")
  [Console]::Error.WriteLine("  Install it:  cargo install --path crates/cargo-cratevista   (from the CrateVista repo)")
  [Console]::Error.WriteLine("  or run this script from inside the CrateVista repository.")
  exit 127
}

# --- Pinned nightly (reported, never installed automatically) ---------------
try {
  $toolchains = & rustup toolchain list 2>$null
  if (-not ($toolchains -match [regex]::Escape($Nightly))) {
    Write-Warning "The pinned nightly '$Nightly' is not installed. Generating rustdoc JSON needs it. Install it yourself with:"
    Write-Warning "  rustup toolchain install $Nightly"
    Write-Warning "(A metadata-only workspace still works without it.)"
  }
} catch {
  # rustup not present: CrateVista's own doctor/generate will report it.
}

if ($Static) {
  # Static snapshot: build the self-contained site under the project's own target.
  Write-Output "Building a static snapshot for: $projectRoot"
  & cargo @prefix build --manifest-path $manifest --toolchain $Nightly
  $status = $LASTEXITCODE
  $site = Join-Path $projectRoot 'target\cratevista\site'
  Write-Output ""
  Write-Output "Static snapshot built at:"
  Write-Output "  $site"
  Write-Output ""
  Write-Output "This is a generated SNAPSHOT:"
  Write-Output "  - no source-content API (/api/**);"
  Write-Output "  - no live reload - rebuild after changes;"
  Write-Output "  - it must be served over HTTP (file:// is unsupported)."
  Write-Output ""
  Write-Output "Automatic serving is not supported by this launcher: CrateVista has no"
  Write-Output "built-in static-directory server, and this script will not pull in Python,"
  Write-Output "Node or another dependency just to serve files. Serve the directory above"
  Write-Output "with any static HTTP host (it uses relative URLs, so a subpath is fine)."
  exit $status
}

# Live explorer: run in the foreground so Ctrl+C stops it; preserve the exit code.
Write-Output "Launching the live CrateVista explorer for: $projectRoot"
Write-Output "  (source access + watch mode, loopback only; Ctrl+C to stop)"
& cargo @prefix open --manifest-path $manifest --toolchain $Nightly --source --watch
exit $LASTEXITCODE
