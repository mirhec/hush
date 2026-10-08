# Sicherheitsmodell / vor dem produktiven Einsatz

Hush 0.1.0 ist nicht extern auditiert. Rust-Code, Bibliotheksauflösung, Schlüsselbund und native Installer konnten in der Erstellungsumgebung nicht ausgeführt werden. Dieses Dokument beschreibt implementierte Maßnahmen und verbleibende Grenzen, keine Sicherheitszertifizierung.

## Datenfluss

```text
GitHub API (api.github.com, HTTPS)
        |
        | Token im Authorization-Header, nur lesende REST-/GraphQL-Abfragen
        v
Hush-Hintergrundprozess -> private SQLite-Datenbank -> egui-Fenster
        |
        +-> lokaler Benachrichtigungsdienst des Betriebssystems

Betriebssystem-Schlüsselbund -> Authentifizierungs-Header im Arbeitsspeicher
Benutzer klickt Link -> github.com im Systembrowser
```

Keine fremden API-Hosts oder externen Bildquellen. Keine im Anwendungscode eingebaute Telemetrie, Crash-Upload-, Update- oder Push-Relay-Funktion. Der Browser kann nach einem bewussten Link-Klick seiner eigenen Konfiguration entsprechend kommunizieren; Betriebssysteme können eigene Diagnosedienste haben. Aussagen über Hushs eigenen Code decken nicht automatisch sämtliche transitiven Abhängigkeiten oder das Betriebssystem ab.

## Zugangsdaten

- Classic-Token nur `notifications`, optional `read:org`; zusätzliche klassische Scopes werden abgewiesen.
- Optionaler Fine-grained-Token: ausgewählte Ressourcen, ausschließlich benötigte Leserechte. Die UI/Verbindung kontrolliert die Identität, aber nicht die vollständige Berechtigungsmatrix des Fine-grained-Tokens.
- Speicherung via `keyring`: Linux Secret Service, macOS Keychain, Windows Credential Manager. Kein fallback in JSON, SQLite, Umgebungsvariablen oder `/tmp`.
- Keine Token-Argumente für Unterprozesse, kein Shell-/curl-Transport. Eingabepuffer werden nach Übergabe nach Möglichkeit mit `Zeroizing` gelöscht; Framework/HTTP-Bibliothek können trotzdem weitere Kopien im Speicher besitzen. Keine Garantie gegen Speicherabbilder, Debugger oder kompromittierte Prozesse desselben Benutzers.
- `HeaderValue::set_sensitive(true)` verhindert die normale Debug-Ausgabe des Auth-Headers. HTTP-Fehler werden auf eigene, tokenfreie Fehlertypen abgebildet.
- Schlüsselbund und SQLite können nicht atomar gemeinsam committen. Scheitert das Speichern dazwischen, kann ein Eintrag im Schlüsselbund verbleiben. Der Fehler wird nicht als erfolgreicher Login ausgegeben.
- „Konto trennen“ invalidiert die Datenbank zuerst, dann werden beide Schlüsselbund-Einträge entfernt. Löschfehler werden angezeigt. Zum vollständigen Widerruf Tokens zusätzlich direkt auf GitHub widerrufen.

## Netzwerk

API-Ziele müssen exakt `https://api.github.com` sein, ohne Userinfo, abweichenden Port oder Fragment. Redirects werden vollständig abgewiesen, auch bei umbenannten Repositories; dies kann die Funktion einschränken, verhindert aber eine Weiterleitung von Zugangsdaten. Öffnen im Browser verlangt exakt `https://github.com`.

Nur GET-Requests und zwei feste, nur lesende GraphQL-Abfragestrukturen über POST `/graphql`. Kein generischer Mutations- oder Ausführungs-Endpunkt. Standard-Proxyerkennung ist deaktiviert; Unternehmensproxys werden nicht unterstützt. TLS-Prüfungen werden nicht abgeschaltet. Antwortgröße, Dauer, Seitenzahl und Request-Budget sind begrenzt. IDs/Repository-Pfade werden validiert. Remote-Inhalte werden nicht als HTML, Skript, Shellbefehl oder egui-Markup ausgeführt.

Die App scannt nicht alle erreichbaren privaten Repositories nach Inhalten; für Reviews kann sie aber eine Benutzersuche durchführen, und für die benachrichtigten Threads werden Kommentare/Reviews gelesen. Den tatsächlichen Umfang bestimmen Token-Rechte und die konfigurierten Regeln. Sie kann damit sensible Informationen erhalten, auch wenn Banner standardmäßig anonym bleiben.

## Lokale Daten und Bildschirmprivatsphäre

SQLite speichert Repository-Namen, Titel, Akteure und gekürzte Ereignis-/Kommentarinhalte sowie technische Thread-Aufgaben. **Die Datenbank ist nicht verschlüsselt.** Das Datenverzeichnis wird unter Unix auf `0700`, die Hauptdatei auf `0600` gesetzt. WAL-/SHM-Dateien liegen im privaten Verzeichnis. Windows verwendet das lokale Benutzerprofil und dessen ACLs, keine eigens gehärtete zusätzliche ACL. Kein geteilter `/tmp`-Fallback.

Der letzte Verzeichnispfad und Datendateien werden auf Symlinks geprüft. Das ist keine vollständige Abwehr gegen einen privilegierten Angreifer, races in kontrollierten Parent-Verzeichnissen oder bereits kompromittierte gleiche Benutzerrechte.

Banner-Inhalte sind standardmäßig privat. Titel/Repo/Akteur werden nur nach ausdrücklichem Einschalten der Inhaltsvorschau gesendet. Das Betriebssystem kann Benachrichtigungen speichern, auf dem Sperrbildschirm anzeigen oder bei Bildschirmfreigabe sichtbar machen. Entsprechende Systemeinstellungen gelten zusätzlich.

„Verlauf leeren“ löscht sichtbare Ereignisse logisch, behält aber IDs/Cursor zur Dublettenvermeidung. „Konto trennen“ löscht zusätzlich Aufgaben und Cursor. SQLite-WAL, freie Datenbankseiten, Backups oder OS-Benachrichtigungshistorien werden dadurch nicht sicher überschrieben. Bei hohen Geheimhaltungsanforderungen Festplattenverschlüsselung und ein passendes Lösch-/Backupkonzept einsetzen.

## Integrität / Lieferkette

Keine privilegierte Installation vorgesehen. Installer verändern nur die dort beschriebenen Benutzerdaten, Startmenü-/Launcher-Einträge und auf ausdrückliche Anforderung Autostart. Vor Ausführung lesen. Die Windows-COM-/AppID- und macOS-Signierungswege sind noch nativ zu validieren.

Die Build-Workflows haben nur `contents: read`; Checkout-Credentials werden nicht behalten. Verwendete GitHub Actions sind auf konkrete Commit-SHAs gepinnt. Es liegt noch **keine aufgelöste Cargo.lock** vor; CI muss diese erzeugen, prüfen und für künftige Builds versionieren. Die erste Auflösung ist nicht reproduzierbar vorgegeben. `cargo audit` ist als Workflow vorhanden, aber nicht gelaufen. Das MIT-Lizenzdokument gilt für den eigenen Code; Abhängigkeiten behalten ihre Lizenzen.

## Noch notwendige Freigabeprüfungen

Erfolgreiche Rust-Builds, Tests und Clippy auf allen Zielen; Abhängigkeitsaudit; Token-Rechte im GitHub-Dialog; Zustellung unter Niri/DMS, Windows und beiden macOS-Architekturen; Schlüsselbund gesperrt/entsperrt; Rate-Limit-/Offline-/Neustarttests; Konto-Trennung während laufender Requests. Erst danach für echte sensible Repositories einsetzen. Keine CVE-freie, vollständig sichere oder lückenlos zustellende Anwendung behaupten.
