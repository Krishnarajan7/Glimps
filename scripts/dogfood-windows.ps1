<#
.SYNOPSIS
  Fresh-clone dogfood helper for GLIMPS on Windows (experimental).

.DESCRIPTION
  Usage: scripts\dogfood-windows.ps1 [check|session|probe]

  check     Build and run the repo-local automated checks. Installs nothing.
  session   Build and start a disposable GLIMPS-wrapped PowerShell.
  probe     Run the ConPTY byte-fidelity probes from docs/windows.md and
            print each capture, so you can see whether ConPTY passes bytes
            through (the open question that decides Windows support).

  No mode edits $PROFILE, installs GLIMPS globally, or changes anything outside
  the repo and a temporary directory. The session uses a temporary .glimpsrc.

  Works on PowerShell 7 (pwsh) and Windows PowerShell 5.1.
  Windows is experimental; read docs/windows.md first.
#>
[CmdletBinding()]
param(
  [Parameter(Position = 0)]
  [ValidateSet('check', 'session', 'probe')]
  [string]$Mode = 'check'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Bin = Join-Path $Root 'target\debug\glimps.exe'

function Require-Tool([string]$Name, [string]$Hint) {
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    throw "missing required tool: $Name. $Hint"
  }
}

function Require-Cargo {
  Require-Tool 'cargo' 'Install rustup (https://rustup.rs) with the default stable-x86_64-pc-windows-msvc toolchain; it needs the Visual Studio Build Tools "Desktop development with C++" workload.'
}

# Run a native command and fail loudly on a non-zero exit. The command is a
# script block so callers can use the stop-parsing token (--%), which is the
# one reliable way to hand `--` through to cargo on every PowerShell version.
function Invoke-Native([string]$Label, [scriptblock]$Command) {
  Write-Host ">> $Label" -ForegroundColor Cyan
  & $Command
  if ($LASTEXITCODE -ne 0) {
    throw "command failed with exit $LASTEXITCODE`: $Label"
  }
}

function Pick-Shell {
  # Mirror the supervisor's own default: PowerShell 7 if present, else 5.1.
  if (Get-Command pwsh -ErrorAction SilentlyContinue) { return 'pwsh' }
  return 'powershell'
}

function Run-Check {
  Require-Cargo
  Set-Location $Root
  Invoke-Native 'cargo fmt --all --check' { cargo --% fmt --all --check }
  Invoke-Native 'cargo clippy --all-targets --all-features -- -D warnings' { cargo --% clippy --all-targets --all-features -- -D warnings }
  Invoke-Native 'cargo test --all --all-features' { cargo --% test --all --all-features }
  Invoke-Native 'cargo bench --no-run' { cargo --% bench --no-run }
  if (Get-Command cargo-audit -ErrorAction SilentlyContinue) {
    Invoke-Native 'cargo audit' { cargo --% audit }
  } else {
    Write-Warning 'cargo-audit is not installed; skipping dependency advisory check.'
  }
  Write-Host 'check: all green.' -ForegroundColor Green
}

function Run-Probe {
  Require-Cargo
  Set-Location $Root
  $env:GLIMPS_PROBE_SHELL = Pick-Shell
  Write-Host "Probing ConPTY through $env:GLIMPS_PROBE_SHELL. Run this once inside Windows Terminal and once inside a legacy conhost window (Win+R, cmd) and compare." -ForegroundColor Yellow
  Write-Host ''
  Write-Host '--- probe 1: a JSON line longer than a 40-column console. Expect: 0 cursor moves, one visible line longer than 40 cells.' -ForegroundColor Cyan
  cargo --% run --quiet --example pty_probe -- --cols 40 %GLIMPS_PROBE_SHELL% -NoLogo -Command "'{""a"":1,""name"":""a fairly long json line that exceeds forty columns"",""n"":[1,2,3]}'"
  Write-Host ''
  Write-Host '--- probe 2: shell-integration markers. Expect: OSC 133;C, 133;D and 7337 sequences present in the capture.' -ForegroundColor Cyan
  $env:GLIMPS_ACTIVE = '1'
  try {
    cargo --% run --quiet --example pty_probe -- --send "glimps init %GLIMPS_PROBE_SHELL% | Out-String | Invoke-Expression\r" --send "echo hi\r" --send "exit\r" %GLIMPS_PROBE_SHELL% -NoLogo
  } finally {
    Remove-Item Env:\GLIMPS_ACTIVE -ErrorAction SilentlyContinue
    Remove-Item Env:\GLIMPS_PROBE_SHELL -ErrorAction SilentlyContinue
  }
  Write-Host ''
  Write-Host 'Read the verdict table in docs/windows.md, step 2.' -ForegroundColor Yellow
}

function Run-Session {
  Require-Cargo
  Set-Location $Root
  Invoke-Native 'cargo build' { cargo --% build }

  $shell = Pick-Shell
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('glimps-dogfood-' + [System.IO.Path]::GetRandomFileName())
  New-Item -ItemType Directory -Path $tmp | Out-Null
  $rc = Join-Path $tmp '.glimpsrc'
  # BOM-less: the rc is parsed as bytes.
  [System.IO.File]::WriteAllText($rc, "enabled = true`ncolor = true`nseparator = true`ntimestamp = true`n", (New-Object System.Text.UTF8Encoding($false)))

  $initLine = "& '$Bin' init $shell | Out-String | Invoke-Expression"
  try { Set-Clipboard -Value $initLine } catch { }

  Write-Host @"
Starting a disposable GLIMPS session wrapping $shell.

PowerShell has no ZDOTDIR equivalent, so the integration is not auto-loaded and
`$PROFILE is left untouched. Once inside, paste this line (already on your
clipboard) to install the OSC-133 markers for this session only:

  $initLine

(That points at the repo's debug binary so nothing needs installing. The
permanent line for `$PROFILE, once you are satisfied, is what
``glimps setup $shell`` prints:
  if (Get-Command glimps -ErrorAction SilentlyContinue) { glimps init $shell | Out-String | Invoke-Expression })

Then try, in this order (docs/windows.md, step 3; items 5-8 there cover the
metadata channel, `exit` from the profile, legacy code pages, StrictMode):
  exit                                  # FIRST: does the outer shell come back intact? arrows, colors?
  '{"alpha":1,"items":[2,3]}'
  "INFO boot``nWARN disk``nERROR boom"
  git status --short
  git --no-pager log --oneline -5
  Get-Content README.md
  mysql -u root -e "SHOW DATABASES"     # if mysql is installed
  `$env:GLIMPS='0'; '{"off":true}'; Remove-Item Env:\GLIMPS
  vim README.md                         # bypass by name, if installed
  Get-ChildItem                         # plain output stays plain

Check: arrow keys, Ctrl+C, Tab completion, dragging the window to resize.
Exit with: exit
"@

  $previousRc = [Environment]::GetEnvironmentVariable('GLIMPSRC')
  $status = 1
  try {
    $env:GLIMPSRC = $rc
    & $Bin --shell $shell
    $status = $LASTEXITCODE
  } finally {
    [Environment]::SetEnvironmentVariable('GLIMPSRC', $previousRc)
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }
  Write-Host "GLIMPS session ended with status $status."
  exit $status
}

switch ($Mode) {
  'check'   { Run-Check }
  'session' { Run-Session }
  'probe'   { Run-Probe }
}
