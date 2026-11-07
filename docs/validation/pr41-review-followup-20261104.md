# PR 41: Validierung der Reviewnacharbeit

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.
Diese Notiz enthält bereinigte Nachweise mit synthetischen Eingaben, darunter
ausdrücklich echte lokale Modellinferenz. Der ganze TEST_REPORT bleibt zur
PR-Basis byteunverändert. Private Rohbelege und Nutzermeetings werden nicht veröffentlicht.

## Abschließende Testfixture-Korrektur

Die Intel-/ARM-Fehlläufe auf 2118eb1 und der ARM-Fehllauf auf 5599001 bleiben
erhalten. Die zusätzliche Empfängerdiagnose auf 5599001 belegt zwischen
631 und 837 ms keine neuen Bytes, obwohl nur 60 ms Sendepause geplant waren.
Die private 200-ms-Idle-Grenze hat deshalb korrekt ausgelöst. Eine geplante
Threadpause ist auf einem ausgelasteten Prüfrechner keine präzise Testuhr.

Der positive synthetische Test benutzt jetzt die kontrollierte Tokio-Testuhr:
acht bestätigte reale Loopback-Empfänge, jeweils danach 60 ms logischer
Zeitfortschritt, insgesamt exakt 480 ms bei unveränderter 200-ms-Idle-Grenze.
Er prüft weiterhin, dass laufender Byteempfang die gesamte Idle-Dauer
überschreiten darf. Eine eigene bereite Aufgabe verhindert unkontrolliertes
Vorspringen der Testuhr während echter Socket-I/O. Der reale 3-Sekunden-Wächter
bricht eine hängende Fixture ab; alle eigenen Threads werden verbunden.
**480 ms sind simulierte Zeit**, kein gemessener 480-ms-Dauerlauf.

Die drei negativen Fälle für fehlende Header, verstummten Erfolgs- und
Fehlerkörper behalten ihre echte 200-ms-Zeitmessung. Produktiv bleiben
300 Sekunden Idle-Zeit und alle bisherigen Schutz-/Größenregeln unverändert.
Nur der cfg(test)-Abschnitt, diese Notiz und das ausschließlich als
Dev-Abhängigkeit aktivierte test-util-Feature der bereits gesperrten
Tokio-Version 1.54.1 ändern sich. Kein neues Paket, keine Versionsänderung;
Cargo.lock, Lizenzen und ganzer TEST_REPORT bleiben byteunverändert.
Alle produktiven Transportbytes und UI-Quellen entsprechen 2118eb1.

Auf Wunsch des Maintainers endet damit die offene Suche nach weiteren
Verbesserungen. Bekannte Befunde werden abgeschlossen und ein separater
Windows-Teststand für die anschließende Erprobung im Alltag vorbereitet.
Eine Geräte-/Zweistundenfreigabe oder Fehlerfreiheit wird daraus nicht abgeleitet.

## Erhaltene produktive Korrekturen

Die [vollständige CodeRabbit-Runde auf fb03a76](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5804343551)
fand zwei weitere gültige Fehler. Die vorherigen sieben Korrekturen für
Bereinigung, Aktualisierung, Audiovorschau, Audiofehlermeldung, beschädigte
klassische Altstände und Modellreservierungen bleiben erhalten.

**Verstummte Modellverbindung:** Der lokale Workspace-Stream hat jetzt eine
Grenze von **300 Sekunden ohne Antwortbytes** pro Lesevorgang. Dafür wird die
vorhandene gesperrte Reqwest-Version **0.12.24** verwendet. Jede erfolgreiche
Leseoperation erneuert diese Grenze. Es gibt weiterhin **keine Gesamtdauergrenze**
für eine aktiv antwortende Workspace-Anfrage. Ein Zwei-/Drei-Minuten-Intervall
bricht keine laufende Inferenz ab. Der Verbindungsaufbau bleibt auf zehn Sekunden
begrenzt; andere Anfragefristen, Antwortgrößen, Decoder und Promptgrenzen bleiben
unverändert. Die lokale Modellladung behält ihre eigene bisherige Grenze.

Beim Verstummen endet nur die betroffene HTTP-Anfrage. Die Routenfreigabe fällt
an die nächste wartende eigene Anfrage zurück; eine klassische Auswertung auf
derselben Route kann fortfahren. Nicht abgeschlossene Modellantworten werden
nicht übernommen. Es erfolgt weder ein automatischer Anbieterwechsel noch eine
fremde Modellentladung. Ein serverseitiger Rechenabbruch durch das Schließen der
Verbindung wird nicht garantiert.

Drei synthetische echte Loopback-Verbindungen prüfen fehlende HTTP-Header,
verstummte Erfolgsdaten und einen verstummten HTTP-400-Fehlerkörper. Jeweils
folgt eine echte klassische Anfrage auf derselben Route. Ein privater interner
Testaufruf verwendet **200 ms** Idle-Zeit; es gibt keinen Laufzeit- oder
Umgebungsschalter dafür. Mit dem veröffentlichten Funktionskörper ohne einzig
die neue Idle-Einstellung blieb der erste Fall bis zum ausdrücklichen Abbruch
hängen. Danach bestehen alle drei Fälle. Ein weiterer Stream sendet achtmal
fortlaufende Bytes mit 60-ms-Abständen der kontrollierten Testuhr und endet
erst nach 480 ms simulierter Zeit:
Er besteht trotz Überschreitung des gesamten 200-ms-Idle-Budgets. Dies ist eine
beschleunigte synthetische Prüfung, kein tatsächlich fünf Minuten langer Ausfall.
Die eigene Anfrage und sämtliche Fixture-Threads werden bereinigt.

**Notizenhöhe bei neuen Hinweisen:** Ein MutationObserver erkennt neue oder
entfernte direkte Geschwister des Notizbereichs. Deren Höhen und ResizeObserver-
Ziele werden neu bestimmt. Der bisherige Rechenweg mit 200 Pixeln Reserve für
den Meetinginhalt, Mindesthöhen und gespeicherter Wunschhöhe bleibt erhalten.
Beide Observer werden beim Schließen getrennt. Neue Hinweise verändern weder
die gespeicherte Höhenpräferenz noch private Notiztexte.

Die Regression verwendet die echte LiveWorkspace-Komponente: Hinweis hinzufügen,
durch einen anderen Typ ersetzen, dessen Höhe ändern, entfernen und Komponente
schließen. Vorher blieb die Höhe bei 500 statt 350 Pixeln. Danach werden auch
die neuen Observer-Ziele und ihre Bereinigung bestätigt. Private Notizen werden
weiterhin nicht als freigegebener Kontext übertragen.

## Frische native Prüfung und erhaltene Nachweise

| Prüfung | Ergebnis dieses Stands |
| --- | --- |
| Rust Workspace/All-Targets unter Windows | **576 bestanden**, darunter **172 Notizen- und 40 Providerfälle**; drei bestehende besondere Einstiege regulär ignoriert. |
| Frontend | **435 Tests in 21 Dateien** im abschließenden lokalen Lauf erneut bestanden. |
| TypeScript und Produktionsbuild | Im abschließenden lokalen Lauf erneut bestanden, gesperrte Pakete. |
| Rust-/Frontendformat und Clippy | Bestanden, Warnungen als Fehler. |
| Projektprüfung | verify.ps1: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen. |
| Browser | Vier tatsächliche Vorherfehler und vier bestandene Konfigurationen auf 2118eb1; unveränderte UI-Quellen, nicht erneut ausgeführt. |
| Erhaltene echte lokale Referenz von 2118eb1, Deutsch | **124.1641588 s**, **0** reguläre Validierungswiederholung(en), alle fünf Kriterien bestanden. |
| Erhaltene echte lokale Referenz von 2118eb1, Englisch | **110.2912457 s**, **0** reguläre Validierungswiederholung(en), alle fünf Kriterien bestanden. |

Browser: vorhandenes Playwright **1.57.0**, Edge **156.0.4314.58**,
Deutsch hell/Englisch dunkel, **390/1280 Pixel**, echte WorkspaceBridge-/Notes-
Komponenten mit synthetischen Hinweisen ohne native IPC, Modelle oder Geräte.
Eine neue 220-Pixel-Meldung ließ vor der Korrektur nur **18/24 Pixel** für den
Meetinginhalt übrig. Danach blieben **200/200.5 Pixel** in den geprüften Fenstern.
Auch Ändern, Ersetzen und Entfernen der Meldung sowie die Wiederherstellung der
gespeicherten Wunschhöhe bestanden. Tatsächliche Tastaturbedienung des Größen-
reglers. Keine relevanten Konsolen-/Seitenfehler, fremden Anfragen oder
horizontalen Seitenüberläufe. Vorher-/Nachherbilder tatsächlich angesehen;
eigene Browser und Server beendet. Das ist keine installierte native Geräteprüfung.

Referenzumgebung: vorhandenes **google/gemma-4-e4b Q4_K_M**, **6.326.936.720
Modellbytes**, LM Studio **0.4.26+4**, Windows 11 Pro **26200**, Intel **i7-13700**,
rund **64 GiB** physischer RAM. Verstrichene Sekunden bis zur validierten
dauerhaften Speicherung und Wiederöffnung; keine allgemeine Geschwindigkeits-
freigabe. Keine eigenen Builds, Tests oder Browser während der Inferenz;
lesende GitHub-Abfragen waren erlaubt. Fremde Last war nicht isoliert. Working
Set, Private Bytes und Grafikspeicher wurden nicht neu gemessen; Dateigröße ist
keine RAM-Messung. Die eigene ausgewählte Modellinstanz war danach freigegeben.

Unveränderter deutscher/englischer Korpus einschließlich I’ll und Budgetfragen.
Alle fünf Kriterien bestanden: positive Aufgabe, unverbindliche Aussage nicht
als Aufgabe, erhaltene Impulse, menschlicher Impulsstatus nach Protokolländerung
und bytegleiche Wiederöffnung. Höchstens eine vollständig validierte Wiederholung
pro Anfrage bleibt zulässig. Keine Cloud, Downloads oder Nutzermeetings. Der
englische Referenzpatch 93335102 bleibt Bestandteil des PR.

## Reviewreichweite und Zuordnung

Alle 16 ursprünglichen Sourcery-, 87 bisherigen CodeRabbit- und 73 bisherigen
Codex-Threads wurden individuell menschlich beantwortet und geschlossen. Die
beiden letzten CodeRabbit-Abhilfen wurden am veröffentlichten 2118eb1 bestätigt.
Dessen erste frische CODE-Runde war sauber; eine zweite wurde wegen des
tatsächlichen Mac-CI-Fehlers nicht mehr auf diesem alten Stand angefordert.
Die anschließend gestarteten Prüfungen werden nur ihrem tatsächlichen Head
zugerechnet. Auf Wunsch des Maintainers werden keine weiteren offenen
Suchrunden angehängt. Fehlgeschlagene Mac-Läufe werden ausdrücklich nicht
als erfolgreich ausgegeben; aktuelle CI und Reviewstatus sind im PR sichtbar.

Die letzte vollständige CodeRabbit-Runde auf fb03a76 war wegen der beiden
in 2118eb1 korrigierten Hinweise nicht sauber, Risiko Low. Alle 100 ausgewählten
Dateien entsprachen dem vollständig paginierten tatsächlichen 102-Dateien-Diff
mit den unveränderten zwei Lockdateifiltern; keine unverfügbaren Änderungen.
Dokumentation 82.71 % bei unveränderter 80-%-Grenze. Sämtliche Risiko-,
Architektur-, Zusatz-, Nitpick-, Outside-diff- und operativen Texte wurden gelesen.
Alte/null Security-Flags sind kein aktueller formaler Securitynachweis. Keine
neue Filterausnahme, Kauf oder abgeschwächte Prüfregel.

## Grenzen

Aufnahmen, Journale, Originaltranskripte und menschliche Bestätigungen bleiben
versioniert erhalten. Private Vorbereitung bleibt privat. Automatische Auswertung
bleibt lokal; Cloud benötigt ausdrückliche Textfreigabe je Auswertung und erhält
weder Audio noch lokale Pfade oder Sitzungsobjekte. Keine Weiterleitungen,
System-Proxys, automatischen Anbieterwechsel oder fremden Modellentladungen.
Prompts, Verträge, Belegbudget, sonstige Fristen, Ollama-TTL 300 s/Erneuerung
alle 60 s, Paketversionen, Lizenztexte, Schutz-/Reviewerregeln, Dokumentationsgrenze,
Version und ganzer TEST_REPORT bleiben unverändert.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und Zweistundenprüfungen sind
auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-, kombinierte
Zweistundenlast-, unabhängige installierte Prozess-, Signatur-, Notarisierungs-
oder forensische Freigabe. Cloudprüfungen ohne Zugangsdaten bleiben Simulationen.
Unbelegte Zuständige/Termine bleiben unbekannt; Abschnittszeiten sind keine
Wortzeiten. Technische Belegprüfung beweist keine inhaltliche Deutung. Ergebnisse
bleiben überprüfbare KI-Entwürfe.

[Unveränderliche vorige Validierung und historische Nachweise](https://github.com/Puma7/meeting-for-handy/blob/2118eb1cf8e98b34a7b53e144988115bc6aa3cc2/docs/validation/pr41-review-followup-20261104.md).
Frühere Fehlläufe und Testvorbereitungen bleiben erhalten. Diese Nacharbeit
erstellt keinen installierten Programmstand oder Release.
