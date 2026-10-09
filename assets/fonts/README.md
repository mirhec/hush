# Bundled CJK fonts

Hush bundles two modified subsets of **Noto Sans CJK Regular 2.004** so Chinese
and Japanese text can render without downloading fonts or relying on an
operating system font installation:

- `HushCJKJapanese-Regular.otf`: Japanese glyph forms, collection face 0.
- `HushCJKChinese-Regular.otf`: Simplified Chinese glyph forms, collection face 2.

The application uses the appropriate locale's font as a fallback behind its
regular UI font. Both subsets cover all translation catalogs and native
language names, all 20,976 basic CJK Unified Ideographs available in the source
font (`U+4E00–U+9FFF`), Hiragana, Katakana, CJK punctuation, fullwidth forms,
and the available common Latin, Greek, and Cyrillic characters. This covers
common Chinese and Japanese GitHub titles. These are subsets, not complete
Unicode fonts; rare ideographs in the CJK extension blocks are not included.

The modified families are named **Hush CJK Japanese** and **Hush CJK Simplified
Chinese**. Hinting and shaping tables that egui does not use are removed;
language-specific default glyph forms and upstream attribution remain.

## Attribution and license

Upstream: [Noto CJK](https://github.com/notofonts/noto-cjk),
[Noto Sans CJK license](https://github.com/notofonts/noto-cjk/blob/main/Sans/LICENSE).

Copyright notice from the source font:

> © 2014-2021 Adobe (http://www.adobe.com/).

Noto is a trademark of Google Inc. The original fonts and these modified fonts
are licensed under the **SIL Open Font License 1.1**, included in
[OFL.txt](OFL.txt). This font license is separate from the application license.
Copyright and designer metadata are retained inside each font. The complete
OFL text is embedded in each font's license metadata as well, so it accompanies
the fonts when they are compiled into the application executable.

## Rebuilding

Obtain `NotoSansCJK-Regular.ttc` from the upstream **Sans 2.004** release. The
collection installed by Arch Linux's `noto-fonts-cjk` package is also suitable
when it has the following SHA-256:

```text
b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a
```

From the repository root, using a Python environment with `fonttools==4.66.1`:

```sh
python scripts/subset-fonts.py /path/to/NotoSansCJK-Regular.ttc
python scripts/subset-fonts.py /path/to/NotoSansCJK-Regular.ttc --check
```

The script verifies the source checksum, reads every catalog in `src/locales`,
checks that no translated character is missing, and preserves source timestamps
for reproducible output. `--check` regenerates in memory and compares the exact
bytes against the bundled files without writing. Regenerate the fonts when
translations add new characters outside the covered ranges. Ordinary builds
use the checked-in fonts and do not require Python, fontTools, or the source
collection.
