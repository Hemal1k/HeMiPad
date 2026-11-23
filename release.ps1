# Builds the downloads of a HeMiPad release - the same on a PC and in the GitHub workflow (.github/workflows/release.yml):
#   build\dist\HeMiPad-Setup.exe            the installer (Inno Setup 6): x64, and the ARM64 program on Windows on ARM
#   build\dist\HeMiPad-Portable.exe         just the program for x64 (one file, no zip around it)
#   build\dist\HeMiPad-Portable-arm64.exe   the same for Windows on ARM
# Needs: Rust (stable, MSVC), Inno Setup 6, and for ARM64 the MSVC ARM64 build tools (on a PC without them ARM64 is
# left out with a warning; the GitHub workflow always builds both). The version comes from Cargo.toml.
param([switch]$NoArm)
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
Set-Location $root
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { $env:Path += ";$env:USERPROFILE\.cargo\bin" }
$app = 'HeMiPad'

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$iscc = @((Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source,
          "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
        Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 is not installed (https://jrsoftware.org/isdl.php).' }

# the builds: cargo writes to build\<target>\release and puts the C runtime into the exe (.cargo\config.toml)
$targets = [ordered]@{ x64 = 'x86_64-pc-windows-msvc'; arm64 = 'aarch64-pc-windows-msvc' }
$exes = [ordered]@{}
foreach ($arch in $targets.Keys) {
    if ($arch -eq 'arm64' -and $NoArm) { continue }
    $triple = $targets[$arch]
    $ErrorActionPreference = 'Continue'
    rustup target add $triple *> $null
    cargo build --release --target $triple
    $ok = $LASTEXITCODE -eq 0
    $ErrorActionPreference = 'Stop'
    if (-not $ok) {
        if ($arch -eq 'arm64' -and -not $env:GITHUB_ACTIONS) { Write-Warning 'ARM64 could not be built on this PC (the MSVC ARM64 build tools are missing?) - x64 only.'; continue }
        throw "The build failed ($arch)."
    }
    $exes[$arch] = Join-Path $root "build\$triple\release\$app.exe"
}

$dist = Join-Path $root 'build\dist'
Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $dist | Out-Null
Write-Host "$app $version ($($exes.Keys -join ', '))" -ForegroundColor Cyan

$isccArgs = @('/Q', "/DAppVersion=$version", "/DAppExe=$($exes['x64'])", "/DOutputDir=$dist")
if ($exes['arm64']) { $isccArgs += "/DAppExeArm=$($exes['arm64'])" }
& $iscc @isccArgs (Join-Path $root "installer\$app.iss")
if ($LASTEXITCODE) { throw 'The installer could not be made.' }

# the portable versions: the program itself, one file - no zip around it
foreach ($arch in $exes.Keys) {
    $portable = if ($arch -eq 'x64') { "$dist\$app-Portable.exe" } else { "$dist\$app-Portable-$arch.exe" }
    Copy-Item $exes[$arch] $portable
}

Get-ChildItem $dist | ForEach-Object { Write-Host ("  {0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB)) }