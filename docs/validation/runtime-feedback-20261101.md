# Lokale Modellabbrüche und Bedienrückmeldungen

Stand: 1. November 2026. Ausgangspunkt ist Commit
`25537fa2a591e30734396716741b133e19f38986`. Die Änderungen betreffen
Anfragebudgets, begrenzte Wiederholungen und die Bedienoberfläche.

Eine lokale Antwort mit `finish_reason=length` wird einmal mit 4096 statt
2048 Ausgabetokens wiederholt. Modell, Adresse, Nachrichten, Schema und
Temperatur bleiben gleich. Eine weiterhin abgeschnittene Antwort wird nicht
übernommen. Andere Abschlussgründe und ungültiges JSON werden nicht repariert.

Nur ausdrücklich erkannte Kontextfehler des lokalen Dienstes erlauben kleinere
Anfragen. Ein begrenzter JSON-Fehlerkörper wird dafür ausgewertet; sein Inhalt
wird weder angezeigt noch gespeichert. Andere HTTP-400-Fehler werden nicht
automatisch wiederholt. Die Nachbearbeitung halbiert denselben Quellenblock
höchstens vier Ebenen tief. Erst ein vollständig geprüfter ursprünglicher Block
erhält seinen Schreibpunkt. Frühere bestätigte Schreibpunkte bleiben bytegleich.

Live-Fenster behalten ungesendete Quellen mit ihren bisherigen Kennungen für das
nächste Fenster. Eine Übersicht faltet kleinere vollständige Fenstergruppen ein
und markiert ausschließlich tatsächlich gesendete Fenster als abgedeckt.
Bei Platzmangel können alte Wiederholungshinweise gekürzt werden; neue Quellen
und ihre Belegprüfung bleiben erhalten. Cloud-Anfragen erhalten diese lokale
Budgetanpassung nicht. Es gibt keinen Anbieterwechsel und keine gelockerten
Quellen-, Zuständigkeits- oder Terminprüfungen.

Die skalare Diagnose behält den ersten und die sieben letzten vorherigen
Versuche. Ausgelassene Versuche werden gezählt. Die erste Fehlerursache bleibt
damit sichtbar, ohne die bisherige begrenzte Diagnosegröße aufzuheben. Freie
Servertexte, Prompts, Antworten, Schlüssel und Gesprächsinhalte gehören nicht
in dieses Protokoll.

Die Oberfläche bestätigt eine angeforderte Live-Aktualisierung sofort. Bis zur
Übernahme durch den nativen Worker bleibt die Anforderung sichtbar und weitere
Klicks bleiben gesperrt. Das Öffnen einer Historienaufnahme zeigt unmittelbar
einen Ladezustand. Die Nachbearbeitungsüberschrift und die Liste unfertiger
Auswertungen haben weniger erklärenden Fließtext; Belegprüfung und Hinweise in
den eigentlichen Ergebnissen bleiben erhalten. Die Seitenleiste bietet eine
ausdrückliche Update-Prüfung samt Ergebnis und Zugang zur vorhandenen
Update-Ansicht. Umbenennen und VIP-Schutz haben erklärende Tooltips.

## Nachweise

- Die neuen HTTP-Regressionsfälle prüfen vollständige statt abgeschnittener
  Antworten, identische Anfrageinhalte, Kontextaufteilung ohne Quellenverlust,
  unveränderte bestätigte Schreibpunkte nach Fehler und Fortsetzung,
  unveränderte Fensterabdeckung, Abbruch, begrenzte Diagnosehistorie und die
  Nichtweitergabe privater synthetischer Fehlerkörper.
- Die vollständige Windows-Rust-Suite umfasst 442 bestandene Tests und zwei
  regulär ignorierte spezielle Einstiege. Formatierung und Workspace-Clippy
  mit `-D warnings` bestehen; Abhängigkeiten bleiben gesperrt.
- 285 Oberflächenprüfungen, TypeScript und Produktionsbuild bestehen.
  Synthetische Browserprüfungen mit Edge 156.0.4314.50 bei 1280 und 390 Pixeln
  bestätigen Update-Prüfung, knappe Überschrift, Ladezustand, Tooltips und
  unterdrückte Mehrfachanforderungen ohne horizontalen Seitenüberlauf oder
  relevante Konsolenfehler. Das ist keine native IPC- oder Geräteprüfung.
- `verify.ps1` besteht mit 47 PCM-, 35 Notizen-, 28 Vorlagen- und acht
  Agentenprüfungen sowie dem Monitor.
- Ein echter lokaler Referenzlauf verwendet ausschließlich künstliche Aussagen
  am bereits vorhandenen ausgewählten Gemma-4-E4B-Modell in LM Studio 0.4.26.0.
  Eine ausdrückliche Aufgabe und eine Entscheidung werden jeweils erkannt;
  unverbindliche Aussagen erzeugen keine Aufgaben oder Entscheidungen und
  keinen erfundenen Termin. Drei Live-Fenster erhalten eine vollständig
  abgedeckte Übersicht; Wiederöffnung ist bytegleich. Die Gesamtlaufzeit beträgt
  99,6201065 Sekunden auf einem i7-13700 mit ungefähr 64 GiB RAM, während
  weitere lokale Prüfprogramme liefen. Das ist ein Funktionsnachweis, kein
  Leistungs- oder Speicherbenchmark. Working Set, Private Bytes und
  Grafikspeicher werden daraus nicht als Spitzenwerte abgeleitet.

## Nacharbeit aus der ersten Reviewrunde

Der Update-Verweis öffnet und fokussiert jetzt den konkreten Update-Bereich,
auch wenn die Einstellungen schon geöffnet sind. Die Fokusanforderung wird
einmal verbraucht; spätere normale Navigation behält den Seitenüberschriftenfokus.
Ein dauerhaft vorhandener höflicher Statusbereich meldet die laufende Update-Prüfung
auf Deutsch und Englisch und leert sich nach deren Ende. Dafür scheiterten vier
neue UI-Fälle vor der Korrektur; danach bestehen alle 283 UI-Prüfungen, TypeScript,
Produktionsbuild und die erneute Gesamtprüfung. Browserprüfungen bei 1280 und
390 Pixeln prüfen Fokus, Sichtbarkeit, wiederholten Verweis und normalen Seitenwechsel
mit synthetischen Schnittstellen. Eine tatsächliche Screenreader-Sprachausgabe
wurde nicht geprüft.

Fehlende Funktionskommentare wurden ergänzt, ohne Prüfkonfiguration oder
Dokumentationsschwelle zu ändern. Die Rust-Änderungen dieser Nacharbeit bestehen
ausschließlich aus Dokumentationszeilen. Der ausführbare Modellablauf ist unverändert;
die oben genannten 442 Rusttests, Clippy und der echte lokale Referenzlauf stammen
aus dem ursprünglichen Kandidaten und wurden für diese reine Dokumentationsänderung
nicht erneut ausgeführt. Rustformatierung und Gesamtprüfung sind erneut bestanden.

In der zweiten Reviewrunde wurde außerdem die Ankündigung eines laufenden
Downloads von einer Update-Prüfung getrennt. Der bestätigte native Downloadstatus
bestimmt nun dieselbe passende Meldung für Schaltfläche und höfliche Statusregion,
auch in den Einstellungen bei ausgeblendetem Footer. Zwei zusätzliche Fälle auf
Deutsch und Englisch scheiterten vorher mit der falschen Prüfmeldung; danach
bestehen alle 285 UI-Prüfungen, TypeScript, Produktionsbuild und Gesamtprüfung.
Browserprüfungen verwenden ausschließlich simulierten Downloadfortschritt und
starten weder Download noch Installation. Native Quellen sind gegenüber der
vorherigen Reviewkorrektur byteunverändert.

Der Referenzlauf lädt oder entlädt keine Modellinstanz, lädt kein Modell herunter
und verwendet keine Nutzermeetings. Er belegt keine neue Residenz- oder
Freigabezusage für fremde Instanzen. Physische Aufnahmeunterbrechungen,
Gerätetests, GPU-Auslastung und ein gemeinsamer Zweistundenlauf sind damit
nicht nachgewiesen oder behoben. `TEST_REPORT.md`, Paketversionen,
Abhängigkeitssperren und Schutzkonfigurationen bleiben unverändert.

## Nacharbeit aus dem vollständigen Review

Der vollständige Review fand einen Verlust gültiger Aussagen nach einer
Kontextaufteilung: Zwei einzeln zulässige Antworten mit je 20 Aussagen wurden
zusammen erneut auf 20 begrenzt. Zwei Regressionen zeigten vorher tatsächlich
20 statt 40 Aussagen. Die Ablaufsteuerung prüft jetzt jeden tatsächlich gesendeten
Teilblock getrennt gegen genau seine Quellen und die unveränderte Grenze von
20 Aussagen je Modellantwort. Erst danach werden die geprüften Aussagen vereinigt.
Ein 21. Eintrag im selben Teilblock und Belege aus einem anderen Teilblock werden
weiterhin verworfen und gezählt. Auch die kombinierte Antwortgröße bleibt begrenzt.

Nur für solche aufgeteilten Antworten erhält der bestätigte Schreibpunkt ein
ausdrücklich versioniertes Teilblockformat. Fortsetzung und Wiederöffnung prüfen
die ursprüngliche binäre Quellenaufteilung erneut; überlappende, fehlende,
vertauschte oder zu tiefe Teilblöcke, unbekannte Formate und fremde Belege werden
abgewiesen. Alle Teilblöcke müssen vor dem ersten Schreibpunkt erfolgreich sein.
Die Regression erhält alle 40 Aussagen über einen späteren Fehler, Fortsetzung
und Wiederöffnung hinweg und bewahrt den bestätigten Schreibpunkt bytegleich.
Normale und vorhandene Schreibpunkte behalten ihre bisherigen Bytes und Grenzen.
Sitzungs-, Snapshot- und fertiges Ergebnisformat bleiben unverändert. Ältere
Programme können einen neuen geteilten Schreibpunkt nicht fortsetzen; sie weisen
ihn ab und lassen die gespeicherten Bytes bestehen.

Der erneute echte lokale Referenztest mit denselben ausschließlich künstlichen
Aussagen besteht auf dem bereits geladenen ausgewählten Modell
`google/gemma-4-e4b` (Q4_K_M, 8192 Kontexttoken) in LM Studio 0.4.26.0:
eine ausdrücklich übernommene Aufgabe, eine ausdrückliche Entscheidung, keine
Aufgabe oder Entscheidung aus unverbindlichen Aussagen, drei Live-Fenster mit
vollständiger Übersicht und bytegleiche Wiederöffnung. Die Gesamtlaufzeit beträgt
211,9731276 Sekunden auf demselben i7-13700 mit ungefähr 64 GiB RAM. Parallel lief
ein eigener Abhängigkeitsbuild mit zwei Arbeitern; dies ist kein Leistungs- oder
Speicherbenchmark. Modell-Download, Instanzladen oder Entladen und Nutzermeetings
waren nicht beteiligt. Die oben genannten früheren Laufzeiten bleiben historische
Belege; dieser Lauf prüft den neuen Kernablauf.

Die vollständige erneute Windows-Rust-Suite besteht mit 448 Tests und zwei
regulär ignorierten besonderen Einstiegen. Die 285 UI-Prüfungen, TypeScript,
Produktionsbuild und Browserbelege stammen aus der vorigen Reviewkorrektur;
sämtliche Oberflächenquellen sind dazu byteunverändert. Rustformatierung und
`verify.ps1` mit 47 PCM-, 35 Notizen-, 28 Vorlagen- und acht Agentenprüfungen
sowie dem Monitor sind erneut bestanden.
Workspace-Clippy besteht erneut mit `-D warnings` und gesperrten Abhängigkeiten.

## Weitere Historienauswahl während des Ladens

Ein zusätzlicher Befund außerhalb der Diffzeilen zeigte verlorene Auswahlen:
Die Suchhistorie ließ weitere Öffnen-Klicks zu, während ein früherer Leseaufruf
noch lief; die Sperre verwarf diese Auswahl. Jetzt bleibt ausschließlich die
neueste gewünschte Aufnahme samt Zielansicht und Quellenanker vorgemerkt.
Die Aufrufe laufen nacheinander. Überholte Antworten wechseln die Ansicht nicht;
ein fehlgeschlagener erster Aufruf verhindert die neueste Auswahl ebenfalls nicht.
Identische Doppelklicks werden zusammengefasst. Löschgeneration und Sperren werden
vor dem nächsten Abruf und vor der Übernahme erneut geprüft.

Zwei neue Fälle scheiterten vorher; danach bestehen alle 287 Oberflächenprüfungen,
TypeScript, Produktionsbuild, Formatprüfung und die erneute Gesamtprüfung.
Der bestehende Doppelklickfall prüft nun auch nach Abschluss, dass kein zweiter
Abruf erfolgt. Eine frische synthetische Browserprüfung mit Edge 156.0.4314.58
bei 1280 und 390 Pixeln klickt A, B, C und nochmals C: ausschließlich A und C
werden nacheinander abgefragt, C wird angezeigt und der Ladestatus endet.
Es gibt keinen horizontalen Überlauf oder Konsolenfehler. Dies ist keine native
Sitzungs-, Geräte- oder Löschprüfung. Sämtliche nativen Quellen, Abhängigkeiten
und der vollständige Prüfbericht bleiben zur vorigen Kernkorrektur bytegleich;
deren 448 Rusttests, Clippy und echter Modellreferenztest wurden für diese reine
UI-Korrektur nicht wiederholt.

Eine weitere Reviewrunde fand einen Konflikt zwischen Historie und Nachbearbeitung:
Eine verspätete Historienantwort konnte die inzwischen gewählte Nachbearbeitung
wieder durch die alte Detailansicht ersetzen. Eine gemeinsame Auswahlgeneration
entwertet nun überholte Leseantworten beider Ansichten und verwirft vorgemerkte
Historienauswahlen beim Seitenwechsel. Auch alte Fehlermeldungen verdrängen den
neueren Zustand nicht. Tatsächlich laufende native Historienabrufe enden weiterhin
vor dem nächsten vorgemerkten Abruf; die Generation ist keine Abbruchzusage für IPC.
Löschgeneration, gesperrte IDs und Quellenanker bleiben geprüft.

Vier Vorherfälle scheiterten: alte Historienantwort nach neuer Nachbearbeitung,
alte Fehlermeldung nach neuer Nachbearbeitung und veraltete Warteschlange nach
Wechsel zu Einstellungen oder Meeting. Nach der Korrektur bestehen alle 292
UI-Prüfungen, einschließlich der zusätzlich geprüften Gegenrichtung einer alten
Nachbearbeitungsantwort nach neuer Historienauswahl. TypeScript, Produktionsbuild,
Formatierung und Gesamtprüfung bestehen erneut. Native Quellen sind weiterhin
bytegleich zur Kernkorrektur; deren tatsächliche Rust- und Modellbelege werden
nicht als neu ausgeführte Prüfungen dieser UI-Nacharbeit bezeichnet.
Die erneute synthetische Browserprüfung bei 1280 und 390 Pixeln bestätigt außerdem
die neu gewählte Nachbearbeitung und die Einstellungen nach verspäteter
Historienantwort; eine veraltete vorgemerkte Aufnahme wird nicht mehr abgerufen.

## Weitere Rückmeldungen und erreichbare Hinweise

Die vollständige Reviewrunde bestätigte drei weitere UI-Probleme. Erläuterungen
waren teilweise nur per Hover-Titel erreichbar. Die knappe Seitenüberschrift
bleibt erhalten; eine geschlossene native Aufklapphilfe im Ergebnisbereich und
eine weitere bei unfertigen Auswertungen machen die Erklärungen jetzt per
Tastatur und Touch erreichbar. Fertige Ergebnisse behalten ihren sichtbaren
KI-Entwurfshinweis. Dauerhafte lange Absätze unter der Seitenüberschrift werden
nicht wieder eingeführt. Die vorhandene Tastaturprüfung belegt bereits den Fokus
des Seitenkopfs nach Navigation.

Eine passende Kontextdiagnose wurde zuvor durch den deutschen nativen Fehlertext
unterdrückt. Stimmen der native Kontextfehler und die gespeicherte Diagnose
überein, bleibt nun die lokalisierte Erklärung sichtbar. Eine ältere Kontextdiagnose
ersetzt keinen neuen unabhängigen Fehler. Die kompakte Update-Aktion dupliziert
keine native Jobfehlermeldung mehr; die maßgebliche vollständige Ansicht zeigt
sie weiter an und kann sie wirksam ausblenden. Eigenständige lokale Aktionsfehler
bleiben sichtbar.

Fünf aussagekräftige Vorherfälle scheiterten gegen die tatsächlich veröffentlichten
06-Produktbytes. Zwei zusätzliche Gegenfälle bestanden schon vorher. Danach
bestehen alle sieben Fälle und sämtliche 299 UI-Prüfungen sowie TypeScript,
Produktionsbuild, Formatierung und die erneute Gesamtprüfung. Eine synthetische
Edge-Prüfung bei 1280 und 390 Pixeln öffnet beide Hilfen per Enter und Klick,
prüft einmalige und wirksam ausgeblendete Updatefehler sowie die englische
Kontextwarnung. Keine Konsolenfehler oder horizontaler Überlauf; keine tatsächliche
Screenreader-Sprachausgabe, native Sitzung oder echte Modell-/Updateanfrage.
Native Quellen und deren frühere reale Referenz bleiben byteunverändert.

Ein früherer Linux-CI-Lauf auf 06 wurde von der unveränderten virtuellen
Audioaufnahmeprüfung abgewiesen: angeforderte acht Sekunden, beobachtete Spuren
10155 und 10150 Millisekunden, erlaubte Obergrenze zehn Sekunden. Die gemeldete
Aufnahme war unvollständig; die Ursache der Überschreitung ist damit nicht bewiesen.
Aufnahmecode, virtuelle Fixture, Verifier und Workflow sind zur Basis bytegleich.
Der fehlgeschlagene Lauf bleibt erhalten. Die Grenze wird nicht angehoben; aktuelle
Ziel-CI-Ergebnisse müssen dem tatsächlichen neuen Head zugeordnet werden.

Die nächste vollständige Runde fand außerdem eine Zustandsänderung durch die
neue Update-Aktion: Bei bereits als `ready` bestätigtem Installer hätte ein
erneuter Prüflauf die native Referenz auf den geprüften Installer verworfen.
Die kompakte Aktion bietet in diesem bestätigten Zustand jetzt ausschließlich
den Zugang zur bestehenden Update-Ansicht; sie startet keine weitere Prüfung.
Der native Updater und seine Installationsprüfungen bleiben unverändert.
Zwei zustandsbehaftete UI-Regressionsfälle auf Deutsch und Englisch zeigten
vorher tatsächlich den Verlust des simulierten geprüften Zustands. Danach
bleibt er erhalten; weder Prüfung noch Download oder Installation wird gerufen.
Alle 301 UI-Prüfungen, TypeScript, Produktionsbuild, Formatierung und Gesamtprüfung
bestehen. Eine synthetische Browserprüfung bei 1280 und 390 Pixeln öffnet nur
den vorhandenen Update-Bereich und weist null neue Updateprüfungen nach.
Es wurde dafür kein realer Installer heruntergeladen, gestartet oder verändert.

## Fertigmeldung, beschädigte Zwischenstände und wörtliche Zuständigkeit

Die nächste vollständige Runde bestätigte zwei weitere Probleme. Ein fertig
geladener und geprüfter Installer wird jetzt mit einer eigenen deutschen oder
englischen Meldung im dauerhaft vorhandenen höflichen Statusbereich angekündigt.
Der sichtbare Kurztext unterscheidet den fertigen Installer von einer lediglich
verfügbaren Version. Es entsteht keine zweite automatische Fertigansage; der
Zugang zur vorhandenen Update-Ansicht bleibt erhalten. Zwei neue Übergangsfälle
scheiterten vorher; danach bestehen alle 303 UI-Prüfungen sowie TypeScript,
Produktionsbuild und Formatierung. Die synthetische Edge-Prüfung bei 1280 und
390 Pixeln bestätigt den Übergang in beiden Sprachen ohne neue Updateprüfung,
Download oder Installation. Eine tatsächliche Screenreader-Sprachausgabe wird
damit nicht belegt.

Beschädigte normale oder geteilte Zwischenstände erhalten eine verständliche
Fehlermeldung zum gespeicherten Auswertungsabschnitt. Die ursprüngliche strenge
Parserursache bleibt erhalten; Inhalt wird weder ausgegeben noch repariert oder
gelöscht. Der neue Regressionstest scheiterte vorher an der rohen Parsermeldung
und prüft danach beide Fehlerwege einschließlich des erhaltenen Ursachenfehlers.

Der erneut ausgeführte echte Modelltest bestand zunächst nicht: Eine ausdrückliche
Aufgabe wurde mit dem Grund `unverifiedOwnerOrDue` verworfen. Eine zusätzliche
neue synthetische Diagnoseantwort nannte die zuständige Person, ließ deren Namen
aber im wörtlichen Beleg weg. Diese neue Antwort ist kein nachträglicher Mitschnitt
der ursprünglichen Antwort. Die Belegprüfung wies sie korrekt zurück und bleibt
unverändert. Der Modellauftrag erklärt nun zusätzlich, dass ein nichtleerer
Zuständigkeits- oder Terminwert selbst im exakt kopierten Zitat stehen muss;
vorhandene Sprecherpräfixe dürfen dafür nicht weggekürzt und Namen niemals in
Zitate eingefügt werden. Fehlt der Wert im Zitat, bleibt er unbekannt.

Dieser präzisierte Nachbearbeitungsauftrag hat die eigene Promptfassung
`handymeeting-rust-notes-4`. Ältere fertige Ergebnisse bleiben lesbar; unfertige
Arbeiten der Fassungen 1 und 3 werden unverändert als nicht fortsetzbar angezeigt,
statt mit dem neuen Auftrag vermischt zu werden. Ein tatsächlicher Vorherfall
zeigte die fehlende Trennung bei beibehaltener Fassung 3. Die Regression prüft
jetzt beide früheren Fassungen samt bytegleicher Erhaltung und ohne neue Anfrage.
Live-Promptfassung, Sitzungsformat, Paketversion und Belegregeln bleiben gleich.

Nach der Präzisierung besteht der unveränderte echte Referenzfall mit einer
Aufgabe, einer Entscheidung und null Aufgaben oder Entscheidungen aus den
unverbindlichen Aussagen. Drei Live-Fenster haben eine vollständig abgedeckte
Übersicht; Wiederöffnung ist bytegleich. Laufzeit: 173,5037189 Sekunden mit
LM Studio 0.4.26.0, dem bereits geladenen ausgewählten Gemma-4-E4B Q4_K_M und
8192 Kontexttokens auf dem oben genannten Rechner. Parallel liefen eigene
Windows-Rust-Prüfungen; dies ist kein Leistungs- oder Speicherbenchmark.
Alle 20 nativen Notizen-Quelldateien wurden vor diesem Lauf eingefroren und
danach bytegleich geprüft. Der frühere fehlgeschlagene Lauf bleibt erhalten;
ein einzelner erfolgreicher Modelllauf garantiert keine vollständige oder
inhaltlich richtige Ausgabe in jedem künftigen Gespräch.

Frisch auf dem finalen nativen Stand bestehen 449 Rust-Workspace/All-Targettests,
zwei regulär ignorierte spezielle Einstiege, Workspace-Clippy mit `-D warnings`,
Rustformatierung und die Gesamtprüfung mit 47 PCM-, 35 Notizen-, 28 Vorlagen-
und acht Agentenprüfungen samt Monitor. Nutzermeetings, Modelldownloads,
Instanzübernahme und Entladung waren nicht beteiligt. Vollständiger Prüfbericht,
gesperrte Abhängigkeiten, Aufnahme-/CI-Verifier und Schutzregeln bleiben unverändert.

## VIP-Schutz ohne Maus-Hinweis

Eine weitere vollständige Runde fand eine nur per Hover erreichbare Erklärung
des VIP-Schalters. Ein kurzer sichtbarer Hinweis erklärt jetzt vor dem Anheften,
dass VIP das Meeting oben hält und vor automatischem Aufräumen schützt,
manuelles Löschen aber möglich bleibt. Der Schalter ist über eine eindeutige
Beschreibung mit diesem Hinweis verbunden; der bisherige Titel bleibt ergänzend.
Beim Umbenennen werden Schalter und zugehörige Erklärung gemeinsam ausgeblendet.
Es wurde keine Aufbewahrungs-, Metadaten- oder Löschregel geändert.

Zwei neue DE/EN-Regressionsfälle scheiterten vorher. Danach bestehen sämtliche
305 UI-Prüfungen, TypeScript, Produktionsbuild, Formatierung und die erneute
Gesamtprüfung. Acht synthetische Browserfälle in Edge 156.0.4314.58 prüfen
Deutsch/Englisch, 1280/390 Pixel und beide VIP-Zustände: Erklärung ohne Hover
sichtbar, Schalter per Tastatur erreichbar und damit beschrieben, kein horizontaler
Überlauf oder Konsolenfehler. Es erfolgten keine echten Metadatenänderungen oder
Löschungen; tatsächliche Screenreader-Sprachausgabe ist kein Teil dieses Nachweises.
Alle nativen Quellen und Abhängigkeiten bleiben bytegleich zum zuvor geprüften
Stand mit 449 Rusttests und dem echten Referenzlauf von 173,5037189 Sekunden.
Diese nativen Prüfungen wurden für den reinen Bedienhinweis nicht erneut ausgeführt.

## Verspätete Live-Aktionen und abgeschlossene Updateprüfungen

Eine verspätete manuelle Live-Aktualisierung konnte nach einem Meetingwechsel
noch den Status des früheren Meetings abfragen und dessen Fehler im neuen Meeting
anzeigen. Eine echte Vorherregression bestätigte das; die gleichartige Folgeabfrage
nach dem Beenden hatte denselben Fehler und ist ebenfalls reproduziert. Beide
Aktionen prüfen jetzt ihre ursprüngliche Auswahlgeneration vor der Folgeabfrage.
Bereits laufende native Befehle werden dadurch nicht nachträglich abgebrochen;
die Oberfläche beginnt keine veraltete Folgeabfrage. Aktuelle Aktionen fragen
weiterhin ihren aktuellen Status ab. Native Worker- und Aufnahmeabläufe bleiben gleich.

Auch die Ergebnisse „App ist aktuell“ und „Update verfügbar“ nutzen jetzt den
dauerhaft vorhandenen höflichen Statusbereich. Sichtbare Ergebnisabsätze tragen
keine zweite Live-Rolle. Vier zusätzliche Vorherfälle auf Deutsch und Englisch
zeigten zuvor einen leeren ursprünglichen Statusbereich nach der Prüfung.
Danach bestehen alle sechs neuen Fälle und sämtliche 311 UI-Prüfungen,
TypeScript, Produktionsbuild, Formatierung und die erneute Gesamtprüfung.
Zwölf synthetische Browserfälle bei 1280 und 390 Pixeln bestätigen beide
Updateergebnisse in beiden Sprachen und die verspätete Live-Aktualisierung bzw.
Beendigung nach Auswahl eines anderen Meetings. Kein veralteter Statusabruf,
keine alte Fehlermeldung im neuen Meeting, kein horizontaler Überlauf oder
Seitenfehler; keine echte native Aktion oder tatsächliche Screenreader-Ausgabe.
Die nativen Quellen bleiben bytegleich zu den 449 Rusttests und dem echten
Referenzlauf mit Promptfassung 4; diese Prüfungen wurden nicht erneut ausgeführt.

Die Wiederholungsgrenze beschränkt die Anzahl der Anfragen, nicht die gesamte
verstrichene Auswertungsdauer: Vier Teilungsebenen erlauben höchstens 31 Knoten
und 62 HTTP-Versuche je Quellenblock. Es gibt keine zusätzliche gemeinsame
Gesamtlaufzeitgrenze; die bestehenden Zeitlimits einzelner Anfragen können sich
entsprechend addieren. Die vorhandenen Abbruchprüfungen bleiben erhalten.
Dies ist eine ausdrücklich dokumentierte Betriebsgrenze, kein Nachweis einer
kurzen Höchstlaufzeit oder einer abgeschwächten Schutzregel.
