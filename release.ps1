# Builds the downloads of a HeMiPad release - the same on a PC and in the GitHub workflow (.github/workflows/release.yml):
#   build\dist\HeMiPad-Setup.exe     the installer (Inno Setup 6)
#   build\dist\HeMiPad-Portable.zip  just the program, runs from any folder
# Needs: Rust (stable, MSVC) and Inno Setup 6. The version comes from Cargo.toml.
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
Set-Location $root
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { $env:Path += ";$env:USERPROFILE\.cargo\bin" }

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$iscc = @((Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source,
          "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
        Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 is not installed (winget install JRSoftware.InnoSetup).' }

# the build: cargo writes to build\ and puts the C runtime into the exe (.cargo\config.toml)
$ErrorActionPreference = 'Continue'
cargo build --release
$ok = $LASTEXITCODE -eq 0
$ErrorActionPreference = 'Stop'
if (-not $ok) { throw 'The build failed.' }
$exe = Join-Path $root 'build\release\HeMiPad.exe'

$dist = Join-Path $root 'build\dist'
Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $dist | Out-Null
Write-Host "HeMiPad $version" -ForegroundColor Cyan

& $iscc /Q "/DAppVersion=$version" "/DAppExe=$exe" "/DOutputDir=$dist" (Join-Path $root 'installer\HeMiPad.iss')
if ($LASTEXITCODE) { throw 'The installer could not be made.' }

$stage = Join-Path $root 'build\pack\HeMiPad'
Remove-Item (Split-Path $stage) -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage | Out-Null
Copy-Item $exe $stage
Compress-Archive $stage -DestinationPath "$dist\HeMiPad-Portable.zip"
Remove-Item (Split-Path $stage) -Recurse -Force

Get-ChildItem $dist | ForEach-Object { Write-Host ("  {0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB)) }
