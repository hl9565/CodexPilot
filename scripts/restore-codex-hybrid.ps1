#requires -Version 5.1

<#
.SYNOPSIS
Restore the exact config.toml snapshot created by enable-codex-hybrid.ps1 without changing auth.json.

.EXAMPLE
.\scripts\restore-codex-hybrid.ps1
#>

[CmdletBinding(SupportsShouldProcess = $true)]
param(
  [string]$CodexHome = $(
    if ($env:CODEX_HOME) {
      $env:CODEX_HOME
    } else {
      Join-Path ([Environment]::GetFolderPath("UserProfile")) ".codex"
    }
  ),

  [switch]$Force
)

$ErrorActionPreference = "Stop"
$ManagedBy = "huanglin-codex-hybrid-scripts"

function Get-FullPath {
  param([Parameter(Mandatory = $true)][string]$Path)
  return [System.IO.Path]::GetFullPath($Path)
}

function Get-FileSha256 {
  param([Parameter(Mandatory = $true)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    return $null
  }
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Write-BytesAtomic {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][byte[]]$Bytes
  )

  $parent = Split-Path -Parent $Path
  if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
  }

  $tempPath = "$Path.tmp.$PID.$([Guid]::NewGuid().ToString('N'))"
  try {
    [System.IO.File]::WriteAllBytes($tempPath, $Bytes)
    Move-Item -LiteralPath $tempPath -Destination $Path -Force
  } finally {
    if (Test-Path -LiteralPath $tempPath) {
      Remove-Item -LiteralPath $tempPath -Force
    }
  }
}

function Write-Utf8FileAtomic {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Content
  )
  $encoding = New-Object System.Text.UTF8Encoding($false)
  Write-BytesAtomic -Path $Path -Bytes $encoding.GetBytes($Content)
}

function Write-JsonFileAtomic {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)]$Value
  )
  $json = $Value | ConvertTo-Json -Depth 8
  Write-Utf8FileAtomic -Path $Path -Content ($json + "`n")
}

function Read-Utf8File {
  param([Parameter(Mandatory = $true)][string]$Path)
  $encoding = New-Object System.Text.UTF8Encoding($false, $true)
  return [System.IO.File]::ReadAllText($Path, $encoding)
}

function Test-PathWithinRoot {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Root
  )
  $fullPath = Get-FullPath $Path
  $fullRoot = (Get-FullPath $Root).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
  $prefix = $fullRoot + [System.IO.Path]::DirectorySeparatorChar
  return $fullPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

$CodexHome = Get-FullPath $CodexHome
$ConfigPath = Join-Path $CodexHome "config.toml"
$BackupRoot = Join-Path $CodexHome "backups_state\hybrid-relay-script"
$ActiveStatePath = Join-Path $BackupRoot "active.json"

if (-not (Test-Path -LiteralPath $ActiveStatePath -PathType Leaf)) {
  throw "No active hybrid-relay state was found at $ActiveStatePath."
}

try {
  $state = Read-Utf8File -Path $ActiveStatePath | ConvertFrom-Json
} catch {
  throw "The active hybrid-relay state is not valid JSON: $($_.Exception.Message)"
}
if ($state.managedBy -ne $ManagedBy) {
  throw "The active state is not owned by these scripts. Refusing to restore it."
}
if ($state.status -notin @("prepared", "active")) {
  throw "The active state has an unsupported status: $($state.status)"
}

$configExisted = [bool]$state.configExisted
$backupPath = if ($configExisted) { [string]$state.backupPath } else { $null }
if ($configExisted) {
  if ([string]::IsNullOrWhiteSpace($backupPath) -or -not (Test-PathWithinRoot -Path $backupPath -Root $BackupRoot)) {
    throw "The recorded backup path is invalid or outside the managed backup directory."
  }
  if (-not (Test-Path -LiteralPath $backupPath -PathType Leaf)) {
    throw "The recorded config backup does not exist: $backupPath"
  }
  $backupHash = Get-FileSha256 $backupPath
  if ($state.originalConfigSha256 -and $backupHash -ne [string]$state.originalConfigSha256) {
    throw "The original config backup hash does not match the recorded value."
  }
}

$currentHash = Get-FileSha256 $ConfigPath
$alreadyRestored = if ($configExisted) {
  $currentHash -and $currentHash -eq [string]$state.originalConfigSha256
} else {
  -not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)
}

$preRestoreBackupPath = $null
if (-not $alreadyRestored -and $state.enabledConfigSha256 -and $currentHash -ne [string]$state.enabledConfigSha256) {
  if (-not $Force) {
    throw "config.toml changed after hybrid relay was enabled. Review it first, or rerun with -Force to create a conflict backup and restore the original."
  }
}

$restoreAction = if ($alreadyRestored) {
  "Finalize the already-restored hybrid relay state"
} else {
  "Restore the config saved before hybrid relay was enabled"
}
if (-not $PSCmdlet.ShouldProcess($ConfigPath, $restoreAction)) {
  return
}

if (-not $alreadyRestored) {
  if ($Force -and $state.enabledConfigSha256 -and $currentHash -ne [string]$state.enabledConfigSha256 -and (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) {
    $stamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss-fff")
    $preRestoreBackupPath = Join-Path $BackupRoot "config.before-forced-restore.$stamp.toml"
    Copy-Item -LiteralPath $ConfigPath -Destination $preRestoreBackupPath
  }
  if ($configExisted) {
    Write-BytesAtomic -Path $ConfigPath -Bytes ([System.IO.File]::ReadAllBytes($backupPath))
  } elseif (Test-Path -LiteralPath $ConfigPath -PathType Leaf) {
    Remove-Item -LiteralPath $ConfigPath -Force
  }
}

$restoredCorrectly = if ($configExisted) {
  (Get-FileSha256 $ConfigPath) -eq [string]$state.originalConfigSha256
} else {
  -not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)
}
if (-not $restoredCorrectly) {
  throw "The restore operation completed, but the resulting config did not match the original snapshot."
}

$state.status = "restored"
$state | Add-Member -NotePropertyName restoredAtUtc -NotePropertyValue ((Get-Date).ToUniversalTime().ToString("o")) -Force
$state | Add-Member -NotePropertyName forcedRestore -NotePropertyValue ([bool]$Force) -Force
$state | Add-Member -NotePropertyName preRestoreBackupPath -NotePropertyValue $preRestoreBackupPath -Force
$archiveName = "restored-$((Get-Date).ToUniversalTime().ToString('yyyyMMdd-HHmmss-fff')).json"
$archivePath = Join-Path $BackupRoot $archiveName
Write-JsonFileAtomic -Path $archivePath -Value $state
Remove-Item -LiteralPath $ActiveStatePath -Force

Write-Host "Hybrid relay configuration restored."
Write-Host "Config:  $ConfigPath"
Write-Host "Record:  $archivePath"
if ($preRestoreBackupPath) {
  Write-Host "Changed config preserved at: $preRestoreBackupPath"
}
Write-Host "auth.json was not modified. Restart Codex before the next turn."
