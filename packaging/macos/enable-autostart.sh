#!/usr/bin/env bash
# Explicit opt-in. Run after moving the app to its final location.
set -euo pipefail
app="${1:-/Applications/Hush.app}"
[[ "$app" = /* && -x "$app/Contents/MacOS/hush" ]] || { echo 'Absoluten Pfad zu einer vorhandenen Hush.app angeben.' >&2; exit 1; }
plist="$HOME/Library/LaunchAgents/io.hush.github.agent.plist"
mkdir -p "$HOME/Library/LaunchAgents"
python3 - "$app" "$plist" <<'PY'
import sys,plistlib,os
app,target=sys.argv[1:]
with open(target,'wb') as f:
    plistlib.dump({'Label':'io.hush.github.agent','ProgramArguments':[app+'/Contents/MacOS/hush','--tray'],'RunAtLoad':True,'KeepAlive':False,'ProcessType':'Interactive'},f)
os.chmod(target,0o600)
PY
launchctl bootout "gui/$(id -u)" "$plist" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$plist"
printf 'Autostart aktiviert. Entfernen: launchctl bootout gui/%s "%s"; anschließend diese Plist löschen.\n' "$(id -u)" "$plist"
