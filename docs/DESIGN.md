# Oberfläche

Hush zeigt GitHub-Benachrichtigungen in einem chronologischen Posteingang. Einstellungen verwenden gleich breite Zeilen mit rechts ausgerichteten Schaltern. Rückmeldungen erscheinen als Toasts für vier Sekunden; Fehlermeldungen bleiben zehn Sekunden sichtbar.

## Visuelles System

Graphit und dunkles Salbeigrün statt reinem Schwarz. Mint für Fokus und erhaltene Reviews, Violett für PR-Anfragen, Amber für Erwähnungen und Blau für neue Issues. Farbe wird stets durch Text und eigene Vektor-Icons ergänzt. Die helle Variante verwendet warmes Off-White und dieselbe semantische Zuordnung.

Kleine, zurückhaltende Metadaten; deutlich größere Überschrift; großzügige, aber nicht verschwenderische Innenabstände. Ein schmaler grüner Akzent trennt den gewählten Eintrag von unaufdringlichen Karten. Kein permanentes Blinken, keine Erfolgs-Konfetti, kein Kreisdiagramm. Status und Demo-Kennzeichnung sind explizit.

Eigene Vektor-Icons werden mit egui-Painter gezeichnet. Native Typografie verwendet die mit eframe gelieferten Standardfonts. Die separate HTML-Vorschau verwendet verfügbare Systemfonts; es sind keine zusätzlichen Schriftdateien beigefügt und kein externes Font-CDN eingebunden.

## Interaktion

Sidebar: Posteingang, vier semantische Filter, Archiv und Einstellungen. Suche ist mit Strg/Cmd+K erreichbar. Karten öffnen einen rechts angeordneten Detailbereich; bei geringerer nativer Fensterbreite ersetzt die Detailansicht den Hauptbereich. Escape führt zurück. Erledigen lässt sich über das Archiv rückgängig machen; es bedeutet nicht, den PR oder das Issue auf GitHub zu schließen.

Die Oberfläche hat normale Einstellungen für die häufigen Entscheidungen. Zugangsdaten und Team-Details stehen separat; sensible Felder sind Passwortfelder und werden nicht in GUI-Persistenz serialisiert. Die normale Voreinstellung für System-Banner enthält keine vertraulichen Repository-/Inhaltsdaten.

Der HTML-Prototyp demonstriert Interaktionen mit erfundenen Personen/Repositories, aber simuliert keine reale Kontoverbindung. Seine eigenen Hinweise nennen ihn eine Designvorschau. Die native egui-Oberfläche ist eigenständig implementiert; pixelgenaue Übereinstimmung, Screenreader-Verhalten und Darstellung bei verschiedenen DPI müssen auf echten Zielsystemen geprüft werden.
