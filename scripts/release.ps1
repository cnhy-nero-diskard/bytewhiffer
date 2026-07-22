<#
.SYNOPSIS
  Bumps the version in Cargo.toml, commits it, and creates the release tag.

.DESCRIPTION
  Replaces the manual "edit version by hand, remember to tag" workflow.
  Does NOT push — review the commit/tag locally, then push yourself:
    git push && git push --tags

.PARAMETER Bump
  One of: patch, minor, major. Mutually exclusive with -Version.

.PARAMETER Version
  Explicit version to set (e.g. "0.3.0"), instead of bumping.

.EXAMPLE
  ./scripts/release.ps1 -Bump patch

.EXAMPLE
  ./scripts/release.ps1 -Version 1.0.0
#>
param(
    [ValidateSet("patch", "minor", "major")]
    [string]$Bump,

    [string]$Version
)

$ErrorActionPreference = "Stop"

if (-not $Bump -and -not $Version) {
    throw "Specify either -Bump <patch|minor|major> or -Version <X.Y.Z>"
}
if ($Bump -and $Version) {
    throw "Specify only one of -Bump or -Version"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoTomlPath = Join-Path $repoRoot "Cargo.toml"

$status = git -C $repoRoot status --porcelain
if ($status) {
    throw "Working tree is not clean. Commit or stash changes before releasing:`n$status"
}

$cargoToml = Get-Content -Raw $cargoTomlPath
if ($cargoToml -notmatch '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"') {
    throw "Could not find a version = `"X.Y.Z`" line in Cargo.toml"
}
$currentVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3])"

if ($Version) {
    if ($Version -notmatch '^\d+\.\d+\.\d+$') {
        throw "-Version must be in X.Y.Z form, got: $Version"
    }
    $newVersion = $Version
} else {
    $major = [int]$Matches[1]
    $minor = [int]$Matches[2]
    $patch = [int]$Matches[3]
    switch ($Bump) {
        "major" { $major++; $minor = 0; $patch = 0 }
        "minor" { $minor++; $patch = 0 }
        "patch" { $patch++ }
    }
    $newVersion = "$major.$minor.$patch"
}

$tag = "v$newVersion"
$existingTag = git -C $repoRoot tag -l $tag
if ($existingTag) {
    throw "Tag $tag already exists"
}

Write-Host "Bumping version: $currentVersion -> $newVersion"

$updatedCargoToml = $cargoToml -replace '(?m)^version\s*=\s*"\d+\.\d+\.\d+"', "version = `"$newVersion`""
Set-Content -Path $cargoTomlPath -Value $updatedCargoToml -NoNewline

# Keep Cargo.lock's own version field for this package in sync, if present.
$cargoLockPath = Join-Path $repoRoot "Cargo.lock"
if (Test-Path $cargoLockPath) {
    $lock = Get-Content -Raw $cargoLockPath
    $pattern = '(?ms)(name = "bytewhiffer"\r?\nversion = ")\d+\.\d+\.\d+(")'
    $lock = $lock -replace $pattern, "`${1}$newVersion`${2}"
    Set-Content -Path $cargoLockPath -Value $lock -NoNewline
}

git -C $repoRoot add Cargo.toml Cargo.lock
git -C $repoRoot commit -m "chore: bump version to $newVersion"
git -C $repoRoot tag $tag

Write-Host ""
Write-Host "Done. Review the commit, then push with:"
Write-Host "  git push && git push origin $tag"
