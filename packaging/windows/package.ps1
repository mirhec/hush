$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $root
try {
    $version = python3 packaging/version.py
    if ($LASTEXITCODE -ne 0) { throw 'Invalid release version' }
    $numeric = python3 packaging/version.py --field numeric
    if ($LASTEXITCODE -ne 0) { throw 'Invalid installer version' }
    & "$PSScriptRoot/verify-runtime.ps1"
    $compiler = Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe'
    if (!(Test-Path $compiler)) { throw 'NSIS 3 is required' }
    New-Item -ItemType Directory -Path dist -Force | Out-Null
    & $compiler /V3 /WX "/DHushVersion=$version" "/DHushNumericVersion=$numeric" packaging/windows/hush.nsi
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    if (!(Test-Path "dist/Hush-$version-windows-x86_64-setup.exe")) { throw 'Installer not found' }
} finally {
    Pop-Location
}
