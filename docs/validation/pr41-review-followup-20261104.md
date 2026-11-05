# PR 41: Validierungsstand

Stand: 5. November 2026. Version: **1.1.0-beta.1**, unverändert.
Diese Fassung korrigiert zusätzlich kontrahierte englische Erledigt-Meldungen für bestehende
Aufgaben. Die vorherigen Intervall-, Testserver-, Speicherhinweis- und Titelkorrekturen bleiben
enthalten. Aktuell ändern sich ausschließlich die native Erledigt-Klassifikation, deren
Validierungs-/Speichergegenfälle und dieser bereinigte Vermerk. Prompts, Transport, Audio/ASR,
Modellreservierungen, Oberfläche, Abhängigkeiten und Schutz-/Reviewerregeln bleiben unverändert.
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

Rusttests, Clippy, Rustformat und beide echten lokalen Modellreferenzen sind für die
Erledigt-Korrektur frisch bestanden. Oberfläche, TypeScript, Build und Frontendformat stammen
vom bytegleichen `fc8433f`-UI-Stand und wurden nicht wiederholt. Die Browserprüfung stammt von
der Titelkorrektur `b387093`; ihre Komponenten und der Prüfer bleiben unverändert.
Ältere Paket-/Langsammodellnachweise werden nicht als neu ausgeführt ausgegeben.
`verify.ps1` ist nach dem letzten Dokumentedit vor Quellupload frisch erforderlich.

| Prüfung | Tatsächliches Ergebnis |
| --- | --- |
| Rust-Workspace, alle Targets | Frisch 560 bestanden, darunter 161 Notizen- und 37 Providerprotokolltests; drei bestehende besondere Einstiege regulär ignoriert. Die neue Erledigt-Regression prüft vier Kontraktionsformen sowie ausgeschriebene und abgelehnte Gegenfälle über die tatsächliche Validierung und dauerhafte Wiederöffnung. Frühere 559/558/557/555 Zahlen sind historisch. |
| Rustformat und Clippy | Frisch bestanden; gesperrte Abhängigkeiten und Warnungen als Fehler. |
| Oberfläche | 406 Tests in 21 Dateien, TypeScript, Produktionsbuild und vollständige Frontendformatprüfung auf dem hier bytegleichen `fc8433f`-UI-Stand bestanden, lokal nicht wiederholt. Fünf tatsächliche Intervall-Vorherfehler und zwei doppelte Speicherhinweise sind dort korrigiert. Zwei Testworker, unveränderte Fristen und Schutzregeln. Frühere 399/395/391/385 Zahlen sind historisch. |
| Projektprüfung | Vorherige veröffentlichte Stände bestanden 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen. Für jede Quellveröffentlichung nach dem letzten Dokumentedit erneut vor Upload erforderlich. |
| Echte lokale Referenzen | Frisch bestanden: Deutsch 112.0512475 s und Englisch 122.7768458 s, jeweils 0 reguläre Validierungswiederholungen. Positive Aufgabe, unverbindliche Aussage, Impulserhalt, menschlicher Status nach Protokolländerung und bytegleiche Wiederöffnung bestanden. Wörtliches englisches „I’ll“ bleibt erhalten. Eine vollständig validierte Wiederholung je Anfrage bleibt zulässig; null Wiederholungen sind keine Freigabeschwelle. Frühere Modellreferenzen sind historische Belege. Die vier konkreten Erledigt-Kontraktionen wurden zusätzlich kontrolliert über die tatsächliche Validierungs-/Speicherschnittstelle geprüft; die realen Referenztexte bleiben unverändert. |
| Paketregression | 21 Prüfungen auf `91f0985`; Paketdateien seitdem unverändert. Tatsächlich erzeugter synthetischer ZIP prüft Streaming- oder Batchhinweis, Launcher, Fingerprint und Nichtüberschreibung. Kein lokales Produktpaket oder Installer daraus bereitgestellt. |
| Kontrolliertes Langsammodell | Auf `35d53e8`: 244.33 s Gesamtlauf, 210 s SSE-Antwortpause, nächster Refresh nach 210.0421383 s, maximal eine Modellanfrage, ein erhaltener Impuls. 3737920 Samples je Spur / 16000 Hz = 233.62 s je Spur. Beide PCM-Schreiber liefen nach Auswertungsstopp weiter. Hier nicht erneut ausgeführt. |
| Sidebar im Browser | Auf `b387093`, hier nicht erneut ausgeführt: 24 tatsächliche Layoutabläufe plus 24 kombinierte Warn-Titel: 390/740/1280 Pixel, DE/EN, hell/dunkel, ein-/ausgeklappt; Titel zusätzlich stilles/stummes Mikrofon mit Aufnahmewarnung und Rückstand. Eigener Server/Browser beendet, keine Seiten-/Konsolenfehler, Fremdanfragen oder horizontalen Überläufe. Die Titelattribute stammen aus dem tatsächlichen Browser-DOM; kein nativer OS-Tooltip- oder Screenreader-Sprachnachweis. |

Die echten Referenzen verwendeten ausschließlich synthetischen Text und das vorhandene
**google/gemma-4-e4b Q4_K_M**, 6.326.936.720 Modellbytes, laufendes LM Studio mit Dateiversion **0.4.26+4**,
Windows 11 Pro **26200**, Intel **i7-13700**, rund **64 GiB** sichtbares physisches RAM.
Keine Downloads oder Cloud. Während dieser neuen nativen Referenzen liefen keine anderen eigenen Prüfungen.
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

## Zusätzliche Quantoren- und Zwischenwortgegenprüfung

Die eigene Gegenprüfung auf `cfa2683` fand weitere Varianten derselben Mechanismen.
„None of us will send …“ und „Neither Robin nor Casey will send …“ durften keine
Aufgaben anlegen. Zudem waren „wurde bereits erstellt“ und der kommafortgesetzte Zustand
„ist bereits erstellt, wie besprochen“ noch nicht vollständig gesperrt.
Die tatsächliche klassische API reproduzierte vier falsche Annahmen, der tatsächliche
Runner drei; er blockierte die satzfinale wurde-bereits-Variante bereits.
Diese unterschiedlichen Vorherergebnisse bleiben getrennt dokumentiert.

None/neither werden jetzt im bereits abgegrenzten Übernahmeausdruck geprüft.
Ein festes „I will check that none …“ oder „I will check whether neither …“ bleibt gültig.
Marker-Zulassung und klassische Generierung verwenden dieselbe Hilfsverberkennung für
abgeschlossene Erstellung, auch mit Zwischenwörtern. Die Rückwärtssuche endet an einer
Satzteilgrenze oder Koordination. Aktive Zusagen nach einer solchen Grenze und abgeschlossene
Zustände im versprochenen Prüfgegenstand bleiben erhalten. Zusätzliche Gegenfälle decken
mehrere Zwischenwörter und „Robin ist bereit und erstellt …“/„Robin war krank, erstellt …“ ab.
Das ist eine begrenzte Sprachheuristik und kein vollständiger syntaktischer Nachweis.

Die API-/Runnergegenfälle bestehen mit wörtlichen Originalbelegen und bytegleicher Wiederöffnung.
Alle bisherigen Fälle, historischen Leser und Statuswechsel bleiben erhalten.
Nach der letzten nativen Änderung wurden alle Rusttests, Clippy, Format und beide tatsächlichen
lokalen synthetischen Modellreferenzen frisch ausgeführt. Keine ältere Referenz oder CI wird
diesem Korrekturstand zugerechnet; die unveränderten Frontendbytes behalten ihren ausdrücklich
historisch bezeichneten 395-Test-Nachweis.

## Aktueller nativer Kontext beim Modellwechsel

Der nächste [Codex-Befund auf cfa2683](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4479890221)
war gültig: Bei A → B → A behielt der wiederverwendete A-Lauf seine älteren freigegebenen
Notizen und Agendapunkte. Die tatsächliche Vorherregression reproduzierte Bearbeitung und
Widerruf unter B, ältere Eingaben bei A und deren erneute Übertragung in facts/reconcile.
Auch der erneute A-Neustart und ein frischer C-Lauf konnten den älteren Browserstand übernehmen.
Eine zweite Vorherregression zeigte die stille Fortsetzung trotz beschädigtem nativem Kontext.

Die Initialisierung wählt nun tatsächliche dauerhafte Änderungen freigegebener Eingaben,
unabhängig von Modell-Ausgabezeitstempeln und bloßen manuellen Bestätigungen. Ein künstlicher
späterer Modellzeitstempel in der Regression ist keine menschliche Freigabe. Ältere Journale
ohne vergleichbare Freigabezeitstempel begründen keine geratene Reihenfolge. Unlesbare oder
gleichzeitige widersprüchliche Kontextstände blockieren, statt einen älteren freigegebenen Text
zu erraten.
Der Browsercache kann vorhandene native Freigaben nicht ersetzen.

Agenda und freigegebene Notizen werden als neue lokale Kontextrevision übernommen.
Die Override-Linie des Ziellaufs bleibt erhalten; menschliche Bestätigungen werden weiterhin
getrennt nach ihrem bisherigen Vertrag vererbt. Originale und frühere Kontextdateien bleiben
byteunverändert. Die kontrollierten lokalen HTTP-Gegenfälle prüfen, dass ältere Notizen/Agendapunkte
beide Phasen nicht erreichen. Aktuelle Notizen erreichen beide Phasen; aktuelle Agendapunkte
werden in der dafür vorgesehenen reconcile-Phase geprüft. Die facts-Phase erhält weiterhin
ihren eingeschränkten Kontext. Dies ändert weder Phasenauftrag noch Schutzgrenzen.

Wiederöffnung ist bytegleich, derselbe Lauf und ein neuer Modelllauf behalten die neuen
Freigaben beziehungsweise den Widerruf. Alle 159 Notizenprüfungen bestehen. Diese Prüfung
belegt den lokalen serialisierten Start-/Wiederaufnahmepfad, keine atomare Koordination
beliebiger fremder Programme. Eine Zwischenreferenz vor dieser Kontextkorrektur scheiterte
an einem verworfenen Modellvorschlag als Beschluss; sie bleibt erhalten und zählt nicht als
finaler Referenznachweis. Die oben genannten finalen Referenzen stammen erst nach allen
Aufgaben- und Kontextkorrekturen; die unveränderte begrenzte Antwortwiederholung bleibt aktiv.

## Abgrenzung von Bestätigung und Freigabechronologie

Eine verstärkte tatsächliche Vorhergegenprobe ergänzte eine spätere manuelle Bestätigung in A,
nachdem B Notizen und Agenda geändert beziehungsweise widerrufen hatte. Die erste lokale
Zwischenkorrektur konnte deshalb erneut A auswählen und die älteren Inhalte in facts/reconcile
senden. Diese Fehlversuche bleiben erhalten. Die Chronologie liest jetzt die letzte tatsächliche
Agenda-/Notizänderung aus dem unveränderten nativen Journal; reine Override-Änderungen sind
keine erneute Textfreigabe. Quelle und Zeitstempel müssen denselben gelesenen Kontextstand
bezeichnen; eine zwischenzeitliche Abweichung blockiert.

Eine weitere Vorherregression bewies, dass Laufanlagezeiten keine widersprüchlichen alten
Freigaben ohne Zeitstempel ordnen dürfen. Solche Stände werden nun als unbekannte Reihenfolge
behandelt. Unbekannte Stände sind nur ohne abweichende freigegebene Inhalte kombinierbar.
Widersprüche verlangen eine ausdrückliche Prüfung statt automatischer Wiederverwendung.
Die Regression erhält alle ursprünglichen Dateien bytegleich. Journalformat und bestehende
Leser werden nicht rückwirkend umgeschrieben; es wird kein Zeitstempel erfunden.

Die verstärkte A-B-A-/C-HTTP-Regressionsprüfung und die Altstand-Gegenprobe bestehen.
Frühere 557-Test-Zwischenläufe, frühere lokale Referenzpaare und die erste Projektprüfung sind
historische Belege. Die damaligen 558 Rusttests, 159 Notizenfälle und tatsächlichen
Modellreferenzen gehören erst zum abschließenden Stand der Freigabechronologie.

## Vollständige Titel für kombinierte Aufnahmehinweise

Der vollständige [CodeRabbit-Lauf auf 87eba39](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5785086446)
fand einen gültigen verbleibenden [Titelbefund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4480560193).
Bei stillem oder stummgeschaltetem Mikrofon fehlten zusätzliche Aufnahmewarnungen im Titel
des eingeklappten Status. Vier tatsächliche DE/EN-DOM-Gegenfälle scheiterten vorher.
Mikrofonzustand, Aufnahmewarnungen und Erkennungsrückstand sind jetzt unabhängige Titelgründe.
Der Mikrofontext, die Warnschwelle, Audio/Erkennung und das bisherige höfliche Ankündigungsverhalten
bleiben erhalten. Entfernte Warnungen verschwinden auch aus dem Titel.

Die vier Gegenfälle und die gesamten 399 UI-Tests bestehen. Der vorhandene Browserprüfer
prüft tatsächliche Sidebar-/RecordingStatus-Komponenten: 24 Layoutabläufe und 24 kombinierte
Titelzustände in Edge 156.0.4314.58 mit Playwright 1.57.0.
Die kompakte und die volle Bildschirmaufnahme wurden visuell angesehen. CPU 25 Prozent und RAM 2/8 GiB
bleiben synthetische Fixturewerte, keine Messung des Rechners. Keine native IPC, Aufnahme,
Providersteuerung, Nutzermeetings oder Screenreader-Sprachausgabe sind damit geprüft.
Der erste Browserdurchlauf erwartete nach einer Desktop-Seitenneuladung irrtümlich den zuvor
eingeklappten lokalen Komponentenstatus. Der Prüfer klappt nun regulär über den vorhandenen
Button ein; Fristen und Layoutgrenzen bleiben unverändert. Der erste Fehllauf bleibt erhalten,
der eigene Server wurde auch dabei beendet. Der endgültige Durchlauf besteht vollständig.

Die zwei sauberen Codex-Codequittungen auf dem vorherigen `87eba39` sind
[Runde 1](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6658139444) und
[Runde 2](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6658347014).
Der vollständige CodeRabbit-Lauf war wegen des gültigen Titelbefunds kein sauberer Abschluss.
Die folgende reine UI-/Prüferkorrektur benötigt eigene frische Reviews und CI auf ihrem Head.
Die damaligen Rust-/Modellnachweise bleiben ihrem `87eba39`-Stand zugeordnet; die obige aktuelle Tabelle weist die erneut ausgeführten Rusttests getrennt aus.

## Kontrahierte englische Erledigt-Meldungen

Die frische [Codex-Prüfung auf `fc8433f`](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5786363771)
fand einen gültigen P2-Befund: Bei einer bestehenden Aufgabe wurden „I've sent the report.“,
„I’ve sent the report.“, „We've completed the report.“ und „We’ve completed the report.“
abgelehnt, obwohl ihre ausgeschriebenen Entsprechungen zulässig waren. Die alte Wortzerlegung
erzeugte i/we, ve und das Partizip; die Prüfung verlangte have. Dadurch konnte ein legitimer
Erledigt-Status die gesamte Quelle einschließlich regulärer Wiederholung scheitern lassen.

Nur vollständige affirmative Erstpersonen-Tokens I've und we've werden jetzt in temporärer
Klassifikation zu i/we have erweitert. Gerade und typografische Apostrophe werden dafür
gleichbehandelt. Alle übrigen Apostroph-Tokens werden wie bisher zerlegt; die bestehenden
Verneinungs- und Unsicherheitswörter bleiben aktiv. Quellen, Originalzitate, Journale und
bestehende gespeicherte Fassungen werden dabei nicht normalisiert oder neu geschrieben.

Eine tatsächliche Validierungs-/Speicherregression reproduzierte vorher alle vier Ablehnungen
in einem Durchlauf; die ausgeschriebenen Gegenfälle bestanden bereits. Danach werden diese
vier und die ausgeschriebenen Formen als Status einer vorhandenen Aufgabe übernommen.
Verneinungen, never, haven't/haven’t, nachgestellte Bedingung, Frage, fehlender Apostroph und
ein angehängter Wortpräfix bleiben abgelehnt. Abgelehnte Änderungen lassen den Zustand und
Checkpoint unverändert. Zulässige Änderungen bewahren den ursprünglichen Aufgabenbeleg,
den neuen wörtlichen Originaltext einschließlich Apostroph, den alten Checkpoint bytegleich
und wiederholt identische Wiederöffnung. Das ist kein vollständiger englischer Grammatik-
oder Bedeutungsnachweis. Die Änderung erweitert weder neue Aufgaben noch künftige Zusagen
zu einer Erledigung. Prompts, Schema, Transport und Prüfgrenzen bleiben unverändert.

Frisch bestanden sind 560 Rusttests, 161 Notizenfälle, 37 Providerprotokollfälle, Clippy mit
Warnungen als Fehler und Rustformat. Beide vorab geplanten echten lokalen Referenzen mit
synthetischen positiven/unverbindlichen Aussagen sind ebenfalls neu bestanden, wie oben
getrennt von den kontrollierten Kontraktionsgegenfällen dokumentiert. Frühere Prüfungen auf
`fc8433f` ersetzen keine frische saubere Reviewrunde oder CI auf dem folgenden Head.
Eine vollständige CodeRabbit-Runde auf `fc8433f` wurde wegen dieses offenen echten Codex-
Befunds nicht erneut beauftragt; die nächste Vollprüfung gilt dem korrigierten Endstand.

## Intervallanzeige, synthetische Server und Speicherhinweis

Der vollständige [CodeRabbit-Lauf auf `b387093`](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5785953920)
prüfte alle 100 unter den unveränderten Filtern zulässigen Dateinamen gegen sämtliche 102
vollständig paginierten Patches. Zwei gültige Inlinebefunde und ein kleiner Hinweis blieben:

- Die erweiterte Ansicht zeigte beim Wiederöffnen stets die lokale Vorgabe 3 Minuten, während
  der native Auftrag beispielsweise mit 2 Minuten weiterlief. Sie zeigt jetzt bei laufendem
  Auftrag dessen `status.intervalMs`, einschließlich vorhandener 5- oder 7,5-Minuten-Werte.
  Ein lokaler Wunsch gilt weiter für den nächsten Start. Eine abgelehnte Änderung zeigt weiterhin
  den tatsächlichen nativen Wert; nach Bestätigung erscheint der neue Wert. Vier DE/EN-Fälle
  und ein Fehler-/Erholungsfall scheiterten vorher und bestehen danach. Es wird kein Auftrag
  durch die Anzeige gestartet oder gestoppt; die laufende Modellverarbeitung bleibt unangetastet.
- Drei kontrollierte HTTP-Testserver konnten bei einer fehlenden Anfrage unendlich in `accept`
  warten. Ein gemeinsamer rein testseitiger Helfer begrenzt ihre Sequenz auf zehn Sekunden,
  behandelt ausschließlich `WouldBlock` erneut und meldet andere Fehler. Angenommene Windows-
  Streams werden ausdrücklich blockierend gesetzt; Lesen und Schreiben bleiben begrenzt.
  Eine Gegenprobe ohne Client überschritt vorher den äußeren 750-ms-Wartepunkt. Ein eigener
  Wake-Client gab ausschließlich diesen Prüfer frei. Danach meldet der Helfer nach 100 ms
  `TimedOut`; eine bereits verbundene Gegenprobe bestätigt Adressen und Lese-/Schreibfristen.
  Alle drei regulären fachlichen Anfragesequenzen bestehen. Produktive Netzwerk-, Intervall-,
  Modell- und CI-Fristen wurden nicht geändert oder gelockert.
- Beim Öffnen der klassischen Optionen stand derselbe kurze Speicherhinweis zweimal da.
  Er erscheint jetzt einmal vor dem Startknopf; die ausführliche Hilfe bleibt in den Optionen.
  Zwei tatsächliche DE/EN-Vorherfälle mit vollständig eingerichtetem synthetischem Anschluss
  zeigten jeweils zwei statt eines Absatzes; danach besteht die gesamte UI-Suite. Erste
  Prüferfehlläufe wegen eines fehlenden `startLiveNotes`-Fixtures bleiben erhalten und zählen
  ausdrücklich nicht als Nachweis des Produktfehlers.

Vor diesem Folgestand waren beide frischen Codex-Codeprüfungen auf `b387093` sauber:
[Runde 1](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6659085140),
[Runde 2](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6659376491).
Alle 22 damaligen Checks schlossen mit 16 Erfolgen und sechs regulären Skips ab:
[Linux/ARM/Intel](https://github.com/Puma7/meeting-for-handy/actions/runs/42033446398),
[Windows Rust](https://github.com/Puma7/meeting-for-handy/actions/runs/42033446153).
Die echten generierten CPU-Sprachprüfungen auf diesem früheren Head dauerten Linux
11.667974286 s, ARM 15.277998458 s und Intel 39.122020657 s; Worker-Neustart und Audio-/Journalerhalt
bestanden. Linux meldete bei virtuellen Quellen ausdrücklich `passed-with-startup-gaps` und
`completeCapture=false`. Nichts davon wird auf den folgenden Head übertragen. Wegen der obigen
Befunde war dieser vollständige CodeRabbit-Lauf kein sauberer Abschluss. Risiko Low enthielt
weiter genau diese Anliegen; eine separate Architekturwertung wurde nicht geliefert.
Die zusätzliche LanguageTool-Prüfung blieb unvollständig, Ursache weiterhin unbekannt.
Für den Folgestand sind eigene frische saubere Reviews und CI erforderlich.

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
| Pakethinweis und Dokumentation | START-HERE.txt nennt ausdrücklich entweder Streaming- oder Batchmodell. Ein tatsächlicher synthetischer ZIP-Vorherfall scheiterte am fehlenden Batchhinweis, danach bestanden 21 Paketfälle. Die damaligen 395 UI-Fälle und ältere Zahlen bleiben historische Belege. |

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
