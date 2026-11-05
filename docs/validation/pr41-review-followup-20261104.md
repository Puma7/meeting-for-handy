# PR 41: Validierungsstand

Stand: 5. November 2026. Version **1.1.0-beta.1**, unverändert.

Die aktuelle Nacharbeit verhindert, dass ausdrücklich noch unvollständige
Arbeit als erledigte Aufgabe veröffentlicht wird. „Fast abgeschlossen“,
„beinahe abgeschlossen“ und vergleichbare begrenzte Fortschrittsmarker
reichen für einen terminalen Status nicht aus. Tatsächlich vollständige
Abschlüsse bleiben zulässig; englisches „fast“ im Sinn von „schnell“ bleibt
von der deutschen Bedeutung unterscheidbar.

Alle vorherigen Korrekturen einschließlich einmaliger Workspace-Projektion,
Quellenidentität beim Modellwechsel, Kartenmenüs, Kontrast, Ollama-Diagnostik
und des englischen Referenzpatches `93335102ed` bleiben enthalten.
Die [ausführliche bisherige Fassung](https://github.com/Puma7/meeting-for-handy/blob/90becddbaabcebe4273732dd4709043a43133168/docs/validation/pr41-review-followup-20261104.md)
und deren verlinkte Detailhistorie bleiben unveränderlich in Git.
Private Rohbelege, fehlgeschlagene Läufe und alte Reviewfassungen bleiben erhalten.
Historische Nachweise werden nicht als erneut ausgeführt ausgegeben.
Der vollständige TEST_REPORT bleibt gegenüber dem PR-Basisstand byteunverändert.

## Verhalten und Daten

Die klassische Transkriptansicht und der optionale Advanced Meeting Assistant
bleiben getrennt nutzbar. Beide verwenden die gemeinsamen Modelleinstellungen.
Die erweiterte Ansicht bietet anpassbare Karten und Layouts, Vorbereitung,
Notizen, wörtliche Originalbelege und begrenzte lokale Audiovorschau.
Das Transkript bewahrt Originaltext; der Gesprächsverlauf zeigt Änderungen.

Parakeet Ultra verarbeitet während der Aufnahme abgeschlossene Abschnitte
beider Spuren nacheinander. Ruhige Stellen können Abschnittsgrenzen bilden;
Kontextüberlappung ist begrenzt. Audio wird unabhängig gesichert. Nur dauerhaft
bestätigter Text erreicht die Auswertung. Vorübergehende Erkennungsfehler erlauben
höchstens drei aufeinanderfolgende Versuche ohne gesicherten Fortschritt.
Ein dauerhaft gespeicherter Abschnitt erneuert dieses Budget. Modell-, Protokoll-,
Integritäts- und Speicherfehler bleiben unmittelbar terminal.

Langsame Textmodellläufe werden durch Intervallticks nicht abgebrochen.
Neue Arbeit wird gesammelt; dieselbe Sitzung startet keine überlappenden Läufe.
Ein ausdrücklicher Auswertungsstopp beendet nicht die Audiosicherung.

Automatische erweiterte Auswertung bleibt lokal. Cloud benötigt ausdrückliche
Textfreigabe je Auswertung und erhält weder Audio noch lokale Pfade oder
Sitzungsobjekte. Weiterleitungen, System-Proxys und automatischer Anbieterwechsel
bleiben gesperrt. Private Vorbereitung bleibt privat. Quellen, Transkriptstände,
Modellfassungen und menschliche Bestätigungen bleiben versioniert erhalten.
Schlüssel werden nicht protokolliert oder als Klartext gespeichert.

## Aktuelle Korrekturen und echte Vorherfälle

### Unvollständiger Fortschritt ist kein Abschluss

Der [Codex-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4486035608)
auf `90becdd` war gültig. Die echte Validatorregression reproduzierte zehn
falsche terminale Annahmen: fast, beinahe, nahezu, teilweise, weitgehend sowie
almost, nearly, partly, partially und mostly.
Die betreffenden wörtlichen Quellen konnten vorher eine vorhandene offene
Aufgabe auf completed setzen.

Die Abschlussprüfung weist diese begrenzten Fortschrittsmarker jetzt zurück.
Deutsches „fast“ verlangt dabei ein deutsches Prädikat; ein englischer Satz
wie „I have completed the report fast.“ bleibt ein zulässiger vollständiger
Abschluss. Vier zusätzliche vollständige Gegenfälle bestanden schon vorher
und bestehen weiterhin. Die bestehenden Negations-, Frage-, Bedingungs-,
Zahlen- und Belegprüfungen bleiben erhalten.

Die vorhandene Regression wurde um diese 14 Varianten erweitert.
Die Zahl der Testfunktionen steigt deshalb nicht.
Sie benutzt die tatsächliche Validierung und dauerhafte Checkpoints.
Abgelehnte Statusantworten lassen Aufgabe, Zitate und alten Checkpoint
byteunverändert und erzeugen keine zweite Revision.
Zulässige Abschlüsse bewahren wörtliche Quellen und stimmen nach wiederholter
Wiederöffnung bytegleich überein. Das ist kein vollständiger Grammatik-
oder Bedeutungsnachweis; menschliche Überprüfung bleibt erforderlich.
Statische Prompts, Vertragsdateien, Fristen und das Belegbudget ändern sich nicht.

### Vorherige Korrekturen

### Einmalige Projektion nach dem Update

Der [CodeRabbit-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4485814725)
war am Vorgänger `5896b2b` bestätigt: Der Workspace-Zweig von
`LiveRunner::update` rief `w.view()` zweimal auf.
Jeder Aufruf kopiert den Workspace und projiziert das Kontextjournal.
Jetzt wird eine Ansicht ermittelt und für `revision` sowie `accepted` wiederverwendet.

Das ist ein statischer Nachweis am tatsächlichen Aufrufpfad: zwei Projektionen
vorher, eine danach. Daraus wird keine gemessene Beschleunigung abgeleitet.
Es wurde kein Test hinzugefügt, der nur die neue lokale Variable nachbildet.
Stattdessen bestanden alle bestehenden 567 Rust-Regressionen auf `90becdd`.
Die aktuelle Gesamtsuite wird nach der zusätzlichen Validatoränderung erneut ausgeführt.
Der Modellaufruf, Request-Inhalt, Prompts, Quellenvalidierung, Commitpunkt,
Fristen, Audioaufnahme und Erkennung ändern sich nicht.

### Modellwechsel und Quellenidentität

Der [Codex-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4485201700)
auf `8d0ba21` war gültig. Modell B übernahm menschlich bestätigte Aufgaben
von Modell A, vergab einem unveränderten Transkriptabschnitt bei der ersten
Verarbeitung aber eine neue Kennung. Dadurch fehlte die vorhandene erledigte
Aufgabe im Aufgaben-Kontext; eine wiederholte Zusage erzeugte ein offenes Duplikat.

Vorher reproduzierten der unmittelbare Wechsel und der Wechsel mit vorheriger
Wiederöffnung jeweils eine zweite Aufgabe und fehlenden terminalen Kontext.
Der neue Test verwendet die tatsächlichen Modelllauf-Ordner, Kontextbestätigung,
Pending-Auswahl, Requests, Validierung und dauerhafte Speicherung.
Nur die Modellantworten liefert ein begrenzter eigener HTTP-Prüfserver.
Nachher bleiben Kennung, menschlicher Status, Metadaten und Wiederöffnung stabil;
der tatsächlich versendete Aufgaben-Kontext enthält die erledigte Identität.
Eine spätere identische Zusage aus einem unabhängigen Abschnitt erhält weiterhin
eine neue Kennung und erzeugt eine neue offene Aufgabe. Alte Dateien bleiben erhalten.

Unveränderte geerbte Quellen behalten ihre Identität nur bei gleicher Abschnitts-,
Spur-, Zeit- und Textzuordnung, ohne Zurücknahme und vor der ersten eigenen
Verarbeitung dieses Abschnitts. Korrekturen und zurückgekehrte frühere Fassungen
werden nicht als unverändert behandelt.

Zusätzlich werden bekannte Zurücknahmen historischer Ereignisquellen vererbt.
Eine kontrollierte Gegenprobe verlor diesen Marker vorher beim Wechsel.
Danach erhält derselbe Text einer zurückgezogenen historischen Quelle eine neue Kennung;
der weiterhin gültige aktuelle Beleg behält dagegen seine bisherige Kennung.
Alle neun Modellvererbungsfälle einschließlich Quellenänderung, Entfernung,
Wiederherstellung, Chronologie und bytegleicher Wiederöffnung bestehen.
Statische Prompts, Vertragsdateien, Belegbudget und Produktionsfristen bleiben
unverändert. Das ist eine kontrollierte Orchestrierungsregression, kein echter
Zweimodell-Inferenzlauf.

### Vorherige Korrekturen auf unveränderten Komponenten

Die folgenden Karten-/Diagnostik-Nachweise stammen vom bytegleichen
`8d0ba21`-Stand und wurden hier nicht erneut ausgeführt.

### Kartenmenü

Der [Pointer-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4484705348)
war gültig. Das native automatische Popover schloss zwischen Pointerdruck und
Klick bereits durch Light-Dismiss. Der Klick las den geschlossenen Zustand und
öffnete es irrtümlich erneut. Der Zustand beim Pointerdruck wird jetzt für diese
Aktivierung berücksichtigt; Tastaturaktivierungen lesen weiterhin den aktuellen
Zustand. Abgebrochene Pointeraktionen blockieren die Tastatur nicht.

Vorher scheiterten acht echte Maus-Schließabläufe sowie vier Touch-Schließabläufe.
Danach bestehen alle acht Kombinationen aus 390/1280 Pixeln, Deutsch/Englisch
und hell/dunkel. Touch wurde in den vier schmalen Kombinationen geprüft.
Enter, Escape, Schließen außerhalb und Tastatur nach einem abgebrochenen Druck
bleiben funktionsfähig. Eine zusätzliche Regression erreicht die tatsächliche
LiveWorkspace-Kartengrenze; sie scheiterte vorher und besteht danach.

Der Browsernachweis verwendet das vorhandene Edge **156.0.4314.58** und
Playwright **1.57.0** auf Windows 11, die tatsächliche OverviewCard, deren CSS
und eigene synthetische Inhalte. Keine Seiten- oder Konsolenfehler und keine
Fremdanfragen wurden beobachtet. Eigener Browser und Server wurden beendet;
der Dunkelmodus-Screenshot wurde visuell geprüft. Das ist keine Prüfung nativer
Tauri-WebViews auf Windows, macOS oder Linux.

### Einfügehinweis im Dunkelmodus

Der [Kontrastbefund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4484705331)
war gültig. Weiß auf dem tatsächlichen hellblauen Hintergrund ergab rechnerisch
**2.0473:1**. Die dunkle Schrift ergibt jetzt **7.5337:1**.
Die vorab gesetzte Mindestgrenze **4.5:1** wurde nicht geändert.
Die Berechnung verwendet die tatsächlich gerenderten sRGB-Vordergrund- und
Hintergrundfarben und deren relative Luminanz. Das helle Theme bleibt unverändert
und erreicht **5.1686:1**.

### Klassische Ollama-Fehler

Der [Diagnostikbefund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4484705369)
war gültig: Die zugelassene native Route `api/chat` meldete bei HTTP-, JSON- und
Zeitfehlern fälschlich eine fehlgeschlagene Modellverwaltung.
Jetzt benennt sie die lokale Auswertung und die nächste Handlung.
HTTP-Status und feste Fehlerkategorien bleiben erhalten.
Anbieterantworten, Schlüssel, Modelltext und lokale Pfade bleiben ausgeblendet.

Ein tatsächlicher lokaler HTTP-Prüfserver reproduzierte vorher drei falsche
Auswertungsmeldungen. Danach bestehen alle sechs Fälle:
Auswertung und Verwaltung jeweils mit HTTP 500, ungültigem JSON und Zeitablauf.
Der Server nimmt nur eigene synthetische Anfragen an; Annahme und E/A sind begrenzt.
Das kurze Zeitlimit dieser Fehlerfixture ändert keine Produktfrist.
Routen, API-Nachrichten, statische Prompts, Abbruchverhalten und Produktionszeitlimits
bleiben unverändert. Echte Ollama-Inferenz wird daraus nicht behauptet.

## Frische Prüfungen dieses Stands

| Prüfung | Tatsächliches Ergebnis |
| --- | --- |
| Rust-Workspace, alle Targets | **567 bestanden**, darunter **168 Notizenfälle** und **37 Providerprotokollfälle**; drei bestehende besondere Einstiege regulär ignoriert. |
| Rustformat und Clippy | Bestanden, gesperrte Abhängigkeiten und Warnungen als Fehler. |
| Frontend | Auf den hier bytegleichen Komponenten von `8d0ba21`: **414 Tests in 21 Dateien**, TypeScript, Produktionsbuild und Format bestanden; hier nicht erneut ausgeführt. Zwei Testworker und bisherige Fristen unverändert. |
| Lokale Modellreferenz Deutsch | Frisch **102.594161 s**; reguläre vollständig validierte Wiederholungen: **0**. |
| Lokale Modellreferenz Englisch | Frisch **115.6522393 s**; reguläre vollständig validierte Wiederholungen: **0**. |
| Projektprüfung | Nach diesem letzten Dokumentedit vor Upload erneut `verify.ps1` erforderlich. Sie prüft 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorfälle. |

Die beiden neuen echten lokalen Referenzen prüfen positive Aufgaben, unverbindliche Aussagen,
Impulserhalt, menschlichen Status nach einer Protokolländerung und bytegleiche
Wiederöffnung. Alle fünf Kriterien bestehen. Wörtliche Zitate einschließlich
„I’ll“ bleiben erhalten. Eine vollständig validierte Wiederholung je Anfrage
war vorher erlaubt und wurde nicht erhöht; null Wiederholungen sind keine
Freigabeschwelle.

Verwendet wurden ausschließlich synthetischer Text und das vorhandene kleine
**google/gemma-4-e4b Q4_K_M**, 6.326.936.720 Modellbytes.
LM Studio: **0.4.26+4**. Windows 11 Pro: **26200**.
Prozessor: Intel **i7-13700**, sichtbarer physischer Speicher rund **64 GiB**.
Die Zeit umfasst Modellverarbeitung, dauerhaften Abschluss und Wiederöffnung.
Während der Inferenz liefen keine anderen eigenen Prüfungen.
Keine Downloads, Cloud oder Nutzermeetings. Keine neuen Working-Set-,
Private-Bytes- oder Grafikspeichermessungen.

## Erhaltene frühere Nachweise und Grenzen

Die Sidebar-/RecordingStatus-/MeetingView-Browserprüfung stammt vom relevanten
bytegleichen `93d48e2`-Komponentenstand und wurde hier nicht erneut ausgeführt:
24 Layout-, 24 Warn-Titel- und 24 klassische Meetingtitel-Zustände.
Die neue Kartenprüfung ersetzt keinen Nachweis für native IPC, OS-Tooltips,
Screenreader-Sprachausgabe oder physische Aufnahme.

Der kontrollierte Langsammodellnachweis auf `35d53e8` bleibt historisch:
244.33 s Gesamtlauf, 210 s SSE-Pause, nächster Refresh nach 210.0421383 s,
höchstens eine Anfrage und ein erhaltener Impuls.
3737920 Samples je Spur / 16000 Hz = 233.62 s je Spur.
Beide synthetischen PCM-Schreiber liefen nach Auswertungsstopp weiter.
Das ist kein echter Parakeet- oder physischer Zweistundenlauf.
Die 21 Paketregressionen auf `91f0985` verwendeten synthetische ZIPs;
hier wurde kein neues Produktpaket oder Installer bereitgestellt.

Ein zusätzlicher echter deutscher Referenzversuch auf `5896b2b`
speicherte die ausdrückliche Zusage nur als unklaren Fakt und keine Aufgabe.
Die Referenzprüfung scheiterte deshalb ausdrücklich. Die Aufgabenphasen-Antwort
und deren Retryzahl wurden nicht separat gespeichert; eine weitergehende Ursache
wird nicht behauptet. Es waren keine geerbten Quellen beteiligt.
Der Fehllauf und alle gesicherten Dateien bleiben erhalten. Das nachfolgende
Referenzpaar ist ein eigener Durchlauf mit unverändertem Code, Modell,
Prompt und Bestehenskriterien. Es beweist keine vollständige Erkennung aller
Aufgaben in beliebigen Modellantworten.

Ein echter deutscher Referenzversuch auf `da6b5c2` speicherte Fenster 1 und
hielt wegen einer nichtleeren `unresolvedContextIds`-Antwort sicher an.
Der konkrete angeforderte Kontext wurde nicht separat gespeichert;
die weitergehende Ursache bleibt unbekannt. Die gesicherten Stände bleiben erhalten.

Der frühere Linux-Lauf auf `689212b` scheiterte mit **10159/10154 ms**
an der unveränderten Obergrenze von **10000 ms**. Eine weitergehende Ursache
wurde nicht nachgewiesen. Der Fehllauf bleibt erhalten; später erfolgreiche
Heads ändern weder dieses Ergebnis noch die Grenzwerte.

Auf dem Vorgänger `5896b2b` bestanden tatsächlich alle 22 Checks:
16 erfolgreich, sechs reguläre Skips.
Die [Plattformläufe](https://github.com/Puma7/meeting-for-handy/actions/runs/42101116336)
enthielten reale generierte Sprachprüfungen mit Version 1.1.0-beta.1,
Handy 0.9.8 und Nemotron 3.5 Q8 auf CPU:
Linux **10.509165387 s**, ARM **13.535942541 s**, Intel **26.465786573 s**.
Eigener Worker-Neustart und Audiohash-/Journalerhalt bestanden.
Der [Windows-Gegenlauf](https://github.com/Puma7/meeting-for-handy/actions/runs/42101116390)
war modellfrei und ist kein ASR-Referenzlauf.
Unit-Fixtures mit `version=test` werden nicht als diese Messungen ausgegeben.

Die virtuelle Linux-Audioaufnahme auf diesem Vorgänger bestand ausdrücklich
mit **passed-with-startup-gaps**, **completeCapture=false**:
9642/9646 ms, 146399/144399 qualifizierte Tonsamples,
native Startauffüllung 7881/9947 Samples und je ein eingehender Phasennullwert.
Alle überlappenden 250-ms-Fenster bis zum letzten Sample, mindestens acht Sekunden
qualifizierter Ton, RMS über 1000, Frequenz-, Dauer- und Differenzgrenzen blieben
unverändert. Die Quellen waren ausschließlich virtuelle Geräte.
Daraus folgt keine lückenlose physische Aufnahme.

Diese früheren CI-Ergebnisse werden nicht auf den neuen Head übertragen.
Dessen eigene aktuelle CI und saubere Reviews bleiben erforderlich.

## Reviewstand und bewahrte Schutzmechanismen

Die erste frische Codex-Runde auf dem direkten Vorgänger `90becdd`
fand die oben korrigierten unvollständigen Abschlussmeldungen.
Sie war kein sauberer Abschluss. Auf diesem Zwischenstand wurde kein weiterer
vollständiger CodeRabbit-Auftrag gestellt. Die vorherige Projektionskorrektur
wurde von CodeRabbit [bestätigt](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4485972047).
Die pauschale zusätzliche Testisolations-/Browser-Sorge wurde mangels konkretem
Fehler [ausdrücklich zurückgenommen](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6669253684).
Native WebView-Prüfung bleibt eine Nachweisgrenze.

Zwei frische Codex-Codeprüfungen auf dem unveränderten Vorgänger `5896b2b`
waren sauber:
[Runde 1](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6668270364) und
[Runde 2](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6668426667).
Der anschließende [vollständige CodeRabbit-Lauf](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5791791993)
fand den oben korrigierten doppelten Lesevorgang; er war deshalb kein sauberer Abschluss.
Alle 100 zulässigen Dateien des vollständig paginierten 102-Dateien-Diffs
waren ausgewählt. Eine reine Whitespace-Datei hatte keine prüfbaren Änderungen.
Die pauschalen zusätzlichen Hinweise zu Testisolation und Browserabdeckung wurden
[mit konkreten Nachweisen zur Klärung gestellt](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6669025661).
Die bekannten Grenzen nativer WebView-Prüfung bleiben ausdrücklich erhalten.

Für den endgültigen Head bleiben zwei frische saubere Codex-Codeprüfungen, eine
vollständige saubere CodeRabbit-Runde und dessen eigene grüne CI erforderlich.
Die zusätzliche LanguageTool-Prüfung blieb unvollständig; eine weitergehende
Ursache ist nicht nachgewiesen. Tool und Regeln bleiben aktiv.
Eine vollständige Hauptreview-Auswahl beweist keine vollständige Sprachprüfung.

Alle 16 ursprünglichen [Sourcery-Anmerkungen](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5748992615)
wurden einzeln gegen den Code geprüft, beantwortet und geschlossen.
Der vollständig paginierte Audit vergleicht ursprüngliche Kommentartexte und
Identitäten. Neue Befunde werden am aktuellen Code validiert.

Die zwei sauberen Codex-Codeprüfungen auf dem Vorgänger `0d8c03c` und dessen
grüne CI waren kein Abschluss: Der anschließende
[vollständige CodeRabbit-Lauf](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5790388696)
fand die drei oben korrigierten Probleme.
Er umfasste alle 100 zulässigen Dateien des vollständig paginierten
102-Dateien-Diffs bei unveränderten zwei Lockdatei-Filtern.
Risiko: Low mit diesen drei Punkten. Eine separate Architekturbewertung
wurde nicht geliefert. Dokumentation: 85.27 Prozent bei unveränderter
80-Prozent-Grenze. Die zusätzliche LanguageTool-Prüfung blieb teilweise
unvollständig; Ursache unbekannt. Tool und Regeln bleiben aktiv.
Eine vollständige Hauptreview-Auswahl beweist keine vollständige Sprachprüfung.

Frühere Fehlalarme zum Produktions-Minutenfilter und zu unkonkreten
Testhygieneproblemen wurden
[ausdrücklich zurückgenommen](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4484154861).
Die fünf tatsächlichen 5/10/15/30/60-Minuten-Komponentenfälle bestanden schon
vorher; der Produktionsfilter wurde nicht verändert.

Alle früher abgesicherten Mechanismen bleiben erhalten:

- Quellenkennungen und wörtliche Zitate müssen vorhanden sein. Abgelehnte
  Modellantworten verändern keine bestätigten Zustände oder alten Checkpoints.
- Impulse, menschliche Änderungen und Notizentwürfe bleiben bei Fehlern erhalten.
  Eine fehlgeschlagene Bestätigung wird nicht automatisch wiederholt.
- Geänderte Originalquellen und gemeinsame Kontextfassungen werden ausdrücklich
  abgeglichen. Erledigte oder zurückgenommene Aufgaben bleiben terminal.
  Ursprüngliche Zusagen aus unveränderlicher Ereignishistorie verhindern
  Duplikate nach Minimierung aktueller Belege. Neue unabhängige Zusagen bleiben neu.
- Fragen, Vorschläge, fehlende Zusagen, Negationen und unabhängige Bedingungen
  werden konservativ geprüft. Eingebettete englische Prüfgegenstände wie
  „check if“ oder „ask if“ verdrängen keine ausdrückliche Zusage.
  Das bleibt eine begrenzte Wortheuristik, kein vollständiger Bedeutungsbeweis.
- Das kombinierte Budget von sechs aktuellen Belegen, Fristen und Retrygrenzen
  bleiben erhalten. Keine Dokumentations- oder Schutzschwelle wurde abgesenkt.
- Fremde unendliche Modellreservierungen bleiben fremd. Eigene endliche TTL:
  300 s, Erneuerung: 60 s. Veränderte Ollama-Kontextlänge verlangt erneut
  bekannte Größe und die bestehende Speicherzulassung.
  Bereits fremd geladene ausgewählte LM-Modelle werden nicht übernommen.
  Unbestätigte Lade-/Entladevorgänge erlauben keine geratene Instanz oder Zweitladung.

## Veröffentlichung, Kompatibilität und offene Grenzen

Vor Upload werden isolierter Git-Root, eigener Branch und tatsächlicher
Remote-Vorgänger geprüft. Danach werden alle Dateien des tatsächlichen
unveränderlichen Remote-ZIP mit lokalen Bytes und Git-Blobs verglichen.
Alle 102 PR-Patches werden vollständig paginiert und mit Git abgeglichen.
Alte oder leere Security-Flags sind kein aktueller formaler Securitynachweis.

Bestehende native Sitzungen behalten Modellkennung und gesicherte Gewichte.
Fortsetzen verifiziert Katalog und Gewichte; fehlende oder geänderte Modelle
werden nicht still ersetzt. Archiviertes Audio muss zuerst wiederhergestellt
werden. Legacy-Originalsitzungen bleiben lesbar; nachträgliche Erkennung dieser
älteren Sitzungen benötigt weiterhin die bisherige Programmversion.

Statische Prompts, Vertragsdateien, Workspace-Anfragen, UI, Transport,
Aufnahme-/ASR-Logik, Modellreservierungen,
Abhängigkeiten, Datenschutz-/Reviewer-Konfiguration, Version und vollständiger
TEST_REPORT bleiben gegenüber `90becdd` unverändert.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und zweistündige Gerätetests
wurden vom Nutzer auf später verschoben. Keine neue echte Parakeet-Dauer-,
GPU-, kombinierte Zweistundenlast-, unabhängige installierte Prozess-,
Signatur-, Notarisierungs- oder forensische Speicherfreigabe.
Cloud-Prüfungen ohne echte Zugangsdaten bleiben Simulationen.
Bekannte gesperrte Abhängigkeitsrisiken bleiben dokumentiert.

Unbekannte Zuständige und Termine bleiben unbekannt. Abschnittszeiten sind
keine Wortzeiten. Technische Belegprüfung beweist keine inhaltliche Deutung.
Ergebnisse bleiben überprüfbare KI-Entwürfe.
Diese Nacharbeit erstellt keinen installierten Programmstand und keinen Release.
