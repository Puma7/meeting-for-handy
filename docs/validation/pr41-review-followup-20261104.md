# PR 41: Validierung der Reviewnacharbeit

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.
Diese Notiz enthält ausschließlich synthetische Prüfsituationen. Der vollständige
TEST_REPORT bleibt gegenüber der PR-Basis byteunverändert. Private Rohbelege und
Nutzermeetings werden nicht veröffentlicht.

## Aktuelle Korrektur

Die [vollständige CodeRabbit-Runde auf 0dad8f4](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5800667095)
fand einen gültigen Kompatibilitätsfehler: Das Kartenmenü verlangte die native
Popover-API und versteckte seinen Inhalt nur mit einem neuen CSS-Pseudoselektor.
Ohne diese Browserfunktion fehlte ein bedienbarer Weg zum Anordnen.
Die Funktion wird jetzt vor Verwendung beider Methoden geprüft. Bei fehlender
oder nur teilweise vorhandener API bleibt das Menü ausdrücklich geschlossen.
Beim Öffnen wird es innerhalb derselben gekapselten, thematisierten Oberfläche
oberhalb der abgeschnittenen Kartenspalten eingeblendet. Tastaturfokus, Escape,
erneuter Klick und Klick außerhalb schließen es verständlich. Der tatsächliche
Ereignispfad berücksichtigt die Shadow-DOM-Kapselung der Produktivoberfläche.
Die vorhandene native Top-Layer-Bedienung und ihre Maus-/Touch-Synchronisierung
bleiben auf unterstützten Browsern erhalten. Keine neue Plattform-Mindestversion,
Abhängigkeit oder Ausnahme für Reviewer, Datenschutz oder Prüfungen.

Drei neue Regressionen stellen fehlende sowie jeweils nur eine vorhandene
Popover-Methode dar. Vorher scheiterten alle drei am ungeschützten Anfangszustand;
danach bestehen die stärkeren Fälle innerhalb einer tatsächlichen Shadow Root.
Sie prüfen Öffnen, Fokus, Escape, erneute Berührung, Außerklick, Spaltenwechsel,
Entfernung beim Unmount und keine Auswertungsstarts. Die bestehende native
Pointer-/Keyboard-Regression besteht ebenfalls. Der englische Layouttest öffnet
das nun korrekt versteckte Menü, bevor er seine übersetzten Optionen prüft.

## Tatsächlich geprüft

| Prüfung                              | Ergebnis und Zuordnung                                                                                                                                                                                                                                                  |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frontend                             | Frisch **428 Tests in 21 Dateien** bestanden.                                                                                                                                                                                                                           |
| TypeScript, Produktionsbuild, Format | Frisch bestanden, vorhandene gesperrte Pakete.                                                                                                                                                                                                                          |
| Gezielte Regressionen                | Drei tatsächliche Vorherfehler; danach drei Shadow-DOM-Fälle und die bestehende native Pointer-Regression bestanden.                                                                                                                                                    |
| Echter Browser                       | Frisch acht Full-App-Konfigurationen bestanden. Vorher vier gültige native Gegenfälle und vier reproduzierte Fehler bei simulierter fehlender Browserfunktion.                                                                                                          |
| Projektprüfung                       | Frisch verify.ps1: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen.                                                                                                                                                                              |
| Native Prüfungen                     | Quellen und Abhängigkeiten bytegleich zu 2694e92. Dessen **569 lokale Windows-Rusttests**, **169 Notizen-/37 Providerfälle**, Format und Clippy mit Warnungen als Fehler wurden hierfür nicht erneut ausgeführt; drei bestehende besondere Einstiege regulär ignoriert. |
| Echte Modellreferenzen               | Modellanbindung und Prompts bytegleich zu 2694e92. Dessen DE/EN-Referenzen bestanden mit **147.1481294 s / 0** und **114.8688505 s / 0** regulären Validierungswiederholungen; nicht erneut ausgeführt.                                                                 |

Die gesonderten lokalen Modellreferenzen verwendeten nur vorhandenes
google/gemma-4-e4b Q4_K_M mit **6.326.936.720 Modellbytes**, LM Studio **0.4.26+4**,
Windows 11 Pro **26200**, Intel **i7-13700** und rund **64 GiB** sichtbaren physischen
RAM. Verstrichene Sekunden vom Start bis zur validierten dauerhaften Speicherung
und Wiederöffnung. Während der Inferenz liefen keine eigenen Builds, Tests oder
Browserprüfungen; lesende GitHub-Abfragen konnten parallel stattfinden. Fremde Last
war nicht isoliert. Working Set, Private Bytes und Grafikspeicher wurden nicht neu
gemessen. Modell-Dateigröße ist keine RAM-Messung. Die eigene ausgewählte
Modellinstanz war nachher tatsächlich freigegeben.

Der synthetische Gesprächskorpus einschließlich I’ll und der Budgetfragen bleibt
unverändert. Alle fünf Kriterien bestehen: positive Aufgabe, unverbindliche
Aussage nicht als Aufgabe, erhaltene Impulse, menschlicher Impulsstatus nach
Protokolländerung und bytegleiche Wiederöffnung. Je Anfrage bleibt höchstens eine
vollständig validierte Wiederholung zulässig. Keine Downloads, Cloud oder
Nutzermeetings in diesen lokalen Referenzen.

## Browserprüfung der Bedienung

Browser-Plugin nicht verfügbar; vorhandenes **Playwright 1.57.0**, **Edge
156.0.4314.58**, eigene neue Browserkontexte. Tatsächliche App und
Workspace-Brücke mit separatem synthetischem API-Adapter. Der Vorherlauf verwendet
den unveränderlichen 0dad8f4-Menücode samt Styles; der Nachherlauf die aktuellen
Arbeitsbytes. Jeweils Deutsch/Englisch, hell/dunkel und 390/1280 Pixel Breite,
mit und ohne native Browserfunktion. Fehlende Methoden, unbekanntes HTML-Attribut
und nicht unterstützter CSS-Selektor sind auf Edge ausdrücklich **simuliert**;
dies ist kein tatsächlich ausgeführter alter Linux-/WebKitGTK-Lauf.

| Kontrolle                       | Ergebnis                                                                                                                         |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Identität und Inhalt            | Eigene lokale Prüf-URL und erwarteter Titel, vollständige App mit Meeting-Tabs sichtbar.                                         |
| Leere Seite / Framework-Overlay | Keine.                                                                                                                           |
| Konsole und Netzwerk            | Nachher keine relevanten Konsolen-/Seitenfehler oder fremden Anfragen.                                                           |
| Interaktionen                   | Echte Maus- und Touch-Klicks, Enter/Escape samt Fokus, Außerklick und tatsächlich geänderter Spaltenplatz in beiden Bedienwegen. |
| Geometrie                       | Offenes Menü erreichbar innerhalb des Fensters; kein horizontaler Seitenüberlauf in den geprüften Konfigurationen.               |
| Bilder                          | Breite helle und schmale dunkle Ersatzmenü-Ansicht tatsächlich angesehen.                                                        |
| Ressourcen                      | Eigene Kontexte, Browser und lokaler Prüfserver beendet. Keine fremden Prozesse beendet.                                         |
| Prüfdaten                       | Keine native IPC, Geräte, Modelle, Audiodateien oder Nutzermeetings.                                                             |

Frühere private Fixture- und Zwischenfehler bleiben erhalten. Ein anfänglicher
Test nutzte einen hier nicht vorhandenen Matcher; dieser Lauf ist kein
Produktfehlernachweis. Der Browserlauf deckte zusätzlich eine Ereignisweitergabe
im neuen Ersatzweg auf; diese wurde vor Veröffentlichung korrigiert und erneut
innerhalb der tatsächlichen gekapselten App geprüft.

## Reichweite und historische Nachweise

Alle 16 ursprünglichen Sourcery-Anmerkungen und alle älteren CodeRabbit-Threads
wurden einzeln geprüft, beantwortet und geschlossen. Der aktuelle Menübefund wird
nach Veröffentlichung einzeln beantwortet. Die zwei sauberen CODE-Runden und alle
22 erfolgreichen oder regulär übersprungenen CI-Checks auf 0dad8f4 bleiben dessen
historische Belege. Seine volle CodeRabbit-Runde war wegen des Menüfehlers nicht
sauber; Risiko Low. Alle 100 ausgewählten Dateien entsprachen den tatsächlichen
102 PR-Dateien abzüglich der unveränderten zwei Lockdateifilter; keine
unverfügbaren Änderungen. Dokumentation 82.58 Prozent bei unveränderter
80-Prozent-Grenze. LanguageTool blieb unvollständig; Ursache unbekannt.
Alle Risiko-, Architektur-, Zusatz-, Nitpick-, Outside-diff- und operativen Texte
wurden gelesen. Die neue Quelle benötigt ihre eigenen sauberen Reviews und ihre
eigene grüne CI. Alte oder leere Security-Flags sind kein aktueller formaler
Securitynachweis. Keine neue Filterausnahme oder abgesenkte Prüfschwelle.

Die bisherigen Korrekturen bleiben erhalten: verbindliche und unverbindliche
Aussagen werden getrennt, Quellen wörtlich geprüft, menschliche Impulsstände
erhalten und versionierte Zustände dauerhaft gespeichert und wieder geöffnet.
Der native Reservierungstest hält echte App-Leases und einen lebenden
synthetischen Active-Slot. Die Updaterkorrektur nutzt die bereits gesperrte SemVer
1.0.27; Herkunft, Größenlimit, Prüfsummen und Installationsschutz bleiben erhalten.
Pause und Mikrofonsteuerung verlangen eine verbundene, manuell steuerbare aktuelle
Aufnahme; eine fremde Agentensitzung sperrt sie nicht. Aufnahme beenden bleibt
als bestehende Aktion erhalten. Die zwei früher zusätzlich behaupteten
Test-Hygiene-Probleme wurden von CodeRabbit ausdrücklich zurückgenommen.

## Grenzen

Aufnahmen, Journale, Transkriptstände und menschliche Bestätigungen bleiben
versioniert erhalten. Private Vorbereitung bleibt privat. Automatische Auswertung
bleibt lokal; Cloud braucht ausdrückliche Textfreigabe je Auswertung und erhält
weder Audio noch lokale Pfade oder Sitzungsobjekte. Keine Weiterleitungen,
System-Proxys, automatischen Anbieterwechsel oder fremden Modellentladungen.
Prompts, Verträge, Belegbudget, Fristen, Ollama-TTL 300 s/Erneuerung alle 60 s,
Paketversionen, Lizenztexte, Schutz-/Reviewerregeln, Dokumentationsgrenze,
Version und vollständiger TEST_REPORT bleiben unverändert.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und Zweistundenprüfungen sind
auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-, kombinierte
Zweistundenlast-, unabhängige installierte Prozess-, Signatur-, Notarisierungs-
oder forensische Freigabe. Virtueller Ton mit Startunterbrechungen bestätigt keine
lückenlose physische Aufnahme. Cloudprüfungen ohne echte Zugangsdaten bleiben
Simulationen. Zuständige und Termine bleiben unbekannt, wenn unbelegt;
Abschnittszeiten sind keine Wortzeiten. Technische Belegprüfung beweist keine
inhaltliche Deutung. Ergebnisse bleiben überprüfbare KI-Entwürfe.

[Unveränderliche vorige Validierung und weitere historische Nachweise](https://github.com/Puma7/meeting-for-handy/blob/0dad8f45ec41ce3d2559b34ff429ad418e915542/docs/validation/pr41-review-followup-20261104.md).
Frühere Fehlläufe und offene Ursachen bleiben erhalten. Diese Nacharbeit erstellt
keinen installierten Programmstand oder Release.
