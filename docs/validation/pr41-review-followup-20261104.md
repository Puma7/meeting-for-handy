# PR 41: Validierung der Reviewnacharbeit

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.

Der optionale Advanced Meeting Assistant ergänzt die klassische Transkriptansicht.
Parakeet Ultra verarbeitet geschlossene Abschnitte beider Spuren nacheinander;
die Audiosicherung läuft unabhängig. Langsame Auswertungen werden nicht durch
Intervallticks unterbrochen. Diese Notiz enthält nur synthetische Prüfsituationen.
Der vollständige TEST_REPORT bleibt gegenüber der PR-Basis byteunverändert.
Private Rohbelege und Nutzermeetings werden nicht veröffentlicht.

## Aktuelle Korrektur und Regressionen

Die [Codex-Runde auf f175d7f](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5796980202)
fand einen weiteren echten Fehler: We must check that the report is complete
konnte einen Abschluss bestätigen; derselbe Prüfauftrag konnte einen Beschluss
zurücknehmen. Die lokale Modalprüfung begann erst hinter that und verlor damit
den noch auszuführenden übergeordneten Auftrag.

Die gemeinsame Prüfung verfolgt jetzt auch die Modalität des übergeordneten
Satzes über that/dass und Kommas hinweg. Sie berücksichtigt verpflichtende
Infinitive mit have/need to und ausdrückliche höfliche oder imperative Prüfaufträge.
Der konkrete Prüfauftrag allein bestätigt weder Abschluss noch Rücknahme oder
Beschluss. Ein unabhängig bestätigter Folgesatz, ein tatsächlich abgeschlossener
Prüfvorgang und May als Name oder Themenbezeichnung bleiben zulässig.

Die drei vorhandenen dauerhaften Regressionen enthalten **43 zusätzliche
Literale und 59 Szenarien**. Vorher wurden sechs Beschlüsse, elf Abschlüsse und
je zwölf Aufgaben-/Beschlussrücknahmen fälschlich angenommen: **41 tatsächliche
Fehlannahmen**. Nachher bestehen alle neuen und bisherigen Fälle einschließlich
positiver Gegenfälle. Ablehnung verändert keinen bestätigten Stand und erzeugt
keine neue Revision. Originalzitate, Checkpointbytes und mehrfache Wiederöffnung
bleiben erhalten. Keine neue Testfunktion oder Filterausnahme.
Die Konstruktionserkennung ist begrenzt und konservativ; sie beweist weder
vollständige Grammatik noch Wahrheit. Die früheren Hoffnungs-, Modalform-,
Infinitiv-, Mengen-, Verneinungs- und Nichtstun-Regressionen bleiben bestehen.

## Tatsächlich geprüft

| Prüfung | Ergebnis und Zuordnung |
| --- | --- |
| Rust, Workspace, alle Targets | Frisch **568 bestanden**, darunter **169 Notizenfälle** und **37 Providerprotokollfälle**; drei bestehende besondere Einstiege regulär ignoriert. |
| Format und Clippy | Frisch bestanden, gesperrte Abhängigkeiten, Warnungen als Fehler. |
| Frontend | Bytegleich zu f7fc62c; dessen **417 Tests in 21 Dateien**, TypeScript, Build und Format wurden hierfür nicht erneut ausgeführt. |
| Browser | Bytegleich zur tatsächlichen vorherigen **16er-Matrix**, nicht erneut ausgeführt. |
| Projektprüfung | Frisch verify.ps1: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen bestanden. |
| Deutsch, echtes lokales Modell | **159.9218233 s**, **0** reguläre Validierungswiederholungen. |
| Englisch, echtes lokales Modell | **122.5649442 s**, **0** reguläre Validierungswiederholungen. |

Beide Modellreferenzen nutzen den unveränderten synthetischen Gesprächskorpus.
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
Für den endgültigen Head bleiben zwei frische saubere Codex-Codeprüfungen,
eine vollständige saubere CodeRabbit-Runde und seine eigene grüne CI nötig.
Spätere tatsächliche Links werden separat im PR dokumentiert. Alte oder leere
Security-Flags sind kein aktueller formaler Securitynachweis.

Die [unveränderliche vorige Fassung mit allen Detailnachweisen](https://github.com/Puma7/meeting-for-handy/blob/f175d7f6d06ee632dc35427accbd34be29d03612/docs/validation/pr41-review-followup-20261104.md)
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
Erneuerung alle 60 s, Abhängigkeiten, Version, Lizenz- und Schutz-/Reviewerregeln
sowie der vollständige TEST_REPORT bleiben unverändert.

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
