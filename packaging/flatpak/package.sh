#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
arch="$(flatpak --default-arch)"
[[ "$arch" == x86_64 ]] || { echo 'This release pipeline currently builds Linux x86_64.' >&2; exit 1; }
version="$(python3 "$root/packaging/version.py")"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
python3 "$root/packaging/flatpak/prepare.py" "$work/staged"
flatpak-builder --user --force-clean --disable-rofiles-fuse \
  --install-deps-from=flathub --repo="$work/repo" \
  "$work/build" "$work/staged/io.hush.github.json"
mkdir -p "$root/dist"
flatpak build-bundle "$work/repo" "$root/dist/Hush-$version-linux-$arch.flatpak" \
  io.hush.github stable --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
