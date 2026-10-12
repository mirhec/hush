"""Exercise NSIS in a fresh Windows CI profile, or an isolated Wine prefix.

This uses a placeholder payload, never launches Hush, and refuses to touch an
existing installation. Run with --wine only in a disposable Linux environment.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
UNINSTALL = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\io.hush.github"
RUN = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
IDENTITY = r"HKCU\Software\Classes\AppUserModelId\io.hush.github"


def smoke(wine, temporary):
    env = dict(os.environ)
    if wine:
        env.update(WINEPREFIX=str(temporary / "wine"), WINEDEBUG="-all",
                   WINEDLLOVERRIDES="mscoree,mshtml=")

    def command(args, check=True, windows=False):
        if wine and windows:
            args = ["wine", *args]
        # Wine services can inherit output handles beyond the command's life.
        # Files let us wait for the command without waiting for pipe EOF.
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            result = subprocess.run(args, env=env, stdout=stdout, stderr=stderr, timeout=90)
            stdout.seek(0)
            stderr.seek(0)
            result.stdout = stdout.read().decode("utf-8", errors="replace")
            result.stderr = stderr.read().decode("utf-8", errors="replace")
        if check and result.returncode:
            raise AssertionError(f"{args}: exit {result.returncode}\n{result.stdout}\n{result.stderr}")
        return result

    def reg(*args, check=True):
        return command(["reg", *args, "/reg:64"], check=check, windows=True)

    def query(key, value=None):
        if not wine:
            import winreg
            try:
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key.removeprefix("HKCU\\"),
                                    access=winreg.KEY_READ | winreg.KEY_WOW64_64KEY) as handle:
                    data = winreg.QueryValueEx(handle, value)[0] if value is not None else ""
                return SimpleNamespace(returncode=0, stdout=str(data))
            except FileNotFoundError:
                return SimpleNamespace(returncode=1, stdout="")
        args = ["query", key]
        if value is not None:
            args += ["/v", value]
        result = reg(*args, check=False)
        if value is not None and result.returncode == 0:
            # reg query uses an OEM console encoding which can lose Unicode.
            # Registry exports preserve strings as UTF-16, like winreg above.
            export = temporary / "registry.reg"
            reg("export", key, windows_path(export), "/y")
            prefix = json.dumps(value) + "="
            for line in export.read_text(encoding="utf-16").splitlines():
                if line.startswith(prefix):
                    result.stdout = json.loads(line[len(prefix):])
                    break
            else:
                raise AssertionError(f"Missing exported registry value: {key} {value}")
        return result

    def host_path(path):
        if wine:
            return Path(command(["winepath", "-u", path]).stdout.strip())
        return Path(path)

    def windows_path(path):
        if wine:
            return command(["winepath", "-w", str(path)]).stdout.strip()
        return str(path)

    def winenv(name):
        if wine:
            return command(["cmd", "/c", f"echo %{name}%"], windows=True).stdout.strip()
        return os.environ[name]

    if wine:
        command(["wineboot", "-u"])
        command(["winecfg", "-v", "win10"])
    elif os.name != "nt" or os.environ.get("GITHUB_ACTIONS") != "true":
        raise SystemExit("Native installer smoke tests require a fresh Windows Actions runner")

    local = host_path(winenv("LOCALAPPDATA"))
    appdata = host_path(winenv("APPDATA"))
    destination = local / "Programs/Hush"
    shortcut = appdata / "Microsoft/Windows/Start Menu/Programs/Hush.lnk"
    desktop = host_path(winenv("USERPROFILE")) / "Desktop/Hush.lnk"
    for key, value in [(UNINSTALL, None), (UNINSTALL + "_is1", None), (RUN, "Hush"), (IDENTITY, None)]:
        assert query(key, value).returncode != 0, f"Existing Hush registry state: {key}"
    for path in [destination, shortcut, desktop]:
        assert not path.exists(), f"Existing Hush installation: {path}"

    stage = temporary / "source"
    shutil.copytree(ROOT / "assets", stage / "assets")
    shutil.copy2(ROOT / "LICENSE", stage / "LICENSE")
    script = stage / "packaging/windows/hush.nsi"
    script.parent.mkdir(parents=True)
    shutil.copy2(ROOT / "packaging/windows/hush.nsi", script)
    binary = stage / "target/release/hush.exe"
    binary.parent.mkdir(parents=True)
    payload = b"NSIS smoke test payload; not a runnable application.\n"
    binary.write_bytes(payload)
    (stage / "dist").mkdir()
    compiler = (str(Path(os.environ["ProgramFiles(x86)"]) / "NSIS/makensis.exe")
                if not wine else "makensis")
    prefix = "-" if wine else "/"
    command([compiler, prefix + "WX", prefix + "V2", prefix + "DHushVersion=9.8.7-rc.1",
             prefix + "DHushNumericVersion=9.8.7", str(script)])
    package = stage / "dist/Hush-9.8.7-rc.1-windows-x86_64-setup.exe"

    def install():
        command([str(package), "/S"], windows=True)

    def verify_install(folder):
        assert (folder / "hush.exe").read_bytes() == payload
        assert (folder / "uninstall.exe").is_file()
        assert (folder / "licenses/fonts/OFL.txt").is_file()
        # Wine 8's IPropertyStore::SetValue/Commit are success-returning stubs.
        # The native Windows job must verify the serialized property itself.
        assert shortcut.is_file()
        if not wine:
            assert "io.hush.github".encode("utf-16le") in shortcut.read_bytes()
        assert query(IDENTITY, "DisplayName").returncode == 0
        assert "9.8.7-rc.1" in query(UNINSTALL, "DisplayVersion").stdout

    def uninstall(folder):
        command([str(folder / "uninstall.exe"), "/S"], windows=True)
        deadline = time.monotonic() + 20
        while (folder / "uninstall.exe").exists() and time.monotonic() < deadline:
            time.sleep(0.2)
        assert not (folder / "hush.exe").exists()
        assert not (folder / "uninstall.exe").exists()
        assert not shortcut.exists()
        assert not desktop.exists()
        assert query(UNINSTALL).returncode != 0
        assert query(IDENTITY).returncode != 0
        assert query(RUN, "Hush").returncode != 0

    install()
    verify_install(destination)
    assert not desktop.exists()
    assert query(RUN, "Hush").returncode != 0
    print("Fresh per-user install, Start menu shortcut, and default autostart: OK", flush=True)
    if wine:
        print("Shortcut AppUserModelID persistence requires the native Windows CI check", flush=True)

    neighbor = destination / "keep-user-file.txt"
    neighbor.write_text("keep")
    data = local / "Hush/Hush/data/installer-smoke-sentinel.txt"
    data.parent.mkdir(parents=True, exist_ok=True)
    with data.open("x") as sentinel:
        sentinel.write("keep account data")
    reg("add", RUN, "/v", "Hush", "/t", "REG_SZ", "/d", '"C:\\old\\hush.exe" --tray', "/f")
    desktop.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(shortcut, desktop)
    install()
    verify_install(destination)
    assert f'"{windows_path(destination)}\\hush.exe" --tray' in query(RUN, "Hush").stdout
    assert desktop.is_file()
    if not wine:
        assert "io.hush.github".encode("utf-16le") in desktop.read_bytes()
    reg("delete", RUN, "/v", "Hush", "/f")
    install()
    assert query(RUN, "Hush").returncode != 0
    print("Upgrades preserve enabled/disabled autostart and desktop shortcut: OK", flush=True)

    # Cover startup enabled after installation, not only installer-created values.
    reg("add", RUN, "/v", "Hush", "/t", "REG_SZ", "/d", "test", "/f")
    uninstall(destination)
    assert neighbor.read_text() == "keep"
    assert data.read_text() == "keep account data"
    print("Uninstall removes only Hush registration/files, retains user data: OK", flush=True)

    # Simulate the former packager's registration, including a custom Unicode
    # path with spaces and a trailing separator, without running that packager.
    legacy = local / "Hush previous 日本語"
    legacy.mkdir()
    (legacy / "hush.exe").write_bytes(payload)
    for name in ["unins000.exe", "unins000.dat", "unins000.msg"]:
        (legacy / name).write_text("old installer metadata")
    old = windows_path(legacy)
    reg("add", UNINSTALL + "_is1", "/v", "InstallLocation", "/d", old + "\\", "/f")
    reg("add", UNINSTALL + "_is1", "/v", "UninstallString", "/d", f'"{old}\\unins000.exe"', "/f")
    reg("add", RUN, "/v", "Hush", "/d", f'"{old}\\hush.exe" --tray', "/f")
    install()
    verify_install(legacy)
    assert query(UNINSTALL + "_is1").returncode != 0
    assert not list(legacy.glob("unins000.*"))
    assert f'"{old}\\hush.exe" --tray' in query(RUN, "Hush").stdout
    uninstall(legacy)
    assert data.read_text() == "keep account data"
    data.unlink()
    neighbor.unlink()
    destination.rmdir()
    print("Migration from the previous installer and subsequent uninstall: OK", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wine", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="hush-installer-smoke-") as directory:
        try:
            smoke(args.wine, Path(directory))
        finally:
            if args.wine:
                subprocess.run(["wineserver", "-k"], check=False,
                               env=dict(os.environ, WINEPREFIX=str(Path(directory) / "wine")))
