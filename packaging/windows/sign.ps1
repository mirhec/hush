param([Parameter(Mandatory)][string]$File)
$ErrorActionPreference = 'Stop'
if (!$env:WINDOWS_CERTIFICATE_PFX) {
    Write-Host 'Building without an Authenticode signature.'
    return
}
if (!$env:RUNNER_TEMP) { throw 'Signing is intended for GitHub-hosted runners' }
if (!$env:WINDOWS_TIMESTAMP_URL) { throw 'WINDOWS_TIMESTAMP_URL is required for signing' }
$tool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" |
    Sort-Object FullName -Descending | Select-Object -First 1
if (!$tool) { throw 'Windows SDK signtool.exe was not found' }
$certificate = Join-Path $env:RUNNER_TEMP 'hush-signing.pfx'
try {
    [IO.File]::WriteAllBytes($certificate, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE_PFX))
    & $tool.FullName sign /f $certificate /p $env:WINDOWS_CERTIFICATE_PASSWORD /fd SHA256 /tr $env:WINDOWS_TIMESTAMP_URL /td SHA256 $File
    if ($LASTEXITCODE -ne 0) { throw 'Authenticode signing failed' }
    & $tool.FullName verify /pa $File
    if ($LASTEXITCODE -ne 0) { throw 'Authenticode verification failed' }
} finally {
    Remove-Item $certificate -Force -ErrorAction SilentlyContinue
}
