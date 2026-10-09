# Architektur und bewusst begrenzte Semantik

## Prozesse

Ein ausführbares Programm mit Fenstermodus und `--background`-Modus. Das Fenster startet den Hintergrundprozess über den absoluten Pfad der eigenen ausführbaren Datei, ohne Shell. Ein exklusives Dateilock verhindert doppelte pollende Prozesse. Fenster und Worker öffnen jeweils ihre eigene SQLite-Verbindung. Kein HTTP-Server auf localhost, kein offener Netzwerkport, kein Tokentransport in IPC-Nachrichten. Der Worker liest Zugangsdaten selbst aus dem Betriebssystem-Schlüsselbund.

Ein Heartbeat aktualisiert einen Zeitstempel alle fünf Sekunden. Der Worker kontrolliert Stop-/Refresh-Flags zwischen Zyklen; ein laufender Zyklus kann aus mehreren jeweils auf 20 Sekunden begrenzten Requests bestehen. „Stop“ ist deshalb keine sofortige Unterbrechung eines laufenden HTTP-Requests. Der Worker ist kein automatisch installierter Systemdienst und startet ohne expliziten Autostart nicht nach einem Reboot neu.

Unter Linux besitzt ein eigener Desktop-Prozess das StatusNotifier-Tray über GLib/GIO und startet das Fenster als Kindprozess (`--window`, interner Modus). Die Tray-Sperre verhindert doppelte Desktop-Prozesse; die Fenstersperre verhindert doppelte Fenster. Ein normaler Schließbefehl beendet den Fensterprozess, während Tray und Worker weiterlaufen. Tray und Launcher öffnen bei Bedarf ein neues Fenster. Damit bleibt Hush nativ unter Wayland/Niri nutzbar: [`winit::Window::set_visible`](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.set_visible) unterstützt Wayland nicht. Ein abgefangener Schließbefehl mit anschließendem `Visible(false)` würde das Fenster dort unbedienbar offen halten.

Das Linux-Tray exportiert `org.kde.StatusNotifierItem` mit `ItemIsMenu=false`. `Activate` öffnet das Fenster; `com.canonical.dbusmenu` stellt das Rechtsklick-Menü bereit. Die Registrierung verwendet Objektpfad und eindeutigen D-Bus-Absender statt einer PID-basierten Servicekennung. Ein Neustart des Tray-Hosts löst eine erneute Registrierung aus. ARGB-Pixmaps werden über D-Bus übertragen und benötigen auch im Flatpak keine Freigabe privater Bilddateien. Windows und macOS verwenden `tray-icon` mit deaktiviertem Linksklick-Menü und einem Handler für den primären Klick.

Unter Windows/macOS besitzt weiterhin das Fenster das native Tray; Tray-Aktionen wecken `App::logic` auch bei ausgeblendetem Fenster. SQLite-Flags übertragen unter Linux Öffnen/Einstellungen/Beenden; Pause und Dienststatus werden aus der Datenbank gelesen. `--tray` startet unter Linux ausschließlich Tray und Worker, ohne zunächst ein Fenster anzulegen; `--background` bleibt auf allen Plattformen ein reiner Worker. Fehlt der Linux-StatusNotifier-Host oder verschwindet er, wird ein Fenster geöffnet. Explizites vollständiges Beenden fordert Fenster und Worker zum Beenden auf und entfernt danach das Tray. Beim bloßen Schließen des Fensters bleibt der Worker aktiv; ein zuvor angehaltener Worker wird beim erneuten Öffnen nicht automatisch gestartet.

Alle SQLite-Transaktionen mit anschließendem Schreibzugriff beginnen mit `BEGIN IMMEDIATE`. Ein vorheriges `BEGIN DEFERRED` konnte beim Wechsel von Lesen zu Schreiben trotz Busy-Timeout sofort mit `database is locked` scheitern, insbesondere beim gleichzeitigen ersten Heartbeat. Der Heartbeat wird vor bestätigtem Start synchron initialisiert und beim Stoppen vor Freigabe der Workersperre beendet. Startaufrufe werden durch eine weitere Dateisperre serialisiert; sie prüfen Prozessende und Heartbeat und erfassen stderr im privaten `service.log`. Fehler des Workers bleiben zusätzlich im Laufzeitstatus erhalten.

Beim Vorbereiten einer vorhandenen Datenbank wird kein zusätzlicher Dateideskriptor geöffnet und geschlossen. Unter POSIX würde dessen Schließen die SQLite-Sperren aller Threads dieses Prozesses aufheben. Das konnte beim Öffnen der Heartbeat-Verbindung dazu führen, dass ein anderer Prozess die noch aktive WAL-Datei entfernt und Worker sowie Oberfläche getrennte Zustände sehen. Die Datenbank wird nur bei der ersten Anlage außerhalb von SQLite geöffnet; bestehende Dateien werden ausschließlich per Metadaten geprüft und auf private Rechte gesetzt. Hintergrund: [SQLite, POSIX advisory locks](https://www.sqlite.org/howtocorrupt.html#_posix_advisory_locks_canceled_by_a_separate_thread_doing_close_).

## Ereignisquellen

1. Notifications API entdeckt aktualisierte Threads, auch wenn GitHub sie schon als gelesen markiert hat. Es werden echte Timeline-/Kommentar-/Review-Einträge nachgelesen, nicht `reason=mention` blind als Ereignis interpretiert.
2. Eine zusätzliche Suche nach kürzlich aktualisierten eigenen PRs entdeckt Reviews. Die Suche ist auf die Sichtbarkeit des verwendeten Tokens und GitHubs Suchindex begrenzt. Sie garantiert keine Echtzeit-Vollständigkeit.
3. Ausgewählte Issue-Repositories werden direkt abgefragt. Nur die Erstellungszeit zählt; neue Kommentare und PR-Einträge zählen nicht als neue Issues.
4. Teams werden bei `read:org` etwa alle 15 Minuten ermittelt. Manuelle `org/slug`-Einträge werden ergänzt; Änderungen einer Organisation können bis zur nächsten Ermittlung verzögert sichtbar werden. Manuelle Einträge sind keine verifizierte Mitgliedschaft.

Für Benachrichtigungen bleiben GitHubs eigene Zustell-/Abonnementregeln relevant. Der Benutzer sollte „On GitHub“ für Teilnahme und Erwähnungen nicht deaktivieren. Ein fehlender, inzwischen gelöschter oder nicht zugänglicher Thread ist nicht rekonstruierbar. Unsupported-Typen werden als Warnung gesammelt, nicht in eine unsichere Fallback-„Erwähnung“ verwandelt.

## Zeit, Wiederholung und lokale Identität

Quell-Cursor werden nur nach erfolgreicher vollständiger Entdeckung aktualisiert; Thread-Aufgaben werden vorher dauerhaft gespeichert. Die Erstauswertung schaut 24 Stunden zurück und bleibt still. Folgende Läufe überlappen um zwei Minuten. Neue/retryende Thread-Aufgaben werden zusammengeführt. Wenn ein stiller Erstimport und ein Live-Lauf kollidieren, hat die neuere Grenze Vorrang, um keinen alten Bannersturm auszulösen; dies kann bei einem unvollständigen Erstimport historische Ereignisse auslassen.

Die konkrete Ereignis-ID (z. B. Review-ID oder Kommentar-ID plus Namespace) verhindert wiederholte Hinweise auf dieselbe Aktivität. Das ist absichtlich nicht die veränderliche Notification-Thread-Zeit. Ein neu eingereichtes weiteres Review hat eine andere ID. Die Änderung eines bereits gemeldeten Kommentars ist kein neues Ereignis. Änderungen, die eine neue Erwähnung in einen bisher nicht gemeldeten Kommentar einfügen, können anhand `updated_at` berücksichtigt werden.

Der Lesestatus ist rein lokal. Eine neuere Zustellung bekommt ihren eigenen Ereigniseintrag. Es gibt keinen Schreibzugriff auf GitHub und keinen Abgleich zwischen Geräten. Die frühere Archivfunktion ist entfernt. Die alte SQLite-Spalte `archived` bleibt zur Kompatibilität erhalten; solche Einträge werden beim Lesen als gelesen in den gemeinsamen Verlauf aufgenommen. Alte JSON-Payloads bleiben lesbar, neue enthalten kein Archivfeld.

## Posteingang und Navigation

Die Oberfläche besteht aus einer Posteingangsliste mit Suche und Filtermenü. Das Fenster startet mit 440 × 640 Pixeln; die Mindestgröße beträgt 360 × 480. Beim Öffnen ist der Ungelesen-Filter aktiv. Das Menü enthält außerdem die Auswahl aller oder einer einzelnen Ereignisart und die Aktion, den gesamten Verlauf als gelesen zu markieren.

Ein Klick auf einen Eintrag validiert den GitHub-Link und übergibt ihn an den Systembrowser. Erst nach einem erfolgreichen Browser-Aufruf wird der lokale Lesestatus gespeichert und die Liste aktualisiert. Ein Fehler beim Öffnen lässt den Eintrag ungelesen und erscheint als Toast. Ein erfolgreicher Aufruf bestätigt die Übergabe an den Browser, nicht das Laden der GitHub-Seite.

Das Zahnrad oben rechts öffnet die Einstellungen mit den Tabs Benachrichtigungen, Konto und Diagnose. `Strg+K` / `Cmd+K` führt zur Suche; `Esc` schließt das Filtermenü oder kehrt aus den Einstellungen zum Posteingang zurück. Die Oberfläche hat keine separate Detailansicht, Seitenleiste oder Archivansicht.

## Fehler und Grenzen

160 Requests und 24 Thread-Aufgaben pro Zyklus begrenzen Last. Jede Seite enthält höchstens 50 Notifications bzw. meist 100 andere Objekte, mit zehn Seiten als Sicherheitsgrenze. Wird die Grenze ohne Vollständigkeitsnachweis erreicht, bleibt der Source-Cursor stehen bzw. der Thread in Retry. Es gibt noch keine automatische zeitliche Aufteilung extrem großer Threads. Netzwerk- und Authentifizierungsfehler, unvollständige Suchergebnisse, nicht lesbare private Repositories und API-Limits sind Statusinformationen, nicht still „keine Ereignisse“.

GitHubs `X-Poll-Interval`, Rate-Limit-Reset und `Retry-After` steuern längere Pausen. Die App ist ein Polling-Client, keine Webhook-Zentrale. Der feste API-Origin und das Redirect-Verbot bedeuten unter anderem: keine Enterprise-Hosts und gegebenenfalls Fehler bei verschobenen/umbenannten Repositories.

## Zustellung

Einfügungen, Account-Abgleich und Deduplizierung laufen transaktional. Eine dauerhaft gespeicherte Outbox wird nach erfolgreichem Aufruf des OS-Backends quittiert. Ein Crash zwischen Betriebssystem-Zustellung und Quittierung kann ein Banner duplizieren: **keine Exactly-once-Garantie**. Ein erfolgreicher OS-Aufruf beweist außerdem nicht, dass der Benutzer ein Banner gesehen hat; Fokusmodus oder Systemrechte können die Anzeige verhindern.

Erstimport, deaktivierte Regeln, ausgeschaltete Systembenachrichtigungen und Ruhepausen erzeugen keine spätere Replay-Flut. Größere Batches werden zu einem privaten Sammelhinweis gebündelt. Schaltet der Benutzer während eines Requests Regeln oder Pause um, werden die aktuellen Einstellungen vor Zustellung nochmals gelesen.

## Erweitern

Für tatsächlich vollständige, serverseitige Ereignisse über viele Organisationen wäre eine GitHub-App mit expliziten Installationen und signierten Webhooks sinnvoll. Das benötigt Infrastruktur und eine andere Vertrauens-/Berechtigungsstruktur; Hush enthält dies nicht. Weitere Detailschlüssel für mehrere Ressourcenbesitzer, Toast-Aktionen und ETag-Caching sind noch nicht implementiert. Signierte Distributionen benötigen die in [RELEASING.md](RELEASING.md) beschriebenen Zertifikate und Secrets.
