# Releases und Installer

`release.yml` ist der einzige GitHub-Actions-Workflow. Er reagiert automatisch
nur auf veröffentlichte GitHub-Releases, einschließlich Prereleases.
Pushes auf `main`, Tag-Pushes und Pull Requests starten keine Actions.
Die Release-Version kommt ausschließlich aus dem gewählten Git-Tag. Der Workflow
erstellt keine zusätzlichen Releases und wählt keine nächste Versionsnummer aus.

Alle Schritte erscheinen in einem gemeinsamen Lauf „Release Hush“:

1. Tag als Versionsnummer prüfen und in die Cargo-Dateien des Build-Verzeichnisses
   übernehmen; Paketskripte prüfen; Existenz des veröffentlichten Releases
   bestätigen; `cargo audit` ausführen.
2. Nach erfolgreicher Prüfung Tests, Clippy, Builds und Installer für alle vier
   Plattformen ausführen. Alle Builds verwenden dieselbe zuvor geprüfte Commit-ID
   und übernehmen denselben Release-Tag vor dem Kompilieren als Paketversion.
3. Nach Erfolg aller Plattformen das vollständige Installer-Set und
   `SHA256SUMS.txt` an das Release hochladen.

Schlägt eine Stufe fehl, laufen ihre nachfolgenden Stufen nicht. Ein falscher
Versionstag stoppt bereits vor den aufwendigen Plattform-Builds.

| Plattform | Release-Datei | Installation |
|---|---|---|
| Windows x86-64 | `Hush-VERSION-windows-x86_64-setup.exe` | Inno Setup, pro Benutzer, mit Deinstallation |
| macOS Apple Silicon | `Hush-VERSION-macos-arm64.pkg` | `/Applications/Hush.app` |
| macOS Intel | `Hush-VERSION-macos-x86_64.pkg` | `/Applications/Hush.app` |
| Linux x86-64 | `Hush-VERSION-linux-x86_64.flatpak` | Flatpak, pro Benutzer |

Autostart wird nicht automatisch aktiviert. Im Windows-Installer ist er eine
abwählbare, zunächst ausgeschaltete Option. Die Deinstallation lässt Kontodaten
und Verlauf bestehen; „Konto trennen und lokale Daten löschen“ in Hush entfernt
diese bei Bedarf vorher.

## Ablauf

1. Gewünschte Änderungen nach `main` pushen.
2. Ein GitHub-Release mit einem neuen Tag auf diesem Stand veröffentlichen,
   beispielsweise `1.0.2` oder `v1.0.2`. Die Cargo-Version muss nicht geändert werden.
3. Unter GitHub Actions „Release Hush“ kontrollieren. Die Installer
   erscheinen erst nach Abschluss aller Builds am Release.

Tags müssen gültige SemVer-Versionen sein, mit optionalem `v` am Anfang.
Prereleases wie `v1.1.0-rc.1` behalten ihren vollständigen Namen in Anwendung und
Dateinamen. Die numerischen Windows-/macOS-Metadaten verwenden dafür `1.1.0`.
Für die nativen Installer dürfen die drei numerischen Teile jeweils höchstens
65535 sein. Ungültige Tags werden vor den Plattform-Builds abgewiesen.

`python3 packaging/version.py --stamp --tag "$RELEASE_TAG"` setzt die Version
von Hush in `Cargo.toml` und im passenden Eintrag in `Cargo.lock`. Abhängigkeiten
und Prüfsummen bleiben unverändert; es wird kein Lockfile neu aufgelöst und kein
Commit zurückgeschrieben. Cargo verwendet diese Version auch für
`CARGO_PKG_VERSION`, damit `hush --version` und die UI zum Installer passen.
Die Flatpak-Quellvorbereitung übernimmt die bereits angepassten Dateien, sodass
auch der Build ohne Git-Metadaten und Netzwerk dieselbe Version bekommt.

Bei lokalen Builds ohne diesen Vorbereitungsschritt bleibt die eingecheckte
Cargo-Version der Entwicklungsstand. Aufrufe ohne `--stamp` lesen und prüfen die
Version nur; Paketierung verändert sie nicht nachträglich.

Ein fehlgeschlagener Lauf kann erneut gestartet werden. „Run workflow“ nimmt
auch den Tag eines vorhandenen Releases entgegen. Uploads ersetzen gleichnamige
Dateien; ein unveränderliches GitHub-Release erlaubt das nach Veröffentlichung
nicht. Für aktivierte Release-Immutability müsste die Veröffentlichung erst nach
dem Asset-Upload erfolgen; diese Pipeline verwendet ausdrücklich `published`.

Die Flatpak-Pipeline installiert das gebaute
Bundle im CI-Profil und prüft Programmversion sowie Dienststart und Status ohne
GitHub-Zugangsdaten. Ein separater Test prüft Tray-Aktivierung, Menü und
Host-Neustart in einer isolierten D-Bus-Sitzung.

### Fehler bei den ursprünglichen Releases 1.0.0 und 1.0.1

Der Tag `1.0.0` zeigte auf Commit `1bf25da`, dessen `Cargo.toml` und `Cargo.lock`
noch `0.1.0` enthalten. Deshalb brachen alle vier Release-Jobs bei der Prüfung mit
`Release tag '1.0.0' does not match Cargo version '0.1.0'` ab. Der gleichzeitig
gestartete Main-Build hatte keinen Release-Tag zu prüfen und konnte weiterlaufen.
Er war unabhängig vom Release-Lauf und stellte diesem keine Artefakte bereit.

Nach dem Anheben der Cargo-Version auf `1.0.0` trat derselbe Fehler mit dem Tag
`1.0.1` auf. Der Workflow übernimmt deshalb jetzt die Release-Version aus dem Tag.
Für den ersten Lauf mit dieser Änderung ein neues Release `1.0.2` auf dem
aktualisierten `main` erstellen. Ältere Tags enthalten noch das alte Skript;
„Re-run jobs“ aktualisiert deren Quellcode nicht. Bestehende veröffentlichte Tags
werden nicht verschoben. Danach reicht für weitere Releases jeweils ein neuer Tag.

## Signierung

Ohne Zertifikate entstehen unsignierte Windows-/macOS-Installer. Das macOS-App-
Bundle ist dann nur ad-hoc signiert, ohne Apple-Notarisierung. Gatekeeper bzw.
SmartScreen können die Installation warnen oder blockieren. Für öffentliche
Distribution die folgenden GitHub-Actions-Secrets hinterlegen; keine Zertifikate
oder Kennwörter ins Repository einchecken.

**macOS:**

- `MACOS_CERTIFICATE_P12`: Base64-kodiertes P12 mit den privaten Schlüsseln für
  **Developer ID Application** und **Developer ID Installer**.
- `MACOS_CERTIFICATE_PASSWORD`: Kennwort dieses P12.
- `MACOS_APP_IDENTITY`: vollständiger Name der Developer-ID-Application-Identität.
- `MACOS_INSTALLER_IDENTITY`: vollständiger Name der Developer-ID-Installer-Identität.
- Für Notarisierung zusätzlich `APPLE_ID`, `APPLE_TEAM_ID` und
  `APPLE_APP_PASSWORD` (app-spezifisches Apple-Kennwort).

Der Build importiert das Zertifikat in einen temporären Runner-Schlüsselbund,
signiert App und Paket und entfernt den Schlüsselbund auch nach Fehlern.
Sind die Apple-Zugangsdaten eingerichtet, wartet er auf die Notarisierung und
heftet das Ticket mit `stapler` an das Paket. Signierfehler brechen den Lauf ab.

**Windows:**

- `WINDOWS_CERTIFICATE_PFX`: Base64-kodiertes Authenticode-PFX.
- `WINDOWS_CERTIFICATE_PASSWORD`: Kennwort des PFX.
- Actions-Variable `WINDOWS_TIMESTAMP_URL`: RFC-3161-Zeitstempel-URL des
  Zertifikatsanbieters.

SignTool signiert zuerst die Programmdatei, danach den fertigen Installer, und
prüft beide Signaturen. Die temporäre PFX-Datei wird anschließend entfernt.
Hardwaregebundene oder Cloud-Signierschlüssel benötigen die Integration des
jeweiligen Anbieters anstelle des PFX-Skripts. Der Inno-Uninstaller erhält in
dieser Variante keine eigene Authenticode-Signatur.

## Linux: Flatpak

Das bestätigte Format ist ein **Flatpak-Bundle für x86-64**. Der Build verwendet
Freedesktop Platform/SDK 25.08 und die Rust-SDK-Erweiterung. Cargo-Abhängigkeiten
werden anhand von `Cargo.lock` vorab in ein isoliertes Quellverzeichnis kopiert;
Tests und Kompilierung innerhalb des SDK laufen mit `--frozen` ohne Netzwerk.
Die Quellvorbereitung übernimmt nur die festgelegten Build-Dateien.

Das Manifest erlaubt Wayland, X11-Fallback und GPU-Zugriff, Netzwerkzugriff auf
GitHub sowie gezielte D-Bus-Verbindungen für StatusNotifier, Secret Service und
Desktop-Benachrichtigungen. Es gibt keine Freigabe für das Home-Verzeichnis oder
den gesamten Session-Bus. Daten liegen unter
`~/.var/app/io.hush.github/data/hush/`, Tokens im System-Schlüsselbund. Bestehende
Daten einer Installation außerhalb Flatpak werden nicht automatisch übernommen.
Tray und Dienst bleiben nach Schließen des Fensters aktiv; der Desktop benötigt
einen StatusNotifier-Host und einen entsperrten Secret Service.

```sh
flatpak install --user Hush-1.0.0-linux-x86_64.flatpak
flatpak run io.hush.github
flatpak run io.hush.github --tray
flatpak run io.hush.github --status
```

Die benötigte Runtime wird bei Bedarf von Flathub geladen. Ein Bundle enthält
keinen automatischen Updatekanal: Für ein Update die neue `.flatpak`-Datei
installieren. Es wird kein Flathub-Eintrag veröffentlicht. Die Windows-/macOS-
Installer richten ebenfalls keinen automatischen Programm-Updater ein.

Für einen lokalen Paketbuild werden `flatpak`, `flatpak-builder` und die Runtime
benötigt:

```sh
flatpak --user remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak --user install flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08
bash packaging/flatpak/package.sh
```

## Lokale Prüfung

```sh
python3 -m unittest discover -s tests -p 'test_packaging.py'
python3 packaging/version.py
cargo test --locked --all-targets
```

Für einen lokalen Release-Build in einer separaten Quellkopie zuerst
`python3 packaging/version.py --stamp --tag v1.0.2` ausführen, danach Cargo-Build
und Paketierung. Der Befehl passt die beiden Cargo-Dateien dieser Kopie an.

Auf macOS nach `cargo build --release`:

```sh
bash packaging/macos/package.sh
```

Auf Windows mit Inno Setup 6 nach `cargo build --release`:

```powershell
./packaging/windows/package.ps1
```

Die Installer müssen auf den jeweiligen Zielsystemen auf Installation, Update,
Startmenü/App-Identität, Tray und Benachrichtigungen geprüft werden. In der lokalen
Linux-Umgebung wurden die Windows-/macOS-Paketwerkzeuge und GitHub Actions
nicht ausgeführt. Der Flatpak-Lauf ist dort durch gesperrte Sandbox-Sockets
blockiert; Manifest und Quellvorbereitung sind lokal geprüft.

Grundlagen: [GitHub-Release-Ereignisse](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release),
[Inno Setup: Benutzerinstallation](https://jrsoftware.org/ishelp/topic_setup_privilegesrequired.htm),
[AppUserModelID in Verknüpfungen](https://jrsoftware.org/ishelp/topic_iconssection.htm),
[Apple-Paketierung](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution),
[Flatpak-Desktopintegration](https://docs.flatpak.org/en/latest/desktop-integration.html).
