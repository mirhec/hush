# Oberfläche

Hush zeigt GitHub-Benachrichtigungen als einzelne chronologische Posteingangsliste. Das Startfenster ist 440 × 640 Pixel groß, die Mindestgröße 360 × 480. Seitenleiste, Archiv und Detailansicht entfallen.

## Visuelles System

Neutrales Graphit mit Mint für Auswahl, Fokus und erhaltene Reviews. Violett kennzeichnet PR-Anfragen, Amber Erwähnungen und Blau neue Issues. Farbe wird durch eigene Vektor-Icons und Beschriftungen ergänzt. Die helle Variante verwendet helle graue Flächen mit derselben semantischen Zuordnung.

Die Kopfzeile enthält den Posteingang, den Ungelesen-Zähler und rechts das Einstellungs-Icon. Darunter stehen Suche und Filter-Button nebeneinander. Ein Listeneintrag ist 56 Pixel hoch: Titel oben, Repository, Person und Zeit darunter. Ereignis-Icon und Ungelesen-Markierung stehen links. Lange Texte werden gekürzt; der Tooltip zeigt den vollständigen Titel. Überschriften sind 18 Pixel groß, Listentitel 13, Metadaten 11. Feine Linien trennen die Einträge.

Eigene Vektor-Icons werden mit egui-Painter gezeichnet. Native Typografie verwendet die mit eframe gelieferten Standardfonts; zusätzliche Schriftdateien oder externe Font-CDNs sind nicht erforderlich.

## Interaktion

Standardmäßig erscheinen nur ungelesene Benachrichtigungen. Im Filtermenü lässt sich „Ungelesen“ abschalten und nach Ereignistyp filtern. Die Suche berücksichtigt Titel, Repository, Person und Inhalt. Die Aktion „Alle als gelesen markieren“ gilt für den gesamten lokalen Verlauf. Ein Klick auf einen Eintrag öffnet ihn direkt auf GitHub und markiert ihn nach erfolgreicher Übergabe an den Browser lokal als gelesen. Schlägt die Übergabe fehl, bleibt er ungelesen und eine Fehlermeldung erscheint als Toast. Einträge, die frühere Versionen archiviert haben, bleiben als gelesener Verlauf zugänglich.

Strg/Cmd+K öffnet die Suche. Escape schließt das Filtermenü oder führt von den Einstellungen zurück zum Posteingang. Der Zurück-Pfeil in den Einstellungen hat dieselbe Funktion. Ungespeicherte Einstellungen bleiben beim Wechsel der Ansicht erhalten.

Einstellungen sind auf drei Tabs verteilt: Benachrichtigungen, Konto und Diagnose. Die Benachrichtigungsarten und Zustelloptionen stehen ab 520 Pixel Inhaltsbreite nebeneinander, darunter untereinander. Schalterzeilen sind höchstens 40 Pixel hoch; ergänzende Erklärungen stehen in Tooltips. Speichern und Test-Benachrichtigung bleiben unter dem Scrollbereich sichtbar. Pause/Fortsetzen steht unter Aktualisierung, der manuelle Abruf unter Diagnose. Das Erscheinungsbild lässt sich in der Kopfzeile der Einstellungen umschalten.

Zugangsdaten und Team-Liste stehen unter Konto; Diagnose enthält Dienststeuerung und lokale Daten. Sensible Felder sind Passwortfelder und werden nicht in GUI-Persistenz serialisiert. Die Voreinstellung für System-Banner enthält keine vertraulichen Repository-/Inhaltsdaten. Rückmeldungen erscheinen als Toasts für vier Sekunden, Fehler für zehn Sekunden.

## Darstellung prüfen

Die Abbildungen [Posteingang](screenshots/compact-inbox.png), [Filtermenü](screenshots/inbox-filters.png) und [kleine Einstellungen](screenshots/compact-settings-small.png) zeigen die tatsächlichen egui-Zeichendaten mit Demoinhalten. Sie wurden ohne Desktop-Sitzung aus den egui-Dreiecken und dem zugehörigen Font-Atlas gerendert. Sie belegen das Layout, keine native Betriebssystem-Interaktion.

```sh
HUSH_UI_CAPTURE_DIR=/tmp/hush-ui cargo test --locked --lib export_native_ui_frames -- --ignored
```

Danach `scripts/render-ui.html` im Browser öffnen und eine exportierte JSON-Datei auswählen. Der Renderer baut keine eigene HTML-Oberfläche nach. Der ältere HTML-Prototyp in `preview/` ist eine separate historische Designvorschau und entspricht nicht mehr dem aktuellen Layout. Bildschirmleser und betriebssystemspezifische DPI-Einstellungen bleiben auf echten Zielsystemen zu prüfen.
