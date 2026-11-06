# PR 41: Validierung der Reviewnacharbeit

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.
Diese Notiz beschreibt ausschließlich synthetische Prüfsituationen. Der ganze
TEST_REPORT bleibt gegenüber der PR-Basis byteunverändert. Private Rohbelege
und Nutzermeetings werden nicht veröffentlicht.

## Aktuelle Testkorrektur

Die [vollständige CodeRabbit-Runde auf e4b7227](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5801739280)
fand ein Zuverlässigkeitsrisiko in zwei zeitabhängigen UI-Tests. Ein Fall wartet
auf eine zurückgestellte Protokolländerung; der andere prüft auf Deutsch und
Englisch eine abgewiesene menschliche Statusänderung mit ausdrücklich erneutem
Versuch. Reale Poll- und Schlafintervalle belegten einen wesentlichen Teil des
standardmäßigen Testbudgets. Ein tatsächlicher Produktfehler oder Fehllauf der
aktuellen CI wurde damit nicht festgestellt.

Der Testhelfer steuert jetzt ausschließlich den vorhandenen Zwei-Sekunden-Poll.
Andere Intervalle, asynchrone Assertions und die Testfrist bleiben reale Timer.
Die bisherige Fünf-Sekunden-Testfrist wird nicht erhöht. Die vorhandenen sechs
beziehungsweise sieben Assertions bleiben strukturell identisch; je eine weitere
Assertion bestätigt, dass tatsächlich ein Poll stattgefunden hat. Alle anderen
bisherigen Anweisungen dieser Testdatei sind nach normalisiertem TypeScript-AST
identisch. Die eigenen Intervalle werden auch bei Testabbruch bereinigt und die
beiden Timer-Mocks wiederhergestellt. Produktive Quellen und Fristen ändern sich
nicht. Weder abgewiesene Protokolländerungen noch menschliche Statusänderungen
werden im Test stillschweigend als übernommen anerkannt.

Ein separater synthetischer Vergleich verzögert nur zwei Statusantworten um
2400 und 1700 Millisekunden. Mit dem unveränderten veröffentlichten Test wurde
tatsächlich die Fünf-Sekunden-Frist überschritten. Die korrigierte Fassung besteht
bei denselben Verzögerungen innerhalb derselben Frist. Dieser kontrollierte lokale
Vergleich ist kein tatsächlich beobachteter langsamer CI-Rechner oder Produktfehler.
Der Ausgangsstand und sämtliche ursprünglichen Assertions bleiben erhalten.

## Tatsächlich geprüft

| Prüfung                              | Ergebnis und Zuordnung                                                                                                                                                                                                                                        |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frontend                             | Frisch **428 Tests in 21 Dateien** bestanden.                                                                                                                                                                                                                 |
| TypeScript, Produktionsbuild, Format | Frisch bestanden; gesperrte vorhandene Pakete.                                                                                                                                                                                                                |
| Gezielte Fälle                       | Drei Fälle bestanden; synthetisch verzögerter Vorherfall lief in das unveränderte 5000-ms-Limit, Nachherfall bestand.                                                                                                                                         |
| Projektprüfung                       | Frisch verify.ps1: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen.                                                                                                                                                                    |
| Produktive Oberfläche                | Bytegleich zu e4b7227; dessen acht tatsächliche Full-App-Browserkonfigurationen nicht erneut ausgeführt.                                                                                                                                                      |
| Native Prüfungen                     | Quellen/Abhängigkeiten bytegleich zu 2694e92. Dessen **569 lokale Windows-Rusttests**, darunter **169 Notizen-/37 Providerfälle**, Format und Clippy mit Warnungen als Fehler nicht erneut ausgeführt; drei bestehende besondere Einstiege regulär ignoriert. |
| Echte Modellreferenzen               | Modellanbindung/Prompts/Korpus bytegleich zu 2694e92. Dessen DE **147.1481294 s / 0** und EN **114.8688505 s / 0** reguläre Validierungswiederholungen sowie alle fünf Kriterien bestanden; nicht erneut ausgeführt.                                          |

Testumgebung: Windows, vorhandenes Node **26.7.0**, Vitest **4.1.12**, jsdom
**26.1.0**. Keine zusätzlichen Pakete oder Downloads. Die kontrollierte Zeit betrifft
nur den UI-Testpoll, keine native Aufnahme, Inferenz oder Providerfrist.

Die gesonderten früheren Modellreferenzen verwendeten ausschließlich vorhandenes
google/gemma-4-e4b Q4_K_M mit **6.326.936.720 Modellbytes**, LM Studio **0.4.26+4**,
Windows 11 Pro **26200**, Intel **i7-13700** und rund **64 GiB** sichtbaren physischen
RAM. Verstrichene Sekunden bis zur validierten dauerhaften Speicherung und
Wiederöffnung. Keine eigenen Builds, Tests oder Browser während der Inferenz;
lesende GitHub-Abfragen konnten parallel stattfinden. Fremde Last war nicht
isoliert. Working Set, Private Bytes und Grafikspeicher wurden nicht neu gemessen.
Dateigröße ist keine RAM-Messung. Eigene ausgewählte Modellinstanz danach frei.

Unveränderter synthetischer Korpus einschließlich I’ll und Budgetfragen. Alle fünf
Kriterien bestanden: positive Aufgabe, unverbindliche Aussage nicht als Aufgabe,
erhaltene Impulse, menschlicher Impulsstatus nach Protokolländerung und bytegleiche
Wiederöffnung. Höchstens eine vollständig validierte Wiederholung pro Anfrage
bleibt zulässig. Keine Cloud, Downloads oder Nutzermeetings in den Referenzen.

## Erhaltene Oberfläche und Nachweise

Die vorige Kartenmenükorrektur bleibt bytegleich: vollständige Erkennung beider
Popover-Methoden, ausdrücklich verborgener Ersatzweg ohne diese API, Portal
innerhalb derselben thematisierten Shadow Root, Fokus/Escape/Außerklick über den
tatsächlichen Ereignispfad und bestehende native Top-Layer-Bedienung. Vorher drei
echte Unit-Fehler; danach drei stärkere Shadow-DOM-Fälle und vorhandene native
Pointer-/Keyboard-Regression bestanden. Acht tatsächliche Full-App-Konfigurationen
bestanden mit Playwright 1.57.0 und vorhandenem Edge, Deutsch/Englisch,
390/1280 Pixel, hell/dunkel, native/fehlende Browserfunktion. Echte Maus-/Touch-
Klicks, Enter/Escape/Fokus, Außerklick und tatsächlicher Spaltenwechsel. Keine
relevanten Konsolen-/Seitenfehler, fremden Anfragen, Framework-Overlays oder
horizontalen Seitenüberläufe. Menü innerhalb des Fensters erreichbar; Bilder
tatsächlich angesehen. Eigene Browser/Server beendet. Fehlende API, Attribut und
Selektor sind auf Edge simuliert, kein tatsächlicher alter Linux-/WebKitGTK-Lauf.
Keine native IPC, Geräte, Modelle oder Nutzermeetings im Browserfixture.

Alle bisherigen Korrekturen für Quellen, verbindliche/unverbindliche Aussagen,
menschliche Statusautorität, Impulserhaltung, Notizversionen, Modellreservierungen,
Capturekontrollen, Layouts und den SemVer-Updater bleiben erhalten. Der native
Reservierungstest hält Recording-/LiveNotes-/OS-Leases und einen lebenden
synthetischen Active-Slot. Die zusätzliche absichtlich falsche Sperre wurde im
lokalen Gegenversuch erkannt, bytegenau zurückgenommen und niemals veröffentlicht.
Produktive Admissionbytes blieben unverändert.

## Reviewreichweite und aktueller Status

Alle 16 ursprünglichen Sourcery-Anmerkungen und alle älteren CodeRabbit-Threads
wurden einzeln geprüft, beantwortet und geschlossen. Der aktuelle Testhinweis
wird nach Veröffentlichung der geprüften Korrektur beantwortet. Auf e4b7227
bestanden zwei frische vollständige CODE-Runden und alle 22 CI-Checks
(16 erfolgreich, sechs regulär übersprungen). Die volle CodeRabbit-Runde war
wegen des Testhinweises nicht sauber; Risiko Low. Alle 100 zulässigen Dateien
entsprachen dem tatsächlichen vollständig paginierten 102-Dateien-Diff abzüglich
der unveränderten zwei Lockdateifilter; keine unverfügbaren Änderungen.
Dokumentation **82.71 Prozent** bei unveränderter **80-Prozent-Grenze**,
133 Funktionen in 50 Dateien, 49 ausgelassen (19 nicht unterstützt, 30 über dem
Dateilimit). LanguageTool blieb unvollständig; Ursache unbekannt.
Alle Risiko-, Architektur-, Zusatz-, Nitpick-, Outside-diff- und operativen Texte
wurden gelesen. Der neue Stand benötigt seine eigenen sauberen Reviews und seine
eigene grüne CI. Alte/null Security-Flags sind kein aktueller formaler
Securitynachweis. Keine neue Filterausnahme oder abgeschwächte Prüfregel.

## Grenzen

Aufnahmen, Journale, Originaltranskripte und menschliche Bestätigungen bleiben
versioniert erhalten. Private Vorbereitung bleibt privat. Automatische Auswertung
bleibt lokal; Cloud benötigt ausdrückliche Textfreigabe je Auswertung und erhält
weder Audio noch lokale Pfade oder Sitzungsobjekte. Keine Weiterleitungen,
System-Proxys, automatischen Anbieterwechsel oder fremden Modellentladungen.
Prompts, Verträge, Belegbudget, Fristen, Ollama-TTL 300 s/Erneuerung alle 60 s,
Pakete, Lizenzen, Schutz-/Reviewerregeln, Dokumentationsgrenze, Version und ganzer
TEST_REPORT bleiben unverändert.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und Zweistundenprüfungen sind
auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-, kombinierte
Zweistundenlast-, unabhängige installierte Prozess-, Signatur-, Notarisierungs-
oder forensische Freigabe. Virtueller Ton mit Startunterbrechungen bestätigt keine
lückenlose physische Aufnahme. Cloudprüfungen ohne Zugangsdaten bleiben
Simulationen. Unbelegte Zuständige/Termine bleiben unbekannt; Abschnittszeiten
sind keine Wortzeiten. Technische Belegprüfung beweist keine inhaltliche Deutung.
Ergebnisse bleiben überprüfbare KI-Entwürfe.

[Unveränderliche vorige Validierung und weitere historische Nachweise](https://github.com/Puma7/meeting-for-handy/blob/e4b7227a900a0242fd4b4aaef63bcaaa2f6fe177/docs/validation/pr41-review-followup-20261104.md).
Frühere Fehlläufe und offene Ursachen bleiben erhalten. Diese Nacharbeit erstellt
keinen installierten Programmstand oder Release.
