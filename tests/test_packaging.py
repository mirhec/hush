"""Portable release/version checks; native installers still run on their own OS."""
import importlib.util
import json
from pathlib import Path
import plistlib
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("hush_version", ROOT / "packaging/version.py")
version = importlib.util.module_from_spec(spec)
spec.loader.exec_module(version)


class ReleaseVersionTests(unittest.TestCase):
    def fixture(self, release="1.2.3", locked=None):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "Cargo.toml").write_text(f'[package]\nname="hush"\nversion="{release}"\n')
        (root / "Cargo.lock").write_text(f'[[package]]\nname="hush"\nversion="{locked or release}"\n')
        return root

    def test_current_version_and_native_identity(self):
        release, numeric = version.versions()
        self.assertEqual(version.versions(tag="v" + release), (release, numeric))
        with (ROOT / "packaging/macos/Info.plist").open("rb") as file:
            info = plistlib.load(file)
        self.assertEqual(info["CFBundleIdentifier"], "io.hush.github")
        self.assertEqual(info["CFBundleExecutable"], "hush")
        self.assertTrue((ROOT / "assets/hush.ico").is_file())
        self.assertTrue((ROOT / "assets/hush.icns").is_file())

    def test_rejects_wrong_release_tag_and_outdated_lockfile(self):
        with self.assertRaisesRegex(ValueError, "does not match"):
            version.versions(self.fixture(), "v1.2.4")
        with self.assertRaisesRegex(ValueError, "same Hush version"):
            version.versions(self.fixture(locked="1.2.2"))
        for tag in ["../main", "v1.2.3\nHUSH_VERSION=other", "v1.2.3;echo injected"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                version.versions(self.fixture(), tag)

    def test_prerelease_uses_numeric_native_version_but_retains_asset_version(self):
        self.assertEqual(version.versions(self.fixture("1.2.3-rc.2"), "v1.2.3-rc.2"), ("1.2.3-rc.2", "1.2.3"))

    def test_rejects_invalid_semver_and_native_version_overflow(self):
        for release in ["01.2.3", "1.2", "1.2.3-01", "1.2.3/other", "65536.0.0"]:
            with self.subTest(release=release), self.assertRaises(ValueError):
                version.versions(self.fixture(release))

    def test_flatpak_stages_only_build_inputs_and_uses_filtered_desktop_access(self):
        spec = importlib.util.spec_from_file_location("flatpak_prepare", ROOT / "packaging/flatpak/prepare.py")
        prepare = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(prepare)
        with tempfile.TemporaryDirectory() as temporary:
            manifest_path = prepare.prepare(Path(temporary) / "stage", vendor=False)
            manifest = json.loads(manifest_path.read_text())
            source = manifest_path.parent / "source"
            self.assertTrue((source / "src/main.rs").is_file())
            self.assertFalse((source / ".git").exists())
            self.assertFalse((source / "target").exists())
            self.assertEqual(manifest["app-id"], "io.hush.github")
            self.assertIn("--socket=wayland", manifest["finish-args"])
            for service in ["org.kde.StatusNotifierWatcher", "org.freedesktop.secrets", "org.freedesktop.Notifications"]:
                self.assertIn("--talk-name=" + service, manifest["finish-args"])
            self.assertFalse(any(arg.startswith(("--filesystem=", "--socket=session-bus", "--socket=system-bus", "--own-name=")) for arg in manifest["finish-args"]))
            metadata = ET.parse(source / "packaging/flatpak/io.hush.github.metainfo.xml")
            self.assertEqual(metadata.find("releases/release").get("version"), version.versions()[0])
            self.assertIn("cargo build --frozen --release", manifest["modules"][0]["build-commands"])
            with self.assertRaises(FileExistsError):
                prepare.prepare(manifest_path.parent, vendor=False)


if __name__ == "__main__":
    unittest.main()
