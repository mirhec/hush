#!/usr/bin/env bash
# Package a locally built executable. Version comes from Cargo.toml/Cargo.lock.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="${1:-$root/target/release/hush}"
[[ -f "$binary" ]] || { echo 'Zuerst cargo build --release ausführen.' >&2; exit 1; }
app="$root/dist/Hush.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 755 "$binary" "$app/Contents/MacOS/hush"
cp "$root/packaging/macos/Info.plist" "$app/Contents/Info.plist"
cp "$root/assets/hush.icns" "$app/Contents/Resources/hush.icns"
version="$(python3 "$root/packaging/version.py")"
numeric="$(python3 "$root/packaging/version.py" --field numeric)"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $numeric" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $numeric" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Add :HushVersion string $version" "$app/Contents/Info.plist"
if [[ -n "${MACOS_APP_IDENTITY:-}" ]]; then
  /usr/bin/codesign --force --options runtime --timestamp --sign "$MACOS_APP_IDENTITY" "$app"
else
  /usr/bin/codesign --force --sign - "$app"
fi
/usr/bin/codesign --verify --strict "$app"
printf 'Erstellt: %s\n' "$app"
