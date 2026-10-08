# Validierungsstand — 8. Oktober 2026

## Aktueller Stand: Oberfläche, Tray-Klick, Releases und Benachrichtigungen

- **52 Rust-Tests erfolgreich** mit `cargo test --locked --offline --all-targets`.
  Dazu gehören feste Einstellungsbreiten und Schalterausrichtung bei 360/540/740 px,
  Maus-/Tastaturbedienung, Toast-Ablauf, StatusNotifier-Aktivierung und D-Bus-
  Menütypen sowie der Import von drei neuen Issues bis in die Zustellwarteschlange.
- Der zusätzliche echte D-Bus-Integrationstest ist lokal **übersprungen**: Diese
  Sandbox erlaubt weder einen privaten D-Bus-Socket noch den Flatpak-Netzwerk-
  Namespace (`Operation not permitted`). Die Linux-CI führt ihn ausdrücklich in
  `dbus-run-session` aus; er prüft Klick, Menü, Host-Neustart und Abmeldung.
- **5 Paketprüfungen erfolgreich**: Versions-/Tag-/Lockfile-Abgleich,
  Prerelease-Metadaten, ungültige Versionen, App-Identität und isolierte Flatpak-
  Quellvorbereitung einschließlich Berechtigungen und Release-Metadaten.
- Workflow-YAML, Shellsyntax, Desktop-Datei und Vorschau-JavaScript geprüft.
- **23 Browserprüfungen** der aktualisierten HTML-Vorschau über Chrome DevTools
  erfolgreich, einschließlich ausgerichteter Schalter, Suche/Archiv, privater
  Banner-Vorschau und automatisch verschwindender Toasts. Das sind keine nativen
  egui-Screenshots. Der frühere Playwright-Bericht unten beschreibt den alten Stand.
- Linux-Release-Datei `target/release/hush` neu gebaut. Clippy erfolgreich mit
  Hinweisen im bestehenden Code; Windows-/macOS-Builds und GitHub Actions lokal
  nicht ausgeführt. Die nativen Installationen und der Flatpak-Paketbuild sind
  noch auf den Zielsystemen bzw. in CI zu bestätigen.

Die laufende Installation wurde ausschließlich lesend diagnostiziert. Der
Hintergrunddienst hatte einen aktuellen Heartbeat, aber alle fünf konfigurierten
Issue-Repositories antworteten mit HTTP 404. Es war kein Detail-Token eingerichtet;
kein Issue war importiert. Namen, Inhalte und Zugangsdaten wurden nicht in diese
Dokumentation übernommen. Repository-Zugriffsfehler stehen jetzt direkt in der UI.
Ein Detail-Token kann ohne erneute Eingabe des gespeicherten Benachrichtigungs-
Tokens ergänzt werden. Danach werden zurückgestellte Thread-Abfragen sofort
wieder freigegeben. Zustellfehler verschwinden erst nach einer tatsächlichen
Erfolgszustellung oder einem erfolgreichen Test, nicht bei leerer Warteschlange.
Die Tests prüfen 404 ohne Cursor-Fortschritt, drei neue Issues nach einem stillen
Erstimport, fehlgeschlagene/erfolgreiche Zustellung und Deduplizierung.

Die erfolgreiche Abfrage privater GitHub-Repositories und tatsächliche
Desktop-Banner bleiben unbestätigt, bis ein berechtigter Detail-Token in der App
hinterlegt und die Zustellung in der Desktop-Sitzung geprüft wurde.


## Frühere Prüfungen bei der ursprünglichen Erstellung

**25 Browserprüfungen bestanden** gegen die separate `preview/hush-preview.html`, in headless Chromium über Python Playwright. Vollständige Einzelliste: [preview-test-results.json](preview-test-results.json). Suche und Leerezustand, Filter, Ungelesen-Zähler, Auswahl/Lesestatus, Archiv/Rückgängig, Tastaturbedienung, Ruhepause, Hell-/Dunkelmodus, Einstellungen, Repository-Validierung, simulierte private Banner und ein 1000-Pixel-Fenster waren Teil der Prüfung. Es wurden keine JavaScript-Laufzeitfehler oder Netzwerk-Requests dieser Vorschau registriert.

**13 statische/Formatprüfungen bestanden:** beide TOML-Dateien und beide Workflow-YAML-Dateien parsebar, vier Shellskripte mit `bash -n`, macOS-Plist parsebar, JavaScript mit `node --check`, SQLite-Schema aus `storage.rs` in Python SQLite angelegt, keine `todo!`-/`unimplemented!`-Makros im Rust-Anwendungscode, keine zusätzlichen Schriftdateien beigefügt. Vollständige Liste: [static-test-results.json](static-test-results.json).

Die vier Screenshots in `preview/screenshots/` wurden aus der HTML-Datei erzeugt. Dunkler Posteingang und Einstellungen wurden visuell auf Lesbarkeit, Positionierung und störende Überlagerungen geprüft. Diese Bilder validieren **nicht** das Rendering des nativen egui-Fensters.

## Rust-Prüfung nach Tray- und Dienstkorrektur

Am 8. Oktober 2026 unter Linux x86_64 mit Rust 1.99.0 geprüft. Die vorhandene `Cargo.lock` wurde um die Tray-Abhängigkeiten ergänzt; Cargo akzeptiert sie mit `--locked`.

Der bisherige Dienststart wurde mit einem frischen isolierten Datenverzeichnis reproduziert: Prozessende mit `Hush: database is locked`, ohne Token oder GitHub-Zugriff. Der neue Konkurrenztest schlug vor der Korrektur an derselben Stelle fehl und besteht mit `BEGIN IMMEDIATE`. Der Test hält einen konkurrierenden SQLite-Schreibzugriff gezielt offen und prüft Steuerbefehle sowie Einstellungsänderungen. Der zusätzliche Prozesstest deckte einen zweiten Fehler auf: Das Öffnen/Schließen der Datenbank außerhalb von SQLite hob POSIX-Sperren auf; nach einem separaten Start sah der Worker Stoppbefehle nicht zuverlässig. Der Test prüft deshalb auch den tatsächlichen Start, fortlaufende Heartbeats, konkurrierende Schreibzugriffe, doppelte Worker, Stoppen und Neustart.

- `cargo test --locked --offline --no-default-features`: 38 Tests erfolgreich (7 Unit-, 27 Ereignis-/Speicher- und 4 neue Diensttests).
- `cargo test --locked --offline --all-targets`: 42 Tests erfolgreich, einschließlich vier zusätzlicher Linux-Fenster-/Tray-Regressionstests.
- `cargo check --locked --offline --all-targets`: erfolgreich mit Tray-Abhängigkeiten.
- `cargo clippy --locked --offline --all-targets`: erfolgreich; bestehende Hinweise zu älterem UI-/Filter-/Testcode bleiben.
- `cargo build --locked --offline --release`: erfolgreich, native Linux-Binärdatei `target/release/hush`.
- Abschließender Test der tatsächlichen Release-Datei mit frischem Datenverzeichnis: Startbestätigung, Status, zweiter Worker, Stoppen, Neustart und Erkennung eines veralteten Heartbeats erfolgreich; keine Zugangsdaten oder GitHub-Zugriffe.
- Shellsyntax der Linux-/macOS-Paketskripte: erfolgreich geprüft.

## Niri-/Wayland-Korrektur

Der vorherige Schließpfad sendete `CancelClose` und `Visible(false)`. Die eingesetzte winit-Version 0.30.13 ignoriert `set_visible` unter Wayland, sodass Niri das Fenster nicht schließen konnte. Unter Linux besitzt jetzt ein unabhängiger Desktop-Prozess das Tray. Der Fensterprozess lässt den nativen Schließbefehl zu; Tray und Worker bleiben bestehen. Der Tray-Prozess kann anschließend ein neues Fenster öffnen. `--tray` benötigt kein verstecktes Wayland-Fenster mehr.

Vier neue Tests prüfen die echten egui-Viewport-Kommandos bei einem Schließereignis (weder `CancelClose` noch `Visible(false)`), gespeicherte Einstellungen beim erneuten Erzeugen der Oberfläche, Pause/Einstellungen/Beenden aus dem Controller sowie das Wiederverwenden vorhandener Tray- und Fensterinstanzen. Sie laufen ohne Desktop-Sitzung und ersetzen keinen interaktiven Niri-Test. Die neu gebaute Release-Datei wurde zusätzlich ohne Display getestet: Fenster-Fallback wird versucht, dessen Fehler landet in `window.log`, der Desktop-Prozess beendet sich mit Fehlerstatus, der Dienst bleibt unabhängig gesund und lässt sich anschließend stoppen.

## Weiterhin nicht durchgeführt

| Bereich | Status |
|---|---|
| Native Tray-Menüs, Schließen/Öffnen, Niri/DMS-Hostwechsel | Desktop-Sitzung und private D-Bus-Sockets durch Sandbox gesperrt; lokal manuell prüfen |
| Windows-/macOS-Build und native Interaktion | Auf diesem Linux-System nicht ausgeführt |
| Echte GitHub-Ereignisse / SSO / Team-Mitgliedschaften | Keine Zugangsdaten verwendet |
| Schlüsselbund und echte OS-Banner | Nicht getestet |
| Installationsskripte / Autostart / macOS-Signierung | Nicht nativ ausgeführt |
| GitHub Actions und `cargo audit` | Nicht ausgeführt |

Der Linux-Build und die lokalen Tests belegen keinen plattformübergreifenden Live-Betrieb. Die separate HTML-Vorschau prüft das native Tray nicht.

Manuelle Tray-Prüfung: Hush normal starten, schließen und über das Tray wieder öffnen; erneut über den Launcher öffnen und genau ein Fenster/Icon prüfen. Pause/Fortsetzen, Aktualisieren, Dienst stoppen/starten und vollständiges Beenden prüfen. `--tray` soll mit verfügbarem Tray ohne Fenster starten; ohne StatusNotifier-Host soll das Fenster mit Hinweis erscheinen. Während Hush im Tray liegt, die Leiste neu starten und Erreichbarkeit prüfen. `--demo` darf weder Tray noch Worker anlegen.

## Reproduzierbare Prüfkommandos

Native Rust-Prüfungen nach Installation der Build-Abhängigkeiten:

```sh
cargo test --locked --no-default-features
cargo check --locked --all-targets
cargo clippy --locked --all-targets
cargo build --locked --release
cargo run --locked --release -- --demo
```

Die enthaltene `Cargo.lock` beibehalten und mit `--locked` prüfen. Eine feste Rust-Toolchainversion für einen echten Release ebenfalls dokumentieren; der mitgelieferte `stable`-Kanal ist beweglich.

Browserprüfung, unabhängig von Rust:

```sh
# Nach separater Installation von Python Playwright und Chromium:
CHROMIUM=/usr/bin/chromium python tests/preview_smoke.py
```

Der Test verwendet `--no-sandbox` ausschließlich für den headless Browser in der isolierten Testumgebung, nicht für die native Hush-Anwendung. Er installiert keinen Browser und kontaktiert keine Website.

## Native Abnahme vor echten privaten Repositories

Mit einem Testkonto und einem nicht vertraulichen Testrepository beginnen. Einrichten, Erstimport und anschließend jedes Ereignis **nach** dem Erstimport auslösen. Frische persönliche Zuweisung, direkte Review-Anfrage, Anfrage an ein tatsächlich eigenes Team sowie an ein fremdes Team; neues Issue vs. Kommentar auf altem Issue; Review auf eigenem vs. fremdem PR; direkte Mention, ähnlicher Login, Codeblock, Folgekommentar ohne Mention und editierter Kommentar. Während einer Pause sollten Einträge, aber keine Banner entstehen.

Zusätzlich Netzwerk trennen/wiederherstellen, GitHub-Rate-Limit/429 simulieren, App-/Worker-Neustart, doppelte Hintergrundstarts, widerrufene Tokens, gesperrten Schlüsselbund, fehlende Repository-Rechte, Kontotrennung während Sync und lokalen Datenzugriff kontrollieren. Netzwerk-Tests sollten auch Redirect-/Fremdhost-Blockade und fehlende Proxyunterstützung bestätigen.

Unter Niri/DMS die Benutzer-D-Bus-Sitzung und entsperrten Secret Service testen. Unter Windows aus dem registrierten Startmenüeintrag starten. Unter macOS aus dem erzeugten `.app`-Bundle starten und Berechtigungen/Fokusmodus prüfen. Alle Zielsysteme separat testen; ein Linux-Test beweist keine funktionierende Windows- oder macOS-Zustellung.
