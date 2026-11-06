# PR 41: Validierung der Reviewnacharbeit

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.

Der optionale Advanced Meeting Assistant ergänzt die klassische Transkriptansicht.
Parakeet Ultra verarbeitet geschlossene Abschnitte beider Spuren nacheinander;
die Audiosicherung läuft unabhängig. Langsame Auswertungen werden nicht durch
Intervallticks unterbrochen. Diese Notiz enthält nur synthetische Prüfsituationen.
Der vollständige TEST_REPORT bleibt gegenüber der PR-Basis byteunverändert.
Private Rohbelege und Nutzermeetings werden nicht veröffentlicht.

## Aktuelle Korrektur und Regressionen

Die [vollständige CodeRabbit-Runde auf 078d710](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5799080244)
enthielt keinen neuen nachgewiesenen Produktfehler, aber einen berechtigten
Testhinweis. Der native Kontext-Test hält jetzt gleichzeitig Recording-,
LiveNotes- und gemeinsame OS-Publikationsreservierungen. Ein tatsächlich belegter
Active-Slot mit eigenem wartenden Testworker bleibt während aller Änderungen
aktiv. Gemeinsamer Text wird bei synthetisch auf updating gesetzter Auswertung zurückgestellt;
menschliche Statusänderungen werden dabei dauerhaft gespeichert. Nach der
Inferenz wird die gemeinsame Notiz dauerhaft übernommen. Aufnahmeslot, Stoppsignal,
Worker und ursprüngliche Manifestbytes bleiben durchgehend erhalten. Ausschließlich
synthetischer Worker und Sitzung, kein Gerät, Audio oder Modellzugriff.

Die verstärkte Regression besteht mit unverändertem Produktverhalten. Eine nur
lokale, absichtlich falsche zusätzliche Aufnahmesperre ließ sie tatsächlich
fehlschlagen. Diese Kontrollmutation wurde bytegenau zurückgenommen und niemals
veröffentlicht. Das belegt einen aussagekräftigen Test für eine bisherige
Abdeckungslücke; es wird kein zuvor nachgewiesener Produktfehler behauptet.

Dieselbe Runde konnte validate.rs nicht vollständig prüfen. Die Ursache der
Anbietermeldung ist nicht belegt. Die reinen Belegbedingungen stehen jetzt in
assertions.rs, getrennt von der dauerhaften Zustandsvalidierung. Sämtliche
Funktionskörper, Literale und Bedingungen sind bytegleich; nur Import,
gleichrangige Weitergabe und ein abschließender Zeilentrenner ändern die Struktur.
Der Hauptteil schrumpft von 72.581 Bytes auf rund 24 kB, der reine Belegteil hat
rund 49 kB. Weitere vollständige Testblöcke wurden ohne Inhaltsänderung in
verwandte bestehende Module eingeordnet: alle 80 Namen und vollständigen
Test-/Hilfsblöcke bleiben bytegleich, abgesehen von abschließenden Modultrennern.
Diese vollständige Zählung schließt den unveränderten Unix-only-Fall ein;
die zuvor ausgewiesenen 79 zählten nur die ohne zusätzliches cfg-Attribut.
Weiterhin 102 tatsächliche PR-Dateien, 100 unter den unveränderten zwei
Lockdatei-Filtern prüfbar. Keine neue Ausnahme und keine abgesenkte Prüfschwelle.
Erst eine neue vollständige Anbieterrunde kann die Reichweitenlücke schließen.

### Erhaltene Updaterkorrektur

Die [Codex-Runde auf 5644ec0](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5798412142)
fand einen gültigen Updaterfehler: Der bisherige Zahlenparser wies die eigene
Vorabversion 1.1.0-beta.1 ab. Eine neue Regression reproduzierte tatsächlich
Unsupported release version. Der Versionsvergleich verwendet jetzt die bereits
gesperrte SemVer-Bibliothek 1.0.27 und ignoriert Buildmetadaten bei der Rangfolge.
Der Wechsel von Beta zu stabiler 1.1.0 wird angeboten, gleiche oder ältere
Versionen nicht. Fehlerhafte Versionen und als stabil fehlmarkierte Vorabtags
bleiben gesperrt. Dieser Kanal bietet weiterhin ausschließlich stabile Tags an.
Downloadherkunft, Größenlimit, Prüfsummen, Plattformauswahl und Installationsschutz
bleiben erhalten. Die Bereinigung erkennt weiterhin nur eigene stabile partielle
Downloads; Vorab- und Metadaten-Dateinamen bleiben unberührt. Synthetische
Release-Metadaten und lokale Testdateien, kein echter Download oder Installationslauf.

Nur eine direkte Abhängigkeitskante kommt hinzu. Paketidentitäten, Prüfsummen,
Versionen und aufgelöste Features sind unverändert. Die Lizenzsammlung wurde für
alle vier Ziele offline geprüft; die vier Manifeste sind an die neuen Lockbytes
gebunden. Vollständige Quelltestmodule wurden in zwei bereits geprüfte verwandte
Module zusammengeführt. Alle 79 Workspace-Testnamen sowie deren vollständige
Funktionen, Attribute und Assertions bleiben erhalten. So kann der vollständige
Anbieterlauf weiterhin jede der 100 zulässigen Dateien der 102-Dateien-PR prüfen;
keine zusätzlichen Filter, keine ausgeschlossenen Funktionen oder Regeländerungen.

### Vorherige Inhalts- und Wiederaufnahmekorrekturen

Die [Codex-Runde auf 0542e31](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5797710027)
fand einen gültigen Fehler: We denied that the report is complete bestätigte
fälschlich einen Abschluss. Dasselbe galt für bestrittene Rücknahmen und Beschlüsse.

Die gemeinsame Prüfung verlangt jetzt für jeden übergeordneten dass/that-Satz
eine finite bejahte Bestätigung oder Aussage. Unbekannte übergeordnete Prädikate
werden nicht als Bestätigung gewertet. Jede Verschachtelung wird gesondert
geprüft; eine bestätigte Bestreitung bestätigt nicht den bestrittenen Inhalt.
Die bestehenden Modal-, Infinitiv-, Mengen-, Verneinungs- und Quellenregeln bleiben
erhalten. Bekannte ausdrückliche Bestreitungen ohne dass/that bleiben ebenfalls
unzulässige Statusbelege. Originalzitate werden nicht umgeschrieben.

Die drei vorhandenen dauerhaften Regressionen enthalten 43 zusätzliche Literale
und 63 Szenarien. Die erste Gruppe mit 30 Literalen/41 Szenarien reproduzierte
auf dem veröffentlichten Stand fünf falsche Beschlüsse, acht Abschlüsse und je
acht Aufgaben-/Beschlussrücknahmen: 29 tatsächliche Fehlannahmen. Dazu gehören
auch bestrittene, vorgestellte und verschachtelte Aussagen mit zuvor unbekannten
übergeordneten Verben. Alle neuen und bisherigen Fälle bestehen nachher.

Sechs weitere Literale mit acht Szenarien prüfen eine noch laufende Prüfung
gegen eine ausdrücklich abgeschlossene Prüfung. Ein unveröffentlichter
Zwischenstand nahm dabei vier falsche Statusaussagen an. Dieser Fehlversuch bleibt
erhalten und zählt nicht als bestanden; die abschließende Prüfung besteht.
Auch der alte Sonderpfad für nicht mehr benötigte Anforderungen verwendet
jetzt dieselbe übergeordnete Prüfung. Vier lokal reproduzierte falsche Rücknahmen
je Zielart wurden mit sieben weiteren Literalen/14 Szenarien abgesichert.
Hoffnungen, Bestreitungen und Prüfaufträge reichen dort nicht aus; tatsächlich
bestätigte Rücknahmen und unabhängige Folgesätze bleiben zulässig.
Die ersten bestandenen Modellreferenzen dieser Korrektur gehörten noch zum
Zwischenstand vor dieser Ergänzung. Sie bleiben erhalten und werden dem
abschließenden Stand nicht zugerechnet. Das unten ausgewiesene Paar wurde
getrennt vorab auf dem vollständig geprüften ergänzten Stand festgelegt.
Lange tatsächlich bestätigende Sätze, bestätigte Folgesätze, Namen und
Themenbezeichnungen wie The denied report bleiben zulässig. Diese vorige Korrektur ergänzte keine neue
Testfunktion oder Datei und senkte keine Schwelle. Die Erkennung
ist begrenzt und konservativ, kein vollständiger Grammatik- oder Wahrheitsbeweis.

Die vorherige Wiederaufnahmekorrektur bleibt vollständig erhalten: Nach dem
Widerruf einer freigegebenen Notiz werden terminale Identitäten ausschließlich
intern und schreibgeschützt zur Deduplizierung verwendet, wenn ihre Originalbelege
noch gültig sind. Die tatsächlichen synthetischen HTTP-Regressionen, neun
Gegenfälle mit ungültigen Statusbelegen, getrennte Replayfenster, Schutz
menschlicher Änderungen, alte Checkpointbytes, unabhängige neue Zusagen und
mehrfache Wiederöffnung bestehen weiter. Zurückgezogene Notiztexte und
ausgeblendete KI-Entwürfe werden nicht erneut an das Modell gesendet.

## Tatsächlich geprüft

| Prüfung | Ergebnis und Zuordnung |
| --- | --- |
| Rust lokal unter Windows, Workspace, alle Targets | Frisch **569 bestanden**, darunter **169 Notizenfälle** und **37 Providerprotokollfälle**; drei bestehende besondere Einstiege regulär ignoriert. |
| Format und Clippy | Frisch bestanden, gesperrte Abhängigkeiten, Warnungen als Fehler. |
| Frontend | Bytegleich zu f7fc62c; dessen **417 Tests in 21 Dateien**, TypeScript, Build und Format wurden hierfür nicht erneut ausgeführt. |
| Browser | Bytegleich zur tatsächlichen vorherigen **16er-Matrix**, nicht erneut ausgeführt. |
| Projektprüfung | Frisch verify.ps1: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen bestanden. |
| Deutsch, echtes lokales Modell | Frisch nach der Modulaufteilung: **147.1481294 s**, **0** reguläre Validierungswiederholungen; alle fünf Kriterien bestanden. |
| Englisch, echtes lokales Modell | Frisch nach der Modulaufteilung: **114.8688505 s**, **0** reguläre Validierungswiederholungen; alle fünf Kriterien bestanden. |

Beide frischen Modellreferenzen liefen nach beendeten eigenen nativen Prüfungen
auf der neuen Modulstruktur und nutzen den unveränderten synthetischen Gesprächskorpus.
Englisch einschließlich I’ll und der ursprüngliche englische Referenzpatch
93335102ed bleiben erhalten. Die fünf Kriterien bestehen: positive Aufgabe,
unverbindliche Aussage nicht als Aufgabe, erhaltene Impulse, menschlicher
Impulsstatus nach Protokolländerung und bytegleiche Wiederöffnung. Ungültige
Antworten zählen nicht als Erfolg. Je Anfrage ist höchstens eine vollständig
validierte Wiederholung zulässig; null Wiederholungen sind kein Freigabekriterium.

Verwendet wurde nur das vorhandene google/gemma-4-e4b Q4_K_M mit
**6.326.936.720 Modellbytes**, LM Studio **0.4.26+4**, Windows 11 Pro **26200**,
Intel **i7-13700** und rund **64 GiB** sichtbarem physischem RAM. Version und
Rechnerdaten wurden frisch gelesen. Zeitmessung: verstrichene Sekunden vom
Start bis zur validierten dauerhaften Speicherung und Wiederöffnung.
Während der Inferenz liefen keine eigenen Builds, Tests oder Browserprüfungen;
lesende GitHub-Abfragen konnten parallel stattfinden. Keine Downloads, Cloud
oder Nutzermeetings. Fremde Prozesse blieben erhalten; ihre Last war nicht
isoliert. Working Set, Private Bytes und Grafikspeicher wurden nicht neu
gemessen. Modell-Dateigröße ist keine RAM-Messung.

## Reviews und erhaltene Nachweise

Zwei frische Codex-Codeprüfungen auf dem vorherigen unveränderten 078d710 waren
sauber: [erste Quittung](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6682766893),
[zweite Quittung](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6682865939).
Die anschließende volle CodeRabbit-Runde 6eb3e1b5-6522-4f39-9403-ac930f0181c7
prüfte Basis bis 078d710 und wählte alle tatsächlichen 100 zulässigen Dateien aus,
ließ aber validate.rs unverfügbar und enthielt den oben ergänzten Testhinweis.
Sie zählt ausdrücklich nicht als saubere vollständige Runde. Dokumentation
83.21 Prozent bei unveränderter 80-Prozent-Grenze; 131 Funktionen in 50 Dateien,
48 ausgelassen (19 nicht unterstützt, 29 über dem Limit des Dokumentationstools).
LanguageTool prüfte nicht alle Textstücke der Notiz, Ursache unbekannt.
Keine gesonderte aktuelle Risiko-/Architektureinstufung in diesem Snapshot.
Alle Risiko-, Zusatz-, Nitpick-, Outside-diff- und operativen Texte wurden gelesen.
Die neue Quellstruktur benötigt neue Prüfungen auf ihrem eigenen finalen Head;
vorherige CODE-/CI-Ergebnisse werden ihr nicht zugerechnet.

Der [vollständige CodeRabbit-Lauf auf f7fc62c](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6678669089)
endete ohne neue Befunde. Alle 100 zulässigen Dateien wurden unabhängig mit
den tatsächlich paginierten 102 PR-Dateien abgeglichen; nur die zwei bestehenden
Lockdatei-Filter blieben angewandt. Keine Datei war mehr unverfügbar.
AudioSources.tsx enthielt laut Anbieter keine prüfbare Änderung; der kleine
Hilfstext-Diff wurde manuell gelesen. Risiko Minimal; keine gesonderte neue
Architektureinstufung. Dokumentation 85.27 Prozent bei unveränderter 80-Prozent-Grenze.
Die Dokumentationsprüfung selbst ließ 18 nicht unterstützte und 31 über ihrem
Dateilimit liegende Dateien aus. LanguageTool konnte weiterhin nicht alle
Textstücke prüfen; Ursache unbekannt, Tool und Regeln bleiben aktiv.
Die geschätzte Reviewarbeit 5/Critical/90 Minuten ist keine Sicherheitsbewertung.
Alle Zusatz-, Risiko-, Nitpick-, Outside-diff- und operativen Texte wurden gelesen.
Wegen der zwei Codex-Befunde war dieser Stand insgesamt nicht abgeschlossen.

Alle **16 ursprünglichen Sourcery-Anmerkungen** wurden einzeln geprüft,
beantwortet und geschlossen. Alle **75 bisherigen CodeRabbit-Threads** hatten
eine Einzelantwort und waren geschlossen. Neue gültige Befunde werden erst
nach tatsächlicher Veröffentlichung der geprüften Korrektur beantwortet.
Die frühere Linux-CI auf f7fc62c scheiterte bei virtuellen Spuren mit 10091/10094 ms
über der unveränderten 10000-ms-Grenze; Startunterbrechungen waren gemeldet.
Ursache des Überlaufs unbekannt, Fehllauf erhalten, keine Grenze geändert.
Der spätere Linux-Lauf auf 0542e31 wurde beim vollständigen GUI-Build
abgebrochen. Seine Paketinstallation dauerte rund 35 Minuten mit langen
Downloads vom Ubuntu-Azure-Spiegel. Der Zeitpunkt passt zur unveränderten
50-Minuten-Jobgrenze; der Log nennt nur den Abbruch, keine getrennt bestätigte
Ursache. Er zählt nicht als Erfolg. Keine Frist, Paket- oder Workflowänderung.
Für den endgültigen Head bleiben zwei frische saubere Codex-Codeprüfungen,
eine vollständige saubere CodeRabbit-Runde und seine eigene grüne CI nötig.
Spätere tatsächliche Links werden separat im PR dokumentiert. Alte oder leere
Security-Flags sind kein aktueller formaler Securitynachweis.

Die [unveränderliche vorige Fassung mit allen Detailnachweisen](https://github.com/Puma7/meeting-for-handy/blob/0542e31ef7fc1c2e2be161b91641db96fd702cce/docs/validation/pr41-review-followup-20261104.md)
erhält frühere Fehler, Korrekturen, Fehlläufe und deren Grenzen. Dazu gehören
Batch-Fixture-Synchronisation bei unveränderten produktiven ASR-Bytes und
Fristen, Belegnavigation, englische Beschriftung, strikte LM-Instanzeigentümerschaft,
historische Aufgaben, manuelle Statusänderungen, Kontextwiederholung und UI.
Frühere fehlgeschlagene Modellversuche bleiben ihren jeweiligen Ständen zugeordnet.
Wo Antwort oder Finish-Reason nicht separat gespeichert wurden, bleibt die
weitergehende Ursache offen. Keine nachträgliche Änderung von Messgrenzen.

Die tatsächliche vorherige Browsermatrix verwendete Edge 156.0.4314.58 und
Playwright 1.57.0: 390/1280 Pixel, Deutsch/Englisch, hell/dunkel und fertige/laufende
Abschnitte. Quellenlinks, Notizen, Impulsstatus und auf Anforderung erzeugtes
stilles synthetisches WAV bestanden ohne fremde Anfragen, relevante Fehler oder
horizontalen Überlauf. Eigene Kontexte und Server wurden beendet. Dies war eine
Komponentenprüfung mit synthetischer Adaptergrenze, keine native IPC-, WebView-,
Screenreader-, echte Audioausgabe- oder Geräteprüfung.

## Grenzen und unveränderte Schutzregeln

Aufnahmen, Journale, Transkriptstände und menschliche Bestätigungen bleiben
versioniert erhalten. Private Vorbereitung bleibt privat. Automatische Auswertung
bleibt lokal; Cloud benötigt ausdrückliche Textfreigabe je Auswertung und erhält
weder Audio noch Dateipfade oder Sitzungsobjekte. Keine Weiterleitungen,
System-Proxys oder automatischen Anbieterwechsel. Keine fremden Modellentladungen.
Statische Prompts, Verträge, Belegbudget, Produktfristen, Ollama-TTL 300 s mit
Erneuerung alle 60 s, Paketversionen, Lizenztexte, Version und Schutz-/Reviewerregeln
sowie der vollständige TEST_REPORT bleiben unverändert. Die einzige neue direkte
Abhängigkeitskante ist oben ausgewiesen.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und Zweistundenprüfungen sind
auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-, kombinierte
Zweistundenlast-, unabhängige installierte Prozess-, Signatur-, Notarisierungs-
oder forensische Freigabe. Frühere CPU-, virtuelle Audio- und Neustartbelege
werden nicht auf einen neuen Head übertragen. Virtueller Ton mit Startlücken
beweist keine lückenlose physische Aufnahme. Cloudtests ohne echte Zugangsdaten
bleiben Simulationen. Zuständige und Termine bleiben unbekannt, wenn unbelegt;
Abschnittszeiten sind keine Wortzeiten. Technische Belegprüfung beweist keine
inhaltliche Deutung. Ergebnisse bleiben überprüfbare KI-Entwürfe.
Diese Nacharbeit erstellt keinen installierten Programmstand oder Release.
