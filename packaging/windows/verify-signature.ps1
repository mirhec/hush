param([Parameter(Mandatory)][string]$File)
$ErrorActionPreference = 'Stop'
$signature = Get-AuthenticodeSignature -LiteralPath $File
if ($signature.Status -ne 'Valid' -or !$signature.SignerCertificate) {
    throw "A trusted Authenticode signature is required for ${File}: $($signature.Status)"
}
if ($signature.SignatureType -ne 'Authenticode') {
    throw "An embedded Authenticode signature is required for ${File}"
}
if (!$signature.TimeStamperCertificate) {
    throw "An Authenticode timestamp is required for ${File}"
}
Write-Host "Verified signature and timestamp: $File ($($signature.SignerCertificate.Subject))"
