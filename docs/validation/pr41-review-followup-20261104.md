# PR 41: Validierung der Reviewnacharbeit

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.

PR 41 ergänzt den optionalen Advanced Meeting Assistant neben der klassischen
Transkriptansicht. Gemeinsame Modelleinstellungen, anpassbare Karten, Vorbereitung,
Notizen und Originalbelege bleiben erhalten. Parakeet Ultra verarbeitet geschlossene
Abschnitte beider Spuren nacheinander; Audio wird unabhängig gesichert.
Langsame Textmodellläufe werden nicht durch Intervallticks unterbrochen.

Dieser bereinigte Vermerk enthält ausschließlich synthetische Prüfsituationen.
Der vollständige TEST_REPORT bleibt gegenüber der PR-Basis byteunverändert.
Private Rohbelege und Nutzermeetings werden nicht veröffentlicht.
Die [vorherige ausführliche Fassung und ihre gesamte Detailhistorie](https://github.com/Puma7/meeting-for-handy/blob/939d7f066259d22207324f7e75bbf9e7f85b2f77/docs/validation/pr41-review-followup-20261104.md)
bleiben unveränderlich erhalten. Alte Fehlläufe werden nicht nachträglich als bestanden ausgegeben.

## Aktuelle Korrekturen mit Vorher- und Nachhernachweisen

Die [Codex-Runde auf 939d7f0](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5796050213)
war nicht sauber. Modalverben konnten als drittes Wort eines nominalen Subjekts
zählen: The team can/must/may cancel bestätigte eine bloße Möglichkeit oder Pflicht
als tatsächliche Rücknahme. Auch modale Vergangenheitsformen reichen nicht als
Abschlussbeleg. Die vorhandenen dauerhaften Regressionen reproduzierten 22 falsche
Rücknahmen für Aufgaben/Beschlüsse und zwei falsche Vereinbarungen.
15 zusätzliche Literale ergeben 27 Szenarien; ein echter Akteur namens May bleibt zulässig.
Alle alten Namen-, Team-, Pronomen-, Einschub-, Infinitiv-, Verneinungs-, Mengen-
und Nichtstun-Gegenfälle bleiben erhalten. Die gemeinsame Satzteilprüfung
überspringt weder Planungsverbphrasen noch Objekte. Originalzitate, Checkpointbytes,
mehrfache Wiederöffnung und Ablehnungen ohne neue Revision sind geprüft.
Dies ist eine konservative Konstruktionserkennung, kein vollständiger Grammatik-
oder inhaltlicher Wahrheitsbeweis.

Der [vollständige CodeRabbit-Lauf auf 939d7f0](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5796103693)
war ebenfalls nicht sauber und nicht vollständig. Die einzelnen Punkte:

- **Synthetische Batch-Synchronisation:** Der Windows-CI-Lauf scheiterte mit
  SendError an der Freigabe der künstlichen Erkennersperre. Eine absichtliche
  sechssekündige Controllerverzögerung reproduzierte lokal denselben Fehler:
  Der bisherige Fixture-Empfänger lief schon nach fünf Sekunden ab. Die Sperre
  wartet jetzt auf das ausdrückliche Freigabesignal und prüft beim Warten den
  vorhandenen Abbruchwächter. Eine gepufferte Eintrittsmeldung verhindert zudem
  ein blockiertes Rendezvous beim Abbruch des Controllers. Die verzögerte
  Gegenprobe besteht mit allen unveränderten PCM-, Abschnitts-, Live-Text- und
  Überlappungsprüfungen. Die produktiven ASR-Bytes sind vollständig unverändert.
  Auch die echten Erkennungsfristen und die bisherigen Fünf-Sekunden-Grenzen für
  angeforderte Fortschritte sind unverändert. Dies ist kein Leistungsnachweis
  physischer Aufnahme oder einer echten Parakeet-Inferenz.
- **Belegnavigation:** Ein fertiger Abschnitt konnte endgültigen Text und einen
  abweichenden früheren committedText besitzen. Die Oberfläche zeigte den endgültigen
  Text, die Verknüpfung suchte jedoch im früheren Stand. Die bestehende
  Komponentenregression reproduzierte den fehlenden Link. Fertige Abschnitte
  verwenden nun text, laufende ausschließlich committedText, jeweils mit leerem
  Ersatzwert. Quellen-, Zeit-, Dateizuordnungs- und Superseded-Schranken bleiben
  bestehen. Beide Zustände und die ursprüngliche Quellenmarkierung sind geprüft.
- **Englische Beschriftung:** Der Textzweig des gemeinsamen SourceButton benutzt
  die vorhandene Sprachvermittlung: Evidence auf Englisch, Beleg auf Deutsch.
  Quellenkennungen und Aufrufargumente bleiben wörtlich unverändert.
- **LM-Studio-Eigentümerschaft:** Die Workspace-Vorbereitung konnte eine bereits
  geladene ausgewählte Instanz mitbenutzen. Der spätere klassische Pfad übernahm
  diesen Zustand ebenfalls. Dies widersprach der bestehenden dokumentierten
  Reservierungspolitik. Der synthetische echte HTTP-Vorherlauf bestätigte beide
  angenommenen Vorbereitungen und eine tatsächliche Textanfrage mit der fremden
  Instanzkennung. Jetzt werden vorbestehende ausgewählte Instanzen für beide
  Pfade vor Laden, Entladen oder Textübertragung abgewiesen. Der Nachherlauf enthält
  nur drei lesende Inventaranfragen; fremde andere Modelle bleiben unberührt.
  Die bisherige Mitbenutzungsvariante wurde entfernt. Eigene bestätigte Instanzen,
  unbestätigte Lade-/Entladezustände, Aliassperren und Wiederaufnahme bleiben geprüft.
- **Speicher-Spy, belegter Fehlalarm:** Die UI-Testeinrichtung ersetzt localStorage
  ausdrücklich durch ein gewöhnliches isoliertes Storage-Objekt mit privater Map.
  Es ist kein jsdom-Storage-Proxy. Der Instanz-Spy ersetzt tatsächlich removeItem;
  Storage.prototype wäre nicht der verwendete Empfänger. Der unveränderte gezielte
  workspace-cache-failure-Fall besteht mit Vitest 4.1.12 und prüft Warnung,
  erhaltene Cachebytes und den unberührten Cache eines anderen Meetings. Die
  [Einzelantwort mit Nachweis](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4489382220)
  wurde von CodeRabbit geprüft und die Anmerkung
  [ausdrücklich zurückgenommen](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4489387938).
  CodeRabbit führte den gemeldeten Test nicht selbst aus. Testkonfiguration,
  Fristen und Assertions bleiben erhalten.
- **Zusätzlicher optionaler Hinweis:** Fehlt Playwright als Paketmetadatum, meldet
  die Browserhilfe nun die vorhandene verständliche RuntimeError-Anleitung.
  Eine synthetische Probe mit fehlenden Paketmetadaten bestätigt die Meldung
  vor und nach der Korrektur.
  Die feste Version 1.57.0 und das Verbot automatischer Installation bleiben erhalten.

## Tatsächlich lokal geprüft

| Prüfung | Nachweis und Zuordnung |
| --- | --- |
| Rust, Workspace und alle Targets | Frisch **568 bestanden**, darunter **169 Notizenfälle** und **37 Providerprotokollfälle**; drei bestehende besondere Einstiege regulär ignoriert. |
| Rustformat und Clippy | Frisch bestanden, gesperrte Abhängigkeiten und Warnungen als Fehler. |
| Frontend | Frisch **417 Tests in 21 Dateien**, TypeScript, Produktionsbuild und Format bestanden. |
| Projektprüfung | Frisch verify.ps1: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen bestanden. |
| Deutsch, tatsächliches lokales Modell | Frisch **135.8649258 s**, **0** reguläre Validierungswiederholungen. |
| Englisch, tatsächliches lokales Modell | Frisch **177.9742246 s**, **1** reguläre vollständig validierte Wiederholung. |

Beide Modellreferenzen benutzen denselben seit der ausdrücklichen Budgetfrage
unveränderten synthetischen Gesprächskorpus. Englisch einschließlich I’ll bleibt
bytegleich. Alle ursprünglichen Zusagen und unverbindlichen Aussagen bleiben erhalten.
Die fünf Kriterien sind positive Aufgabe, unverbindliche Aussage nicht als Aufgabe,
erhaltene Impulse, menschlicher Impulsstatus nach Protokolländerung und bytegleiche
Wiederöffnung. Eine leere oder ungültige Antwort wird nicht als Erfolg übernommen.
Je Anfrage bleibt höchstens eine vollständig validierte Wiederholung zulässig;
ein Durchlauf ohne Wiederholung ist kein zusätzliches Freigabekriterium.

Ein erster tatsächlicher deutscher Versuch dieser Korrektur brach bereits im
ersten Fenster mit der Meldung zum Antwortlimit ab. Es entstand weder eine
Ergebnisrevision noch ein Referenz-Erfolgsvermerk; Agendakontext und leere
Koordinationslocks blieben erhalten. Die eigene ausgewählte Modellinstanz war
danach tatsächlich freigegeben. Englisch wurde nach diesem Fehler nicht gestartet.
Konkrete Phase, Finish-Reason, Tokenverbrauch und verworfene Antwort wurden vom
Helfer nicht separat gespeichert. Der Fehler kann auch aus einem validierten
moreChangesPossible oder mehr als 20 Änderungen stammen; tatsächlich beobachtete
4096 Tokens werden nicht behauptet. Die weitergehende Ursache bleibt offen.
Der Fehlversuch bleibt erhalten und wird nicht als bestanden gezählt.
Das oben ausgewiesene Paar ist eine getrennt vorab festgelegte Prüfung mit
unverändertem Code, Modell, Gespräch und denselben fünf Kriterien und Grenzen.

Verwendet wird ausschließlich das vorhandene kleine google/gemma-4-e4b Q4_K_M
mit **6.326.936.720 Modellbytes**, LM Studio **0.4.26+4**, Windows 11 Pro **26200**,
Intel **i7-13700** und rund **64 GiB** sichtbarem physischem RAM. Programmversion,
Rechnerdaten und ausgewählte Modellinstanz wurden frisch lesend erfasst.
Die Helfer melden die verstrichene Zeit von Start bis validierter dauerhafter
Speicherung und Wiederöffnung in Sekunden. Während der Modellverarbeitung laufen
keine eigenen Builds, lokalen Test- oder Browserläufe; lesende GitHub-Statusabrufe
können parallel stattfinden. Keine Downloads, Cloud oder Nutzermeetings.
Fremde Prozesse wurden nicht angehalten; ihre Last wurde nicht isoliert gemessen.
Working Set, Private Bytes und Grafikspeicher wurden nicht neu gemessen;
Modell-Dateigröße ist keine RAM-Messung.

## Tatsächliche Browserprüfung und Grenzen

Die wirkliche LiveWorkspace-Komponente wurde mit synthetischer Adaptergrenze in
Edge **156.0.4314.58** und dem vorhandenen Playwright **1.57.0** geprüft:
390/1280 Pixel, Deutsch/Englisch, hell/dunkel sowie vollständige/laufende
Transkriptabschnitte, insgesamt **16 Konfigurationen**. Der Vorhervergleich
verwendete unveränderliche Komponentenbytes aus 939d7f0 und zeigte acht fehlende
Belegverknüpfungen sowie acht englische Beschriftungen mit Beleg. Nachher bestehen
alle 16 Konfigurationen. Quellenlink und Quellenmarkierung passen zum tatsächlich
angezeigten Text. Quellenkennungen und Callbackargumente bleiben unverändert.

Die Vorschau wird erst nach Klick angefordert. Verwendet wurde ausschließlich
ein neu erzeugtes kurzes stilles synthetisches WAV; keine Aufnahme und kein
Nutzer-Audio. Die Vorschau startet nicht automatisch, und die eigene Blob-URL
wird beim Schließen wieder freigegeben. Alle Notizwechsel-, Leerungs-, Reload-,
Privattexttrennungs- und sichtbaren Impulsstatusprüfungen aus der vorherigen
Browserrunde bestehen auch hier. Keine relevanten Konsolen-/Seitenfehler,
keine fremden Netzwerkanfragen und kein horizontaler Überlauf. Die eigenen
Browserkontexte und der Fixture-Server wurden beendet. Die gespeicherten
Ansichten 390/Deutsch/hell und 1280/Englisch/dunkel wurden auch visuell geprüft.

Ein erster privater Browserversuch hatte noch einen falschen Namen für den
Schließen-Button und brach nach dem ersten bereits reproduzierten Linkfehler ab.
Er bleibt erhalten und zählt nicht als vollständige Matrix. Der getrennte
finale Vorherlauf und der Nachherlauf verwenden den tatsächlichen Dialognamen.
Der zusätzliche SourceButton-Textzweig wurde in einer ausdrücklich synthetischen
Komponentenfläche geprüft. Native Tauri-WebViews, native IPC, Screenreader-
Sprachausgabe, echte Audioausgabe und physische Geräte sind damit nicht geprüft.

## Vollständigkeit der Reviewrunde

Alle 16 ursprünglichen Sourcery-Anmerkungen wurden einzeln gegen den Code
geprüft, beantwortet und geschlossen. Die bisherigen CodeRabbit-Anmerkungen
wurden einzeln bearbeitet; neue gültige Befunde werden erst nach Veröffentlichung
der geprüften Korrektur beantwortet. Ein geschlossener alter Thread ersetzt
keinen frischen sauberen Review auf dem endgültigen Stand.

Der vollständige 939d7f0-CodeRabbit-Lauf wählte alle 100 unter den unveränderten
zwei Lockdatei-Filtern zulässigen Dateien aus. Dennoch blieb validation_01.rs
wieder nicht vollständig verfügbar. Die Ursache ist nicht bewiesen. Die
bestehenden Regressionen wurden deshalb ohne Änderungen an Funktionsbytes,
Assertions oder Fristen auf vier bereits vorhandene Testmodule verteilt.
Nur abschließende Leerzeilen zwischen Dateiblöcken wurden durch Rustfmt normalisiert.
Keine neue Datei und keine Filterausnahme; die PR-Dateizahl bleibt 102.
Die betroffene Datei umfasst danach rund 21 KiB. Erst ein weiterer tatsächlicher
vollständiger Lauf ohne unverfügbare Dateien kann diese Prüflücke schließen.
AudioSources.tsx wurde erneut als ohne prüfbare Änderung markiert; sein kleiner
vorhandener Diff entfernt lediglich den sichtbaren Pegel-Hilfstext und ist
manuell geprüft. Die zugängliche Erklärung und Aufnahmefunktion bleiben bestehen.

LanguageTool konnte zuletzt nicht alle Textstücke des vorherigen langen Vermerks
prüfen. Ursache unbekannt, Tool und Regeln bleiben aktiv. Dieser Vermerk ist
gekürzt; die vollständige frühere Detailhistorie bleibt über den unveränderlichen
Link oben erhalten. Die Dokumentationsgrenze bleibt **80 Prozent**; zuletzt
meldete CodeRabbit **85.27 Prozent**. Der Wert gilt nicht als neue Messung auf
einem späteren Stand. Die geschätzte Reviewarbeit 5/Critical/90 Minuten ist
keine Risiko-, Architektur- oder Sicherheitsbewertung. Eine gesonderte neue
Risiko-/Architektureinstufung wurde in dieser Runde nicht bereitgestellt.
Alle Zusatz-, Nitpick-, Outside-diff- und operativen Texte wurden gelesen.

Die [fehlgeschlagene Windows-CI auf 939d7f0](https://github.com/Puma7/meeting-for-handy/actions/runs/42166489882/job/127724106073)
bleibt erhalten. Sie wird nicht diesem neuen Stand als bestanden zugerechnet.
Für den endgültigen Head sind zwei frische saubere Codex-Codeprüfungen,
eine vollständige saubere CodeRabbit-Runde und seine eigene grüne aktuelle CI
nötig. Tatsächliche spätere Review-/CI-Links werden im PR nachgelesen und separat
vermerkt, ohne nachträglich Produktbytes umzuschreiben. Alte oder leere
Security-Flags sind kein aktueller formaler Securitynachweis.

## Erhaltene frühere Nachweise und Grenzen

Der englische Referenzpatch 93335102ed bleibt in der Historie enthalten.
Frühere Korrekturen an Quellenidentität, manuellen Statusänderungen, historischen
Aufgaben/Beschlüssen, Kontextwiederholung, einmaliger Workspace-Projektion,
Notizversionen, Kartenmenüs, Kontrast und Fehlerdiagnostik bleiben erhalten.
Die bisherigen ausführlichen Vorher-/Nachherfälle sind im unveränderlichen
Dokument und dessen Git-Historie erreichbar.

Frühere Modellfehlläufe auf 49, 52, 53, 54 und 59 bleiben ihren damaligen Ständen
zugeordnet: unvollständige positive Zusage, ungelöste Kontextkennungen,
NumberNotQuoted, fehlender deutscher Impuls bei zuvor fehlender Frage und
Antwortlimit. Ungültige Ergebnisse wurden nicht übernommen; vorhandene
Checkpoints und menschliche Änderungen blieben erhalten. Wo der Helfer die
abgelehnte Antwort oder Finish-Reason nicht separat speicherte, bleibt die
weitergehende Ursache offen. Die spätere explizite deutsche Budgetfrage behielt
alle Zusagen und unverbindlichen Aussagen bei; Englisch enthielt sie bereits.
Kein Fehllauf wurde durch nachträgliche Grenzänderung in einen Erfolg umgedeutet.

Die gemeinsame Rust-Validierung und Oberfläche gelten für Windows, Linux und
macOS. Die frische lokale Referenz und Browserprüfung stammen von Windows;
Ziel-CI und ihre tatsächlichen Plattform-Verdicts werden separat auf demselben
finalen Head geprüft. Frühere CPU-/virtuelle Audio-/Neustartnachweise sind
historische Einzelnachweise und werden nicht auf einen neuen Head übertragen.
Virtuelle Audioaufnahme mit Startunterbrechungen ist kein lückenloser
physischer Aufnahmenachweis. Frühere fehlgeschlagene Plattformläufe bleiben erhalten.

Originalaufnahmen, Journale, Transkriptstände, menschliche Bestätigungen und
Modellfassungen bleiben versioniert erhalten. Abgelehnte Antworten verändern
keinen bestätigten Stand. Automatische erweiterte Auswertung bleibt lokal;
Cloud braucht je Auswertung ausdrückliche Textfreigabe und erhält weder Audio,
Dateipfade noch Sitzungsobjekte. Keine Weiterleitungen, System-Proxys oder
automatischer Anbieterwechsel. Private Vorbereitung bleibt privat. Schlüssel
werden weder protokolliert noch als Klartext gespeichert.

Statische Produktprompts, statische Vertragsdateien, Belegbudget, echte
Produktfristen, Ollama-TTL 300 s/Erneuerung 60 s, Schutz-/Reviewerregeln,
Abhängigkeiten, Lizenznachweise, Version und vollständiger TEST_REPORT sind
unverändert. Die dokumentierte strikte LM-Studio-Eigentümerschaft wird wieder
für beide Auswertungsmodi eingehalten; fremde Modelle werden nicht entladen.
Synthetische Tests bleiben als solche gekennzeichnet. Bekannte gesperrte
Abhängigkeitsrisiken bleiben dokumentiert.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und zweistündige Gerätetests
wurden auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-, kombinierte
Zweistundenlast-, unabhängige installierte Prozess-, Signatur-, Notarisierungs-
oder forensische Freigabe. Cloud-Prüfungen ohne echte Zugangsdaten bleiben
Simulationen. Unbekannte Zuständige und Termine bleiben unbekannt;
Abschnittszeiten sind keine Wortzeiten. Technische Belegprüfung beweist keine
inhaltliche Deutung. Ergebnisse bleiben überprüfbare KI-Entwürfe.
Diese Nacharbeit erstellt keinen installierten Programmstand und keinen Release.
