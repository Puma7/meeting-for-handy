# PR 41: Lokale Validierung vor Veröffentlichung

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.

Diese Nacharbeit bindet deutsches Hilfsverb und Beschlussprädikat an denselben
Satzteil. Ein früheres Hilfsverb aus einem Vorschlag begründet keinen späteren
Beschluss im Nebensatz. Gültige einfache Vergangenheit und tatsächlich ausgesagte
Vergangenheit beziehungsweise passive Beschlusszustände bleiben zulässig.
Die bisherigen UI-, Rücknahme-, Mengen- und Nichtstun-Korrekturen bleiben erhalten. Beide Referenzsprachen
enthalten unverändert eine ausdrückliche Budgetfrage, wenn der Test einen Gesprächsimpuls verlangt.
Die ursprünglichen Zusagen und unverbindlichen Aussagen bleiben erhalten.

Die [bisherige ausführliche Fassung](https://github.com/Puma7/meeting-for-handy/blob/b8b1ed83ab5f4b918ec6cf01a3fbeff3b09c188e/docs/validation/pr41-review-followup-20261104.md)
und deren Detailhistorie bleiben unveränderlich in Git.
Private Rohbelege, alte Checkpoints und fehlgeschlagene Läufe bleiben erhalten.
Der vollständige TEST_REPORT bleibt gegenüber dem PR-Basisstand byteunverändert.
Dieser bereinigte Vermerk veröffentlicht keine Nutzermeetings oder privaten Sitzungsdaten.

## Hilfsverb und Beschlussprädikat im selben Satzteil

Die [Codex-Runde auf b8b1ed8](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4488482090)
fand einen gültigen Fehler: „Wir haben vorgeschlagen, dass der Pilotstart
beschlossen wird.“ und „Es ist wichtig, dass der Pilotstart beschlossen wird.“
konnten als bereits gefasster Beschluss gespeichert werden. Die Prüfung band
das spätere Partizip an haben beziehungsweise ist aus dem übergeordneten Satz.

Die bestehende dauerhafte Regression erhielt zwölf zusätzliche wörtliche Fälle.
Der finale Vorherlauf reproduzierte sechs falsche Annahmen: die beiden Aussagen
mit und ohne ASR-Komma sowie zwei beabsichtigte finite Nebensätze. Er zeigte
auch drei zu Unrecht abgewiesene echte Vergangenheitsformen: direkte einfache
Vergangenheit und eine bestätigte abgeschlossene Entscheidung im Nebensatz.
Ein vorheriger kleinerer Zehn-Fälle-Lauf bleibt getrennt erhalten.

Die Zuordnung ist jetzt auf denselben Satzteil begrenzt. Satzzeichen und
Nebensatzmarker trennen ihn auch bei fehlendem Komma. Ein bloßes finites
Präsens im dass-Nebensatz genügt nicht. Echte direkte Vergangenheitsformen
bleiben zulässig; im ausgesagten vergangenen oder passiven Nebensatz kann das
zugehörige finite Hilfsverb am Ende desselben Satzteils stehen. Ein wird aus
einem bloß geplanten zukünftigen Beschluss erfüllt diese Konstruktion nicht.
Die Grenze der Beschlussbehauptung folgt dem tatsächlichen Satzteil.

Alle zwölf Fälle und sämtliche bisherigen Beschluss-, Nichtstun-, Bedingungs-,
Rücknahme- und Mengen-Gegenfälle bestehen. Die Regression prüft Originalzitate,
den produktiven Zustandsschreiber, mehrfaches Wiederöffnen und dass eine
abgelehnte Antwort keine Revision oder bestätigte Zustandsänderung erzeugt.
Dies bleibt eine begrenzte konservative Belegprüfung, kein vollständiger
Grammatik-, Bedeutungs- oder Wahrheitsbeweis.

## Notizstände und sichtbare Schaltflächenzustände

Der [vollständige CodeRabbit-Lauf auf 35a9bee](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5794845322)
fand zwei weitere gültige UI-Fehler. Die Hervorhebung benutzte eine nicht
definierte CSS-Variable. „Gemerkt“, „Wieder öffnen“ und die Kompatibilitätsregel
für ausgewählte Layoutschaltflächen verwenden jetzt die vorhandene Themenfarbe
--tint. Farben und Auswahlverhalten bleiben sonst erhalten.

Die Notizsicherung verglich nur den letzten gemeinsamen Verlaufseintrag.
Wechsel zwischen privater und Protokollnotiz erzeugten dadurch identische
Kopien; auch unveränderte leere Notizen erhielten einen Stand. Zwei echte
Komponentenregressionen reproduzierten beide Fehler. Jetzt wird rückwärts der
letzte Stand derselben Notizart gesucht, ohne den Verlauf zu verändern oder
zu kopieren. Ein fehlender Stand entspricht leerem Text. Tatsächliche Änderungen
und das spätere bewusste Leeren bleiben versioniert; alte Stände werden weder
gekürzt noch automatisch gelöscht. Ein beliebig langer tatsächlicher Verlauf
ist damit nicht zugesichert; die vorhandene sichtbare Speicherfehlerbehandlung bleibt erhalten.

Auf b8b1ed8 bestanden 416 UI-Tests in 21 Dateien, TypeScript, Produktionsbuild und
Format. Sämtliche UI-Dateien bleiben hier bytegleich; diese Prüfungen wurden nicht erneut ausgeführt. Die zusätzlichen Tests prüfen leere Notizen, wechselnde Notizarten,
echte Änderungen, Leeren, gespeicherte Originalstände, Wiederöffnung und dass
private Texte nicht an die synthetische native Adaptergrenze gelangen.
Der erste private Testversuch hatte für den zweiten Fall noch einen absichtlich
belegten Auswertungszustand; er zählt nicht als Nachweis des Duplikatfehlers.
Der getrennte finale Vorherlauf verwendete eine freie synthetische Auswertung
und zeigte beide tatsächlichen Fehler. Beide Läufe bleiben erhalten.

Die Browserprüfung auf b8b1ed8 verwendete die hier bytegleiche LiveWorkspace-Komponente in
Edge 156.0.4314.58 mit dem vorhandenen Playwright 1.57.0: 390/1280 Pixel,
Deutsch/Englisch und hell/dunkel, insgesamt acht Konfigurationen. Die drei
Hervorhebungsregeln waren vorher in allen 24 Messungen transparent und stimmen
nachher mit der Themenfarbe überein. Die Kompatibilitätsregel verwendet eine
zusätzliche synthetische Schaltfläche; Gemerkt/Wieder öffnen sind die echten
Komponentenaktionen. Leere Notizen, zwei Notizarten, echte Änderungen, Leeren,
Reload und private Adaptertrennung bestehen in allen acht Konfigurationen.
Seitenidentität, sichtbarer Inhalt, fehlendes Framework-Fehleroverlay, keine
relevanten Konsolen-/Seitenfehler und kein horizontaler Überlauf wurden geprüft.
Desktop- und schmale Screenshots wurden visuell gelesen; keine fremden Requests.
Browser-Plugin nicht verfügbar, daher vorhandenes Playwright ohne Installation.
Eigener Browser und eigener lokaler Server wurden beendet.
Das ist eine synthetische Browser-/Adapterprüfung, kein nativer IPC-, Modell-,
Audio-, Screenreader- oder Tauri-WebView-Nachweis.

## Vollständigkeit der nächsten Reviewrunde

Der genannte CodeRabbit-Lauf wählte alle 100 zulässigen Dateien aus, meldete aber
validation_01.rs als nicht vollständig verfügbar. Er ist deshalb ausdrücklich
keine saubere vollständige Runde. Die tatsächlichen GitHub-Patches waren separat
vollständig paginiert und byte-/zeilengeprüft; die Ursache der Anbieter-Prüflücke
ist damit nicht bewiesen. Zwei vorhandene, zusammengehörige Runner-/Replaytests
wurden mit exakt unverändertem Funktionsinhalt nach inference_02.rs verschoben.
Beide Dateien waren bereits Teil desselben PR; keine neue Datei, kein Filter,
kein Timeout und kein Testfall wurde entfernt oder abgeschwächt.
Bei dieser Umverteilung sank die Testdatei von 83049 auf 62611 Bytes; die jetzigen
zwölf zusätzlichen Tabellenfälle bleiben vollständig enthalten. Erst eine frische
vollständige Runde ohne unverfügbare Datei darf die Prüflücke als geschlossen bestätigen.
AudioSources.tsx enthielt in dieser Runde nur die bereits manuell nachgelesene
Entfernung des sichtbaren Pegel-Hilfetexts; die zugängliche Pegelerklärung bleibt erhalten.

## Deutsche Beschlussaussage statt bloßer Absicht

Die [Codex-Runde auf 35a9bee](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4488211575)
fand einen gültigen Fehler: „Wir müssen entscheiden, ob wir am Freitag
veröffentlichen.“ und „Wir möchten beschließen, den Bericht freizugeben.“
konnten als bereits gefasste Beschlüsse erscheinen. Die bestehende
Validator-/Checkpoint-Regression reproduzierte diese beiden und zwei weitere
Absichtssätze mit wollen und versuchen als vier tatsächliche falsche Annahmen.

Die damalige deutsche Korrektur verlangte eine ausdrückliche finite Konstruktion
für entscheiden/beschließen oder ein passendes Hilfsverb vor dem Partizip
beschlossen/entschieden. Ein modaler oder beabsichtigter bloßer Infinitiv
genügt nicht; ein zwischen Hilfsverb und Partizip stehendes zu wird nicht
als affirmative Vergangenheit behandelt. Gegenbeispiele erhalten die
tatsächlich ausgesagte Entscheidung in der Vergangenheit sowie die
umgestellte finite Form „Heute beschließen wir den Pilotstart.“.

Sechs zusätzliche dauerhafte Tabellenfälle prüfen die vier Ablehnungen und
zwei gültigen Aussagen mit Originalzitaten und mehrfacher Wiederöffnung.
Alle bisherigen finiten, vergangenen, passiven, negierten und bedingten
Gegenfälle bleiben erhalten, einschließlich der gültigen Entscheidung über
bewusstes Nichtstun mit und ohne ASR-Komma. Abgelehnte Antworten schreiben
keine Revision und verändern keinen bestätigten Stand.
Die Regeln bleiben begrenzt und ersetzen keine inhaltliche Überprüfung.

## Verneinte Rücknahmen und mehrteilige Mengenangaben

Die [Codex-Runde auf c8ffeed](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5794427330)
fand zwei gültige Fehler. „Wir haben nichts zurückgenommen.“, „We withdrew
nothing.“ und verwandte negative Pronomen konnten vorhandene Aufgaben oder
Beschlüsse zurücknehmen. Sechs negative Aussagen wurden vorher für beide
Eintragsarten angenommen: zwölf tatsächliche falsche Rücknahmen.

Die gemeinsame Prüfung für terminale Aussagen weist diese negativen Pronomen
jetzt zurück. Die vorhandene Regression verwendet für beide Eintragsarten auch
den produktiven Zustandsschreiber: gültige Rücknahmen bleiben nach mehrfacher
Wiederöffnung erhalten; abgelehnte Antworten verändern weder Eintrag noch
Originalzitat oder früheren Checkpoint und erzeugen keine neue Revision.
Drei zusätzliche echte Rücknahmen bleiben zulässig. Alle älteren Fälle,
darunter „Wir brauchen den Bericht nicht mehr.“, bleiben erhalten.

Dieselbe verneinte Behauptung war auch für neue Beschlüsse problematisch:
„We decided nothing.“, „Wir haben nichts entschieden.“ und „We have decided
none of the options.“ wurden vorher tatsächlich als Beschlussbelege akzeptiert.
Sie werden jetzt abgewiesen. „We decided to do nothing.“ und „Wir haben
beschlossen, nichts zu ändern.“ bleiben ausdrücklich gültige Beschlüsse.
Der zusätzliche Gegenfall ohne Komma reproduzierte zunächst eine falsche
Ablehnung der engeren Prüfung. Die Klauselgrenze berücksichtigt jetzt auch
„nichts zu“ mit anschließendem Infinitiv; die Beschlussbehauptung bleibt von
ihrem Inhalt getrennt. Auch dieser Vorherfehler und seine Korrektur sind belegt.

„I have completed 1 out of 2 reports.“ und die Variante mit exactly wurden
vorher als vollständige Erledigung angenommen. „out of“ wird jetzt als ein
Mengenoperator gelesen; der Zähler steht vor out. Vollständige 2-out-of-2-
Mengen und der einzelne benannte „report 1 out of 2“ bleiben zulässig.

Die drei vorhandenen dauerhaften Regressionen ergänzen insgesamt 20 wörtliche
Fälle mit 29 ausgeführten Szenarien, einschließlich beider Rücknahmearten.
Die Vorherläufe zeigen 17 falsche Annahmen sowie die separat reproduzierte
falsche Ablehnung des gültigen Beschlusses ohne Komma. Kein alter Fall wurde
entfernt; keine Grenze wurde nachträglich an die Ergebnisse angepasst.

## Korrektur der Abschlussprüfung

Der [Codex-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4486325695)
war gültig: „Ich habe den Bericht zu 50 Prozent abgeschlossen.“ und
„I have completed 50% of the report.“ konnten eine offene Aufgabe schließen.
Die tatsächliche Validator-/Checkpoint-Regression reproduzierte insgesamt
15 nach den konservativen Kriterien unzulässige oder unklare Prozentbelege.

Die Abschlussprüfung erkennt Prozentzeichen, Prozent, percent und per cent.
Bei einer Fortschrittsangabe akzeptiert sie nur den eindeutigen numerischen
Wert 100, auch mit Dezimalnullen. Ziffern werden exakt verglichen:
99.9999999999999999999 wird nicht durch Fließkommarundung zu 100.
Teilwerte, unplausible Werte über 100 sowie Bereiche und Bruchangaben mit
abschließendem Wert 100 bleiben ohne automatische Erledigung.
Ausgeschriebene unbekannte Prozentwerte werden konservativ zurückgewiesen.

Fünf eindeutige 100-Prozent-Formen bestanden bereits vorher und weiterhin:
Deutsch/Englisch, Symbol oder Worteinheit sowie Dezimalnullen mit Punkt oder Komma.
Ein zusätzlicher vollständiger Satz über einen „percentage report“ besteht;
percentage als Substantiv wird nicht als Prozenteinheit behandelt.

Der [nachfolgende Codex-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4486763837)
war ebenfalls gültig: Vollständige Abschlüsse eines 50-Prozent-Rabattberichts
wurden zu Unrecht abgewiesen. Zwei tatsächliche Vorherfälle reproduzierten dies.
Jetzt wird ein Prozentwert vor einem Gegenstandsnamen von ausdrücklich
prozentualem Fortschritt unterschieden; zusammengesetzte Berichtstitel bleiben zulässig.

Zehn zusätzliche Vorherfälle schlossen Aufgaben trotz half of, zur Hälfte,
ein Drittel, 1 von 2 oder äquivalenten Unicode-Prozentzeichen.
Diese ausdrücklichen Teilmengen bleiben jetzt offen. Vollständige 2-von-2-
und Drei-Drittel-Angaben, Datumsangaben, ordinales third und half-year bleiben zulässig.
Ziffernverhältnisse werden ohne Fließkommarundung verglichen.

Die vorhandene dauerhafte Regression erhielt zuvor 21 Prozentfälle und
25 weitere Fälle für Teilmengen und Berichtstitel; kein bestehender Fall wurde entfernt.

Ein [weiterer Codex-Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4487111857)
reproduzierte noch falsche Abschlüsse bei „zu 50 Prozent heute abgeschlossen“
und „I have completed 50% today.“. Sechs Vorherfälle mit Zeit-, anderen Adverbien
oder einer Präpositionalgruppe wurden tatsächlich als vollständig angenommen.
Eine unbekannte Nachfolge allein beweist jetzt keinen Gegenstandsnamen. Die
Ausnahme verlangt eine ausdrückliche Gegenstandseinleitung und einen folgenden
Bezeichner; eine Fortschrittsangabe bleibt auch vor Adverbien ein Teilfortschritt.
Acht bereits vorhandene dauerhafte Fälle prüfen diese sechs Ablehnungen und
zwei zulässige 100-Prozent-Abschlüsse mit Adverbien. Alle Rabattbericht-Gegenbeispiele bleiben erhalten.

Die zweite Codex-Runde auf 6170d4a fand einen weiteren
[gültigen Befund](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4487395730):
Jahresbezeichnungen wie 2026/2027 und Quartalsnamen wie Q1/2026 wurden als
unvollständiges Verhältnis behandelt. Sechs tatsächliche Vorherfälle zeigten
fälschlich abgewiesene vollständige Jahres-/Quartalsberichte und einzelne
Berichte mit Ordnungsbezeichnung 1 von 2.

Die Verhältnisprüfung verlangt jetzt einen ausdrücklichen Teilmengenbezug oder
eine Menge direkt nach einem Fortschrittsprädikat beziehungsweise einer
Fortschrittspräposition. Eingebettete Kennungen wie Q1/2026 sind keine
freistehenden Mengen. Dreizehn zusätzliche dauerhafte Fälle erhalten zulässige
Perioden-/Berichtsnamen, echte Teilmengen mit und ohne Abstand zur Teileinheit
und vollständige 2-von-2-Mengen am Satzende. Ein Satzpunkt wird nicht mehr als
leere Dezimalstelle eines Mengenverhältnisses behandelt.

Auf e502c91 waren zwei weitere
[Codex-Befunde](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5793991439)
gültig. Sechs Vorherfälle schlossen Aufgaben trotz „exactly 1/2“ oder
„genau 1 von 2“, auch mit aufeinanderfolgenden Mengenadverbien. Die Verbindung
zum Fortschrittsprädikat wird jetzt über benachbarte Mengenadverbien erhalten,
ohne über Gegenstandsnamen oder Satzgrenzen zu springen. Acht weitere Fälle
prüfen sechs Teilmengen und zwei zulässige vollständige Mengen.

Ein neuer Regressionstest verwendet den tatsächlichen Workspace-Runner mit
synthetischen HTTP-Modellantworten und dem produktiven Zustandsschreiber.
Eine Agendaänderung spielt vier alte Ausschnitte mit zusammen 6000 Zeichen
erneut ab; der nächste Ausschnitt würde die unveränderte 6500-Zeichen-Grenze
überschreiten. Die Rücknahme liegt im späteren Fenster. Sechs neuere Fakten
verdrängen den zurückgenommenen Beschluss aus den optionalen Modell-Seeds.
Der alte Beschlussbeleg ist zudem im aktuellen Belegsatz bereits minimiert,
bleibt aber in der unveränderlichen aufgezeichneten Historie vorhanden.

Vorher entstanden eine neue aktuelle Beschlussidentität und bei ausdrücklich
angegebener alter Zielkennung ein unerlaubtes Wiederöffnen. Jetzt erkennt die
Deduplizierung auch zurückgenommene Beschlüsse anhand ihrer ursprünglichen
aufgezeichneten Belege. Ein bloßer Modell-Replay öffnet die Identität nicht.
Native Validierung und dynamisches Fakten-Schema geben zurückgenommene
Beschlüsse nur als Lesekontext frei. Ein eigenständiger neuer Beschluss mit
einer neuen Originalquelle bleibt möglich und erhält einen neuen Eintrag.

Der Test prüft beide Vorherfehler, byteunveränderte frühere Checkpoints,
mehrfache Wiederöffnung und den unabhängigen neuen Beschluss. Historische
Belege werden nicht in den begrenzten aktuellen Belegsatz hineinkopiert;
die unveränderte Sechs-Belege-Grenze und aktive Quellenidentität bleiben erhalten.
Die bisherigen Tabellenfälle erhöhen keine Testfunktionszählung. Diese
Nacharbeit ergänzt zusätzlich einen eigenen Produktions-Runner-Replaytest.
Abgelehnte Antworten verändern weder Aufgabe noch Zitate oder alten Checkpoint
und schreiben keine zweite Revision.
Zulässige Abschlüsse bewahren die wörtlichen Quellen und stimmen nach
wiederholter Wiederöffnung bytegleich überein.

Die vorherige Wortprüfung bleibt erhalten: fast, beinahe, nahezu, teilweise,
weitgehend und entsprechende englische Marker reichen nicht als Abschluss.
Englisches fast im Sinn von schnell bleibt von deutschem fast unterscheidbar.
Das sind begrenzte konservative Regeln, kein vollständiger Grammatik-
oder Bedeutungsbeweis. Menschliche Überprüfung bleibt erforderlich.

## Echte Referenzen und korrigierte Prüfsituation

Die frühere deutsche Prüfsituation verlangte einen offenen Gesprächsimpuls,
enthielt aber keine ausdrückliche Frage.
Ein tatsächlicher Durchlauf speicherte Aufgabe und Fakten ohne Impuls
und fiel daher durch diese zusätzliche Erwartung.
Die englische Referenz enthielt bereits „Who can approve the budget?“.

Die dokumentierte Fixture-Korrektur ergänzte „Wer kann das Budget freigeben?“.
Dieser Korpus und der Referenzhelfer bleiben in dieser Nacharbeit byteunverändert.
Alle bisherigen positiven Zusagen, der fehlende Budgetauftrag und der
unverbindliche Webseitenvorschlag bleiben enthalten.
Die englische Fassung einschließlich des wörtlichen „I’ll“ ist byteunverändert.
Alle fünf Kriterien, Quellenprüfung, Zeitlimits und das vorhandene Budget
von einer vollständig validierten Wiederholung je Anfrage bleiben unverändert.
Der neue Korpus wurde vor seinen Läufen separat festgelegt;
alte Fehlläufe werden dadurch nicht als bestanden umgedeutet.

| Prüfung | Tatsächliches Ergebnis |
| --- | --- |
| Rust-Workspace, alle Targets | Frisch **568 bestanden**, darunter **169 Notizenfälle** und **37 Providerprotokollfälle**; drei bestehende besondere Einstiege regulär ignoriert. |
| Rustformat und Clippy | Frisch bestanden, gesperrte Abhängigkeiten und Warnungen als Fehler. |
| Deutsch, tatsächliches lokales Modell | Frisch **136.64991 s**; reguläre vollständig validierte Wiederholungen: **0**. |
| Englisch, tatsächliches lokales Modell | Frisch **108.0559909 s**; reguläre vollständig validierte Wiederholungen: **0**. |
| Frontend | **416 Tests in 21 Dateien**, TypeScript, Produktionsbuild und Format auf dem hier bytegleichen b8b1ed8-UI-Stand bestanden; nicht erneut ausgeführt. |
| Projektprüfung | Nach dem letzten Dokumentedit vor Upload erneut verify.ps1 erforderlich: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen. |

Beide frischen Modellreferenzen auf dem unveränderten Korpus bestehen für positive
Aufgaben, unverbindliche Aussagen, Impulserhalt, menschlichen Status nach Protokolländerung und bytegleiche
Wiederöffnung. Die alte Grenze wurde nicht erhöht; null Wiederholungen sind
keine Freigabeschwelle.

Verwendet wurden ausschließlich synthetischer Text und das vorhandene
**google/gemma-4-e4b Q4_K_M**, 6.326.936.720 Modellbytes.
LM Studio **0.4.26+4**, Windows 11 Pro **26200**, Intel **i7-13700**,
rund **64 GiB** sichtbarer physischer Speicher.
Rechnerdaten, installierte LM-Studio-Version und ausgewählte Modellinstanz
wurden für diese Referenz frisch lesend geprüft.
Die Zeitmessung des Referenzhelfers umfasst Modellverarbeitung, dauerhaften
Abschluss und Wiederöffnung. Während der Inferenz liefen keine anderen eigenen
Prüfungen. Keine Downloads, Cloud oder Nutzermeetings.
Working Set, Private Bytes und Grafikspeicher wurden hier nicht neu gemessen;
Modell-Dateigröße ist keine RAM-Messung.

## Erhaltene Fehlläufe und Nachweisgrenzen

Ein erster deutscher Modellversuch dieses Satzteil-Stands hielt im ersten
Fenster mit der Meldung zum Antwortlimit an. Es entstand weder eine
Ergebnisrevision noch ein Referenz-Erfolgsvermerk; der ursprünglich geschriebene
Agendakontext blieb erhalten. Der englische Lauf wurde nach diesem Fehler
noch nicht gestartet. Die eigene Modellinstanz war danach nicht mehr geladen.
Die konkrete Phase, Finish-Reason und abgelehnte Antwort wurden vom Helfer
nicht separat gespeichert. Dieselbe Fehlerklasse kann aus dem Transport oder
aus einer Antwort mit weiteren Änderungen entstehen; ein tatsächlicher
4096-Token-Verbrauch ist damit nicht bewiesen. Die weitergehende Ursache
bleibt offen. Der Fehllauf bleibt erhalten und zählt nicht als bestanden.
Das hier gemessene getrennte Referenzpaar wurde anschließend mit identischem
Code, Modell, Korpus, Fristen und allen fünf Kriterien separat festgelegt.
Keine Akzeptanzgrenze oder Wiederholungsregel wurde dafür erweitert.


Ein deutscher Versuch auf dem Vorgängerstand 6170d4a hielt im zweiten Fenster wegen
einer Zahl ohne passenden Originalbeleg in einer Impulsänderung an
(NumberNotQuoted). Erster Checkpoint, menschlich besprochener Impuls und
Protokolländerung blieben erhalten; keine zweite Ergebnisrevision entstand.
Die konkrete abgelehnte Zahl, ihr Zitat und die Modellantwort wurden nicht
separat gespeichert, die weitergehende Ursache ist unbekannt. Ein damals getrenntes
weiteres Paar bestand mit identischem Code, Modell, Korpus und Kriterien;
dieser historische Erfolg wird nicht den frischen Messungen hier zugeordnet.
Der Fehllauf bleibt ausdrücklich erhalten und zählt nicht als bestanden.

Ein deutscher Versuch auf 9da4d6d brach im ersten Fenster
mit der Meldung zum Antwortlimit ab. Es entstand keine Ergebnisrevision; der ursprüngliche
Agendakontext blieb erhalten. Die genaue Phase und abgelehnte Modellantwort wurden
nicht separat gespeichert, die weitergehende Ursache ist unbekannt. Ein damals
separates weiteres Paar bestand mit identischem Code, Modell, Korpus und Kriterien;
dieser historische Erfolg wird den frischen Messungen hier nicht zugeordnet.
Auch dieser Fehllauf bleibt erhalten und zählt nicht als bestandene Referenz.

- Ein früher englischer Versuch des Vorgängers hielt im zweiten Fenster
  wegen einer nichtleeren unresolvedContextIds-Antwort korrekt an.
  Erster Checkpoint, besprochener menschlicher Impuls und Protokolländerung
  blieben erhalten. Die angeforderten Kennungen und die abgelehnte Antwort
  wurden nicht separat gespeichert; die weitergehende Ursache ist unbekannt.
- Der darauf folgende deutsche Versuch ohne ausdrückliche Budgetfrage fiel
  mangels Gesprächsimpuls durch. Sein Zustand und das erfolgreiche erste
  deutsche Ergebnis bleiben erhalten. Das oben genannte Paar verwendet den
  ausdrücklich beschriebenen neuen deutschen Korpus.
- Auf 5896b2b speicherte ein früher deutscher Versuch die Zusage nur als
  unklaren Fakt ohne Aufgabe und fiel durch. Ein damaliges weiteres Paar
  bestand auf unverändertem Code und Korpus. Auch dieser Fehllauf bleibt erhalten.
- Ein früherer deutscher Versuch auf da6b5c2 hielt wegen fehlend gemeldeten
  Kontexts an. Der damalige Linux-Lauf auf 689212b scheiterte mit 10159/10154 ms
  an der unveränderten Obergrenze 10000 ms. Weitergehende Ursachen wurden
  nicht nachgewiesen. Spätere erfolgreiche Heads ersetzen diese Ergebnisse nicht.

Kleine Modelle können ungültige oder unvollständige Antworten liefern.
Ungültige Bündel werden nicht veröffentlicht; gesicherte Arbeit bleibt erhalten.
Ein bestandenes Referenzpaar beweist keine vollständige Erkennung aller Aussagen.

Die Sidebar-/RecordingStatus-/MeetingView-Browserprüfung bleibt dem relevanten
bytegleichen 93d48e2-Komponentenstand zugeordnet: 24 Layout-, 24 Warn-Titel-
und 24 klassische Meetingtitel-Zustände.
Die Kartenprüfung auf 8d0ba21 verwendete die tatsächliche OverviewCard in Edge
156.0.4314.58 mit Playwright 1.57.0: acht Kombinationen aus 390/1280 Pixeln,
Deutsch/Englisch und hell/dunkel, vier Touchfälle, Tastatur und Außenklick.
Keine Seiten-/Konsolenfehler oder Fremdanfragen; eigener Browser und Server beendet,
Screenshot visuell geprüft. Diese Browserprüfungen wurden hier nicht erneut ausgeführt.
Native Tauri-WebViews, IPC, OS-Tooltips und Screenreader-Sprache sind damit nicht geprüft.

Der frühere kontrollierte Langsammodelllauf auf 35d53e8 dauerte 244.33 s
mit 210 s SSE-Pause. Höchstens eine Anfrage lief gleichzeitig;
ein Impuls blieb erhalten. 3737920 Samples / 16000 Hz = 233.62 s je Spur.
Beide synthetischen PCM-Schreiber liefen nach Auswertungsstopp weiter.
Das ist kein realer kombinierter Zweistundenlauf.
Die 21 Paketregressionen auf 91f0985 nutzten synthetische ZIPs;
hier wurde kein neues Produktpaket oder Installer bereitgestellt.

Auf 5896b2b bestanden tatsächlich alle 22 damaligen Checks:
16 erfolgreich und sechs reguläre Skips.
Die [Plattformläufe](https://github.com/Puma7/meeting-for-handy/actions/runs/42101116336)
enthielten tatsächliche generierte Sprachprüfungen mit Version 1.1.0-beta.1,
Handy 0.9.8 und Nemotron 3.5 Q8 auf CPU:
Linux 10.509165387 s, ARM 13.535942541 s, Intel 26.465786573 s.
Eigener Worker-Neustart und Audiohash-/Journalerhalt bestanden.
Der Windows-Gegenlauf war modellfrei und ist kein ASR-Referenzlauf.
Unit-Fixtures mit version=test werden nicht als diese Messungen ausgegeben.

Die virtuelle Linux-Aufnahme dieses Vorgängers bestand ausdrücklich
mit passed-with-startup-gaps und completeCapture=false:
9642/9646 ms, 146399/144399 qualifizierte Tonsamples,
native Startauffüllung 7881/9947 Samples und je ein eingehender Phasennullwert.
Ton-, Fenster-, Dauer-, Differenz- und Zeitgrenzen blieben unverändert.
Die Quellen waren ausschließlich virtuelle Geräte.
Daraus folgt keine lückenlose physische Aufnahme.
Diese CI-Ergebnisse werden nicht dem neuen Head zugeordnet.

## Reviewstand und Veröffentlichung

Alle 16 ursprünglichen [Sourcery-Anmerkungen](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5748992615)
wurden einzeln geprüft, beantwortet und geschlossen.
Der paginierte Audit vergleicht die ursprünglichen Kommentartexte und Identitäten.
Alle bisherigen Korrekturen bleiben enthalten: geschützte Originalbelege,
menschliche Änderungen, erhaltene Impulse und Notizentwürfe, stabile Aufgaben
beim Modellwechsel, Zurücknahme veralteter Quellen, sichere Wiederaufnahme,
Modellreservierungen und kompakte zugängliche Bedienung.
Die ausführlichen Vorher-/Nachherfälle bleiben über die unveränderliche Historie erreichbar.

Die zwei sauberen Codex-Codeprüfungen auf 5896b2b waren historische Einzelnachweise.
Der folgende vollständige CodeRabbit-Lauf fand die inzwischen korrigierte
doppelte Workspace-Projektion. Nachfolgende Codex-Läufe fanden Wortmarker
auf 90becdd, quantitative Teilfortschritte auf dfe5d65 und die zu breite
Prozentprüfung auf 94b3ee6 sowie die nachgestellten Adverbien auf 9da4d6d. Eine erste Code-Runde auf
6170d4a war sauber; dessen zweite Runde fand die zu breite Verhältnisprüfung.
Die folgende Runde auf e502c91 fand Mengenadverbien und zurückgenommene Beschlüsse.
Die Runde auf c8ffeed fand verneinte Rücknahmen und den mehrteiligen Operator out of.
Die folgende Runde auf 35a9bee fand beabsichtigte deutsche Entscheidungen.
Die Runde auf b8b1ed8 fand eine falsche Hilfsverb-Zuordnung über Satzteilgrenzen.
Diese Runden waren keine sauberen Abschlüsse.

Die Projektionskorrektur wurde von CodeRabbit
[bestätigt](https://github.com/Puma7/meeting-for-handy/pull/41#discussion_r4485972047).
Die pauschale zusätzliche Testisolations-/Browser-Sorge wurde mangels konkretem
Fehler [zurückgenommen](https://github.com/Puma7/meeting-for-handy/pull/41#issuecomment-6669253684).
Native WebViews bleiben eine Nachweisgrenze.
Die zusätzliche LanguageTool-Prüfung war zuletzt teilweise unvollständig;
Ursache unbekannt, Tool und Regeln bleiben aktiv.
Alte oder leere Security-Flags sind kein aktueller formaler Securitynachweis.

Vor Upload werden eigener Git-Root, Branch und tatsächlicher Remote-Vorgänger geprüft.
Alle Dateien des tatsächlichen unveränderlichen Remote-ZIP werden mit lokalen Bytes
und Git-Blobs verglichen. Alle 102 PR-Patches werden vollständig paginiert;
100 zulässige Dateien bleiben unter den unveränderten zwei Lockdatei-Filtern.
Für den endgültigen Head sind zwei frische saubere Codex-Codeprüfungen,
eine vollständige saubere CodeRabbit-Runde und dessen eigene grüne CI erforderlich.
Später tatsächlich abgeschlossene Prüfungen und Links werden im PR nachgelesen
und dokumentiert, ohne nachträglich Produktbytes umzuschreiben.

## Daten, Verhalten und Kompatibilität

Die klassische Transkriptansicht und der optionale Advanced Meeting Assistant
bleiben getrennt nutzbar und verwenden gemeinsame Modelleinstellungen.
Parakeet Ultra verarbeitet abgeschlossene Abschnitte beider Spuren während
der Aufnahme nacheinander. Audio wird unabhängig gesichert.
Erst dauerhaft bestätigter Text erreicht die Auswertung.
Intervallticks brechen langsame Modellläufe nicht ab;
dieselbe Sitzung startet keine überlappenden Auswertungen.

Automatische erweiterte Auswertung bleibt lokal. Cloud braucht ausdrückliche
Textfreigabe je Auswertung und erhält weder Audio noch lokale Pfade
oder Sitzungsobjekte. Weiterleitungen, System-Proxys und automatischer
Anbieterwechsel bleiben gesperrt. Private Vorbereitung bleibt privat.
Originaljournale, Transkriptstände, Modellfassungen und menschliche Bestätigungen
bleiben versioniert erhalten. Schlüssel werden nicht protokolliert oder als Klartext gespeichert.

Bestehende native Sitzungen behalten Modellkennung und gesicherte Gewichte.
Fortsetzen verifiziert Katalog und Gewichte; fehlende oder geänderte Modelle
werden nicht still ersetzt. Archiviertes Audio muss zuerst wiederhergestellt werden.
Legacy-Originalsitzungen bleiben lesbar; ihre nachträgliche Erkennung
benötigt weiterhin die bisherige Programmversion.
Fremde Modellreservierungen bleiben fremd; TTL 300 s und Erneuerung 60 s
sowie bestehende Speicher- und Verwaltungsschranken bleiben erhalten.

Statische Produktprompts, statische Vertragsdateien, Belegbudget, Fristen und Transport,
Aufnahme-/ASR-Logik, Modellreservierungen, Abhängigkeiten, Schutz-/Reviewerregeln,
Version und vollständiger TEST_REPORT bleiben gegenüber b8b1ed8 unverändert.
Die bereits enger gefassten dynamischen Änderungszweige für zurückgenommene
Beschlüsse bleiben hier ebenfalls byteunverändert.
Auch der deutsche und englische synthetische Referenztext bleiben hier byteunverändert.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und zweistündige Gerätetests
wurden auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-,
kombinierte Zweistundenlast-, unabhängige installierte Prozess-, Signatur-,
Notarisierungs- oder forensische Speicherfreigabe.
Cloud-Prüfungen ohne echte Zugangsdaten bleiben Simulationen.
Bekannte gesperrte Abhängigkeitsrisiken bleiben dokumentiert.
Unbekannte Zuständige und Termine bleiben unbekannt; Abschnittszeiten sind keine Wortzeiten.
Technische Belegprüfung beweist keine inhaltliche Deutung.
Ergebnisse bleiben überprüfbare KI-Entwürfe.
Diese Nacharbeit erstellt keinen installierten Programmstand und keinen Release.
