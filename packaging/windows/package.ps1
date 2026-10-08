$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $root
try {
    $version = python3 packaging/version.py
    if ($LASTEXITCODE -ne 0) { throw 'Invalid release version' }
    $numeric = python3 packaging/version.py --field numeric
    if ($LASTEXITCODE -ne 0) { throw 'Invalid installer version' }
    $compiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
    if (!(Test-Path $compiler)) { throw 'Inno Setup 6 is required' }
    & $compiler "/DHushVersion=$version" "/DHushNumericVersion=$numeric" packaging/windows/hush.iss
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    if (!(Test-Path "dist/Hush-$version-windows-x86_64-setup.exe")) { throw 'Installer not found' }
} finally {
    Pop-Location
}
