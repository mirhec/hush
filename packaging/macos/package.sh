#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
arch="${1:-$(uname -m)}"
case "$arch" in arm64|x86_64) ;; *) echo "Unsupported architecture: $arch" >&2; exit 1 ;; esac
version="$(python3 "$root/packaging/version.py")"
numeric="$(python3 "$root/packaging/version.py" --field numeric)"
[[ "$(lipo -archs "$root/target/release/hush")" == "$arch" ]] || { echo 'Binary architecture mismatch' >&2; exit 1; }
bash "$root/packaging/macos/bundle.sh"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
mkdir -p "$work/root/Applications"
ditto "$root/dist/Hush.app" "$work/root/Applications/Hush.app"
pkgbuild --analyze --root "$work/root" "$work/components.plist"
/usr/libexec/PlistBuddy -c 'Set :0:BundleIsRelocatable false' "$work/components.plist"
package="$root/dist/Hush-$version-macos-$arch.pkg"
arguments=(--root "$work/root" --component-plist "$work/components.plist"
  --identifier io.hush.github --version "$numeric" --install-location / --ownership recommended)
if [[ -n "${MACOS_INSTALLER_IDENTITY:-}" ]]; then
  [[ -n "${MACOS_APP_IDENTITY:-}" ]] || { echo 'Installer signing also requires app signing' >&2; exit 1; }
  arguments+=(--sign "$MACOS_INSTALLER_IDENTITY" --timestamp)
fi
pkgbuild "${arguments[@]}" "$package"
if [[ -n "${APPLE_ID:-}" ]]; then
  : "${APPLE_TEAM_ID:?Missing Apple team ID}"
  : "${APPLE_APP_PASSWORD:?Missing Apple app-specific password}"
  : "${MACOS_INSTALLER_IDENTITY:?Notarization requires signed packages}"
  xcrun notarytool submit "$package" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_PASSWORD" --wait --timeout 20m
  xcrun stapler staple "$package"
  xcrun stapler validate "$package"
fi
pkgutil --payload-files "$package" | grep -q 'Hush.app/Contents/MacOS/hush'
printf 'Erstellt: %s\n' "$package"
