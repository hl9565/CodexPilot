#requires -Version 5.1

<#
.SYNOPSIS
Preserve ChatGPT OAuth and route Codex model requests to a Responses-compatible API with Fast enabled.

.EXAMPLE
.\scripts\enable-codex-hybrid.ps1 -BaseUrl "https://relay.example/v1"

.DESCRIPTION
When the current model_provider is a custom provider, the script reuses that
provider id by default. This keeps existing Codex thread metadata associated
with the same provider id. Pass -ProviderName only to intentionally use a new
provider id. The relay API key is written to the provider's
experimental_bearer_token setting. auth.json and existing session data are not
modified.
#>

[CmdletBinding(SupportsShouldProcess = $true)]
param(
  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [string]$BaseUrl,

  [ValidatePattern('^[A-Za-z0-9_-]+$')]
  [string]$ProviderName,

  [string]$CodexHome = $(
    if ($env:CODEX_HOME) {
      $env:CODEX_HOME
    } else {
      Join-Path ([Environment]::GetFolderPath("UserProfile")) ".codex"
    }
  ),

  [switch]$AllowInsecureHttp,
  [switch]$SkipLoginCheck
)

$ErrorActionPreference = "Stop"
$ManagedBy = "huanglin-codex-hybrid-scripts"
$StateVersion = 1
$RootMarkerStart = "# BEGIN huanglin Codex hybrid relay root settings"
$RootMarkerEnd = "# END huanglin Codex hybrid relay root settings"
$ProviderMarkerStart = "# BEGIN huanglin Codex hybrid relay provider"
$ProviderMarkerEnd = "# END huanglin Codex hybrid relay provider"

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

function Get-Utf8Sha256 {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Content)
  $encoding = New-Object System.Text.UTF8Encoding($false)
  $algorithm = [System.Security.Cryptography.SHA256]::Create()
  try {
    $hash = $algorithm.ComputeHash($encoding.GetBytes($Content))
    return ([BitConverter]::ToString($hash) -replace '-', '').ToLowerInvariant()
  } finally {
    $algorithm.Dispose()
  }
}

function Write-Utf8FileAtomic {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Content
  )

  $parent = Split-Path -Parent $Path
  if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
  }

  $tempPath = "$Path.tmp.$PID.$([Guid]::NewGuid().ToString('N'))"
  $encoding = New-Object System.Text.UTF8Encoding($false)
  try {
    [System.IO.File]::WriteAllText($tempPath, $Content, $encoding)
    Move-Item -LiteralPath $tempPath -Destination $Path -Force
  } finally {
    if (Test-Path -LiteralPath $tempPath) {
      Remove-Item -LiteralPath $tempPath -Force
    }
  }
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

function Get-TomlLineInfo {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Line,
    [Parameter(Mandatory = $true)][hashtable]$State
  )

  $isStructural = [string]::IsNullOrEmpty([string]$State.MultilineType) -and [int]$State.NestingDepth -eq 0
  $keyPattern = '(?:[A-Za-z0-9_-]+|"(?:\\.|[^"\\])*"|''[^'']*'')'
  $dottedKeyPattern = "$keyPattern(?:\s*\.\s*$keyPattern)*"
  $tablePattern = '^\s*(?:\[\[\s*{0}\s*\]\]|\[\s*{0}\s*\])\s*(?:#.*)?$' -f $dottedKeyPattern
  $isTableHeader = $isStructural -and [Regex]::IsMatch($Line, $tablePattern)

  $index = 0
  while ($index -lt $Line.Length) {
    if ($State.MultilineType -eq "basic") {
      if ($index + 2 -lt $Line.Length -and $Line.Substring($index, 3) -eq '"""') {
        $backslashCount = 0
        for ($previous = $index - 1; $previous -ge 0 -and $Line[$previous] -eq '\'; $previous--) {
          $backslashCount++
        }
        if (($backslashCount % 2) -eq 0) {
          $State.MultilineType = $null
          $index += 3
          continue
        }
      }
      $index++
      continue
    }

    if ($State.MultilineType -eq "literal") {
      if ($index + 2 -lt $Line.Length -and $Line.Substring($index, 3) -eq "'''") {
        $State.MultilineType = $null
        $index += 3
      } else {
        $index++
      }
      continue
    }

    $character = $Line[$index]
    if ($character -eq '#') {
      break
    }
    if ($character -eq '"') {
      if ($index + 2 -lt $Line.Length -and $Line.Substring($index, 3) -eq '"""') {
        $State.MultilineType = "basic"
        $index += 3
        continue
      }
      $index++
      while ($index -lt $Line.Length) {
        if ($Line[$index] -eq '\') {
          $index += 2
        } elseif ($Line[$index] -eq '"') {
          $index++
          break
        } else {
          $index++
        }
      }
      continue
    }
    if ($character -eq "'") {
      if ($index + 2 -lt $Line.Length -and $Line.Substring($index, 3) -eq "'''") {
        $State.MultilineType = "literal"
        $index += 3
        continue
      }
      $index++
      while ($index -lt $Line.Length -and $Line[$index] -ne "'") {
        $index++
      }
      if ($index -lt $Line.Length) {
        $index++
      }
      continue
    }
    if ($character -eq '[' -or $character -eq '{') {
      $State.NestingDepth = [int]$State.NestingDepth + 1
    } elseif ($character -eq ']' -or $character -eq '}') {
      $State.NestingDepth = [Math]::Max(0, [int]$State.NestingDepth - 1)
    }
    $index++
  }

  return [pscustomobject]@{
    IsStructural = $isStructural
    IsTableHeader = $isTableHeader
  }
}

function ConvertTo-TomlBasicString {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)
  return $Value.Replace('\', '\\').Replace('"', '\"').Replace("`b", '\b').Replace("`t", '\t').Replace("`n", '\n').Replace("`f", '\f').Replace("`r", '\r')
}

function Test-ChatGptLogin {
  param([Parameter(Mandatory = $true)][string]$AuthPath)

  if (-not (Test-Path -LiteralPath $AuthPath -PathType Leaf)) {
    return $false
  }
  try {
    $auth = Read-Utf8File -Path $AuthPath | ConvertFrom-Json
  } catch {
    return $false
  }
  if (-not $auth.auth_mode -or -not $auth.auth_mode.Equals("chatgpt", [StringComparison]::OrdinalIgnoreCase)) {
    return $false
  }
  if (-not $auth.tokens) {
    return $false
  }
  foreach ($name in @("access_token", "id_token", "refresh_token")) {
    $value = $auth.tokens.$name
    if ($value -and -not [string]::IsNullOrWhiteSpace([string]$value)) {
      return $true
    }
  }
  return $false
}

function Get-ApiKey {
  $processValue = [Environment]::GetEnvironmentVariable("CODEX_HYBRID_API_KEY", [EnvironmentVariableTarget]::Process)
  if (-not [string]::IsNullOrWhiteSpace($processValue)) {
    return $processValue
  }

  $secure = Read-Host "Upstream API key (input is hidden)" -AsSecureString
  $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
  try {
    return ([Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)).Trim()
  } finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
  }
}

function Get-RootModelProvider {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Content)

  $tomlState = @{ MultilineType = $null; NestingDepth = 0 }
  foreach ($line in [Regex]::Split($Content, "\r?\n")) {
    $lineInfo = Get-TomlLineInfo -Line $line -State $tomlState
    if ($lineInfo.IsTableHeader) {
      break
    }
    if ($lineInfo.IsStructural -and $line -cmatch '^\s*(?:model_provider|"model_provider"|''model_provider'')\s*=\s*(?:"([A-Za-z0-9_-]+)"|''([A-Za-z0-9_-]+)'')\s*(?:#.*)?$') {
      return $(if ($Matches[1]) { $Matches[1] } else { $Matches[2] })
    }
  }
  return "openai"
}

function New-HybridConfig {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Original,
    [Parameter(Mandatory = $true)][string]$ProviderName,
    [Parameter(Mandatory = $true)][string]$CurrentProviderName,
    [Parameter(Mandatory = $true)][string]$BaseUrl,
    [Parameter(Mandatory = $true)][string]$ApiKey
  )

  if ($Original.Contains($RootMarkerStart) -or $Original.Contains($ProviderMarkerStart)) {
    throw "The config already contains hybrid-script markers. Restore or remove that managed state first."
  }

  $providerPattern = '(?m)^\s*\[\s*model_providers\s*\.\s*(?:"{0}"|''{0}''|{0})(?:\s*\.\s*(?:[A-Za-z0-9_-]+|"(?:\\.|[^"\\])*"|''[^'']*''))*\s*\]\s*(?:#.*)?$' -f [Regex]::Escape($ProviderName)
  $providerExists = $false
  $providerScanState = @{ MultilineType = $null; NestingDepth = 0 }
  foreach ($line in [Regex]::Split($Original, "\r?\n")) {
    $lineInfo = Get-TomlLineInfo -Line $line -State $providerScanState
    if ($lineInfo.IsTableHeader -and [Regex]::IsMatch($line, $providerPattern)) {
      $providerExists = $true
      break
    }
  }
  $reuseCurrentProvider = $providerExists -and $ProviderName.Equals($CurrentProviderName, [StringComparison]::Ordinal)
  if ($providerExists -and -not $reuseCurrentProvider) {
    throw "Provider '$ProviderName' already exists but is not the active model_provider. Choose another -ProviderName."
  }

  $newLine = if ($Original.Contains("`r`n")) { "`r`n" } else { "`n" }
  $sourceLines = [Regex]::Split($Original, "\r?\n")
  $lines = New-Object System.Collections.Generic.List[object]
  $tomlState = @{ MultilineType = $null; NestingDepth = 0 }
  $skipProviderSection = $false
  foreach ($line in $sourceLines) {
    $lineInfo = Get-TomlLineInfo -Line $line -State $tomlState
    if ($lineInfo.IsTableHeader) {
      $skipProviderSection = [Regex]::IsMatch($line, $providerPattern)
    }
    if (-not $skipProviderSection) {
      $lines.Add([pscustomobject]@{
        Text = $line
        IsStructural = $lineInfo.IsStructural
        IsTableHeader = $lineInfo.IsTableHeader
      })
    }
  }
  $firstTable = $lines.Count
  for ($index = 0; $index -lt $lines.Count; $index++) {
    if ($lines[$index].IsTableHeader) {
      $firstTable = $index
      break
    }
  }

  $rootLines = New-Object System.Collections.Generic.List[string]
  for ($index = 0; $index -lt $firstTable; $index++) {
    $line = $lines[$index].Text
    if ($lines[$index].IsStructural -and $line -cmatch '^\s*(?:model_provider|service_tier|"model_provider"|"service_tier"|''model_provider''|''service_tier'')\s*=') {
      continue
    }
    $rootLines.Add($line)
  }
  while ($rootLines.Count -gt 0 -and [string]::IsNullOrWhiteSpace($rootLines[$rootLines.Count - 1])) {
    $rootLines.RemoveAt($rootLines.Count - 1)
  }

  $result = New-Object System.Collections.Generic.List[string]
  foreach ($line in $rootLines) {
    $result.Add($line)
  }
  if ($result.Count -gt 0) {
    $result.Add("")
  }
  $result.Add($RootMarkerStart)
  $result.Add(('model_provider = "{0}"' -f (ConvertTo-TomlBasicString $ProviderName)))
  $result.Add('service_tier = "priority"')
  $result.Add($RootMarkerEnd)

  if ($firstTable -lt $lines.Count) {
    $result.Add("")
    for ($index = $firstTable; $index -lt $lines.Count; $index++) {
      $result.Add($lines[$index].Text)
    }
  }
  while ($result.Count -gt 0 -and [string]::IsNullOrWhiteSpace($result[$result.Count - 1])) {
    $result.RemoveAt($result.Count - 1)
  }

  $result.Add("")
  $result.Add($ProviderMarkerStart)
  $result.Add(('[model_providers.{0}]' -f $ProviderName))
  $result.Add(('name = "{0}"' -f (ConvertTo-TomlBasicString $ProviderName)))
  $result.Add('wire_api = "responses"')
  $result.Add('requires_openai_auth = true')
  $result.Add(('base_url = "{0}"' -f (ConvertTo-TomlBasicString $BaseUrl)))
  $result.Add(('experimental_bearer_token = "{0}"' -f (ConvertTo-TomlBasicString $ApiKey)))
  $result.Add($ProviderMarkerEnd)

  return (($result -join $newLine) + $newLine)
}

$CodexHome = Get-FullPath $CodexHome
$ConfigPath = Join-Path $CodexHome "config.toml"
$AuthPath = Join-Path $CodexHome "auth.json"
$BackupRoot = Join-Path $CodexHome "backups_state\hybrid-relay-script"
$ActiveStatePath = Join-Path $BackupRoot "active.json"

$parsedBaseUrl = $null
if (-not [Uri]::TryCreate($BaseUrl, [UriKind]::Absolute, [ref]$parsedBaseUrl)) {
  throw "BaseUrl must be an absolute URL."
}
if ($parsedBaseUrl.Scheme -notin @("http", "https")) {
  throw "BaseUrl must use http or https."
}
if (-not [string]::IsNullOrWhiteSpace($parsedBaseUrl.UserInfo)) {
  throw "BaseUrl must not contain embedded credentials."
}
if (-not [string]::IsNullOrWhiteSpace($parsedBaseUrl.Query) -or -not [string]::IsNullOrWhiteSpace($parsedBaseUrl.Fragment)) {
  throw "BaseUrl must not contain a query string or fragment."
}
if ($parsedBaseUrl.Scheme -eq "http" -and -not $parsedBaseUrl.IsLoopback -and -not $AllowInsecureHttp) {
  throw "Plain HTTP is allowed only for loopback URLs. Use HTTPS or pass -AllowInsecureHttp explicitly."
}
$BaseUrl = $BaseUrl.Trim().TrimEnd('/')

if (-not $SkipLoginCheck -and -not (Test-ChatGptLogin -AuthPath $AuthPath)) {
  throw "No ChatGPT OAuth login was found in auth.json. Log in with ChatGPT first, or use -SkipLoginCheck only when credentials are stored elsewhere."
}
if (Test-Path -LiteralPath $ActiveStatePath -PathType Leaf) {
  throw "Hybrid relay is already managed by these scripts. Run restore-codex-hybrid.ps1 before enabling it again."
}

$originalExists = Test-Path -LiteralPath $ConfigPath -PathType Leaf
$original = if ($originalExists) { Read-Utf8File -Path $ConfigPath } else { "" }
$currentProviderName = Get-RootModelProvider -Content $original
if ([string]::IsNullOrWhiteSpace($ProviderName)) {
  $ProviderName = if ($currentProviderName -notin @("openai", "ollama", "lmstudio")) {
    $currentProviderName
  } else {
    "HybridRelay"
  }
}

if (-not $PSCmdlet.ShouldProcess($ConfigPath, "Enable the hybrid Responses provider and priority service tier")) {
  return
}

$apiKey = Get-ApiKey
if ([string]::IsNullOrWhiteSpace($apiKey)) {
  throw "The upstream API key cannot be empty. Set CODEX_HYBRID_API_KEY for this PowerShell process or enter the key when prompted."
}

$enabledConfig = New-HybridConfig -Original $original -ProviderName $ProviderName -CurrentProviderName $currentProviderName -BaseUrl $BaseUrl -ApiKey $apiKey
$expectedEnabledHash = Get-Utf8Sha256 $enabledConfig
$snapshotName = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss-fff")
$snapshotDir = Join-Path $BackupRoot $snapshotName
$backupPath = Join-Path $snapshotDir "config.original.toml"

New-Item -ItemType Directory -Path $snapshotDir -Force | Out-Null
if ($originalExists) {
  Copy-Item -LiteralPath $ConfigPath -Destination $backupPath
}

$state = [ordered]@{
  version = $StateVersion
  managedBy = $ManagedBy
  status = "prepared"
  enabledAtUtc = (Get-Date).ToUniversalTime().ToString("o")
  codexHome = $CodexHome
  configExisted = [bool]$originalExists
  backupPath = if ($originalExists) { Get-FullPath $backupPath } else { $null }
  originalConfigSha256 = if ($originalExists) { Get-FileSha256 $ConfigPath } else { $null }
  enabledConfigSha256 = $expectedEnabledHash
  providerName = $ProviderName
  previousProviderName = $currentProviderName
  providerIdPreserved = [bool]$ProviderName.Equals($currentProviderName, [StringComparison]::Ordinal)
  baseUrl = $BaseUrl
  serviceTier = "priority"
}

New-Item -ItemType Directory -Path $BackupRoot -Force | Out-Null
Write-JsonFileAtomic -Path $ActiveStatePath -Value $state

try {
  Write-Utf8FileAtomic -Path $ConfigPath -Content $enabledConfig
  if ((Get-FileSha256 $ConfigPath) -ne $expectedEnabledHash) {
    throw "The enabled config hash did not match the prepared content."
  }
  $state.status = "active"
  Write-JsonFileAtomic -Path $ActiveStatePath -Value $state
} catch {
  if ($originalExists) {
    Copy-Item -LiteralPath $backupPath -Destination $ConfigPath -Force
  } elseif (Test-Path -LiteralPath $ConfigPath -PathType Leaf) {
    Remove-Item -LiteralPath $ConfigPath -Force
  }
  if (Test-Path -LiteralPath $ActiveStatePath -PathType Leaf) {
    Remove-Item -LiteralPath $ActiveStatePath -Force
  }
  throw
} finally {
  $apiKey = $null
}

Write-Host "Hybrid relay enabled."
Write-Host "Provider: $ProviderName"
Write-Host "API key:  experimental_bearer_token in config.toml"
Write-Host "Config:   $ConfigPath"
Write-Host "Backup:   $snapshotDir"
Write-Host "Restart Codex before starting the next turn."
