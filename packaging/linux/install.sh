#!/usr/bin/env bash
# Explicit, per-user install. Does not start anything and never requires sudo.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="${1:-$root/target/release/hush}"
[[ -f "$binary" ]] || { printf 'Binary fehlt: %s\nZuerst cargo build --release ausführen.\n' "$binary" >&2; exit 1; }
bin="$HOME/.local/bin/hush"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
install -d -m 755 "$HOME/.local/bin" "$data/applications" "$data/icons/hicolor/scalable/apps"
install -m 755 "$binary" "$bin"
install -m 644 "$root/assets/io.hush.github.svg" "$data/icons/hicolor/scalable/apps/io.hush.github.svg"
# Escape the Desktop Entry Exec value without evaluating it as a shell command.
escaped="${bin//\\/\\\\}"; escaped="${escaped//\"/\\\"}"; escaped="${escaped//%/%%}"
cat > "$data/applications/io.hush.github.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Hush
GenericName=GitHub Inbox
Comment=GitHub-Benachrichtigungen
Exec="$escaped"
Icon=io.hush.github
Terminal=false
Categories=Development;Utility;
StartupNotify=true
StartupWMClass=io.hush.github
DESKTOP
chmod 644 "$data/applications/io.hush.github.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
printf 'Installiert: %s\nIm Launcher als Hush verfügbar. Autostart ist nicht aktiviert.\n' "$bin"
