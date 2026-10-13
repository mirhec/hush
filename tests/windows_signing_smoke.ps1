# Verify the real Windows trust checks without a production signing key.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$verify = Join-Path $root 'packaging/windows/verify-signature.ps1'
$sign = Join-Path $root 'packaging/windows/sign.ps1'

function Assert-Rejected([scriptblock]$Action, [string]$Expected) {
    $failure = $null
    try { & $Action } catch { $failure = $_.Exception.Message }
    if (!$failure -or $failure -notlike "*$Expected*") {
        throw "Expected rejection containing '$Expected', got '$failure'"
    }
}

$temporary = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory $temporary | Out-Null
$previousPfx = $env:WINDOWS_CERTIFICATE_PFX
$certificate = $null
try {
    $unsigned = Join-Path $temporary 'unsigned.ps1'
    Set-Content -LiteralPath $unsigned -Value 'Write-Output "Signing test fixture"'
    Assert-Rejected { & $verify -File $unsigned } 'trusted Authenticode signature'
    Write-Host 'Unsigned files are rejected: OK'

    # Only this process is changed; never display or use any production key.
    Remove-Item Env:WINDOWS_CERTIFICATE_PFX -ErrorAction SilentlyContinue
    Assert-Rejected { & $sign -File $unsigned } 'requires WINDOWS_CERTIFICATE_PFX'
    Write-Host 'Missing signing credentials fail instead of producing an unsigned release: OK'

    # The installed PowerShell executable has a real Microsoft signature and
    # timestamp. No test CA is added to Windows trusted root stores.
    $trusted = Join-Path $PSHOME 'pwsh.exe'
    & $verify -File $trusted
    Write-Host 'Trusted, timestamped executable is accepted: OK'

    $tampered = Join-Path $temporary 'tampered.exe'
    $bytes = [IO.File]::ReadAllBytes($trusted)
    # Change a DOS stub byte, preserving the PE header and certificate table.
    $bytes[0x40] = $bytes[0x40] -bxor 1
    [IO.File]::WriteAllBytes($tampered, $bytes)
    Assert-Rejected { & $verify -File $tampered } 'trusted Authenticode signature'
    Write-Host 'Modified signed executable is rejected: OK'

    $certificate = New-SelfSignedCertificate -Type CodeSigningCert `
        -Subject 'CN=Hush CI untrusted test certificate' -CertStoreLocation Cert:\CurrentUser\My
    $testSignature = Set-AuthenticodeSignature -LiteralPath $unsigned -Certificate $certificate -HashAlgorithm SHA256
    if (!$testSignature.SignerCertificate -or $testSignature.SignerCertificate.Thumbprint -ne $certificate.Thumbprint) {
        throw 'The untrusted signature fixture was not created'
    }
    Assert-Rejected { & $verify -File $unsigned } 'trusted Authenticode signature'
    Write-Host 'Self-signed certificate without public trust is rejected: OK'
} finally {
    if ($null -eq $previousPfx) {
        Remove-Item Env:WINDOWS_CERTIFICATE_PFX -ErrorAction SilentlyContinue
    } else {
        $env:WINDOWS_CERTIFICATE_PFX = $previousPfx
    }
    if ($certificate) {
        Remove-Item "Cert:\CurrentUser\My\$($certificate.Thumbprint)" -DeleteKey -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
