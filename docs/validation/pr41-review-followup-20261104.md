# PR 41: Validierungsstand

Stand: 5. November 2026. Version **1.1.0-beta.1**, unverändert.

Die aktuelle Nacharbeit erhält beim Wechsel des Textmodells unveränderte
geerbte Quellenkennungen. Eine manuell erledigte Aufgabe wird dadurch bei der
erneuten Verarbeitung desselben Transkripts nicht als neuer offener Auftrag
angelegt. Tatsächlich zurückgezogene Quellen behalten ihre Zurücknahme;
neue unabhängige Zusagen bleiben neue Aufgaben.

Die vorherigen Kartenmenü-, Kontrast- und Ollama-Diagnostikkorrekturen sowie
alle früheren Reviewkorrekturen einschließlich des englischen Referenzpatches
`93335102ed` bleiben enthalten.
Die [ausführliche bisherige Fassung](https://github.com/Puma7/meeting-for-handy/blob/8d0ba2139028af03a44e6e6f507df3efe46f8a9a/docs/validation/pr41-review-followup-20261104.md)
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
| Lokale Modellreferenz Deutsch | **134.2789829 s**; reguläre vollständig validierte Wiederholungen: **1**. |
| Lokale Modellreferenz Englisch | **164.4314505 s**; reguläre vollständig validierte Wiederholungen: **0**. |
| Projektprüfung | Nach diesem letzten Dokumentedit vor Upload erneut `verify.ps1` erforderlich. Sie prüft 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorfälle. |

Beide neuen Referenzen prüfen positive Aufgaben, unverbindliche Aussagen,
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

Ein zusätzlicher echter deutscher Referenzversuch dieses nativen Stands
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

Auf dem Vorgänger `0d8c03c` bestanden tatsächlich alle 22 Checks:
16 erfolgreich, sechs reguläre Skips.
Die [Plattformläufe](https://github.com/Puma7/meeting-for-handy/actions/runs/42082444293)
enthielten reale generierte Sprachprüfungen mit Version 1.1.0-beta.1,
Handy 0.9.8 und Nemotron 3.5 Q8 auf CPU:
Linux **9.035697134 s**, ARM **18.562827875 s**, Intel **30.730105316 s**.
Eigener Worker-Neustart und Audiohash-/Journalerhalt bestanden.
Der Windows-Gegenlauf war modellfrei und ist kein ASR-Referenzlauf.
Unit-Fixtures mit `version=test` werden nicht als diese Messungen ausgegeben.

Die virtuelle Linux-Audioaufnahme auf diesem Vorgänger bestand ausdrücklich
mit **passed-with-startup-gaps**, **completeCapture=false**:
9802/9802 ms, 151399/144999 qualifizierte Tonsamples,
native Startauffüllung 5444/11843 Samples und je ein eingehender Phasennullwert.
Alle überlappenden 250-ms-Fenster bis zum letzten Sample, mindestens acht Sekunden
qualifizierter Ton, RMS über 1000, Frequenz-, Dauer- und Differenzgrenzen blieben
unverändert. Die Quellen waren ausschließlich virtuelle Geräte.
Daraus folgt keine lückenlose physische Aufnahme.

Diese früheren CI-Ergebnisse werden nicht auf den neuen Head übertragen.
Dessen eigene aktuelle CI und saubere Reviews bleiben erforderlich.

## Reviewstand und bewahrte Schutzmechanismen

Die erste Codex-Runde auf dem direkten Vorgänger `8d0ba21` fand den oben
korrigierten Quellenfehler. Sie war kein sauberer Abschluss. Auf diesem
Zwischenstand wurde kein weiterer vollständiger CodeRabbit-Auftrag verbraucht.
Für den endgültigen Head bleiben zwei frische saubere Codex-Codeprüfungen, eine
vollständige saubere CodeRabbit-Runde und dessen eigene grüne CI erforderlich.

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

Statische Prompts, Vertragsdateien, UI, Transport, Aufnahme-/ASR-Logik, Modellreservierungen,
Abhängigkeiten, Datenschutz-/Reviewer-Konfiguration, Version und vollständiger
TEST_REPORT bleiben gegenüber `8d0ba21` unverändert.

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
