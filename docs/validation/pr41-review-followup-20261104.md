# PR 41: Lokale Validierung vor Veröffentlichung

Stand: 5. November 2026. Version **1.1.0-beta.1**, unverändert.

Diese Nacharbeit unterscheidet Teilfortschritt von Prozentwerten im Namen
einer vollständig erledigten Aufgabe. Explizite Teilmengen schließen eine
Aufgabe nicht automatisch. Beide Referenzsprachen enthalten seit dem Vorgänger
eine ausdrückliche Budgetfrage, wenn der Test einen Gesprächsimpuls verlangt.
Die ursprünglichen Zusagen und unverbindlichen Aussagen bleiben erhalten.

Die [bisherige ausführliche Fassung](https://github.com/Puma7/meeting-for-handy/blob/94b3ee6ad282dcb1208244f4dd53e9b57df67ba1/docs/validation/pr41-review-followup-20261104.md)
und deren Detailhistorie bleiben unveränderlich in Git.
Private Rohbelege, alte Checkpoints und fehlgeschlagene Läufe bleiben erhalten.
Der vollständige TEST_REPORT bleibt gegenüber dem PR-Basisstand byteunverändert.
Dieser bereinigte Vermerk veröffentlicht keine Nutzermeetings oder privaten Sitzungsdaten.

## Korrektur der Abschlussprüfung

Der [Codex-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4486325695)
war gültig: „Ich habe den Bericht zu 50 Prozent abgeschlossen.“ und
„I have completed 50% of the report.“ konnten eine offene Aufgabe schließen.
Die tatsächliche Validator-/Checkpoint-Regression reproduzierte insgesamt
15 nach den konservativen Kriterien unzulässige oder unklare Prozentbelege.

Die Abschlussprüfung erkennt Prozentzeichen, Prozent, percent und per cent.
Bei einer Fortschrittsangabe akzeptiert sie nur den eindeutigen numerischen
Wert 100, auch mit Dezimalnullen. Ziffern werden exakt verglichen:
99.9999999999999999999 wird nicht durch Fließkommarundung zu 100.
Teilwerte, unplausible Werte über 100 sowie Bereiche und Bruchangaben mit
abschließendem Wert 100 bleiben ohne automatische Erledigung.
Ausgeschriebene unbekannte Prozentwerte werden konservativ zurückgewiesen.

Fünf eindeutige 100-Prozent-Formen bestanden bereits vorher und weiterhin:
Deutsch/Englisch, Symbol oder Worteinheit sowie Dezimalnullen mit Punkt oder Komma.
Ein zusätzlicher vollständiger Satz über einen „percentage report“ besteht;
percentage als Substantiv wird nicht als Prozenteinheit behandelt.

Der [nachfolgende Codex-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4486763837)
war ebenfalls gültig: Vollständige Abschlüsse eines 50-Prozent-Rabattberichts
wurden zu Unrecht abgewiesen. Zwei tatsächliche Vorherfälle reproduzierten dies.
Jetzt wird ein Prozentwert vor einem Gegenstandsnamen von ausdrücklich
prozentualem Fortschritt unterschieden; zusammengesetzte Berichtstitel bleiben zulässig.

Zehn zusätzliche Vorherfälle schlossen Aufgaben trotz half of, zur Hälfte,
ein Drittel, 1 von 2 oder äquivalenten Unicode-Prozentzeichen.
Diese ausdrücklichen Teilmengen bleiben jetzt offen. Vollständige 2-von-2-
und Drei-Drittel-Angaben, Datumsangaben, ordinales third und half-year bleiben zulässig.
Ziffernverhältnisse werden ohne Fließkommarundung verglichen.

Die vorhandene dauerhafte Regression erhielt im Vorgänger 21 Fälle und
in dieser Nacharbeit weitere **25 Fälle**; kein bestehender Fall wurde entfernt.
Die Zahl der Testfunktionen bleibt unverändert.
Abgelehnte Antworten verändern weder Aufgabe noch Zitate oder alten Checkpoint
und schreiben keine zweite Revision.
Zulässige Abschlüsse bewahren die wörtlichen Quellen und stimmen nach
wiederholter Wiederöffnung bytegleich überein.

Die vorherige Wortprüfung bleibt erhalten: fast, beinahe, nahezu, teilweise,
weitgehend und entsprechende englische Marker reichen nicht als Abschluss.
Englisches fast im Sinn von schnell bleibt von deutschem fast unterscheidbar.
Das sind begrenzte konservative Regeln, kein vollständiger Grammatik-
oder Bedeutungsbeweis. Menschliche Überprüfung bleibt erforderlich.

## Echte Referenzen und korrigierte Prüfsituation

Vor dem Vorgänger verlangte die deutsche Referenz einen offenen Gesprächsimpuls,
enthielt aber keine ausdrückliche Frage.
Ein tatsächlicher Durchlauf speicherte Aufgabe und Fakten ohne Impuls
und fiel daher durch diese zusätzliche Erwartung.
Die englische Referenz enthielt bereits „Who can approve the budget?“.

Der Vorgänger ergänzte „Wer kann das Budget freigeben?“.
Dieser Korpus und der Referenzhelfer bleiben in dieser Nacharbeit byteunverändert.
Alle bisherigen positiven Zusagen, der fehlende Budgetauftrag und der
unverbindliche Webseitenvorschlag bleiben enthalten.
Die englische Fassung einschließlich des wörtlichen „I’ll“ ist byteunverändert.
Alle fünf Kriterien, Quellenprüfung, Zeitlimits und das vorhandene Budget
von einer vollständig validierten Wiederholung je Anfrage bleiben unverändert.
Der neue Korpus wurde vor seinen Läufen separat festgelegt;
alte Fehlläufe werden dadurch nicht als bestanden umgedeutet.

| Prüfung | Tatsächliches Ergebnis |
| --- | --- |
| Rust-Workspace, alle Targets | Frisch **567 bestanden**, darunter **168 Notizenfälle** und **37 Providerprotokollfälle**; drei bestehende besondere Einstiege regulär ignoriert. |
| Rustformat und Clippy | Frisch bestanden, gesperrte Abhängigkeiten und Warnungen als Fehler. |
| Deutsch, tatsächliches lokales Modell | Frisch **164.4825003 s**; reguläre vollständig validierte Wiederholungen: **2**. |
| Englisch, tatsächliches lokales Modell | Frisch **110.0172312 s**; reguläre vollständig validierte Wiederholungen: **0**. |
| Frontend | **414 Tests in 21 Dateien**, TypeScript, Produktionsbuild und Format auf den hier bytegleichen Komponenten von 8d0ba21 bestanden; hier nicht erneut ausgeführt. |
| Projektprüfung | Nach dem letzten Dokumentedit vor Upload erneut verify.ps1 erforderlich: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen. |

Beide frischen Modellreferenzen auf dem unveränderten Korpus bestehen für positive
Aufgaben, unverbindliche Aussagen, Impulserhalt, menschlichen Status nach Protokolländerung und bytegleiche
Wiederöffnung. Die alte Grenze wurde nicht erhöht; null Wiederholungen sind
keine Freigabeschwelle.

Verwendet wurden ausschließlich synthetischer Text und das vorhandene
**google/gemma-4-e4b Q4_K_M**, 6.326.936.720 Modellbytes.
LM Studio **0.4.26+4**, Windows 11 Pro **26200**, Intel **i7-13700**,
rund **64 GiB** sichtbarer physischer Speicher.
Die Umgebungsdaten wurden im unmittelbar vorherigen Durchlauf lesend geprüft;
die ausgewählte Modellinstanz wurde vor dieser Referenz erneut lesend geprüft.
Die Zeitmessung des Referenzhelfers umfasst Modellverarbeitung, dauerhaften
Abschluss und Wiederöffnung. Während der Inferenz liefen keine anderen eigenen
Prüfungen. Keine Downloads, Cloud oder Nutzermeetings.
Working Set, Private Bytes und Grafikspeicher wurden hier nicht neu gemessen;
Modell-Dateigröße ist keine RAM-Messung.

## Erhaltene Fehlläufe und Nachweisgrenzen

Der erste deutsche Versuch dieser Nacharbeit brach im ersten Fenster mit der
Meldung zum Antwortlimit ab. Es entstand keine Ergebnisrevision; der ursprüngliche
Agendakontext blieb erhalten. Die genaue Phase und abgelehnte Modellantwort wurden
nicht separat gespeichert, die weitergehende Ursache ist unbekannt. Das oben
genannte weitere Paar lief mit identischem Code, Modell, Korpus und Kriterien.
Auch dieser Fehllauf bleibt erhalten und zählt nicht als bestandene Referenz.

- Ein früher englischer Versuch des Vorgängers hielt im zweiten Fenster
  wegen einer nichtleeren unresolvedContextIds-Antwort korrekt an.
  Erster Checkpoint, besprochener menschlicher Impuls und Protokolländerung
  blieben erhalten. Die angeforderten Kennungen und die abgelehnte Antwort
  wurden nicht separat gespeichert; die weitergehende Ursache ist unbekannt.
- Der darauf folgende deutsche Versuch ohne ausdrückliche Budgetfrage fiel
  mangels Gesprächsimpuls durch. Sein Zustand und das erfolgreiche erste
  deutsche Ergebnis bleiben erhalten. Das oben genannte Paar verwendet den
  ausdrücklich beschriebenen neuen deutschen Korpus.
- Auf 5896b2b speicherte ein früher deutscher Versuch die Zusage nur als
  unklaren Fakt ohne Aufgabe und fiel durch. Ein damaliges weiteres Paar
  bestand auf unverändertem Code und Korpus. Auch dieser Fehllauf bleibt erhalten.
- Ein früherer deutscher Versuch auf da6b5c2 hielt wegen fehlend gemeldeten
  Kontexts an. Der damalige Linux-Lauf auf 689212b scheiterte mit 10159/10154 ms
  an der unveränderten Obergrenze 10000 ms. Weitergehende Ursachen wurden
  nicht nachgewiesen. Spätere erfolgreiche Heads ersetzen diese Ergebnisse nicht.

Kleine Modelle können ungültige oder unvollständige Antworten liefern.
Ungültige Bündel werden nicht veröffentlicht; gesicherte Arbeit bleibt erhalten.
Ein bestandenes Referenzpaar beweist keine vollständige Erkennung aller Aussagen.

Die Sidebar-/RecordingStatus-/MeetingView-Browserprüfung bleibt dem relevanten
bytegleichen 93d48e2-Komponentenstand zugeordnet: 24 Layout-, 24 Warn-Titel-
und 24 klassische Meetingtitel-Zustände.
Die Kartenprüfung auf 8d0ba21 verwendete die tatsächliche OverviewCard in Edge
156.0.4314.58 mit Playwright 1.57.0: acht Kombinationen aus 390/1280 Pixeln,
Deutsch/Englisch und hell/dunkel, vier Touchfälle, Tastatur und Außenklick.
Keine Seiten-/Konsolenfehler oder Fremdanfragen; eigener Browser und Server beendet,
Screenshot visuell geprüft. Diese Browserprüfungen wurden hier nicht erneut ausgeführt.
Native Tauri-WebViews, IPC, OS-Tooltips und Screenreader-Sprache sind damit nicht geprüft.

Der frühere kontrollierte Langsammodelllauf auf 35d53e8 dauerte 244.33 s
mit 210 s SSE-Pause. Höchstens eine Anfrage lief gleichzeitig;
ein Impuls blieb erhalten. 3737920 Samples / 16000 Hz = 233.62 s je Spur.
Beide synthetischen PCM-Schreiber liefen nach Auswertungsstopp weiter.
Das ist kein realer kombinierter Zweistundenlauf.
Die 21 Paketregressionen auf 91f0985 nutzten synthetische ZIPs;
hier wurde kein neues Produktpaket oder Installer bereitgestellt.

Auf 5896b2b bestanden tatsächlich alle 22 damaligen Checks:
16 erfolgreich und sechs reguläre Skips.
Die [Plattformläufe](https://github.com/Puma7/meeting-for-handy/actions/runs/42101116336)
enthielten tatsächliche generierte Sprachprüfungen mit Version 1.1.0-beta.1,
Handy 0.9.8 und Nemotron 3.5 Q8 auf CPU:
Linux 10.509165387 s, ARM 13.535942541 s, Intel 26.465786573 s.
Eigener Worker-Neustart und Audiohash-/Journalerhalt bestanden.
Der Windows-Gegenlauf war modellfrei und ist kein ASR-Referenzlauf.
Unit-Fixtures mit version=test werden nicht als diese Messungen ausgegeben.

Die virtuelle Linux-Aufnahme dieses Vorgängers bestand ausdrücklich
mit passed-with-startup-gaps und completeCapture=false:
9642/9646 ms, 146399/144399 qualifizierte Tonsamples,
native Startauffüllung 7881/9947 Samples und je ein eingehender Phasennullwert.
Ton-, Fenster-, Dauer-, Differenz- und Zeitgrenzen blieben unverändert.
Die Quellen waren ausschließlich virtuelle Geräte.
Daraus folgt keine lückenlose physische Aufnahme.
Diese CI-Ergebnisse werden nicht dem neuen Head zugeordnet.

## Reviewstand und Veröffentlichung

Alle 16 ursprünglichen [Sourcery-Anmerkungen](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5748992615)
wurden einzeln geprüft, beantwortet und geschlossen.
Der paginierte Audit vergleicht die ursprünglichen Kommentartexte und Identitäten.
Alle bisherigen Korrekturen bleiben enthalten: geschützte Originalbelege,
menschliche Änderungen, erhaltene Impulse und Notizentwürfe, stabile Aufgaben
beim Modellwechsel, Zurücknahme veralteter Quellen, sichere Wiederaufnahme,
Modellreservierungen und kompakte zugängliche Bedienung.
Die ausführlichen Vorher-/Nachherfälle bleiben über die unveränderliche Historie erreichbar.

Die zwei sauberen Codex-Codeprüfungen auf 5896b2b waren historische Einzelnachweise.
Der folgende vollständige CodeRabbit-Lauf fand die inzwischen korrigierte
doppelte Workspace-Projektion. Nachfolgende Codex-Läufe fanden Wortmarker
auf 90becdd, quantitative Teilfortschritte auf dfe5d65 und die zu breite
Prozentprüfung auf 94b3ee6.
Diese Runden waren keine sauberen Abschlüsse.

Die Projektionskorrektur wurde von CodeRabbit
[bestätigt](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4485972047).
Die pauschale zusätzliche Testisolations-/Browser-Sorge wurde mangels konkretem
Fehler [zurückgenommen](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6669253684).
Native WebViews bleiben eine Nachweisgrenze.
Die zusätzliche LanguageTool-Prüfung war zuletzt teilweise unvollständig;
Ursache unbekannt, Tool und Regeln bleiben aktiv.
Alte oder leere Security-Flags sind kein aktueller formaler Securitynachweis.

Vor Upload werden eigener Git-Root, Branch und tatsächlicher Remote-Vorgänger geprüft.
Alle Dateien des tatsächlichen unveränderlichen Remote-ZIP werden mit lokalen Bytes
und Git-Blobs verglichen. Alle 102 PR-Patches werden vollständig paginiert;
100 zulässige Dateien bleiben unter den unveränderten zwei Lockdatei-Filtern.
Für den endgültigen Head sind zwei frische saubere Codex-Codeprüfungen,
eine vollständige saubere CodeRabbit-Runde und dessen eigene grüne CI erforderlich.
Später tatsächlich abgeschlossene Prüfungen und Links werden im PR nachgelesen
und dokumentiert, ohne nachträglich Produktbytes umzuschreiben.

## Daten, Verhalten und Kompatibilität

Die klassische Transkriptansicht und der optionale Advanced Meeting Assistant
bleiben getrennt nutzbar und verwenden gemeinsame Modelleinstellungen.
Parakeet Ultra verarbeitet abgeschlossene Abschnitte beider Spuren während
der Aufnahme nacheinander. Audio wird unabhängig gesichert.
Erst dauerhaft bestätigter Text erreicht die Auswertung.
Intervallticks brechen langsame Modellläufe nicht ab;
dieselbe Sitzung startet keine überlappenden Auswertungen.

Automatische erweiterte Auswertung bleibt lokal. Cloud braucht ausdrückliche
Textfreigabe je Auswertung und erhält weder Audio noch lokale Pfade
oder Sitzungsobjekte. Weiterleitungen, System-Proxys und automatischer
Anbieterwechsel bleiben gesperrt. Private Vorbereitung bleibt privat.
Originaljournale, Transkriptstände, Modellfassungen und menschliche Bestätigungen
bleiben versioniert erhalten. Schlüssel werden nicht protokolliert oder als Klartext gespeichert.

Bestehende native Sitzungen behalten Modellkennung und gesicherte Gewichte.
Fortsetzen verifiziert Katalog und Gewichte; fehlende oder geänderte Modelle
werden nicht still ersetzt. Archiviertes Audio muss zuerst wiederhergestellt werden.
Legacy-Originalsitzungen bleiben lesbar; ihre nachträgliche Erkennung
benötigt weiterhin die bisherige Programmversion.
Fremde Modellreservierungen bleiben fremd; TTL 300 s und Erneuerung 60 s
sowie bestehende Speicher- und Verwaltungsschranken bleiben erhalten.

Statische Produktprompts, Vertragsdateien, Belegbudget, Fristen, UI, Transport,
Aufnahme-/ASR-Logik, Modellreservierungen, Abhängigkeiten, Schutz-/Reviewerregeln,
Version und vollständiger TEST_REPORT bleiben gegenüber 94b3ee6 unverändert.
Auch der deutsche und englische synthetische Referenztext bleiben hier byteunverändert.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und zweistündige Gerätetests
wurden auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-,
kombinierte Zweistundenlast-, unabhängige installierte Prozess-, Signatur-,
Notarisierungs- oder forensische Speicherfreigabe.
Cloud-Prüfungen ohne echte Zugangsdaten bleiben Simulationen.
Bekannte gesperrte Abhängigkeitsrisiken bleiben dokumentiert.
Unbekannte Zuständige und Termine bleiben unbekannt; Abschnittszeiten sind keine Wortzeiten.
Technische Belegprüfung beweist keine inhaltliche Deutung.
Ergebnisse bleiben überprüfbare KI-Entwürfe.
Diese Nacharbeit erstellt keinen installierten Programmstand und keinen Release.
