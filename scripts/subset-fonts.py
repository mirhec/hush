#!/usr/bin/env python3
"""Build Hush's bundled CJK fonts from Noto Sans CJK Regular 2.004.

Requires fonttools==4.66.1. The source collection is supplied locally; this
script and the application never download fonts. See assets/fonts/README.md.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont


ROOT = Path(__file__).resolve().parents[1]
SOURCE_SHA256 = "b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a"
LANGUAGE_NAMES = "Deutsch English Español Français Português 简体中文 日本語"
FACES = (
    (0, "Noto Sans CJK JP", "Hush CJK Japanese", "HushCJKJapanese-Regular.otf"),
    (2, "Noto Sans CJK SC", "Hush CJK Simplified Chinese", "HushCJKChinese-Regular.otf"),
)


def catalog_codepoints() -> set[int]:
    """Include every translation and native language name, not just UI labels."""
    characters = set(LANGUAGE_NAMES)
    for path in sorted((ROOT / "src" / "locales").glob("*.json")):
        messages = json.loads(path.read_text(encoding="utf-8"))
        characters.update("".join(messages.values()))
    return {ord(character) for character in characters if not character.isspace()}


def requested_codepoints() -> set[int]:
    codepoints = catalog_codepoints()
    # Cover normal GitHub titles as well as the fixed interface. Unicode's
    # basic Han block includes the common Chinese and Japanese ideographs.
    for first, last in (
        (0x0020, 0x024F),  # Latin and Latin extensions
        (0x0300, 0x052F),  # Combining marks, Greek and Cyrillic
        (0x2000, 0x206F),  # General punctuation
        (0x20A0, 0x20CF),  # Currency symbols
        (0x2190, 0x21FF),  # Arrows
        (0x3000, 0x30FF),  # CJK punctuation, Hiragana and Katakana
        (0x31F0, 0x31FF),  # Katakana phonetic extensions
        (0x4E00, 0x9FFF),  # CJK Unified Ideographs
        (0xFF00, 0xFFEF),  # Fullwidth forms and halfwidth Katakana
    ):
        codepoints.update(range(first, last + 1))
    return codepoints


def rename_subset(font: TTFont, family: str) -> None:
    """Identify the modified fonts while preserving upstream attribution."""
    postscript_name = family.replace(" ", "") + "-Regular"
    replacements = {
        1: family,
        2: "Regular",
        3: postscript_name + ";2.004;Hush subset",
        4: family + " Regular",
        6: postscript_name,
        # Keep the full license with the embedded font even when an installer
        # ships only the application executable and omits repository docs.
        13: (ROOT / "assets" / "fonts" / "OFL.txt").read_text(encoding="utf-8"),
        16: family,
        17: "Regular",
        18: family + " Regular",
    }
    for name in font["name"].names:
        if name.nameID in replacements:
            name.string = replacements[name.nameID].encode(name.getEncoding())
    cff = font["CFF "].cff
    cff.fontNames = [postscript_name]
    cff.topDictIndex[0].FamilyName = family
    cff.topDictIndex[0].FullName = family + " Regular"


def build_font(source: Path, index: int, expected_family: str, family: str) -> bytes:
    font = TTFont(source, fontNumber=index, recalcTimestamp=False)
    if font["name"].getDebugName(1) != expected_family:
        raise ValueError(f"Unexpected font family at collection index {index}")
    available = set(font.getBestCmap())
    missing = catalog_codepoints() - available
    if missing:
        raise ValueError(f"Source font lacks translated characters: {sorted(missing)}")

    options = subset.Options()
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 13, 14, 16, 17, 18]
    options.name_languages = ["*"]
    options.name_legacy = True
    # egui rasterizes outlines without TrueType/CFF hinting or OpenType shaping.
    # Each face's own cmap already selects its language's default glyph forms.
    options.hinting = False
    options.layout_features = []
    options.recalc_timestamp = False
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=requested_codepoints() & available)
    subsetter.subset(font)
    rename_subset(font, family)
    output = io.BytesIO()
    font.save(output, reorderTables=True)
    font.close()

    # Reopen the serialized font to check the actual shipped cmap.
    with TTFont(io.BytesIO(output.getvalue())) as shipped:
        if catalog_codepoints() - set(shipped.getBestCmap()):
            raise ValueError("Subset lost a translated character")
    return output.getvalue()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="Path to NotoSansCJK-Regular.ttc 2.004")
    parser.add_argument(
        "--output-dir", type=Path, default=ROOT / "assets" / "fonts"
    )
    parser.add_argument(
        "--check", action="store_true", help="Verify byte-for-byte reproducibility without writing"
    )
    args = parser.parse_args()
    digest = hashlib.sha256(args.source.read_bytes()).hexdigest()
    if digest != SOURCE_SHA256:
        parser.error("Source checksum differs from Noto Sans CJK Regular 2.004; see assets/fonts/README.md")
    for index, expected, family, filename in FACES:
        contents = build_font(args.source, index, expected, family)
        destination = args.output_dir / filename
        if args.check:
            if not destination.exists() or destination.read_bytes() != contents:
                parser.error(f"{destination} needs regeneration")
        else:
            args.output_dir.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(contents)
        print(f"{filename}: {len(contents):,} bytes; sha256={hashlib.sha256(contents).hexdigest()}")


if __name__ == "__main__":
    main()
