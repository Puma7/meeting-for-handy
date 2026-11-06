# PR 41: Validierung der Reviewnacharbeit

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.
Diese Notiz enthält bereinigte Nachweise mit synthetischen Eingaben, darunter
ausdrücklich echte lokale Modellinferenz. Der ganze TEST_REPORT bleibt zur
PR-Basis byteunverändert. Private Rohbelege und Nutzermeetings werden nicht veröffentlicht.

## Aktuelle Testfixture-Korrektur

Die echten Intel- und ARM-Mac-Läufe auf 2118eb1 scheiterten in der neuen positiven
Streamingfixture: jeweils 173 Notizenfälle bestanden, einer erhielt ein Zeitlimit.
Beide Läufe bleiben erhalten. Das ist kein nachgewiesener Fehler der produktiven
300-Sekunden-Grenze. Die alte Fixture protokollierte keine Empfängerzeitpunkte;
die genaue frühere Kernel-/Schedulingursache ist rückwirkend nicht bewiesen.

Winzige write_all-Aufrufe im 60-ms-Abstand belegen allein keinen entsprechenden
Byteempfang. Die Fixture aktiviert deshalb TCP_NODELAY auf ihren eigenen
angenommenen Sockets, wartet vor dem Messbeginn auf den bereiten Server und
bestätigt jedes der acht Fortschrittspakete über den tatsächlichen
AnalysisControl-Bytezähler. Bei einem neuen Fehler stehen synthetische
Empfangszähler/Zeitpunkte für die Diagnose bereit. Es gibt keine fremden Prozesse
oder echten Modelldienste in dieser Fixture.

Die produktiven **300 Sekunden**, internen **200 Millisekunden**, acht
**60-ms-Abstände**, die ursprünglichen Assertions einschließlich **mehr als
400 ms Gesamtdauer** und sämtliche bisherigen Testfristen bleiben erhalten.
Acht bestätigte Empfangsschritte sind zusätzlich verlangt. Es wird kein alter
Head erneut ausgeführt, um seinen Fehllauf zu ersetzen. Ein neuer erfolgreicher
Intel-/ARM-Lauf auf diesem Stand bleibt erforderlich.

**Nur cfg(test)-Code und diese Notiz ändern sich.** Der gesamte produktiv
kompilierte Präfix von workspace_stream.rs ist zu 2118eb1 tatsächlich
raw-bytegleich; andere produktive Quellen, Oberfläche, Prompts, Abhängigkeiten
und ganzer TEST_REPORT ebenfalls unverändert. Damit sind die unten genannten
Browser- und Modellreferenzen ausdrücklich erhaltene Nachweise von 2118eb1,
keine neu ausgeführten Läufe dieses Teststands.

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
fortlaufende Bytes mit 60-ms-Abständen und endet erst nach mehr als 400 ms:
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
| Frontend | **435 Tests in 21 Dateien** auf 2118eb1 bestanden; alle Frontendbytes unverändert, nicht erneut ausgeführt. |
| TypeScript und Produktionsbuild | Auf 2118eb1 bestanden, unveränderte Frontend-/Paketbytes; nicht erneut ausgeführt. |
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
Der neue Teststand braucht zwei eigene saubere CODE-Runden, eine vollständige
CodeRabbit-Runde und seine eigene grüne aktuelle CI. Die Mac-Fehlläufe werden
ausdrücklich nicht als erfolgreich ausgegeben.

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
