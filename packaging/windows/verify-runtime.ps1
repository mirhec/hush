param([string]$File = 'target/release/hush.exe')
$ErrorActionPreference = 'Stop'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$dumpbin = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe' | Select-Object -Last 1
if (!$dumpbin) { throw 'dumpbin.exe is required to verify Windows runtime dependencies' }
$dependencies = & $dumpbin /DEPENDENTS $File
if ($LASTEXITCODE -ne 0) { throw 'Could not inspect the Windows executable' }
if ($dependencies -match '(?i)\b(vcruntime\w*|msvcp\w*|msvcr\w*)\.dll') {
    throw 'Hush must statically link the VC++ runtime; rebuild using .cargo/config.toml'
}
if ($dependencies -match '(?i)\b(opengl32|dxcompiler|dxil)\.dll') {
    throw 'Hush must use Windows DX12/FXC without external graphics runtime DLLs'
}
Write-Host 'Windows runtime imports: no VC++ redistributable or OpenGL dependency'
