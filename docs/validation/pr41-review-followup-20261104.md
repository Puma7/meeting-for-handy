# PR 41: Validierungsstand

Stand: 4. November 2026. Version: **1.1.0-beta.1**, unverändert.
Dieser Vermerk beschreibt die tatsächlich geprüften Funktionen und ihre Grenzen.
Ausführliche frühere Fassungen bleiben in der Git-Historie und den privaten Belegen erhalten.
Der vollständige TEST_REPORT bleibt byteunverändert; er wurde für diese Nacharbeit nicht neu veröffentlicht.

## Verhalten und Schutzgrenzen

- Klassische Transkriptansicht und optionaler Advanced Meeting Assistant bleiben getrennt nutzbar.
  Die erweiterte Ansicht verwendet die gemeinsamen Modelleinstellungen und bietet anpassbare Karten,
  Notizen, überprüfbare Originalbelege und eine begrenzte lokale Audiovorschau.
- Parakeet Ultra verarbeitet abgeschlossene Abschnitte beider Spuren nacheinander.
  Audio wird unabhängig gesichert; dauerhaft bestätigter Text erreicht die laufende Auswertung.
  Vorübergehende Fehler haben höchstens drei aufeinanderfolgende Versuche ohne gesicherten Fortschritt.
  Ein tatsächlich dauerhaft gespeicherter Abschnitt erneuert dieses Budget.
  Modell-, Protokoll-, Integritäts- und Speicherfehler bleiben unmittelbar terminal.
- Langsame lokale Modellläufe werden nicht durch einen Intervalltick abgebrochen.
  Neue Arbeit wird gesammelt; dieselbe Sitzung startet keine überlappenden Auswertungen.
- Automatische erweiterte Auswertung bleibt lokal. Cloud benötigt ausdrückliche Textfreigabe je Auswertung.
  Audio, lokale Dateipfade und Sitzungsobjekte gelangen nicht zum Textmodelltransport.
  Lokale Weiterleitungen, System-Proxys und automatischer Anbieterwechsel bleiben gesperrt.
- Originaljournale, Transkriptstände, menschliche Bestätigungen und ältere Modellfassungen bleiben erhalten.
  Quellenkennungen, wörtliche Zitate, Zuständigkeiten und Zahlen werden technisch geprüft.
  Diese Prüfung beweist keine inhaltliche Deutung; Ergebnisse bleiben überprüfbare KI-Entwürfe.

## Lokale Nachweise und genauer Bezug

| Prüfung | Ergebnis und Bezug |
| --- | --- |
| Rust-Workspace / alle Targets | 542 frisch bestanden, darunter 146 Notizen- und 34 Providerprotokolltests; drei bestehende besondere Einstiege regulär ignoriert. Die drei zusätzlichen Statusregressionen sind enthalten. |
| Rustformat / Clippy | Auf diesem Korrekturstand frisch bestanden; gesperrte Abhängigkeiten und Warnungen als Fehler. |
| Oberfläche | 375 Tests, TypeScript, Produktionsbuild und Format auf `89b85e8` bestanden. Sämtliche UI-Bytes sind gegenüber diesem Stand unverändert; diese Prüfungen wurden für die anschließende reine Rust-Statuskorrektur nicht erneut ausgeführt. |
| Projektfreigabe | `verify.ps1` vor jeder Bereitstellung: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen bestanden. Der folgende Quellupload bleibt an einen abschließenden erfolgreichen Lauf gebunden. |
| Echte lokale englische Referenz | Nach den drei zusätzlichen Statuskorrekturen 125,2169942 s, keine Validierungswiederholung. Positive Aufgabe, unverbindliche Aussage, Impulserhalt, menschlicher Status nach Protokolländerung und bytegleiche Wiederöffnung bestanden. Die früheren Läufe auf `89b85e8` mit 138,1599687 s, auf `1768125` mit 129,685532 s und auf `34c1b20` mit 138,0743712 s bleiben erhalten und werden nicht als neu ausgeführt gezählt. |
| Frühere lokale Referenzen | Deutsch 100,9989873 s und Englisch 125,0518623 s auf dem Elternstand `3d4d013`, jeweils ohne Validierungswiederholung. Die deutsche Zusage enthält tatsächlich „jedenfalls“. Diese Läufe werden nicht als erneut ausgeführt ausgegeben. |
| Langsammodell / zwei Spuren | Historischer, ausdrücklich ausgeführter besonderer Test auf `1768125`: 244,43 s Laufzeit, 210 s kontrollierte SSE-Antwortpause, nächster Refresh nach 210,0497661 s, maximal eine Modellanfrage, ein erhaltener Impuls. Je 3.706.880 Samples / 16.000 Hz = 231,68 s pro synthetischer Spur. Beide dauerhaften PCM-Schreiber liefen nach dem Auswertungsstopp weiter. Der frühere Lauf auf `3d4d013` bleibt ebenfalls historischer Beleg. Für diese reine Status-/Beschriftungsnacharbeit wurde der lange Test nicht erneut ausgeführt; sein Worker-/Transport-/Aufnahmecode ist byteunverändert. |
| Browsergegenprüfung | Tatsächliche Sidebar-/RecordingStatus-Komponenten auf `3d4d013`, danach unveränderte Sidebar-/Status-/CSS-Bytes: bei 390/740/1280 px ausgeklappt alle vier Statusfelder sichtbar, eingeklappt verborgen; Rechnerdetails erreichbar, keine Konsolenfehler. |

Die echten Referenzen verwendeten ausschließlich synthetischen Text und das vorhandene
**google/gemma-4-e4b Q4_K_M** mit 6.326.936.720 Modellbytes, LM Studio **0.4.26.0**,
Windows 11 Pro **26200**, Intel **i7-13700** und rund **64 GiB** physischem RAM.
Keine Downloads, Cloud oder Nutzermeetings. Bei früheren Referenzen liefen UI-Prüfungen zeitweise parallel; beim aktuellen Referenzlauf liefen keine weiteren eigenen Tests.
Die Referenzdauer umfasst Modellverarbeitung, Abschluss und Wiederöffnung.
Bei den früheren Referenzen wurden Working Set, Private Bytes und Grafikspeicher nicht gemessen.
Beim vorherigen englischen Lauf auf `1768125` wurde am 4. November um 20:20:14 UTC eine einzelne Stichprobe des
Modellprozesses erfasst: Working Set 6.540.374.016 Byte, Private Bytes 6.484.041.728 Byte.
Diese Werte werden nicht addiert und sind weder Höchstwerte noch Grafikspeicher. Grafikspeicher
wurde nicht gemessen; die vorhandene RTX 4070 begründet allein keinen GPU-Prüfnachweis.
Nutzerprozesse liefen unverändert weiter; dies war keine kontrollierte Leerlaufumgebung.
Beim aktuellen Lauf wurde keine neue Speicherstichprobe erfasst.

Der 210-Sekunden-Test nutzt den tatsächlichen nativen Worker und dauerhafte Audio-Schreiber,
aber einen kontrollierten HTTP-/SSE-Anbieter und erzeugte PCM-Samples: keine physischen Geräte
oder echte ASR dieses Tests. Frühere Batchregressionen mit simuliertem Erkenner prüften
sechs dauerhaft gespeicherte Abschnitte nach fünf getrennten Prozessfehlern,
tatsächliche Journalwiederöffnung und unveränderte ursprüngliche Audiohashes.

Browserumgebung: vorhandenes Edge **156.0.4314.58**, Python-Playwright **1.57.0**;
der Browser-Plugin-Skill war nicht verfügbar. Keine Browser- oder Abhängigkeitsdownloads.
Separate synthetische API-Fixture ohne native IPC; nur der anhand seines Pfads und Ports
geprüfte eigene Testserver wurde beendet. Bilder und Messwerte bleiben als private Belege erhalten.

## Wesentliche Regressionen aus den Reviewrunden

- Agentenfreigaben verlangen auch in der Oberfläche ein streamingfähiges Modell.
  Der vollständige App-Test bewahrt manuelle Batchaufnahme und blockiert falsche Agentenfreigaben.
- Audiovorschau-Wiederholungen setzen keine bereits freigegebene URL erneut ein.
  Der Test beobachtet tatsächlich gesetzte Audio-src-Attribute und beide URL-Freigaben.
  Sein vollständiger Funktionskörper bleibt erhalten; die ursprüngliche Frist ist ausdrücklich **5.000 ms**.
- Aufgabenbedingungen werden als ganze alphabetische Wörter geprüft.
  „jedenfalls“, „ebenfalls“ und „Tarif“ blockieren klare Zusagen nicht mehr;
  zehn Gegenfälle für echte Bedingungen, fehlende Zusagen und Fragen bleiben abgewiesen.
- Workspace-Schreiber geben ihre eigene Dateisperre beim Ende ausdrücklich frei.
  Ein zuvor mit Windows OS33 reproduzierter Duplikatfall öffnet danach wieder;
  der neue Besitzer bleibt exklusiv, auch wenn das alte Duplikat später schließt.
  Der ursprüngliche macOS-ARM-Testfehler mit OS35 bleibt als fehlgeschlagener Lauf erhalten.
- Ein vorhandenes Kontextjournal muss die Modellcheckpointrevision erreichen und dort vollständig übereinstimmen.
  Neun Schadens-/Zugriffskombinationen blockieren Öffnen, Lesen und Aktualisieren ohne Dateiänderung.
  Gültige ältere Checkpoints ohne Journal bleiben lesbar und können einen unmittelbaren Nachfolger anlegen.
- Vorläufig abgewiesene menschliche Statusklicks werden verständlich gemeldet und bewusst erneut geklickt.
  Erfolgreiche bestätigte Wiederholung entfernt die alte Meldung; keine automatische veraltete Wiederholung.
- Zu große freigegebene Kontexte werden vor der Übertragung mit derselben nativen 8.000-Skalar-Grenze abgewiesen.
  Entwurf, Historie und letzte gültige Freigabe bleiben erhalten; unabhängige Themenrücknahmen bleiben möglich.
- DE/EN-Beschriftungen und Einfügemarker haben geprüfte Wortabstände.
  Vier Pointerfälle prüfen den exakten Marker im normalen und gestapelten Layout.
  Neue Aufnahmehinweise werden höflich angekündigt; initiale und unveränderte Polls bleiben still.
- Quellenkorrektur, geerbte menschliche Zustände, Revisionskonflikte und Statusänderungen bleiben versioniert.
  Neue Statusbelege werden unabhängig geprüft; gültige ursprüngliche Text-/Zahlbelege bleiben erhalten.

## Kontextgröße und eigene Layoutnamen: zusätzliche Reviewkorrektur

Die vollständige CodeRabbit-Runde auf `d96491a` hat alle 100 vorgesehenen Dateien geprüft,
keine nicht verfügbaren Dateien gemeldet und zwei neue gültige Befunde geliefert.
Der vorliegende Stand korrigiert diese; er benötigt anschließend neue Reviews auf seinem eigenen Head.

- Die englische Übersetzung berücksichtigt ausschließlich eigene Wörterbucheinträge.
  Eigene Layoutnamen bleiben unverändert, auch wenn sie „Transkript“, „constructor“, „toString“,
  „valueOf“ oder „__proto__“ heißen. Vorher scheiterten sechs Regressionen; „__proto__“ ließ die
  tatsächliche React-Ansicht abstürzen. Nachher bestehen diese sechs Fälle in der vollständigen Suite.
- Das bisher vorgesehene Kontextbudget von 8.192 Tokens wird Ollama ausdrücklich als `num_ctx`
  übermittelt. Eine gemeinsame Konstante steuert Workspace-Vorbereitung und Streaminganfragen.
  Die Reservierung merkt sich dieses Budget auch für ihre Erneuerung und gemeinsame klassische Nutzer.
  Eine bereits klassische Reservierung bestätigt die Umstellung, bevor Workspace-Text gesendet wird.
  Klassische Reservierungen ohne Workspace behalten ihre bisherigen Standardanforderungen.
  TTL, Wiederholungsfristen, Routensperren, fremde unendliche Reservierungen und Audioaufnahme bleiben erhalten.
  Bestehende LM-Studio-Instanzen werden dadurch nicht umkonfiguriert.

Beide neuen nativen Regressionen scheiterten vorher an fehlenden Kontextwerten in den tatsächlichen
HTTP-Anforderungen; nachher bestehen sie einschließlich des tatsächlichen 60-Sekunden-Erneuerungstimers.
Dies sind synthetische Ollama-Protokolltests, kein echter Ollama-Modelllauf. Der separate echte
Referenzlauf verwendet das vorhandene E4B-Modell in LM Studio. Das explizite Kontextbudget beschreibt
unsere Anforderung und beweist keine beliebige tatsächliche Modellallokation.
Die Parameter sind in der [Ollama-API](https://docs.ollama.com/api/chat) und der
[Kontextdokumentation](https://docs.ollama.com/context-length) beschrieben.

Der Linux-Lauf auf dem früheren `d96491a` scheiterte zunächst korrekt an 10,186 Sekunden langen
virtuellen Spuren oberhalb der unveränderten Zehn-Sekunden-Grenze. Eine einzige Wiederholung auf
identischen Bytes bestand mit 9,053 / 9,057 Sekunden und 144.799 / 144.399 qualifizierten Tonsamples.
Sie bleibt ausdrücklich `passed-with-startup-gaps`, `completeCapture=false`. Der Fehlversuch bleibt erhalten;
dieser historische Erfolg ersetzt keine CI auf dem neuen Korrekturstand.

## Rücknahmen, Beschlüsse und englische Modulnamen: weitere Gegenprüfung

Die vollständige CodeRabbit-Runde auf `1768125` prüfte alle 100 vorgesehenen Dateien ohne
Verfügbarkeitslücke. Sie fand die fehlende englische Modulbezeichnung „Gesprächsverlauf“,
eine zu lockere Rücknahmeprüfung und einen nicht entfernten eigenen Testordner.
Die beiden früheren Kontext-/Layoutnamenkorrekturen waren dabei bereits bestätigt.

Der aktuelle Stand prüft Rücknahmesignale pro Originalzitat als ganze Wörter und in passenden
Mehrwortfolgen. Verneinungen, Fragen, Bedingungen und unklare Rückkehrformulierungen werden in den
abgesicherten Gegenfällen abgewiesen. „Nicht mehr“ allein reicht nicht: Eine ausdrücklich nicht
mehr benötigte Anforderung bleibt zulässig, „nicht mehr zurücknehmen“ wird nicht umgekehrt.
Die Gegenprüfung wies denselben Fehler für neue Beschlüsse nach; auch diese müssen nun ein
bejahendes, nicht nur ähnlich geschriebenes Beschlusssignal aus einem einzelnen Zitat tragen.
Die damalige Korrektur ließ die 27 Wörter der Erledigungsprüfung unverändert und ergänzte weitere
Negationen nur für die beiden neuen Prüfungen; die nachfolgende Runde korrigiert diese Lücke ebenfalls. Das ist ein begrenzter lexikalischer Schutz, kein Beweis der inhaltlichen Deutung.

Vorher akzeptierte die native Validierung 40 von 42 unzulässigen Rücknahmefällen und alle zwölf
zunächst geprüften falschen Beschlussfälle. Zusätzliche Fälle mit „nobody“, „noone“ und „keineswegs“
belegten neun weitere unzulässige Übergänge während der Gegenprüfung. Nachher bestehen insgesamt
90 Fallprüfungen: 48 abgewiesene und 20 gültige Rücknahmen auf Aufgaben/Beschlüssen sowie 15
abgewiesene und sieben gültige neue Beschlüsse. Abweisung bewahrt die gesamten serialisierten
Zustandsbytes; gültige Statusänderungen erhalten ursprünglichen Text und Belege.

Alle sieben bestehenden Modulnamen haben eine englische Übersetzung. Der vollständige
Komponententest prüft zusätzlich die tatsächlichen englischen Karten-, Verschiebe- und
Modulauswahlnamen. Eigene Layoutnamen bleiben wörtlich. Dieser Test behauptet keine echte
Browser-Popover-, Screenreader- oder native IPC-Prüfung.
Der Archivquellentest schließt seinen Dienst und entfernt nur seinen frisch erzeugten Testordner
mit dem vorhandenen geprüften Hilfsaufruf: kanonischer unmittelbarer Temp-Unterordner, eigener
Namenspräfix und kein symbolischer Link. Alte Testreste oder Nutzerordner werden dabei nicht gesucht.

## Zusätzliche Codex-Gegenfälle: Status und Aussagebereich

Die nächste Codex-Prüfung auf `89b85e8` fand drei gültige Probleme, keine saubere Runde:
Eine Entscheidung gegen eine Handlung wurde als verneinter Beschluss verworfen; „we’ll“ konnte
als aktuelle Rücknahme gelten; „niemals/keineswegs verschickt“ konnte eine offene Aufgabe abschließen.
Alle drei neuen Regressionstests scheiterten vor der Korrektur am tatsächlichen öffentlichen Validator.

Die Beschlussprüfung unterscheidet jetzt die bejahte Beschlussaussage von ihrem nachfolgenden
Infinitiv-/dass-Inhalt. „We decided not to release …“ und „Wir haben beschlossen, den Piloten nicht
zu starten“ sind zulässig; „nicht beschlossen“ und „beschließen nicht, …“ bleiben abgewiesen.
Englische gerade und typografische Apostrophe ergeben denselben geprüften Zukunftsmarker.
Die zusätzlichen Negationswörter gelten jetzt auch für Erledigung. Die Modellantwort bleibt weiterhin
an ein einzelnes vorhandenes wörtliches Zitat gebunden; der Schutz ist keine vollständige Sprachanalyse.

32 zusätzliche Fallprüfungen bestehen: elf Beschlussfälle, 14 Rücknahmefälle für Aufgaben/Beschlüsse
und sieben Erledigungsfälle. Zusammen mit den vorherigen 90 Fällen sind das 122 Statusgegenprüfungen.
Abgewiesene Übergänge bewahren die gesamten serialisierten Zustandsbytes; erlaubte Änderungen
bewahren den ursprünglichen Aufgabentext und sämtliche ursprünglichen Belege.
Der erste breitere Lauf war durch beschränkte lokale Datei-/Verbindungsrechte blockiert und wurde
kontrolliert beendet; er zählt nicht als bestandene Prüfung. Der anschließende vollständige Lauf
bestand mit den erforderlichen lokalen Ausführungsrechten, unveränderten Prüfregeln und gesperrten Paketen.

## Reviewabdeckung und Veröffentlichung

Alle 16 ursprünglichen Sourcery-Anmerkungen und sämtliche bisherigen CodeRabbit-/Codex-Befunde
bis einschließlich der vorherigen Runde haben individuelle Antworten und Dispositionen in PR 41.
Die in den jeweiligen Abschnitten beschriebenen neuen Befunde werden mit ihren Korrekturen beantwortet.
Zwei saubere Codex-Runden auf `ae6d062`, zwei auf `d96491a` und zwei auf `1768125`
bleiben historische Nachweise und werden nicht auf einen späteren Quellstand übertragen.

Der CodeRabbit-Lauf `bc2f7615-f3e6-4fdd-9ef3-f72621db511d` auf `ae6d062` veröffentlichte keine
neuen Inlinebefunde, konnte jedoch diesen zuvor 74.677 Byte langen Vermerk nicht vollständig lesen.
GitHub lieferte dafür keinen Dateipatch. Das ist **kein sauberer vollständiger Reviewabschluss**.
Die ausführliche Fassung bleibt erhalten; dieser kompakte Vermerk ersetzt ihre Wiederholungen.

Der vollständige Basis-bis-Head-Diff umfasst 102 Dateien, davon 100 nach den bisherigen
zwei Anbieterfilter-Ausnahmen für Lockdateien. Drei kleine neue Test-/Typdateien wurden fachlich
gebündelt, ohne neue Filter oder Prüfregeländerungen. Zwei isolierte Markerfälle wurden in vier
bestehende vollständige Interaktionsfälle übernommen; damals deshalb 368 statt zuvor 370 Tests.
Die sechs Layoutregressionen ergaben zuletzt 374; der neue Modulnamenfall erhöht die aktuelle Anzahl auf 375.
Die ursprüngliche Fünf-Sekunden-Frist des verschobenen Vorschautests bleibt ausdrücklich erhalten.

Vor Veröffentlichung werden Git-Root, Branch und bisheriger Remote-Head geprüft; normaler Fast-forward.
Alle Dateien des tatsächlichen unveränderlichen Remote-ZIP werden mit Git-Blobs und lokalen Bytes verglichen.
Alte Belege und Gitfassungen bleiben erhalten. Abhängigkeiten, Datenschutz-/Reviewer-Konfiguration,
Dokumentationsgrenze von 80 Prozent und Version bleiben unverändert.
Frische unabhängige Reviews und aktuelle CI müssen am selben finalen Head tatsächlich abschließen.
Alte oder leere Security-Flags sind kein aktueller formaler Securitynachweis.

## Verbleibende Grenzen

Physische Aufnahme-/Berechtigungsprüfungen und der zweistündige Gerätelauf wurden auf später verschoben.
Keine neue echte Parakeet-Dauer-, GPU-, unabhängige installierte Prozess-, Signatur-,
Notarisierungs- oder forensische Speicherprüfung wird aus den obigen Tests abgeleitet.
Bekannte gesperrte Abhängigkeitsrisiken bleiben in RUST_DEPENDENCIES dokumentiert.
Diese Nacharbeit erzeugt keinen installierten Programmstand und keinen Release.
