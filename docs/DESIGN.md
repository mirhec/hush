# Oberfläche

Hush zeigt GitHub-Benachrichtigungen in einer kompakten chronologischen Liste. Ein Eintrag ist 62 Pixel hoch: Titel und Zeit in der ersten Zeile, Repository und Person in der zweiten. Ereignis-Icon und Ungelesen-Markierung stehen links. Lange Titel werden gekürzt und sind im Tooltip sowie in der Detailansicht vollständig lesbar.

## Visuelles System

Neutrales Graphit mit Mint für Auswahl, Fokus und erhaltene Reviews. Violett kennzeichnet PR-Anfragen, Amber Erwähnungen und Blau neue Issues. Farbe wird durch eigene Vektor-Icons und Beschriftungen ergänzt. Die helle Variante verwendet helle graue Flächen mit derselben semantischen Zuordnung.

Eine gemeinsame Kopfzeile enthält Seitentitel, Dienststatus und Aktionen. Überschriften sind 20 Pixel groß, Listentitel 14, Fließtext 13. Trennlinien ersetzen große abgerundete Karten. Innenabstände betragen überwiegend 6–16 Pixel. Ein grüner Streifen und eine dezente Hintergrundfarbe markieren den ausgewählten Eintrag.

Das Startfenster ist 1040 × 720 Pixel groß, die Mindestgröße 640 × 480. Die Navigation ist 188 Pixel breit; unter 840 Pixel Fensterbreite wird daraus eine 56 Pixel breite Icon-Leiste mit Tooltips. Ab 980 Pixeln steht eine geöffnete Detailansicht neben der Liste, darunter ersetzt sie diese. Der gesamte Detailinhalt ist scrollbar; die Aktionen bleiben unten erreichbar.

Eigene Vektor-Icons werden mit egui-Painter gezeichnet. Native Typografie verwendet die mit eframe gelieferten Standardfonts. Die separate HTML-Vorschau verwendet verfügbare Systemfonts; es sind keine zusätzlichen Schriftdateien beigefügt und kein externes Font-CDN eingebunden.

## Interaktion

Navigation: Posteingang, vier Ereignisfilter, Archiv und Einstellungen. Suche ist mit Strg/Cmd+K erreichbar. Escape schließt Details und Toasts. Erledigen lässt sich über das Archiv rückgängig machen; es schließt keine Issues oder Pull Requests auf GitHub.

Einstellungen sind auf drei Tabs verteilt: Benachrichtigungen, Konto und Diagnose. Die Benachrichtigungsarten und Zustelloptionen stehen ab 520 Pixel Inhaltsbreite nebeneinander. Schalterzeilen sind höchstens 40 Pixel hoch; ergänzende Erklärungen stehen in Tooltips. Speichern und Test-Benachrichtigung bleiben unter dem Scrollbereich sichtbar. Ein Wechsel zwischen den Tabs erhält den Entwurf.

Zugangsdaten und Team-Liste stehen unter Konto; Diagnose enthält Dienststeuerung und lokale Daten. Sensible Felder sind Passwortfelder und werden nicht in GUI-Persistenz serialisiert. Die Voreinstellung für System-Banner enthält keine vertraulichen Repository-/Inhaltsdaten. Rückmeldungen erscheinen als Toasts für vier Sekunden, Fehler für zehn Sekunden.

## Darstellung prüfen

Die Abbildungen [Posteingang](screenshots/compact-inbox.png) und [kleine Einstellungen](screenshots/compact-settings-small.png) zeigen die tatsächlichen egui-Zeichendaten mit Demoinhalten. Sie wurden ohne Desktop-Sitzung aus den egui-Dreiecken und dem zugehörigen Font-Atlas gerendert. Sie belegen das Layout, keine native Betriebssystem-Interaktion.

```sh
HUSH_UI_CAPTURE_DIR=/tmp/hush-ui cargo test --locked --lib export_native_ui_frames -- --ignored
```

Danach `scripts/render-ui.html` im Browser öffnen und eine exportierte JSON-Datei auswählen. Der Renderer baut keine eigene HTML-Oberfläche nach. Der ältere HTML-Prototyp in `preview/` ist eine separate historische Designvorschau und entspricht nicht mehr dem kompakten Layout. Bildschirmleser und betriebssystemspezifische DPI-Einstellungen bleiben auf echten Zielsystemen zu prüfen.
