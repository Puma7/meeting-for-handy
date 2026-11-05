# PR 41: Validierungsstand

Stand: 5. November 2026. Version: **1.1.0-beta.1**, unverändert.
Diese Fassung ergänzt die gemeinsame Aufgabenprüfung für negierte Prüfgegenstände,
negative Aufgabenträger und bereits erledigte Erstellung.
Native Validierung und die zugehörigen Rustregressionen sind aktualisiert. Frontend, Prompts,
Transport, Audio/ASR, Modellreservierungen, Abhängigkeiten, Konfiguration und kompletter
TEST_REPORT bleiben gegenüber dem jeweils benannten geprüften Elternstand byteunverändert.
Der vollständige TEST_REPORT wurde nicht für diese Nacharbeit neu veröffentlicht.

Die [ausführliche bisherige Fassung](https://github.com/Puma7/meeting-for-handy/blob/96109374c3fa7d72ed0a3cea68706a90280fc484/docs/validation/pr41-review-followup-20261104.md)
bleibt mit allen detaillierten Vorher-/Nachherbeschreibungen unveränderlich in Git erhalten.
Private Rohbelege, frühere fehlgeschlagene Läufe und alte Reviewfassungen bleiben ebenfalls erhalten.
Verlinkte historische Texte werden nicht als neu ausgeführt oder vollständig sprachgeprüft ausgegeben.

## Verhalten und Daten

Die klassische Transkriptansicht und der optionale Advanced Meeting Assistant bleiben getrennt
nutzbar. Die erweiterte Ansicht verwendet die gemeinsamen Modelleinstellungen. Sie bietet
anpassbare Karten und Layouts, Vorbereitung, Notizen, überprüfbare Originalbelege und eine
begrenzte lokale Audiovorschau. Der Gesprächsverlauf stellt Änderungen dar; das Transkript
bewahrt den gesprochenen Originaltext. Menschliche Bestätigungen bleiben gesondert erhalten.

Parakeet Ultra verarbeitet während der Aufnahme abgeschlossene Abschnitte beider Spuren
nacheinander. Ruhige Stellen können Abschnittsgrenzen bilden; Kontextüberlappung wird begrenzt
verarbeitet. Audio wird unabhängig gesichert. Erst dauerhaft bestätigter Text erreicht die
Auswertung. Vorübergehende Erkennungsfehler erhalten höchstens drei aufeinanderfolgende
Versuche ohne gesicherten Fortschritt; ein dauerhaft gespeicherter Abschnitt erneuert das Budget.
Modell-, Protokoll-, Integritäts- und Speicherfehler bleiben unmittelbar terminal.

Langsame lokale Textmodellläufe werden nicht durch einen Intervalltick abgebrochen. Neue Arbeit
wird gesammelt; dieselbe Sitzung startet keine überlappenden Auswertungen. Ein ausdrücklicher
Auswertungsstopp beendet nicht die Audiosicherung. Diese Trennung ist mit kontrollierten
langsamen Antworten und zwei synthetischen PCM-Schreibern geprüft, nicht mit zwei Stunden
physischer Aufnahme unter gleichzeitiger realer ASR- und Modelllast.

Automatische erweiterte Auswertung bleibt lokal. Cloud benötigt ausdrückliche Textfreigabe je
Auswertung und erhält weder Audio noch lokale Pfade oder Sitzungsobjekte. Weiterleitungen,
System-Proxys und automatischer Anbieterwechsel bleiben gesperrt. Private Vorbereitung bleibt
privat; nur ausdrücklich freigegebene Inhalte werden geteilt. Originaljournale, Transkriptstände,
Modellfassungen und menschliche Bestätigungen bleiben versioniert erhalten. Schlüssel werden
nicht protokolliert oder als Klartext gespeichert. Keine Nutzermeetings wurden für diese Tests benutzt.

## Prüfungen und genauer Bezug

Rust- und Modellprüfungen wurden nach der abschließenden nativen Korrektur frisch ausgeführt.
Die nachfolgend ausdrücklich älteren Frontend-, Paket-, Browser- und Langsammodellnachweise
werden nicht als erneut ausgeführt ausgegeben. `verify.ps1` ist vor Quellupload frisch erforderlich.

| Prüfung | Tatsächliches Ergebnis |
| --- | --- |
| Rust-Workspace, alle Targets | Nach der abschließenden Zusagenprüfung frisch 555 bestanden, darunter 156 Notizen- und 37 Providerprotokolltests. Drei bestehende besondere Einstiege regulär ignoriert. Frühere Zwischenläufe vor den zusätzlichen Affirmations-, Sprecher- und Erledigt-Gegenproben sind kein endgültiger Nachweis. |
| Rustformat und Clippy | Auf diesem nativen Korrekturstand frisch bestanden; gesperrte Abhängigkeiten, Warnungen als Fehler. |
| Oberfläche | 395 Tests, TypeScript, Produktionsbuild und vollständige Frontendformatprüfung auf `aa72dbd` bestanden. Sämtliche Frontend-Produktions-, Typ- und Testbytes sind seitdem unverändert; lokal nicht wiederholt. 391 und 385 sind frühere Zwischenstände. |
| Projektprüfung | Vorherige veröffentlichte Stände bestanden 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen. Für diese native Korrektur nach dem letzten Dokumentedit erneut vor Upload erforderlich. |
| Echte lokale Referenzen | Nach der abschließenden nativen Korrektur frisch: Deutsch 99.8768426 s (0 reguläre Validierungswiederholungen), Englisch 120.4560198 s (0 Wiederholungen). Positive Aufgabe, unverbindliche Aussage, Impulserhalt, menschlicher Status nach Protokolländerung und bytegleiche Wiederöffnung bestanden. Wörtliches englisches „I’ll“ bleibt erhalten. Eine vollständig validierte Wiederholung je Anfrage bleibt zulässig; null Wiederholungen sind keine Freigabeschwelle. Frühere DE-/EN-Referenzen vor den abschließenden Aufgabenträger- und Erledigt-Korrekturen sind nicht dieser finale Nachweis. |
| Paketregression | 21 Prüfungen auf `91f0985`; Paketdateien seitdem unverändert. Tatsächlich erzeugter synthetischer ZIP prüft Streaming- oder Batchhinweis, Launcher, Fingerprint und Nichtüberschreibung. Kein lokales Produktpaket oder Installer daraus bereitgestellt. |
| Kontrolliertes Langsammodell | Auf `35d53e8`: 244.33 s Gesamtlauf, 210 s SSE-Antwortpause, nächster Refresh nach 210.0421383 s, maximal eine Modellanfrage, ein erhaltener Impuls. 3737920 Samples je Spur / 16000 Hz = 233.62 s je Spur. Beide PCM-Schreiber liefen nach Auswertungsstopp weiter. Hier nicht erneut ausgeführt. |
| Sidebar im Browser | 24 Abläufe auf `35d53e8`: 390/740/1280 Pixel × Deutsch/Englisch × hell/dunkel × ausgeklappt/eingeklappt. Geöffnete Rechnerdetails innerhalb des Fensters, kein horizontaler Überlauf, keine Seiten-/Konsolenfehler oder fremden Anfragen; eigener Server beendet. Betroffene Komponenten und Prüferbytes unverändert, hier nicht wiederholt. |

Die echten Referenzen verwendeten ausschließlich synthetischen Text und das vorhandene
**google/gemma-4-e4b Q4_K_M**, 6.326.936.720 Modellbytes, LM Studio **0.4.26.0**,
Windows 11 Pro **26200**, Intel **i7-13700**, rund **64 GiB** physisches RAM.
Keine Downloads oder Cloud. Während dieser Referenzen liefen keine anderen eigenen Prüfungen.
Die Dauer umfasst Modellverarbeitung, Abschluss und Wiederöffnung. Beim aktuellen Referenzpaar
wurden keine neuen Working-Set-, Private-Bytes- oder Grafikspeicherwerte erhoben.

Der Browserlauf verwendete vorhandenes Edge **156.0.4314.58** und Playwright **1.57.0** unter
Windows 11. Wiederholbarer Aufruf: `python -X utf8 scripts/check-sidebar-browser.py --browser msedge`.
Er verwendet tatsächliche Sidebar-/RecordingStatus-Komponenten, eigene synthetische Daten und
einen eigenen lokalen Server. Er installiert nichts und hat keinen Zugriff auf native Aufnahme,
Modellsteuerung oder Nutzermeetings. CPU 25 Prozent und RAM 2/8 GiB sind Fixturewerte,
keine Messung des Rechners. Die bestehende CI wurde dafür nicht umkonfiguriert.

## Tatsächlich abgeschlossene Plattformläufe

Auf dem früheren Produktstand `aa72dbd`, vor der jetzigen Negationskorrektur, bestanden alle **22** damaligen Checks:
**16 erfolgreich, sechs reguläre Skips**. Es wurde kein manueller CI-Neustart angefordert.
Diese Ergebnisse bleiben diesem Head zugeordnet und werden nicht auf spätere Heads umgehängt.

- [Linux, macOS ARM und Intel](https://github.com/Puma7/meeting-for-handy/actions/runs/41989925116): vollständige Desktopbuilds und tatsächliche generierte Sprachprüfungen mit finaler Version 1.1.0-beta.1, Handy 0.9.8 und Nemotron 3.5 Q8 auf CPU. Zeiten: Linux 10.398902639 s, ARM 13.1745865 s, Intel 29.221693595 s. Eigener Worker-Neustart und Erhalt der Audiohashes/Journale bestanden. Unit-Fixtures mit version=test werden nicht als diese Ergebnisse ausgegeben.
- [Windows Rustprüfung](https://github.com/Puma7/meeting-for-handy/actions/runs/41989925115): erfolgreich. Der modellfreie Windows-Gegenlauf ist kein echter ASR-Referenzlauf.
- Linux prüfte ausschließlich virtuelle Audiogeräte. Ergebnis ausdrücklich **passed-with-startup-gaps**, **completeCapture=false**. PCM/Ausrichtung und strenge Fenster-, Ton-, Dauer- und Zeitgrenzen blieben erhalten; keine Ausnahme für undeclared silence. Daraus folgt keine lückenlose physische Aufnahme.

Für den endgültigen veröffentlichten Head bleiben dessen eigene CI und frische Reviewquittungen
erforderlich. Spätere Ergebnisse und Links gehören in die PR-Beschreibung und den Abschlusskommentar,
damit nachträgliche Verifikation keinen weiteren Produktcommit vortäuscht.

## Negierte Prüfgegenstände und gemeinsame Aufgabenprüfung

Der [Codex-Befund auf e80f576](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4478440846)
war gültig: „I will check that the report is not public.“ und „Ich übernehme die Prüfung,
ob der Bericht nicht öffentlich ist.“ sind feste Zusagen. Die Verneinung gehört zum Gegenstand,
nicht zur Übernahme. Sechs tatsächliche Runner-Vorherfälle übersprangen die Aufgabenphase,
speicherten keine Aufgabe und markierten die Quellen dennoch als verarbeitet.
Die klassische Antwortvalidierung reproduzierte acht Auslassungen einschließlich einer
Aufgabe mit unverbindlichem Termin. Eine weitere echte Gegenprobe zeigte zwei falsche
klassische Annahmen unabhängiger if-/sofern-Bedingungen.

Schema-Zulassung und klassische Generierung verwenden jetzt dieselbe Zusagenprüfung.
Sie betrachtet einen belegten untergeordneten Prüfgegenstand getrennt vom Übernahmeausdruck.
Unabhängige Bedingungen behalten die konservative Gesamtprüfung. Nachfolgende Satzteile,
Satzzeichen und Koordinationen bleiben im Negationsfilter, damit echte nachgestellte Absagen
nicht versteckt werden. Das ist eine begrenzte Wort-/Satzteilheuristik, keine vollständige
Sprach- oder Bedeutungsprüfung. Die Agendaprüfung fehlender Zusagen bleibt davon getrennt.

Alle bisherigen Zusagenmarker bleiben erhalten und werden an Wortgrenzen erkannt, auch vor
Satzzeichen wie bei „Robin prüft, ob …“. Die bereits klassisch anerkannte affirmative Form
„I won’t forget to …“ bleibt auch zusammen mit that-/if-Prüfgegenständen erhalten.
Die zusätzliche Vorhergegenprobe reproduzierte zwei Auslassungen in jedem der beiden Wege;
auch diese Fälle sind korrigiert. Ein ausdrückliches „not check“ oder eine unabhängige
won’t-/cannot-Absage bleibt gesperrt. Ein unverbindlicher Termin bleibt unbestätigt,
selbst wenn die Aufgabe zulässig ist. Zuständige werden nicht erfunden.

Zusätzliche Gegenproben reproduzierten in beiden Wegen drei Auslassungen bei
„Robin will check if …“, „Robin will check that … not …“ und „Robin will ask if …“.
Die beiden konkreten Zukunftsprädikate will-check/will-ask und ihre unmittelbare
Satzteilgrenze sind jetzt berücksichtigt. Ein vorgeschaltetes „will not check“
und eine unabhängige if-Bedingung bleiben abgelehnt; alle bisherigen Marker bleiben erhalten.

Die tatsächlichen klassischen API- und Runner-/HTTP-Gegenfälle bestehen. Der Runner prüft
die Aufgabenphase, wörtliche Originalbelege und bytegleiche Wiederöffnung; die klassische
API prüft außerdem unbekannte Metadaten und abgelehnte unverbindliche Termine.
Originalzitate werden nur vorübergehend zur Klassifikation normalisiert. Gespeicherte ältere
Checkpoints werden weiterhin ausschließlich nach ihrem ursprünglichen Quellenvertrag gelesen,
nicht mit neuen Generierungsregeln umgedeutet oder gefiltert.

## Negative Aufgabenträger und erledigte Erstellung

Die nächste vollständige Runde auf `2c93a72` fand zwei gültige Fehler:
Der [Codex-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4479537375)
zeigte, dass „No one will check the report.“ oder „No employee will send the report.“
fälschlich eine offene Aufgabe zuließen. Vier tatsächliche klassische Vorherfälle behielten
eine Aufgabe; dieselben vier Runnerfälle starteten die tasks-Phase und speicherten Arbeit.
Negative Aufgabenträger vor dem Übernahmeprädikat werden jetzt abgefangen. Der Prüfgegenstand
einer festen Zusage wie „I will check that no one …“ bleibt davon getrennt.
Die bestehenden affirmativen Wendungen bleiben erhalten; „No problem“ und die zeitlichen
Grenzen „no later than“/„no earlier than“ sind ausdrücklich keine Absage.
Das ist weiterhin eine begrenzte Satzteilheuristik, kein vollständiger Sprachbeweis.

Der [CodeRabbit-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4479577671)
war ebenfalls gültig: Nach Trimmen des Markers konnte „erstellt“ auch vor Satzende passen.
„Das Protokoll wurde erstellt.“ und „Der Bericht ist erstellt.“ sind keine neue Zusage.
Vier tatsächliche klassische API- und vier Runner-Vorherfälle reproduzierten die Fehlannahme,
einschließlich Abstand vor Satzende und kommafortgesetztem abgeschlossenen Zustand.
Leerzeichenbeendete Marker benötigen jetzt eine Leerraum- oder Kommafortsetzung.
Unmittelbare Erledigt-Hilfsverben vor „erstellt“ werden nicht als aktives Prädikat zugelassen.
Die gemeinsame neue Aufgabenprüfung weist eine solche Erledigt-Aussage ohne zusätzliche
explizite Zusage auch in der klassischen API ab. Eine tatsächlich zusätzliche Übernahme
und aktive „Robin erstellt …“-/„Robin erstellt, …“-Zusagen bleiben zulässig.

Alle neuen Gegenfälle bestehen einschließlich Aufgabenphase, wörtlicher Originalbelege und
bytegleicher Wiederöffnung. Die bereits erfassten Negations-, Bedingungs-, Termin- und
Affirmationsfälle bleiben erhalten. Bereits gespeicherte Ergebnisse und Statuswechsel einer
bestehenden Aufgabe laufen weiterhin über ihre bisherigen Verträge; sie werden nicht als neue
Aufgabe neu klassifiziert. Der erste große Zwischenlauf vor der zusätzlichen Erledigt-Korrektur
ist nicht der endgültige Rustnachweis. Nach beiden Korrekturen wurden Rust, Clippy, Format und
beide echten lokalen synthetischen Referenzen frisch ausgeführt.

## Reviewkorrekturen

Alle 16 ursprünglichen [Sourcery-Anmerkungen](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5748992615)
wurden einzeln gegen den Code beurteilt, beantwortet und geschlossen. Identitäten und ursprüngliche
Kommentartexte wurden mit dem vollständig paginierten Audit abgeglichen. Nachfolgende Codex- und
CodeRabbit-Befunde wurden ebenfalls einzeln validiert. Die ausführliche Historie oben bewahrt
die genauen Fälle und früheren Fehlläufe. Wesentliche abgesicherte Mechanismen:

| Bereich | Korrektur und Regression |
| --- | --- |
| Quellen und Wiederaufnahme | Geänderte Originalquellen, journalisierte Kontextstände, Teilantworten und fehlerhafte Checkpoints können bestätigte Zustände nicht still überschreiben. Abgelehnte Antworten lassen den Zustand unverändert. Quellenkennungen und wörtliche Zitate müssen tatsächlich vorhanden sein. |
| Menschliche Änderungen | Impulse bleiben erhalten. Menschlicher Status bleibt auch nach Protokolländerung und Wiederöffnung erhalten. Fehlgeschlagene Bestätigungen werden nicht automatisch wiederholt; Notizentwürfe bleiben bei Poll-/Publikationsfehlern erhalten. |
| Belege und Zahlen | Textfassung und Statuswechsel verwenden getrennt ausreichende Originalbelege. Vorhandene Zahlen und unveränderte Textbelege werden erhalten. Die bestehende Grenze von sechs Belegen wird nicht erhöht. |
| Zusagen und Entscheidungen | Fehlende Zusagen, Vorschläge, Verneinung, Fragen und unabhängige Bedingungen sind keine feste Aufgabe oder Entscheidung. Feste negative Entscheidungsalternativen bleiben unterscheidbar. Die Prüfung bleibt eine begrenzte Wortheuristik, kein vollständiger Sprach- oder Bedeutungsbeweis. |
| Eingebettete englische Fragen | Vier falsche Entscheidungsablehnungen und drei tatsächlich verlorene Aufgaben sind vorher reproduziert. Aufgaben übersprangen die tasks-Phase und markierten Quellen trotzdem als verarbeitet. Gemeinsame begrenzte check-if/ask-if-Prüfung nach einer ausdrücklichen englischen Zusage/Entscheidung behebt dies. Whether/ob bleiben zulässig; unabhängige vorangestellte, nachgestellte und zweite Bedingungen sowie unless bleiben gesperrt. Originalzitate bleiben wörtlich; die Runnerregression prüft Aufgabenphase und bytegleiche Wiederöffnung. |
| Deutsche Bedingungen | Eine zusätzliche tatsächliche sofern-Fehlannahme bei Aufgaben ist behoben. Jedenfalls und ebenfalls bleiben unbedingte Wörter. Bestehende Negationskontrollen und Prüfgrenzen bleiben erhalten. |
| Modellreservierungen | Geladene Ollama-Kontextlänge wird vor abweichender Wiederverwendung/Erneuerung erneut gelesen. Andere oder unbekannte Länge verlangt bekannte Größe und die bestehende Zulassung: 30 Prozent Zuschlag plus 2 GiB Reserve. Gleicher bestätigter Kontext benötigt keinen neuen Ladeversuch. Vier frühere Mutationsfehler, echter 60-s-Timer und Cache-Gegenfall sind abgesichert. Alle 37 Providerfälle bestehen. |
| Fremderhaltung | Fremde unendliche Reservierungen bleiben fremd. Endliche TTL 300 s, Erneuerung 60 s und begrenzte Verwaltungsanfragen bleiben erhalten. Bereits fremd geladenes ausgewähltes LM-Modell wird nicht übernommen oder nochmals geladen. Unbestätigte Load-/Unload-Vorgänge erlauben keine geratene Instanz oder automatische Zweitladung. Protokollfixtures sind kein realer Ollama-Allokations- oder atomarer Eigentumsnachweis. |
| Oberfläche und Hinweise | Globale Hinweise erreichen auch die erweiterte Ansicht. Poll-, Aktions- und Notizfehler bleiben verständlich und verschwinden erst bei passender Wiederherstellung. Layout-, Spalten-, Fokus-, Quellen- und Ressourcenanzeigen haben Interaktionsgegenproben. Alte kleine Testmodule wurden mit unveränderten Funktionskörpern gebündelt; keine fachlichen Fälle oder Fristen entfernt. |
| Beschädigte gespeicherte Auswertungen | Vier echte DE/EN-Vorherfehler bei error=null sind korrigiert. Der Warnstatus unterscheidet älteren lesbaren Stand von unverfügbaren Ergebnissen. Dateien bleiben erhalten; Standardansicht ist als Prüfweg benannt. Erholung entfernt die Warnung. Kein Start/Stopp/Kontextschreiben wird dadurch ausgelöst. |
| Pakethinweis und Dokumentation | START-HERE.txt nennt ausdrücklich entweder Streaming- oder Batchmodell. Ein tatsächlicher synthetischer ZIP-Vorherfall scheiterte am fehlenden Batchhinweis, danach bestanden 21 Paketfälle. Historische 385/391 UI-Fälle sind keine aktuellen 395 Fälle. |

Die zwei sauberen Codex-Codeprüfungen auf `aa72dbd` sind
[Runde 1](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6652858349) und
[Runde 2](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6652999692).
Der vollständige [CodeRabbit-Lauf auf diesem Head](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5781611293)
fand noch die inzwischen korrigierte historische Testzahl. Er war deshalb kein sauberer Abschluss.
Die zusätzlichen pauschalen Endpoint-/Tempordner-Behauptungen wurden
[ausdrücklich als unbelegt zurückgenommen](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6653055283).
Für den endgültigen Head sind eigene frische saubere Reviews erforderlich.

Bei diesem früheren Lauf konnte LanguageTool nicht alle Textabschnitte prüfen.
Die konkrete Ursache ist nicht bekannt; eine Größen- oder Zeitgrenze ist nicht nachgewiesen.
Auch der verdichtete Vermerk hatte noch unvollständige zusätzliche Sprachprüfung. Ein Tool-Fix oder eine Ursache ist damit nicht nachgewiesen.
Eine vollständige Hauptreview-Auswahl beweist keine vollständige zusätzliche Sprachprüfung.
Die nächste tatsächliche Runde muss ihren eigenen Status ausweisen; Tool und Regeln bleiben aktiv.

## Veröffentlichung und Grenzen

Vor Upload werden isolierter Git-Root, eigener Branch und tatsächlicher bisheriger Remote-Head
geprüft; normaler Fast-forward. Alle Dateien des tatsächlichen unveränderlichen Remote-ZIP
werden mit Git-Blobs und lokalen Bytes verglichen. Der vollständige PR-Diff umfasst 102 Dateien,
davon 100 unter den unveränderten zwei Lockdatei-Filtern. Patches werden vollständig paginiert
und ihre Zeilenzahlen mit Git verglichen. Alle früheren Belege und Gitfassungen bleiben erhalten.
Abhängigkeiten, Datenschutz-/Reviewer-Konfiguration, Dokumentationsgrenze von 80 Prozent,
Version und kompletter TEST_REPORT bleiben unverändert. Alte oder leere Security-Flags
sind kein aktueller formaler Securitynachweis.

Physische Aufnahme-/Berechtigungs-/Geräteverlustprüfungen und der zweistündige Gerätelauf
wurden auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-, kombinierte reale
Zweistundenlast-, unabhängige installierte Prozess-, Signatur-, Notarisierungs- oder forensische
Speicherprüfung wird aus den obigen Tests abgeleitet. Unbekannte Zuständige und Termine bleiben
unbekannt; Abschnittszeiten sind keine Wortzeiten. Technische Belegprüfung beweist keine
inhaltliche Deutung. Ergebnisse bleiben überprüfbare KI-Entwürfe. Bekannte gesperrte
Abhängigkeitsrisiken bleiben in RUST_DEPENDENCIES dokumentiert. Diese Nacharbeit erstellt
keinen installierten Programmstand und keinen Release.
