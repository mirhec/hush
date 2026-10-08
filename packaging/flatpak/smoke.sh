#!/usr/bin/env bash
# For an isolated CI runner: installs the just-built bundle in that runner's profile.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
version="$(python3 "$root/packaging/version.py")"
flatpak --user install --noninteractive "$root/dist/Hush-$version-linux-x86_64.flatpak"
[[ "$(flatpak run io.hush.github --version)" == "Hush $version" ]]
flatpak run --command=sh io.hush.github -eu -c '
  trap "hush --stop" EXIT
  hush --start
  hush --status
  hush --stop
' > "$root/dist/flatpak-status.json"
python3 - "$root/dist/flatpak-status.json" <<'PY'
import json, sys
with open(sys.argv[1]) as file:
    status = json.load(file)
assert status["healthy"] is True, status
assert status["configured"] is False, status
assert status["service_error"] is None, status
print("Installed Flatpak version and background service verified without credentials")
PY
flatpak --user uninstall --noninteractive io.hush.github
