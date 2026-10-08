#!/usr/bin/env bash
# Optional building block for a custom DMS widget; not an automatically installed plugin.
# No token, title, actor or repository is printed. jq is needed only for this helper.
set -euo pipefail
"${HOME}/.local/bin/hush" --status | jq -c '{text:(if .unread>0 then (.unread|tostring) else "" end),tooltip:(if .paused then "Hush · pausiert" elif .running then "Hush · GitHub Inbox" else "Hush · Dienst inaktiv" end),class:(if .warnings>0 then "warning" else "normal" end)}'
