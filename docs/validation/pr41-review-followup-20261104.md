# PR 41: Validierungsstand

Stand: 5. November 2026. Version: **1.1.0-beta.1**, unverändert.
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
| Rust-Workspace / alle Targets | Auf `5b20e40` bestanden 551; native Quellen/Tests/Abhängigkeiten sind seitdem bytegleich, deshalb lokal nicht erneut ausgeführt. Darunter 155 Notizen- und 34 Providerprotokolltests; drei bestehende besondere Einstiege regulär ignoriert. Die bisherigen Kapazitäts-, Zahlen- und Wiederöffnungsfälle bleiben erhalten. Zwei neue Fälle prüfen Zahl und Verbindlichkeit bei unterschiedlicher Reihenfolge gültiger Fristbelege. |
| Rustformat / Clippy | Auf dem bytegleichen nativen Stand `5b20e40` bestanden, lokal nicht erneut ausgeführt; gesperrte Abhängigkeiten und Warnungen als Fehler. |
| Oberfläche | 391 Tests, TypeScript, Produktionsbuild und vollständiger Formatcheck unter src frisch bestanden. Vier neue vollständige App-Fälle prüfen später eingehende Agentenanfragen und Geräteverluste in Deutsch/Englisch. Ein zusätzlicher tatsächlicher Bridge-/Shadow-DOM-Fall prüft, dass wechselnde globale Hinweise den gewählten Tab und die laufende Oberfläche erhalten. |
| Projektfreigabe | `verify.ps1` vor jeder Bereitstellung: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen bestanden. Der folgende Quellupload bleibt an einen abschließenden erfolgreichen Lauf gebunden. |
| Echte lokale Referenzen Deutsch und Englisch | Auf `5b20e40`, vor der anschließenden reinen UI-Fehlermeldungsformatierung: Deutsch 116.6886547 s (0 reguläre Validierungswiederholungen), Englisch 127.5065369 s (0 Wiederholungen). Beide bestanden positive Aufgabe, unverbindliche Aussage, Impulserhalt, menschlichen Status nach Protokolländerung und bytegleiche Wiederöffnung; englische Aufgaben behalten das wörtliche „I’ll“. Unverändert ist eine vollständig validierte Wiederholung je Anfrage zulässig; null Wiederholungen sind keine Freigabeschwelle. Die vorherigen Läufe auf `7fb1ad2` mit DE 106,8706213 s und EN 111,7106681 s bleiben historische Nachweise. |
| Frühere lokale Referenzen | Deutsch 100,9989873 s und Englisch 125,0518623 s auf dem Elternstand `3d4d013`, jeweils ohne Validierungswiederholung. Die deutsche Zusage enthält tatsächlich „jedenfalls“. Diese Läufe werden nicht als erneut ausgeführt ausgegeben. |
| Langsammodell / zwei Spuren | Auf `5b20e40` ausdrücklich ausgeführter besonderer Test; bytegleicher nativer Stand, hier nicht wiederholt: 244,23 s Laufzeit, 210 s kontrollierte SSE-Antwortpause, nächster Refresh nach 210,0402066 s, maximal eine Modellanfrage, ein erhaltener Impuls. Samples je Spur 3749440/3749440, geteilt durch 16.000 Hz = 234,34/234,34 s. Beide dauerhaften PCM-Schreiber liefen nach dem Auswertungsstopp weiter. Tatsächlicher nativer Worker mit kontrollierter Antwort und generiertem PCM, keine Geräte/ASR oder Zweistundenprüfung. Frühere Läufe bleiben historische Belege. |
| Browsergegenprüfung | Auf `5b20e40` acht Abläufe mit tatsächlicher vollständiger App einschließlich Workspace/Shadow-DOM bei 390×844 dunkel und 740×900 px hell, jeweils Deutsch/Englisch: späte Agentenanfrage sichtbar, Prüfen erreichbar, bestehende Ablehnung versendet exakt einmal false; Geräteverlust während synthetischer Aufnahme sichtbar und schließbar. Keine Aufnahme-/Modellstarts oder -stopps, keine Konsolen-/Seitenfehler und kein horizontaler Überlauf. Bestehender Edge 156 / Playwright 1.57 mit synthetischem IPC-Adapter, kein nativer Geräte-/Agentenprozess oder echter Löschlauf. Sechs Pollfehlerabläufe auf `f728081` und frühere Sidebarprüfung bleiben historische Nachweise. |

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

## Neue Aufgaben und schmale Seitenleiste: weitere vollständige Runde

Der vollständige CodeRabbit-Lauf auf `1e73d8f` prüfte wieder alle 100 vorgesehenen Dateien ohne
Verfügbarkeitslücke und fand eine zu schmale Sprachauswahl in der aufgeklappten Seitenleiste.
Der ebenfalls erwähnte horizontale Aufnahmestatus war nicht reproduzierbar: Eine spätere Regel
setzte ihn bei derselben Fensterbreite bereits auf Spaltenrichtung. Die eigentlichen 68-Pixel-Sonderregeln
für Sprachauswahl und Statuskarte gelten jetzt nur im ausdrücklich eingeklappten Zustand.
Die Browsergegenprobe bestätigt die Korrektur; eingeklappt bleiben die bisherigen Breiten erhalten.
Zwei zusätzliche Hygieneaussagen im Risikotext wurden auf Nachfrage vom Anbieter nach Quellenprüfung
ausdrücklich zurückgenommen: Die Listener nutzen freie Ports; die genannten Festport-URLs sind reine
Metadaten. Der ausdrücklich langsame Test behält absichtlich synthetische Belege. Daraus folgt kein
neuer Cleanupauftrag und keine Löschung früherer Quellen oder Belege.

Die parallele Codex-Prüfung fand einen echten weiteren Fehler: „Ich übernehme den Bericht nicht“
konnte eine neue Aufgabe erzeugen. Zwei neue öffentliche Validatorregressionen scheiterten vorher.
Nachher bestehen elf Zusagen-/Ablehnungsfälle im Workspace und zehn in der klassischen Auswertung.
Sechs früher gespeicherte, wörtlich belegte klassische Ergebnisse bleiben bei Wiederöffnung bytegleich;
neue Erzeugungsregeln werden nicht rückwirkend auf alte Checkpoints angewendet.

Die gemeinsame Prüfung unterscheidet neue Verpflichtungen von bestehenden Aufgabenstatusänderungen.
Die neuen Ablehnungsregeln ergänzen ausschließlich neue Verpflichtungen; alle früheren Quellen-, Zahlen-,
Metadaten- und Unsicherheitsprüfungen bleiben erhalten. Abschluss und Rücknahme benötigen weiterhin ihre
eigenen geprüften Originalbelege. Die vorhandene enge Ausnahme „nicht vergessen / not forget“ bleibt erhalten.
Der Zwischenentwurf, der auch gültige „nicht mehr benötigt“-Rücknahmen abwies, wurde durch die vorhandene
Regression erkannt und vor Veröffentlichung korrigiert. Die 122 früheren Statusfälle bestehen weiter.

Die zwei lediglich nummerierten Validator-Testmodule sind unter Erhalt aller Testkörper und Fallzahlen
zusammengeführt. Dadurch bleibt auch die gemeinsame Produktionskorrektur innerhalb der unveränderten
100-Dateien-Anbietergrenze: 102 tatsächliche PR-Dateien, weiterhin nur die zwei bisherigen Lockdatei-Filter.
Keine Prüfkonfiguration, Frist oder Dokumentationsgrenze wurde abgeschwächt.

## Vererbte Bestätigungen: Reihenfolge vor und nach Wiederöffnung

Die nächste vollständige CodeRabbit-Runde auf `aa63003` prüfte alle 100 vorgesehenen Dateien ohne
Verfügbarkeitslücke und fand einen gültigen weiteren Fehler. Beim Ersetzen eines bestehenden Eintrags
entfernte der Import den Eintrag und hing ihn hinten an; das Journal ersetzte denselben Eintrag an
seiner ursprünglichen Position. Laufende Ansicht und Wiederöffnung konnten dadurch abweichen.
Die beiden damaligen sauberen Codex-Prüfungen und grünen Ziel-CI-Läufe bleiben historische Nachweise;
die vollständige CodeRabbit-Runde war wegen dieses Befunds nicht sauber.

Die neue Regression zeigte vor der Korrektur tatsächlich [zweiter, dritter, erster Eintrag] im Speicher
gegen [erster, zweiter, dritter Eintrag] nach dem Lesen. Der Import ersetzt bestehende Kennungen jetzt
an ihrer Position und hängt nur neue Kennungen an. Vier Varianten prüfen den ersten, mittleren und
letzten von drei Einträgen sowie einen zusätzlichen neuen Eintrag bei umgekehrter Herkunftsreihenfolge.
Geprüft werden ganze Entitätsfolgen, identische serialisierte Zustände, menschliche Änderungszeiten,
unveränderte vorherige JSON-Dateien, wiederholter Import ohne neue Revision und erneutes Öffnen des Writers.
Alle 149 Notizen-Bibliothekstests bestehen, einschließlich der neuen Regression.

Der erneut zusätzlich genannte angeblich regressionsverdeckende Modellserver-Port war kein belegter
Fehler. CodeRabbit verfolgte den konkreten Anzeigetest und seine Initialisierung bis zum Transport:
Die URL ist gespeicherte Metadaten; kein Request wird versendet. Der Anbieter nahm auch diese Aussage
ausdrücklich zurück. Es wurde keine Festport-Ausnahme, Netzwerkausnahme oder Prüfschwelle eingeführt.

## Zusagen trotz verneinter Redewendung

Die vollständige CodeRabbit-Runde auf `5c7d6ab` prüfte alle 100 vorgesehenen Dateien und fand
einen weiteren echten Fehler: „Kein Problem, ich übernehme den Bericht“ wurde allein wegen „kein“
als unverbindlich verworfen. Zwei echte Vorherfehler belegen dies im klassischen und erweiterten
Auswertungsweg. Die gemeinsame Prüfung erkennt nun die festen Redewendungen „kein Problem“,
„keine Sorge“ und „not a problem“ als ganze Wörter. Unabhängige Ablehnungen bleiben erhalten;
ein längeres Wort wie „Sorgepflicht“ wird nicht versehentlich herausgeschnitten.

Auf diesem Zwischenstand prüften die öffentlichen Regressionen 20 Workspace- und 19 klassische Fälle,
darunter 18 neue Varianten mit Großschreibung, Satzzeichen, positiver Zusage, nachfolgender Ablehnung
und Wortgrenzen. Abgewiesene Workspace-Antworten lassen den ganzen Zustand byteunverändert.
Zehn synthetische ältere klassische Checkpoints bleiben beim strikten Lesen bytegleich;
die neue Generierungsregel filtert keine gespeicherten Ergebnisse nachträglich.
Alle 545 Rusttests bestehen einschließlich der 149 Notizen- und 34 Providerprotokolltests.

Der zusätzliche vage Testhygienehinweis wurde nach genauer Prüfung der betreffenden Replaytests,
temporären Verzeichnisse, netzwerklosen Metadateninitialisierung und begrenzten Fixturebereinigung
ausdrücklich zurückgenommen. Er war kein weiterer belegter Fehler. Die damaligen zwei sauberen
Codex-Runden bleiben historische Nachweise; die vollständige CodeRabbit-Runde war nicht sauber.
LanguageTool konnte dort nicht jeden Textchunk prüfen; diese Hilfsanalyse wird nicht als vollständig
ausgegeben. Es fehlte jedoch kein Dateipatch in der eigentlichen vollständigen Quellprüfung.

## Typografischer Apostroph in englischer Ablehnung

Eine eigene weitere Gegenprüfung reproduzierte im klassischen Weg einen echten Fehler:
„I won’t email the report“ wurde mit typografischem Apostroph als Aufgabe zugelassen,
während die bereits unterstützte Schreibweise „won't“ abgewiesen wurde. Der Workspace wies
beide neuen Ablehnungsfälle bereits vor dieser Korrektur ab; ihm wird kein Vorherfehler zugerechnet.

Nur der temporäre Prüftext vereinheitlicht jetzt den typografischen Apostroph.
Originalzitate, Quellen und gespeicherte Ergebnisse werden dadurch nicht umgeschrieben.
Die Regression prüft jetzt 22 Workspace- und 22 klassische Fälle, darunter fünf zusätzliche
Ablehnungs-/Kontrollvarianten. „I won’t forget to email the report“ bleibt als positiver Gegenfall
zulässig. Zwölf synthetische ältere klassische Checkpoints behalten ihre exakten JSON-Bytes.
Nach dem echten Vorherfehler bestehen alle 149 Notizen- und 545 Workspace-/All-Target-Rusttests.
Die Standardreferenzen mit echtem lokalen Modell und die gezielten Apostroph-Regressionsfälle
sind getrennte Nachweise; keine neue sprachliche Vollständigkeitsgarantie wird daraus abgeleitet.

Die zwei sauberen Codex-Runden auf `b197aaa` bleiben historische Nachweise. Eine neue vollständige
CodeRabbit-Runde war dort noch nicht angefordert; Reviews müssen auf dem letzten Korrekturstand
erneut tatsächlich abschließen.

## Typografische englische Zusagen und weitere Ablehnungen

Die Codex-Prüfung auf `be47b8b` zeigte einen echten vorgelagerten Zulassungsfehler:
„I’ll email the report“ erreichte die Aufgabenphase nicht; der Abschnitt galt danach bereits
als verarbeitet. Nur der temporäre Zulassungstext vereinheitlicht jetzt den Apostroph.
Ein vollständiger öffentlicher Worker-/HTTP-/SSE-Test mit acht Varianten prüft I/we,
gerade/typografische Apostrophe und vier ausdrückliche Ablehnungen. Vorher scheiterte tatsächlich
die Phasenfolge am ersten typografischen I’ll-Fall; nachher bleiben Quote, Quellen und gespeicherter
Zustand beim erneuten Öffnen exakt erhalten. Die echte englische Modellreferenz prüft zusätzlich
das wörtliche I’ll-Zitat in jedem Auswertungsfenster, ohne die gespeicherte Evidenz zu normalisieren.

Die vollständige CodeRabbit-Prüfung desselben Stands fand zwei weitere echte Fehler:
„I don’t promise“/„I can’t take“/„I cannot write“ konnten klassische Aufgaben werden;
„We can’t promise that we will send“ passierte auch die Workspace-Zulassung.
Die gemeinsame Ablehnungsprüfung berücksichtigt nun verbleibende n't-Kontraktionen und cannot.
Klare Zusagen nach festen Redewendungen wie „Don't worry“, „I can't wait, I will…“ oder
„It doesn't matter, I will…“ bleiben erhalten; eine unabhängige spätere Ablehnung blockiert weiter.
Die beiden öffentlichen Testgruppen enthalten jetzt jeweils 41 Workspace-/klassische Fälle.
24 abgewiesene klassische Varianten werden außerdem als ältere synthetische Checkpoints
streng bytegleich gelesen: keine nachträgliche Filterung bestehender Auswertungen.

Zwei neue positive Gegenfälle entdeckten vor Veröffentlichung, dass die zunächst zu breite
Kontraktionsprüfung auch klare Zusagen nach „I can't wait“ abwies. Die endgültige Fassung
behebt dies mit ganzen festen Redewendungen und gepaarten unabhängigen Ablehnungen.
Ein zusätzlich fehlgeschlagener Zwischenlauf beruhte auf einem neuen positiven Testtext ohne
die verlangte ausdrückliche Zusage; der Kontrolltext wurde entsprechend korrigiert.
Dieser Fixturefehler wird nicht als weiterer Produktfehler oder sauberer Abschluss ausgegeben.

Der zweite CodeRabbit-Befund betraf vorübergehende Statusfehler in der Oberfläche:
Sie blieben nach Erholung sichtbar und konnten einen wichtigen Fehler einer menschlichen Aktion
überschreiben. Der Pollstatus hat jetzt einen eigenen Fehlerzustand. Ein erfolgreicher Poll löscht
nur diesen; fehlgeschlagene Änderungen und Bestätigungen bleiben sichtbar und werden nicht
automatisch erneut versandt. Vier echte Vorherfehler sind danach behoben, zusätzlich durch sechs
Browserabläufe am tatsächlichen UI-Code bestätigt. Aufnahmeablauf und Intervall bleiben unverändert.

Die vollständige `be47b8b`-Runde deckte alle 100 vorgesehenen Dateien ab und war wegen dieser
zwei CodeRabbit-Befunde nicht sauber. Den zusätzlichen vagen Satz über angeblich nötige
Test-/Hilfstextbereinigung nahm CodeRabbit nach konkreter Rückfrage ausdrücklich zurück;
es wurde dafür kein weiterer belegter Fehler benannt. Alle Risiko-/Zusatz-/outside-diff-Texte
und Antworten wurden gelesen. Die drei Korrekturen benötigen neue Reviews und CI auf demselben
veröffentlichten Stand; frühere saubere Codex-Runden und grüne CI bleiben historische Belege.

## Statusbelege ohne unbegrenztes Anwachsen

Die nächste Codex-Runde auf `f728081` fand einen echten Fehler bei aufeinanderfolgenden
Statusänderungen. Fünf neue unklare Bewertungen eines Impulses sammelten sechs Belege;
die anschließend gültige besprochen-Bewertung scheiterte als siebter Beleg an EvidenceCount.
Die öffentliche Persistenzregression reproduzierte denselben Überlauf auch bei Aufgaben.

Der aktuelle Eintrag behält jetzt die Belege seiner unveränderten Textfassung aus dem bestehenden
Verlauf und nötigenfalls einen wörtlichen Nachweis je übernommener Zuständigkeit/Frist.
Die neu eingereichte Statusaussage muss weiterhin eigenständig gültig sein. Frühere Statuszitate
bleiben unverändert in den historischen Ereignissen und Revisionsdateien. Die Grenze bleibt sechs;
wenn tatsächlich mehr Belege erforderlich wären, scheitert die Änderung weiterhin vollständig.
Die zusätzliche Abschlussprüfung prüft auch den zusammengesetzten Text samt übernommenen Metadaten.

Zwei Tests mit vier Varianten durchlaufen Erzeugung, optionalen Textwechsel, fünf Statusänderungen
und abschließende Erledigung/Besprechung. Nach jedem Schritt wird dauerhaft gespeichert, der Writer
geschlossen und der gesamte serialisierte Stand identisch wieder geöffnet. Originalzitat, Zahlen,
Zuständige/Frist, jede ältere Revisionsdatei und alle historischen Statusquellen bleiben erhalten.
Am Ende genügen den kontrollierten Fällen genau zwei aktuelle Belege: Textfassung und neuester Status.
Leere, fremde, nichtwörtliche, sieben eingereichte Belege sowie fehlender Textquellkontext werden
weiterhin ohne Zustandsänderung verworfen. Es wurde kein Speicherformat oder Quellenbeleg erfunden.

Frühere neue Test-Zwischenläufe kompilierten zunächst nicht bzw. übergaben Quellen doppelt an
den Test-Commit. Nur der danach tatsächlich beobachtete EvidenceCount-Vorherfehler wird als
Produktreproduktion gezählt. Die endgültige vollständige Suite besteht; keine Warnungsregel wurde
für eine zwischenzeitlich von Clippy beanstandete verschachtelte Bedingung ausgesetzt.

## Automatische Modellvorauswahl und bewusste Batchaufnahme

Der vollständige CodeRabbit-Lauf auf `f728081` prüfte alle 100 vorgesehenen Dateien und fand
eine weitere reale Bedienungsstörung: Ein zuerst gelistetes Batchmodell wurde automatisch gewählt,
obwohl ein Streamingmodell vorhanden war; dadurch blieb die Agentenfreigabe unnötig gesperrt.
Die anfängliche automatische Auswahl bevorzugt jetzt ein kompatibles installiertes Streamingmodell
und nimmt andernfalls weiterhin das erste verfügbare Modell. Leere Kataloge und die bestehende
Auswahlwache bleiben unverändert. Eine bewusste Batchauswahl bleibt bei der Katalogaktualisierung
erhalten; manuelle Abschnittsaufnahme bleibt möglich, Agentenaufnahme verlangt weiter Streaming.

Die verstärkte vollständige App-Regression scheiterte vorher tatsächlich am gesperrten Freigabeknopf,
während ihr Gegenfall ohne Streamingmodell bereits bestand. Beide Fälle bestehen nach der Korrektur,
einschließlich Modellwert, manueller Batchwahl, Katalogaktualisierung und erneuter Streamingwahl.
Die ersten zwei Teststarts ohne nötigen temporären Dateizugriff führten keine Tests aus und sind
keine Produktfehler oder Testnachweise. Es wurden keine installierten Modelle oder Geräte verwendet.

CodeRabbits zusätzlich behauptete „zwei Testhygiene-Punkte“ hatten keine konkreten aktuellen
Fundstellen. Auf Rückfrage nahm der Anbieter beide ausdrücklich als unbelegt zurück.
Der Lauf war wegen des tatsächlichen Vorauswahlfehlers dennoch nicht sauber. Zwei neue saubere
Codex-Codeprüfungen, eine neue vollständige CodeRabbit-Runde und aktuelle CI bleiben am selben
abschließend veröffentlichten Head erforderlich; frühere Reviews werden nicht umgehängt.

## Kapazität bei sechs ursprünglichen Belegen

Die Codex-Runde auf `255d878` fand einen weiteren echten Randfall: Ein bereits mit sechs
verschiedenen Zitaten angelegter Impuls konnte seine erste neue Statusbewertung nicht aufnehmen.
Die bis dahin korrigierte Wiederholungsfolge genügte dafür nicht. Drei echte Vorherfehler traten
mit sechs ursprünglichen Belegen bei Impuls und Aufgabe sowie verteilten Zahlenbelegen auf.

Die Übernahme auf `7fb1ad2` wählte eine kleine zulässige Teilmenge der Belege der unveränderten Textfassung
innerhalb des gemeinsamen Budgets aus. Mindestens ein zugehöriger Originalbeleg bleibt dabei;
alle eingereichten neuen Statuszitate werden unverändert behalten und eigenständig geprüft.
Übernommene Zuständige und Fristen benötigen weiterhin ihre wörtlichen Nachweise. Höchstens
64 mögliche alte Textbelegkombinationen werden anhand derselben Quellen-, Zahlen- und
Metadatenprüfungen bewertet. Keine eingereichte fehlerhafte Quote wird zur Annahme weggelassen.
Diese technische Auswahl beweist weiterhin keine inhaltliche Deutung der paraphrasierten Aussage.

Die vollständige ursprüngliche Belegsammlung bleibt in den historischen Ereignissen und Dateien.
Acht dauerhafte Wiederöffnungsvarianten prüfen die einzelnen und sechs ursprünglichen Zitate,
optionale neue Textfassungen sowie sechs spätere Statusänderungen. Die zusätzliche Zahlenregression
verlangt zwei getrennte alte Zahlenbelege plus neuen Statusbeleg; drei aktuelle Belege genügen.
Benötigt der Text hingegen tatsächlich sechs unabhängige Zahlenbelege zusätzlich zum neuen Status,
bleibt die gesamte Antwort unverändert abgewiesen: keine Erhöhung des Sechs-Belege-Limits.

Auf `7fb1ad2` bestanden 153 Notizen- und 549 Workspace-/All-Target-Rusttests. Oberfläche, Abhängigkeiten,
Audio-, Transport-, Prompt- und Modellreferenz-Fixture-Dateien sind gegenüber `255d878` bytegleich.
Die tatsächlichen lokalen Referenzen wurden nach der nativen Änderung trotzdem erneut ausgeführt.
Auf `255d878` gab es keine neue vollständige CodeRabbit-Anforderung; der letzte vollständige Lauf
gehört ausdrücklich `f728081`. Aktuelle saubere Reviews und CI müssen am neuen finalen Head enden.

## Reihenfolge übernommener Metadatenbelege

Eine zusätzliche lokale Gegenprüfung nach zwei sauberen Codex-Codeprüfungen auf `7fb1ad2`
fand einen echten Reihenfolgefehler. Eine gültige Aufgabe enthielt bereits zwei Fristzitate:
ein Zitat mit „15. Mai“ und das passende verbindliche Zitat mit „5. Mai“. Die vorherige
greifende Auswahl übernahm nur das zuerst passende Teilwort-Zitat. Bei einer späteren
Statusänderung wurde deshalb Zahl 5 nicht belegt, obwohl der richtige Originalbeleg vorhanden war.
Entsprechend konnte ein zuerst angeführter unverbindlicher Fristsatz den richtigen verbindlichen
Beleg verdecken. Zwei tatsächliche Vorherfehler sind erhalten; ihre umgekehrten Reihenfolgen
bestanden bereits. Die Ausgangszustände sind durch den regulären Ablauf gültig angelegt.

Die Auswahl berücksichtigt jetzt alle geeigneten alten Text- und aktuellen Metadatenbelege
gemeinsam: höchstens zwölf Kandidaten, höchstens 4096 Kombinationen, früh beschnitten durch
das unveränderte Sechs-Belege-Budget und bereits gefundene kleinere gültige Kombinationen.
Jede Auswahl durchläuft dieselben vollständigen Quellen-, Zahlen-, Metadaten- und
Verbindlichkeitsprüfungen. Alle eingereichten neuen Zitate und mindestens ein zugehöriger
Originaltextbeleg bleiben erforderlich; vollständige historische Ereignisse bleiben erhalten.
Echte nicht darstellbare Kombinationen werden weiterhin atomar abgewiesen.
Nachher bestehen alle 155 Notizen- und 551 Rusttests sowie die erneut ausgeführten echten
lokalen deutschen und englischen Referenzen. Die technische Prüfung beweist keine inhaltliche Deutung.

## Globale Hinweise in der erweiterten Meetingansicht

Der vollständige CodeRabbit-Lauf auf `7fb1ad2` fand einen gültigen Bedienungsfehler:
Der frühe Rücksprung in die erweiterte Ansicht übersprang die bisherigen Agenten-,
Geräte- und Löschhinweise. Nach Öffnen dieser Ansicht eingehende Agentenanfragen
konnten deshalb unbemerkt ablaufen; reine Listen-Geräteverluste blieben unsichtbar.

Ein kompakter Hinweis macht die Agentenanfrage jetzt sichtbar und führt zur bisherigen
Prüfung/Freigabe/Ablehnung. Bestehende Gerätehinweise und Löschstatus werden mit denselben
Komponenten und Aktionen auch im begrenzten, scrollbar bleibenden Hinweisbereich der
erweiterten Ansicht angezeigt. Die App genehmigt nichts automatisch und startet oder stoppt
dadurch weder Aufnahme noch Auswertung. Ohne Hinweise entsteht kein zusätzlicher Platzhalter.

Vier vollständige App-Regressionen scheiterten vorher tatsächlich an fehlenden Hinweisen
oder Aktionen und bestehen nachher. Ein zusätzlicher Bridge-Fall prüft den unveränderten
aktiven Tab beim Einblenden, Wechseln und Entfernen eines synthetischen Löschstatus.
Alle 385 UI-Tests, TypeScript/Build/Format und acht tatsächliche App-Browserabläufe bestehen.
Frühere Fixture-, Typprüfungs- und private Favicon-Fehler bleiben als Zwischenstufen erhalten;
sie sind keine weiteren Produktfehler. Kein echter Lösch-, Geräte- oder installierter Agententest.

Der vollständige vorherige CodeRabbit-Lauf deckte alle 100 vorgesehenen Dateien ab,
war wegen dieses Hinweismangels aber nicht sauber. Den zusätzlich behaupteten separaten
„minor test cleanup“-Punkt nahm der Anbieter auf Rückfrage ausdrücklich als unbelegt zurück.
Die zwei sauberen Codex-Codeprüfungen auf `7fb1ad2` bleiben historische Nachweise.
Die Linux-CI desselben alten Heads scheiterte tatsächlich: beide virtuellen Tonspuren
hatten 10.066 ms statt höchstens erlaubten 10.000 ms. Die darüber hinausgehende Ursache
ist nicht bewiesen. Kein erneuter alter Lauf wurde angefordert; der neue Head benötigt
neue erfolgreiche Ziel-CI. Dauergrenzen, PCM und Verifier bleiben unverändert.

## Einheitliche verständliche Fehlermeldungen

Der vollständige CodeRabbit-Lauf auf `5b20e40` fand noch eine sichtbare Unstimmigkeit:
Modellaktionen, menschliche Statusänderungen, Notizveröffentlichung und die im Statuspoll
angestoßene Kontextveröffentlichung zeigten bei geworfenen Error-Objekten den technischen
Vorsatz „Error:“. Die eigentliche Statusabfrage verwendete bereits nur die Meldung.
Alle Wege verwenden jetzt dieselbe Darstellung: Error.message für Error-Objekte,
sonst weiterhin String-Konvertierung für native Zeichenketten und andere Rückweisungswerte.
Fehlerkanäle, Sichtbarkeitsdauer, Daten, Wiederholungsregeln und Aufnahmezustand bleiben gleich.

Drei echte Vorherfehler sind durch die tatsächlich gerenderten Aktions-, Bestätigungs-
und Notizwege belegt; sieben Kontrollfälle bestanden bereits. Alle zehn Fälle bestehen
nachher. Die Notizregression erhält ihren Entwurf auch nach dem erneuten Pollversuch;
fehlgeschlagene menschliche Bestätigungen werden nicht automatisch wiederholt.
Zeichenketten und Zahlen als Rückweisungen bleiben lesbar. Alle 391 UI-Tests,
TypeScript, Produktionsbuild und vollständige Frontendformatprüfung bestehen frisch.
Die acht Browserabläufe und echten DE/EN-Referenzen des Elternstands sind nicht wiederholt;
native Implementierung, Prompts, Transport, Referenzfixture und Abhängigkeiten sind bytegleich.

Die beiden zusätzlich behaupteten Testprobleme mit temporären Dateien und lokalen
Modellservern nahm CodeRabbit auf Rückfrage ausdrücklich als unbelegt zurück.
Das behebt oder versteckt keinen Testfehler. Der vollständige Lauf deckte alle 100
vorgesehenen Dateien ab, war wegen des gültigen Fehlermeldungsbefunds aber nicht sauber.
Zwei saubere Codex-Codeprüfungen auf `5b20e40` bleiben historische Nachweise;
neue Reviews und aktuelle CI müssen am endgültigen Head tatsächlich abschließen.

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
Die sechs Layoutregressionen und der Modulnamenfall ergaben zuletzt 375; vier Pollfehlerregressionen ergaben 379; der zusätzliche Batch-Fallback-Gegenfall erhöhte die damalige Anzahl auf 380; die fünf zusätzlichen Hinweis-/Bridge-Fälle ergaben 385; sechs zusätzliche Varianten für Error-/String-/Zahl-Rückweisungen ergeben aktuell 391.
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
