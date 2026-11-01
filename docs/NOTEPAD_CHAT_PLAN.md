# Masterplan: Notizblock und Chat

**Stand:** 30. Oktober 2026, Basis Version 1.0.7 und `main`. Dies ist ein Produktplan für die Diskussion, kein Nachweis vorhandener Funktionen. Mobile Apps sind bewusst ausgeklammert. Ziele und Freigabekriterien aus dem [Masterplan](MASTERPLAN.md) und die Regeln in [AGENTS.md](../AGENTS.md) gelten unverändert; die übrigen Wünsche stehen in der [Roadmap](OPTIONAL_FEATURE_ROADMAP.md).

## Ausgangslage

Die App nimmt Computerton und Mikrofon als getrennte Spuren auf, transkribiert lokal, führt eine Live-Zusammenfassung und erzeugt danach Auswertungen mit Vorlagen und Tabs. Jede Aussage verweist auf einen Transkriptabschnitt und ein wörtliches Zitat. Es fehlen ein Ort für eigene Gedanken während des Meetings und ein Weg, dem Material Fragen zu stellen, statt nur fertige Auswertungen zu lesen.

Jede Idee wird an vier Grundsätzen gemessen:

| Grundsatz | Bedeutung für Notizblock und Chat |
| --- | --- |
| Lokal zuerst | Audio verlässt den Computer nie. Cloud-Textmodelle nur mit ausdrücklicher Freigabe je Lauf. |
| Belegt statt erfunden | Jede KI-Aussage hat Quellenkennung und Originalzitat. Ohne Beleg gibt es keine Aussage, sondern „nicht gefunden“. Unbekanntes bleibt unbekannt. |
| Originale bleiben | Notizen, Chatverläufe und Bearbeitungen sind neue Fassungen oder Dateien; nichts wird überschrieben oder automatisch gelöscht. |
| Aufnahme hat Vorrang | Zusatzfunktionen dürfen Aufnahme und Erkennung weder bremsen noch beenden. |

## N · Notizblock

Kernidee: Der Mensch schreibt mit, was ihm wichtig ist; die App macht daraus belegte Notizen. Eigene Worte sind das stärkste Relevanzsignal.

| ID | Idee | Was | Passung und Anknüpfung |
| --- | --- | --- | --- |
| N-1 | Notizblock neben dem Live-Transkript | Einfacher Editor in der Aufnahmeansicht (Stichpunkte, Überschriften, Fett). Jede Zeile merkt sich die aktuelle Transkriptposition. Ein- und ausklappbar, Tastenkürzel zum Fokus. | Notizen als eigene Datei im Sitzungsordner mit Schreibpunkten wie die Journale. `MeetingView.tsx`, Sitzungsdateien in `meeting-core`. |
| N-2 | Notizen nach dem Meeting ausbauen | Eigene Stichpunkte bleiben wörtlich stehen und bilden die Gliederung; darunter ergänzt das Textmodell Details, jede Ergänzung mit Quelle und Zitat. Eigene Zeilen und Ergänzungen sind durch Einrückung, Beschriftung und Farbe getrennt, nicht nur durch Farbe. | Läuft durch die bestehende Belegprüfung in `meeting-notes`; Ergebnis als neue Fassung in `NotesView.tsx`. |
| N-3 | Markierungen per Tastenkürzel | „Wichtig“, „Aufgabe“, „Frage“, „Entscheidung“ an die aktuelle Stelle setzen, auch nachträglich im Transkript. Auswertungen gewichten markierte Stellen, die Historie filtert danach. | Entspricht den Quellen-Lesezeichen der Roadmap; Markierungen sind Zeitachsen-Ereignisse wie Pause und Stummschaltung. |
| N-4 | Agenda vorab, Abgleich danach | Ziele und Fragen vor dem Meeting notieren. Danach je Punkt: besprochen (mit Beleg), angerissen, nicht besprochen. Offene Punkte werden zur nächsten Agenda. | „Nicht besprochen“ heißt nur: kein Beleg im Transkript gefunden. |
| N-5 | Vorlagen formen den Notizblock | Vorlagen erhalten optional ein Notizblock-Gerüst (z. B. „Ziele / Einwände / Nächste Schritte“); beim Start Vorschlag passend zum Meetingnamen. | Baut auf der revisionierten Vorlagenbibliothek auf (`useTemplateLibrary.ts`, `TemplateDialog.tsx`); geänderte Vorlagen ändern keine gespeicherten Ergebnisse. |
| N-6 | Ergebnisse bearbeiten mit Fassungsspur | KI-Text, Namen oder Termine direkt korrigieren. Jede Bearbeitung ist eine eigene Fassung, das Original bleibt abrufbar; bearbeitete Stellen gelten als „vom Menschen geprüft“. | Originale bleiben; der Prüfstatus wird in Export und Chat sichtbar. |

## C · Chat

Kernidee: Fragen an das eigene Material stellen und nachprüfbare Antworten bekommen. Antwortregel: Zitat oder „nicht gefunden“, nie eine freie Behauptung. Jede Antwort zeigt Modell, lokal oder Cloud und wie viel Material gelesen wurde.

| ID | Idee | Was | Passung und Anknüpfung |
| --- | --- | --- | --- |
| C-1 | Chat mit einem Meeting | Eingabefeld in der Meeting-Ansicht („Was wurde zum Budget gesagt?“). Antwort mit Abschnitten und Zitaten, Klick springt ins Transkript; ohne Treffer eine klare Meldung. | Lokal über Ollama oder LM Studio, dieselbe Belegprüfung wie Auswertungen (`meeting-notes`, `useLocalTextModels.ts`). Verlauf als Datei im Sitzungsordner, löschbar. |
| C-2 | Fragen während der Aufnahme | Kurzbefehle wie „Was habe ich in den letzten fünf Minuten verpasst?“ aus Live-Zusammenfassung und letzten Abschnitten. | Nachrangig zur Erkennung: Bei hoher Last oder Rückstand (Seitenleiste) wartet die Frage oder wird mit Hinweis abgelehnt. `live_notes.rs`, `LiveNotesPanel.tsx`. |
| C-3 | Gespeicherte Aufträge per „/“ | `/follow-up-mail`, `/offene-punkte`, `/kurz-für-chef`, `/protokoll`, `/feedback-gesprächsführung`; eigene Aufträge speichern und als Datei weitergeben. | Leichte Variante der revisionierten Vorlagen; Ergebnisse bleiben belegte Entwürfe. |
| C-4 | Chat über mehrere Meetings | Auswahl nach Ordner, Zeitraum, Name oder VIP-Markierung, dann fragen („Was wurde mit Firma Müller seit März vereinbart?“). Die Volltextsuche findet Kandidaten, das Textmodell antwortet mit Zitaten aus mehreren Meetings. | Wie in der Roadmap: exakte Suche zuerst, semantische Suche später und optional. Antwort nennt die Abdeckung („14 von 37 Meetings durchsucht“). SQLite-Volltextsuche, `HistorySearch.tsx`. |
| C-5 | Antwort übernehmen | Antwort als neuen Ergebnis-Tab speichern, in den Notizblock einfügen oder mit Belegen kopieren. | Tabs und Exporte mit Quellenkennungen gibt es bereits. |
| C-6 | Vorgeschlagene Fragen | Drei Startfragen je Meetingtyp unter dem Eingabefeld. | Statisch aus der Vorlage, ohne zusätzlichen Modelllauf. |

## W · Rund um beides

| ID | Idee | Was | Passung |
| --- | --- | --- | --- |
| W-1 | Ordner für Kunden und Projekte | Meetings in Ordnern sammeln (auch mehrfach); Rahmen für Chat über mehrere Meetings und Sammelauswertungen. | Nur Metadaten; Sitzungsdateien bleiben, wo sie sind. |
| W-2 | Kalender lesen, nie selbst aufnehmen | Systemkalender oder ICS-Datei lesend nutzen: Meetingname, Eingeladene, Agenda vorbefüllen, Vorlage vorschlagen; beim Terminbeginn Hinweis „Aufnahme starten?“. | Der Start bleibt eine sichtbare, bewusste Handlung. Eingeladene sind keine erkannten Sprecher. |
| W-3 | Personenansicht | Je bestätigtem Namen: gemeinsame Meetings, offene Aufgaben und Zusagen mit Belegen. | Nur aus bestätigten Zuordnungen; ein erwähnter Name identifiziert niemanden. Hängt an der Teilnehmer-Arbeit der Roadmap. |
| W-4 | Weitergeben | Follow-up als Mail-Text zum Kopieren; Notizen und Ergebnisse als Markdown in einen gewählten Ordner. | Erst ausdrücklicher Ordnerexport mit Hinweis auf Sync-Dienste; die App versendet nichts selbst. |
| W-5 | Lokale Schnittstelle für KI-Assistenten | Lokaler MCP-Server auf Basis des Agentenprotokolls, damit andere Werkzeuge auf diesem Computer mit Erlaubnis suchen und Zitate abrufen. | Freigabe je Zugriff, nur lesend, nur Text mit Quellenkennungen; nie Audio oder Dateipfade. `docs/RUST_AGENT_PROTOCOL.md`, `AgentPanel.tsx`. |

## Bewertung und Wellen

Erste Einschätzung als Gesprächsgrundlage. Nutzen 1–4; Risiko meint die Nähe zu den Grundsätzen oben.

| ID | Idee | Nutzen | Aufwand | Risiko | Welle |
| --- | --- | --- | --- | --- | --- |
| N-3 | Markierungen per Tastenkürzel | 3 | S | gering | 1 |
| N-5 | Vorlagen formen den Notizblock | 2 | S | gering | 1 |
| C-3 | Gespeicherte Aufträge per „/“ | 3 | S | gering | 1 |
| C-5 | Antwort übernehmen | 2 | S | gering | 1 |
| N-1 | Notizblock neben dem Live-Transkript | 4 | M | gering | 1 |
| C-1 | Chat mit einem Meeting | 4 | M | mittel | 1 |
| N-2 | Notizen nach dem Meeting ausbauen | 4 | L | mittel | 2 |
| C-2 | Fragen während der Aufnahme | 3 | M | hoch (Last) | 2 |
| N-4 | Agenda vorab, Abgleich danach | 3 | M | gering | 2 |
| N-6 | Ergebnisse bearbeiten mit Fassungsspur | 3 | M | gering | 2 |
| W-1 | Ordner für Kunden und Projekte | 3 | M | gering | 2 |
| W-4 | Weitergeben | 2 | S | mittel (Sync) | 2 |
| C-4 | Chat über mehrere Meetings | 4 | L | mittel | 3 |
| W-2 | Kalender lesen | 3 | L | mittel (Datenschutz) | 3 |
| W-3 | Personenansicht | 3 | L | hoch (Identität) | 3 |
| W-5 | Lokale Schnittstelle für KI-Assistenten | 2 | M | mittel | 3 |
| C-6 | Vorgeschlagene Fragen | 1 | S | gering | 3 |

1. **Welle 1, schnelle Wirkung:** N-3 und N-1 zuerst, weil sie die Datengrundlage (eigene Notizen, Markierungen) für alles Weitere liefern; dann N-5, C-1, C-3 und C-5.
2. **Welle 2, Profil schärfen:** N-2, C-2, N-4, N-6, W-1 und W-4.
3. **Welle 3, Gedächtnis:** C-4, W-2, W-3, W-5 und C-6.

Vor der Umsetzung erhält jede Idee eine Annahmegrenze wie in der Roadmap: was als fertig gilt, welche Tests synthetisch sind und welche echte Geräte oder lokale Referenzmodelle brauchen.

## Bewusst nicht vorgesehen

- Notizen oder Transkripte standardmäßig in einer Cloud; Teamarbeit nur über Dateien und Ordner, die Nutzer selbst teilen.
- Öffentliche Teilen-Links, weil sie einen Server voraussetzen; stattdessen Export und Mail-Entwurf.
- Automatischer Aufnahmestart durch den Kalender; es gibt keine versteckte Aufnahme.
- Frei formulierte KI-Ergänzungen ohne Beleg.
- Mobile Apps in dieser Planungsrunde.

## Offene Fragen

1. Ist der Notizblock in der Aufnahmeansicht standardmäßig offen, oder bleibt das Live-Transkript die Hauptansicht?
2. Wie werden eigene Zeilen und Ergänzungen barrierefrei unterschieden: Einrückung, Beschriftung, Symbol?
3. Reicht ein lokales Modell mit 2–4 Mrd. Parametern auf Rechnern mit 8 GB für Chat-Antworten mit Belegen, oder braucht C-1 eine Mindestausstattung?
4. Darf der Chat während der Aufnahme Modellzeit beanspruchen, oder nur bei pausierter Live-Zusammenfassung?
5. Wie lange bleiben Chatverläufe gespeichert, und gelten für sie Archiv- und Aufbewahrungsregeln?
6. Welche fünf „/“-Aufträge werden auf Deutsch und Englisch ausgeliefert?
7. Woran wird Erfolg gemessen: Anteil der Meetings mit eigenen Notizen, Chatfragen je Meeting, übernommene Antworten?
