#!/usr/bin/env python3
"""Read one version for Cargo, release assets, and native installers."""
import argparse
import os
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-((?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)


def versions(root=ROOT, tag=""):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    version = manifest["package"]["version"]
    match = SEMVER.fullmatch(version)
    if not match or any(int(n) > 65535 for n in match.groups()[:3]):
        raise ValueError(f"Unsupported installer version: {version}")
    packages = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    locked = [p["version"] for p in packages if p["name"] == "hush" and "source" not in p]
    if locked != [version]:
        raise ValueError("Cargo.toml and Cargo.lock must contain the same Hush version")
    if tag and tag not in (version, "v" + version):
        raise ValueError(f"Release tag {tag!r} does not match Cargo version {version!r}")
    return version, ".".join(match.groups()[:3])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", default=os.environ.get("RELEASE_TAG", ""))
    parser.add_argument("--field", choices=("version", "numeric"), default="version")
    parser.add_argument("--github-env", type=Path)
    args = parser.parse_args()
    try:
        version, numeric = versions(tag=args.tag)
    except (ValueError, KeyError) as error:
        parser.error(str(error))
    if args.github_env:
        with args.github_env.open("a", encoding="utf-8") as env:
            env.write(f"HUSH_VERSION={version}\nHUSH_NUMERIC_VERSION={numeric}\n")
    print(version if args.field == "version" else numeric)


if __name__ == "__main__":
    main()
