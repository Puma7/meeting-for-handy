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
