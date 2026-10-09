#!/usr/bin/env bash
# Optional command-line alternative to Start at login in Hush settings.
# Run after moving the app to its final location. Takes effect at the next login.
set -euo pipefail
app="${1:-/Applications/Hush.app}"
[[ "$app" = /* && -x "$app/Contents/MacOS/hush" ]] || { echo 'Provide the absolute path to an existing Hush.app.' >&2; exit 1; }
plist="$HOME/Library/LaunchAgents/io.hush.github.agent.plist"
mkdir -p "$HOME/Library/LaunchAgents"
python3 - "$app" "$plist" <<'PY'
import sys,plistlib,os
app,target=sys.argv[1:]
with open(target,'wb') as f:
    plistlib.dump({'Label':'io.hush.github.agent','ProgramArguments':[app+'/Contents/MacOS/hush','--tray'],'RunAtLoad':True,'KeepAlive':False,'ProcessType':'Interactive'},f)
os.chmod(target,0o600)
PY
# Avoid unloading a running LaunchAgent: it may own the current Hush process.
printf 'Autostart enabled for the next login. Disable it in Hush settings or remove "%s".\n' "$plist"
