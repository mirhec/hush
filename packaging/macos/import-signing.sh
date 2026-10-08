#!/usr/bin/env bash
# A P12 may contain both Developer ID Application and Developer ID Installer.
set -euo pipefail
if [[ -z "${MACOS_CERTIFICATE_P12:-}" ]]; then
  if [[ -n "${MACOS_APP_IDENTITY:-}${MACOS_INSTALLER_IDENTITY:-}${APPLE_ID:-}" ]]; then
    echo 'Signing configuration is incomplete: MACOS_CERTIFICATE_P12 is missing' >&2
    exit 1
  fi
  echo 'Building without Developer ID signature or notarization.'
  exit 0
fi
: "${MACOS_CERTIFICATE_PASSWORD:?Missing certificate password}"
: "${MACOS_APP_IDENTITY:?Missing Developer ID Application identity}"
: "${MACOS_INSTALLER_IDENTITY:?Missing Developer ID Installer identity}"
: "${RUNNER_TEMP:?This script is intended for GitHub-hosted runners}"
certificate="$RUNNER_TEMP/hush-signing.p12"
keychain="$RUNNER_TEMP/hush-signing.keychain-db"
keychain_password="$(openssl rand -hex 32)"
printf '%s' "$MACOS_CERTIFICATE_P12" | base64 --decode > "$certificate"
chmod 600 "$certificate"
trap 'rm -f -- "$certificate"' EXIT
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security import "$certificate" -P "$MACOS_CERTIFICATE_PASSWORD" -k "$keychain" -T /usr/bin/codesign -T /usr/bin/productsign -T /usr/bin/pkgbuild
security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" "$HOME/Library/Keychains/login.keychain-db"
