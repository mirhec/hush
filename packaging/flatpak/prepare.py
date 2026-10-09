#!/usr/bin/env python3
"""Stage only build inputs, then vendor locked Cargo sources for the sandbox."""
import argparse
import datetime
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("hush_version", ROOT / "packaging/version.py")
version_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(version_module)


def prepare(destination, vendor=True):
    version, _ = version_module.versions()
    # Refuse to overwrite existing files or copy arbitrary worktree content.
    destination.mkdir(parents=True, exist_ok=False)
    source = destination / "source"
    source.mkdir()
    for name in ["src", "tests", "assets", "packaging"]:
        shutil.copytree(ROOT / name, source / name, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    for name in ["Cargo.toml", "Cargo.lock", "LICENSE"]:
        shutil.copy2(ROOT / name, source / name)
    manifest = json.loads((ROOT / "packaging/flatpak/io.hush.github.json").read_text())
    # Forward only this public build identifier into the sandbox, never access tokens.
    client_id = os.environ.get("HUSH_GITHUB_CLIENT_ID", "").strip()
    if client_id:
        if not client_id.isascii() or not client_id.replace("_", "").isalnum():
            raise ValueError("Invalid HUSH_GITHUB_CLIENT_ID")
        manifest["modules"][0]["build-options"]["env"]["HUSH_GITHUB_CLIENT_ID"] = client_id
    (destination / "io.hush.github.json").write_text(json.dumps(manifest, indent=2) + "\n")
    metadata = source / "packaging/flatpak/io.hush.github.metainfo.xml"
    tree = ET.parse(metadata)
    ET.SubElement(tree.getroot().find("releases"), "release", {
        "version": version, "date": datetime.date.today().isoformat(),
        "type": "development" if "-" in version else "stable",
    })
    tree.write(metadata, encoding="utf-8", xml_declaration=True)
    if vendor:
        (source / ".cargo").mkdir()
        with (source / ".cargo/config.toml").open("w") as config:
            subprocess.run(["cargo", "vendor", "--locked", "--versioned-dirs", "vendor"], cwd=source, stdout=config, check=True)
    return destination / "io.hush.github.json"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    print(prepare(args.destination.resolve()))
