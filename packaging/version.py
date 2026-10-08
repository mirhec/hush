#!/usr/bin/env python3
"""Stamp release tags into Cargo; read one version for the app and installers."""
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


def numeric_version(version):
    match = SEMVER.fullmatch(version)
    if not match or any(int(n) > 65535 for n in match.groups()[:3]):
        raise ValueError(f"Unsupported installer version: {version}")
    return ".".join(match.groups()[:3])


def versions(root=ROOT, tag=""):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    version = manifest["package"]["version"]
    numeric = numeric_version(version)
    packages = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    locked = [p["version"] for p in packages if p["name"] == "hush" and "source" not in p]
    if locked != [version]:
        raise ValueError("Cargo.toml and Cargo.lock must contain the same Hush version")
    if tag and tag not in (version, "v" + version):
        raise ValueError(f"Release tag {tag!r} does not match Cargo version {version!r}")
    return version, numeric


def stamp_release(tag, root=ROOT):
    """Change only Hush's package versions in an isolated release checkout.

    Keep dependency versions, checksums and formatting intact so --locked and
    Flatpak's offline --frozen builds continue to use the committed dependencies.
    Prepare and validate both replacements before writing either file.
    """
    if not tag:
        raise ValueError("--stamp requires a release tag, for example 1.0.2 or v1.0.2")
    release = tag.removeprefix("v")
    numeric = numeric_version(release)
    versions(root)  # Refuse to hide an already inconsistent manifest/lockfile.
    replacements = []
    for filename, header, array in [
        ("Cargo.toml", "[package]", False),
        ("Cargo.lock", "[[package]]", True),
    ]:
        path = root / filename
        original = path.read_text(encoding="utf-8")
        pattern = re.compile(r"^" + re.escape(header) + r"[^\n]*\n.*?(?=^\[|\Z)", re.M | re.S)
        changed = 0

        def update(match):
            nonlocal changed
            block = match.group()
            package = tomllib.loads(block)["package"]
            if array:
                package = package[0]
            if package.get("name") != "hush" or "source" in package:
                return block
            updated, count = re.subn(
                r'''^([ \t]*version[ \t]*=[ \t]*)(["'])[^\n]*?\2([ \t]*(?:#.*)?)$''',
                lambda field: field[1] + field[2] + release + field[2] + field[3],
                block,
                flags=re.M,
            )
            if count != 1:
                raise ValueError(f"Expected one explicit Hush version in {filename}")
            changed += 1
            return updated

        updated = pattern.sub(update, original)
        if changed != 1:
            raise ValueError(f"Expected one Hush package in {filename}")
        parsed = tomllib.loads(updated)["package"]
        packages = parsed if array else [parsed]
        if [p["version"] for p in packages if p["name"] == "hush" and "source" not in p] != [release]:
            raise ValueError(f"Failed to set the release version in {filename}")
        replacements.append((path, updated))
    for path, updated in replacements:
        path.write_text(updated, encoding="utf-8", newline="\n")
    return release, numeric


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", default=os.environ.get("RELEASE_TAG", ""))
    parser.add_argument("--stamp", action="store_true", help="Apply the release tag to Cargo.toml and Cargo.lock before building")
    parser.add_argument("--field", choices=("version", "numeric"), default="version")
    parser.add_argument("--github-env", type=Path)
    args = parser.parse_args()
    try:
        if args.stamp:
            stamp_release(args.tag)
        version, numeric = versions(tag=args.tag)
    except (ValueError, KeyError) as error:
        parser.error(str(error))
    if args.github_env:
        with args.github_env.open("a", encoding="utf-8") as env:
            env.write(f"HUSH_VERSION={version}\nHUSH_NUMERIC_VERSION={numeric}\n")
    print(version if args.field == "version" else numeric)


if __name__ == "__main__":
    main()
