# Hush
GitHub-Benachrichtigungen für den Desktop.

Eine eigenständige Desktop-Anwendung in **Rust + egui/eframe** für einen bewusst kleinen GitHub-Posteingang. Dunkles Salbeigrün, eine helle Alternative, vier Ereignisarten und keine eingebettete Browser-Engine in der nativen Anwendung.

**Release-Version aus dem Git-Tag · MIT**

Der lokale Linux-Build und die Rust-Tests sind geprüft. Auf GitHub Actions wurden außerdem Windows- und macOS-Apple-Silicon-Installer gebaut; Installation, Schlüsselbund, Tray und Desktop-Benachrichtigungen benötigen weiterhin Tests auf den Zielsystemen. Den aktuellen Prüfstand und offene Punkte dokumentiert [VALIDATION.md](docs/VALIDATION.md).

## Zwei unterschiedliche Dinge im Paket

**Die Anwendung:** `src/` enthält die tatsächliche Rust-Implementierung: Oberfläche, GitHub-Client, Ereignisfilter, Hintergrundprozess, SQLite-Speicher und Schlüsselbund-/Benachrichtigungsanbindung. Es handelt sich nicht nur um ein HTML-Mockup.

**Die Designvorschau:** `preview/hush-preview.html` ist eine separat implementierte, interaktive Offline-Vorschau mit erfundenen Daten. Sie zeigt Posteingang, Details, Filter, Suche, Archiv und Einstellungen, verbindet sich aber niemals mit GitHub. Die Bilder in `preview/screenshots/` stammen aus dieser Vorschau, **nicht aus einer ausgeführten egui-Anwendung**. Browser- und egui-Typografie/Abstände können abweichen. Es werden keine externen Fonts oder Bilder nachgeladen.

## Die vier Regeln

| Regel | Implementierung | Grenze |
|---|---|---|
| PR dir zugewiesen / Review für dich oder dein Team | Zuweisungs- und Review-Request-Einträge aus der PR-Timeline; Abgleich mit deinem Login und konfigurierten/ermittelten Teams | GitHub muss den Thread auffindbar machen; Team-Mitgliedschaften brauchen `read:org` oder eine manuelle Liste. |
| Neues Issue in ausgewählten Repositories | Repository-Issue-Abfrage mit Prüfung von `created_at`; PRs und bloße spätere Updates werden ausgeschlossen | Bis zu 20 Repositories; keine GitHub-Projects-Boards. |
| Review auf einem deiner PRs | Abfrage eingereichter Reviews und Prüfung des PR-Autors; `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED` | Lesezugriff auf den PR erforderlich; Suchindex und API können verzögert sein. |
| Direkte Erwähnung in Kommentaren | Prüft tatsächliche Kommentartexte in Issues, PR-Kommentaren, Review-Kommentaren/-Texten, Commit-Kommentaren und Repository-Discussions | Nur auffindbare und zugängliche Threads. Kein vollständiges „überall auf GitHub“; Gists und Organisations-Discussions sind nicht abgedeckt. |

GitHubs Benachrichtigungsgrund `mention` ist ausdrücklich **kein** Beweis für eine neue Erwähnung. Hush liest die zugrunde liegenden Ereignisse. Markdown-Code und Zitate werden bei Erwähnungen ignoriert; diese lokale Auswertung ist eine Annäherung und kein identischer Nachbau von GitHubs Mention-Parser. Ein später erneut bearbeiteter, bereits gemeldeter Kommentar erhält nicht nochmals ein Banner.

## Bedienung

Kompakter Posteingang mit zweizeiligen Einträgen, Suche, Ereignisfiltern, Ungelesen-Ansicht, Detailbereich und lokalem Archiv. Das Fenster startet mit 1040 × 720 Pixeln und lässt sich bis 640 × 480 verkleinern; dabei wird die Navigation zur Icon-Leiste. Einstellungen sind in Benachrichtigungen, Konto und Diagnose aufgeteilt. `Strg+K` / `Cmd+K` fokussiert die Suche; `Esc` schließt Details. Links werden erst auf Knopfdruck im Systembrowser geöffnet und müssen zu `https://github.com` gehören.

Die normalen Einstellungen beschränken sich auf die vier Regeln, Issue-Repositories, Systembenachrichtigungen, vertrauliche Inhaltsvorschau und ein Abfrageintervall von 1/2/5 Minuten. Eine Pause hält Desktop-Benachrichtigungen für 30 Minuten zurück; die Ereignisse erscheinen weiter im Posteingang. Es gibt nach Ende der Pause keine nachträgliche Bannerflut. Die Hell-/Dunkel-Auswahl gilt in 0.1 für die laufende Fenstersitzung.

Beim ersten Synchronisieren werden maximal die letzten 24 Stunden importiert, **ohne Banner**. Der sichtbare Verlauf ist auf 500 Einträge bzw. 30 Tage begrenzt. Ältere Dubletten-IDs werden noch bis zu 90 Tage aufbewahrt.

**Gelesen und erledigt sind lokal:** Hush verändert keine Issues, PRs oder GitHub-Lesestände. Auf mehreren Rechnern entstehen unabhängige Posteingänge und gegebenenfalls mehrere Banner für dasselbe Ereignis.

## Starten / bauen

Voraussetzung: Rust über Rustup, ein aktueller C/C++-Linker für dein Betriebssystem und bei Linux die unten genannten Bibliotheken. `rust-toolchain.toml` verwendet den stabilen Toolchain-Kanal; die im Projekt festgelegte Mindestversion ist Rust 1.95. Abhängigkeiten werden beim ersten Cargo-Aufruf aus dem Internet bezogen.

```sh
# Zuerst ohne Token, ohne Netzwerk und ohne lokale Kontodaten ansehen:
cargo run --release -- --demo

# Native Anwendung mit echtem Posteingang:
cargo run --release

# Vor dem echten Einsatz:
cargo test --no-default-features
cargo check --all-targets
cargo clippy --all-targets
cargo build --release
```

`Cargo.lock` ist enthalten. Mit `--locked` bauen, damit die geprüften Abhängigkeitsversionen verwendet werden. `eframe`, `keyring`, `notify-rust` und `tray-icon` sind zusätzlich direkt auf Versionen festgelegt.

### Linux, Niri und Dank Material Shell

Wayland und X11 sind als eframe-Backends aktiviert. Hush sendet native Benachrichtigungen über den vorhandenen Freedesktop-/D-Bus-Dienst. Wenn DMS diesen Dienst bereitstellt, ist kein zusätzlicher Dunst-/Mako-Dienst vorgesehen. Für Zugangsdaten muss ein entsperrter **Secret Service** laufen, etwa ein entsprechend eingerichteter GNOME Keyring oder KWallet mit Secret-Service-Unterstützung. Fehlt er, wird nicht auf Klartextspeicherung ausgewichen.

Beispiel für Debian/Ubuntu-Build-Abhängigkeiten; andere Distributionen verwenden ihre entsprechenden Pakete:

```sh
sudo apt install build-essential pkg-config libdbus-1-dev libx11-dev libxi-dev \
  libxrandr-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev \
  libglib2.0-dev
cargo build --release
bash packaging/linux/install.sh
```

Das Linux-Tray verwendet StatusNotifier und D-Bus über GLib/GIO. Unter Arch/CachyOS wird `glib2` benötigt; AppIndicator ist nicht mehr erforderlich. In Niri/DMS muss das System-Tray in der Leiste aktiv sein. Ohne erreichbare Tray-Leiste bleibt das Fenster zugänglich und zeigt einen Hinweis.

Die Installation erfolgt pro Benutzer nach `~/.local/bin` und `~/.local/share`, ohne Root-Rechte. Hush danach über den App-Launcher oder `~/.local/bin/hush` starten. Es wird kein Autostart eingerichtet.

Für einen **bewusst gewählten** Niri-Autostart die Vorlage `packaging/linux/niri.kdl` an den eigenen Benutzernamen anpassen. Nicht blind eine bestehende Niri-Konfiguration überschreiben. `packaging/linux/dms-status.sh` ist ein optionaler JSON-Helfer für ein zusätzliches eigenes Leisten-Widget, **kein fertig installierbares DMS-Plugin**; es benötigt `jq` und veröffentlicht keine Titel, Repository-Namen oder Tokens.

```sh
~/.local/bin/hush --status
~/.local/bin/hush --stop
```

### macOS

Xcode Command Line Tools und Rustup installieren. Anschließend:

```sh
cargo build --release
bash packaging/macos/bundle.sh
open dist/Hush.app
```

Das Skript erzeugt ein lokal/ad-hoc signiertes `.app`-Bundle mit eigener Kennung, aber **keine Developer-ID-Signatur oder Notarisierung**. Für native Benachrichtigungen aus diesem Bundle starten und Betriebssystem-Berechtigungen prüfen. Die macOS-Benachrichtigungsintegration muss auf dem eingesetzten macOS separat getestet werden. Nicht Gatekeeper oder andere Schutzmechanismen systemweit abschalten.

Apple Silicon und Intel werden im Build-Workflow getrennt gebaut, nicht als Universal-Binary. Optionaler, ausdrücklicher Autostart nach Kopieren nach `/Applications`: `bash packaging/macos/enable-autostart.sh`. Er installiert einen Benutzer-LaunchAgent, keinen Systemdienst.

### Windows

Rustup und die Visual-Studio-C++-Build-Tools verwenden. In einer PowerShell im Projektordner:

```powershell
cargo build --release
powershell -NoProfile -File packaging/windows/install.ps1
```

Das Skript installiert unter `%LOCALAPPDATA%\Programs\Hush`, erstellt einen Startmenüeintrag und registriert Hushs eigene AppUserModelID für Windows-Benachrichtigungen. Es braucht keine Administratorrechte und verwendet keine fremde App-Identität. Falls eine PowerShell-Richtlinie das lokale Skript sperrt, zuerst Skript/Herkunft prüfen; keine organisationsweite Sicherheitsrichtlinie umgehen.

Autostart ist standardmäßig aus. Er wird nur mit dem ausdrücklichen Parameter `-AutoStart` gesetzt. Die Release-EXE hat kein separates Konsolenfenster. CLI-Ausgabeumleitung/`--status` unter Windows ist nicht getestet; der Statushelfer ist primär für Linux gedacht.

### Builds und Releases

Der einzige Workflow `release.yml` startet automatisch nur beim Veröffentlichen eines GitHub-Releases. Er übernimmt die Version aus dem Tag (z. B. `1.0.2` oder `v1.0.2`) für Anwendung und Installer. `Cargo.toml` und `Cargo.lock` werden dafür ausschließlich im Build-Verzeichnis angepasst; manuelle Versionsänderungen oder zusätzliche Versions-Commits sind nicht nötig. Danach prüft er Paketskripte und Abhängigkeiten, testet und baut Linux x86-64, Windows x86-64, macOS Apple Silicon und Intel und lädt alle Installer und SHA-256-Prüfsummen hoch. Pushes auf `main`, Tag-Pushes und Pull Requests starten keine Actions. Ein vorhandenes Release kann manuell erneut gebaut werden. Signierung und Apple-Notarisierung sind über GitHub-Secrets vorbereitet. Ohne Zertifikate bleiben die Installer unsigniert.

Für Linux x86-64 erstellt derselbe Workflow ein Flatpak-Bundle. Installieren: `flatpak install --user Hush-1.0.0-linux-x86_64.flatpak`; starten: `flatpak run io.hush.github`. Die Freedesktop-Runtime wird bei Bedarf von Flathub geladen. Neue Versionen werden durch Installation des jeweiligen neuen Bundles aktualisiert.

Einrichtung, Dateinamen und Release-Ablauf: [RELEASING.md](docs/RELEASING.md). Der zusammengefasste Release-Workflow wurde lokal geprüft und muss nach Übernahme ins Repository erstmals auf GitHub laufen.

## GitHub verbinden: minimale Rechte statt voller Repository-Zugriff

Hush spricht in 0.1 ausschließlich **github.com** an. Unterstützt wird ein GitHub-Konto pro Betriebssystem-Benutzerprofil.

**1. Benachrichtigungs-Token:** Einen separaten, ablaufenden **klassischen Personal Access Token** mit ausschließlich `notifications` anlegen. Optional zusätzlich `read:org`, wenn Hush deine Teams automatisch ermitteln soll. Andere klassische Scopes, auch `repo`, `public_repo`, `gist` oder `user`, werden beim Verbinden abgewiesen. Das ist absichtlich strenger als bei vielen anderen Clients.

**2. Optionaler Detail-Token:** Für private Repositories zusätzlich einen **Fine-grained Personal Access Token** erstellen: nur ausgewählte Repositories; `Issues: Read-only`, `Pull requests: Read-only`, bei Bedarf `Discussions: Read-only`. Metadaten-Lesezugriff ist Bestandteil des GitHub-Modells. Für Commit-Kommentare kann zusätzlich `Contents: Read-only` nötig sein. Keine Schreib- oder Administrationsrechte hinzufügen. Organisationen können eine Genehmigung verlangen.

Der Detail-Token muss zum selben Benutzer gehören. Hush überprüft Identität und dass keine klassischen Scopes gemeldet werden; **die tatsächliche Fine-grained-Berechtigungsmatrix kann diese Version nicht vollständig nachprüfen**. Du musst die Nur-Lesen-Auswahl bei GitHub kontrollieren. Die Anwendung selbst verwendet ausschließlich Leseoperationen, unabhängig davon, ob ein zu mächtiger Detail-Token eingegeben wurde.

Nur ein Detail-Token ist konfigurierbar. Ein Fine-grained-Token ist auf seinen ausgewählten Ressourcenbesitzer beschränkt; mehrere private Organisationen gleichzeitig können deshalb in 0.1 nicht vollständig abgedeckt sein. Fehlender Zugriff soll im Verbindungsstatus sichtbar werden, nicht als „kein neues Ereignis“ verschwinden.

Fehlender Zugriff ist in den Einstellungen unter „Repository-Zugriff“ und als Hinweis im Hauptfenster sichtbar. Bei privaten Repositories kann GitHub fehlende Berechtigungen als HTTP 404 melden. Einen Detail-Token im Feld „Detail-Token für private Repositories“ ergänzen und „Tokens speichern“ wählen; der bereits gespeicherte Benachrichtigungs-Token muss dazu nicht erneut eingegeben werden. Die ersten vorhandenen Issues werden ohne System-Banner importiert; spätere neue Issues erzeugen Benachrichtigungen. Die Schaltfläche „Test-Benachrichtigung“ prüft den Betriebssystem-Kanal unabhängig von GitHub.

Die beiden Tokens in den Passwortfeldern der Kontoeinstellungen eingeben, **nie als Kommandozeilenargument, Konfigurationsdatei oder Screenshot**. Falls du `read:org` vermeiden möchtest, kannst du deine Team-Slugs unter „Erweitert“ selbst eintragen, beispielsweise `meine-org/frontend`. Diese manuelle Liste wird nicht auf Mitgliedschaft verifiziert und muss aktuell gehalten werden.

Auf GitHub `Participating and @mentions` / „On GitHub“ aktiviert lassen. Hush verwendet den Benachrichtigungsstrom zur Entdeckung von Threads. Die ausgewählten Issue-Repositories werden unabhängig davon abgefragt; dafür ist kein pauschales `Watch → All Activity` erforderlich.

## Fenster und Hintergrundprozess

Der normale Start öffnet das Fenster, legt ein Tray-/Menüleisten-Symbol an und startet bei Bedarf einen getrennten Hintergrundprozess. Unter Linux wird das Fenster beim Schließen tatsächlich beendet; Tray und Dienst laufen unabhängig weiter. „Hush öffnen“ oder ein erneuter Launcher-Start öffnet ein neues Fenster mit den gespeicherten Einstellungen und Ereignissen. Unter Windows/macOS wird das vorhandene Fenster ausgeblendet und wieder eingeblendet. Je ein Dateilock verhindert doppelte Hintergrundprozesse und doppelte Fenster/Tray-Symbole. Ohne verfügbares Tray schließt das Fenster wie bisher, und der Hintergrunddienst bleibt aktiv.

Ein Linksklick auf das Tray-Icon öffnet das Fenster oder holt es nach vorne. Ein Rechtsklick öffnet das Menü. Das Tray zeigt Dienststatus und Ungelesen-Zähler, ohne Ereignistitel oder Repository-Namen. Es bietet Öffnen, Einstellungen, Aktualisieren, 30 Minuten Pause/Fortsetzen, Dienst starten und „Hush beenden“. Diese letzte Aktion beendet Fenster, Tray und Dienst, sobald laufende API-Anfragen abgeschlossen sind. „Hintergrunddienst beenden“ in den Einstellungen oder `hush --stop` stoppt nur das Polling; das Tray bleibt erreichbar.

- `hush --tray`: direkt im Tray starten; falls kein Tray verfügbar ist, wird das Fenster angezeigt. Die mitgelieferten Autostart-Vorlagen verwenden diesen Modus.
- `hush --start`: nur den Dienst starten und dessen Start bestätigen lassen.
- `hush --background`: Dienst im aufrufenden Prozess ausführen, ohne Fenster/Tray. Fehler stehen auf stderr.
- `hush --status`: JSON mit `running`, `healthy`, `phase` und `service_error` sowie den bisherigen Statusfeldern.
- `hush --demo`: reine Offline-Demo, ohne Dienst, Tray oder Kontodaten.

Startfehler erscheinen in der Oberfläche und beim Start über `--start`. Das private Datenverzeichnis enthält außerdem `service.log` (unter Linux normalerweise `~/.local/share/hush/service.log`); bei einem neuen Dienststart wird dieses Fehlerprotokoll ersetzt. Unter Linux enthält `window.log` gegebenenfalls Fehler beim Öffnen des Fensters. Ein erfolgreicher Prozessstart zählt erst nach bestätigtem Heartbeat als erfolgreicher Dienststart. Gleichzeitige SQLite-Schreibzugriffe von Oberfläche, Worker und Heartbeat werden vor dem Lesen serialisiert, damit sie nicht an einem Lock-Upgrade scheitern. Die Dateivorbereitung erhält außerdem bestehende SQLite-Sperren, sodass Dienst und Oberfläche denselben Datenstand sehen.

Ausloggen/Neustart beendet die Prozesse; Autostart erfolgt nur nach expliziter Einrichtung. System-Banner haben weiterhin keine Klick-Aktion.

Die Übertragung ist **Polling, kein serverseitiger Push**. Standard: alle zwei Minuten, mindestens nach GitHubs `X-Poll-Interval`, bei Rate-Limits mit Wartezeit. Netzwerkunterbrechungen, Suchindex-Latenz, große Backlogs und fehlende Berechtigungen können zu Verzögerungen oder unvollständiger Erfassung führen.

## Bewusste Grenzen von 0.1

Keine GitHub-Projects-Boards, Gists, organisationsweiten Discussions, GitHub-Enterprise-Hosts, Multi-Account- oder Geräte-Synchronisation. Keine Garantie für Erwähnungen in Threads, die GitHub gar nicht in deinen Notifications liefert, für inzwischen gelöschte Ereignisse oder lückenlosen historischen Import. Die Freigabe eines Team-Reviews ist etwas anderes als eine Gruppen-Zuweisung an einen PR. Standardmäßig sind es direkte persönliche Erwähnungen, nicht jede Team-Erwähnung.

Requests sind begrenzt: 8 MiB je Antwort, maximal zehn Seiten je Abfrage, 160 API-Aufrufe je Zyklus, 24 Thread-Aufgaben je Zyklus. Bei Notifications entspricht die Seitengrenze derzeit höchstens 500 Einträgen pro vollständiger Abfrage, bei vielen anderen Listen 1.000. Sehr große Threads/Backlogs bleiben mit Warnung offen, statt still einen erfolgreichen vollständigen Sync vorzutäuschen. Ein bei jeder Wiederholung zu großer Thread wird nicht automatisch aufgeteilt.

Keine Remote-Avatare, kein Cloud-Relay, keine Telemetrie und keine automatischen Programm-Updates im eigenen Anwendungscode. Corporate-Proxys werden derzeit nicht automatisch übernommen. Weitere Sicherheitsgrenzen: [SECURITY.md](docs/SECURITY.md).

## Projektstruktur

```text
src/ui/          Native egui-Oberfläche, Designsystem, Vektor-Icons, Offline-Demo
src/api.rs       Begrenzter, nur lesender GitHub-Transport
src/filter.rs    Konkrete Ereigniserkennung statt sticky Notification-Gründe
src/engine.rs    Hintergrundprozess, Scheduling, Wiederholungen, Zustellung
src/storage.rs   Private SQLite-Datenbank, IDs, Cursor, Aufgaben, Outbox
src/secrets.rs   Betriebssystem-Schlüsselbund; kein Klartext-Fallback
src/notify.rs    Native Banner, private Voreinstellung
preview/         Separate interaktive HTML-Designvorschau
packaging/       Benutzerinstallation und optionale Autostarts
.github/         Noch auszuführende Build-/Audit-Workflows
```

## Primärquellen zur Implementierung

GitHub: [Notifications](https://docs.github.com/en/rest/activity/notifications), [Issue-Timeline](https://docs.github.com/en/rest/issues/timeline), [PR-Reviews](https://docs.github.com/en/rest/pulls/reviews), [Issues](https://docs.github.com/en/rest/issues/issues), [Teams](https://docs.github.com/en/rest/teams/teams), [GraphQL-Objekte](https://docs.github.com/en/graphql/reference/objects), [Personal Access Tokens](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens).

Desktop: [eframe 0.36.2](https://docs.rs/eframe/0.36.2/eframe/), [keyring 3.6.3](https://docs.rs/keyring/3.6.3/keyring/), [notify-rust](https://docs.rs/notify-rust/latest/notify_rust/), [Dank Material Shell](https://danklinux.com/docs/dankmaterialshell/overview). Abgerufen bzw. gegengeprüft am 8. Oktober 2026. Dokumentation und APIs können sich ändern.
