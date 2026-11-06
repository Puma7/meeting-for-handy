# PR 41: Validierung der Reviewnacharbeit

Stand: 6. November 2026. Version **1.1.0-beta.1**, unverändert.
Diese Notiz beschreibt synthetische Prüfsituationen, darunter ausdrücklich echte
lokale Modellinferenz mit synthetischem Text. Der ganze TEST_REPORT bleibt zur
PR-Basis byteunverändert. Private Rohbelege und Nutzermeetings werden nicht veröffentlicht.

## Sieben aktuelle Korrekturen

Die [vollständige CodeRabbit-Runde auf a8e5554](https://github.com/Puma7/meeting-for-handy/pull/41#pullrequestreview-5802955306)
lieferte sieben nachvollziehbare Hinweise. Die Korrekturen betreffen:

- **Lokale Bereinigung:** Scheitert ein einzelner Browser-Speicherschlüssel nach
  einer bestätigten Löschung, werden die übrigen genau zugeordneten Schlüssel
  weiterhin versucht. Der unvollständige Zustand bleibt sichtbar. Ein Fehler am
  zuerst eingefügten Basisschlüssel ließ vorher den späteren Notizhöhenschlüssel
  liegen; danach wird dieser entfernt. Andere Meetings bleiben erhalten. Dies ist
  eine synthetische UI-Speicherprüfung, keine Löschung echter Nutzermeetings.
- **Aktualisierung nach Bedienaktionen:** Stoppen oder menschliches Abhaken
  während einer laufenden Statusabfrage merkt genau eine weitere Abfrage vor.
  Normale Pollticks sammeln keine Warteschlange an. Nach dem Schließen wird keine
  nachgeholte Abfrage gestartet. Vorher blieben die beiden Änderungen zunächst
  hinter der älteren Antwort verborgen; danach werden sie aktuell dargestellt.
- **Audiovorschau:** Ein Wechsel der Quelldatei beginnt eine vollständig neue
  Vorschau-Lebenszeit. Verspäteter Erfolg und Fehler der alten Datei werden
  ignoriert, alte Objekt-URLs freigegeben, spätere Antworten nach Unmount verworfen.
  Der Transport selbst wird dabei nicht als abgebrochen ausgegeben. Vorher konnten
  alte Bytes oder eine alte Fehlermeldung die neue Quelle überschreiben.
- **Fehlermeldung für Audioabschnitte:** Die gemeinsame Prüfung nennt nun den
  tatsächlich übergebenen Höchstwert und verlangt einen geschlossenen, nicht leeren
  Abschnitt. Die Grenzen bleiben 120 Sekunden für Analyse, 300 Sekunden für die
  vollständig geprüfte Vorschauquelle und 30 Sekunden für zurückgegebene
  Vorschau-Samples. Auch nicht zurückgegebenes Audio wird weiterhin hashgeprüft.
- **Beschädigte klassische Altstände:** Ein beschädigter Lauf ohne Workspace darf
  einen neuen Meetingkontext erst nach positiv bestätigter Abwesenheit von
  workspace-v9 passieren lassen. Existierende, unlesbare, verlinkte oder nicht als
  Verzeichnis bestätigte Einträge blockieren weiter. Alle vorhandenen Dateien
  bleiben erhalten; ältere Freigaben werden nicht über einen beschädigten
  Workspace hinweg wiederbelebt. Vier synthetische Klassifikationen geprüft.
- **Eigene LM-Studio-Instanz:** Der angeforderte Ladekontext wird zur eigenen
  Instanz gespeichert. Eine zuvor klassisch mit Standardkontext geladene Instanz
  wird nicht still als 8192-Kontext der Meetingbegleitung wiederverwendet. Die
  klassische Auswertung bleibt verwendbar; weder zweite Ladung noch fremde
  Übernahme oder Entladung. Gleich konfigurierte eigene Workspace-Instanzen werden
  in den echten aufeinanderfolgenden Modellreferenzen wiederverwendet.
- **Ollama-Reservierung:** Die Speicherzulassung einer Kontextänderung geschieht
  vor der Änderung der bestehenden Reservierung. Eine abgewiesene Zuteilung mit
  unbekannter Modellgröße ließ vorher die klassische Auswertung unbenutzbar;
  danach bleibt deren ursprünglicher Kontext verwendbar. Nach einer tatsächlich
  gesendeten Verlängerung bleiben beabsichtigter Kontext und unbestätigter Zustand
  bis zur bestätigten Erneuerung erhalten. Dieser zusätzliche Gegenfall besteht
  auch auf dem Ausgangsstand und ist kein weiterer behaupteter ursprünglicher Fehler.

## Frische Prüfungen dieses Stands

| Prüfung | Tatsächliches Ergebnis |
| --- | --- |
| Rust Workspace/All-Targets | **574 bestanden**, darunter **170 Notizen- und 40 Providerfälle**; drei bestehende besondere Einstiege regulär ignoriert. |
| Frontend | **434 Tests in 21 Dateien** bestanden. |
| TypeScript und Produktionsbuild | Bestanden, vorhandene gesperrte Pakete. |
| Rustformat, Frontendformat, Clippy | Bestanden; Compiler- und Clippywarnungen als Fehler. |
| Projektprüfung | verify.ps1: 47 PCM-, 35 Notizen-, 28 Vorlagen-, acht Agenten- und Monitorprüfungen. |
| Browser | **12** tatsächliche Konfigurationen: Stoppen, menschlicher Status und Vorschau; Deutsch/Englisch, 390/1280 Pixel. |
| Echtes kleines lokales Modell, DE | **150.9987959 s**, **1** reguläre Validierungswiederholung(en), alle fünf Kriterien bestanden. |
| Echtes kleines lokales Modell, EN | **130.9460047 s**, **0** reguläre Validierungswiederholung(en), alle fünf Kriterien bestanden. |

Alle sieben Hinweise haben gezielte Regressionen mit tatsächlichen Fehlern des
Ausgangsstands und bestandenem korrigiertem Stand. Ursprüngliche und zwischenzeitlich
fehlerhafte private Testvorbereitungen bleiben erhalten und sind keine behaupteten
Produktfehler: der zuerst entfernte Core-Test lief null Tests; die erste
Bereinigungsfixture hatte eine nicht auslösende Schlüsselreihenfolge; die erste
Erneuerungsfixture erwartete fälschlich einen Fehler trotz erlaubter bestätigter
Wiederholung. Diese Fälle wurden vor der endgültigen Bewertung korrigiert.

Die Browsermatrix verwendete vorhandenes Playwright **1.57.0** und Edge,
Deutsch hell/Englisch dunkel. Tatsächliche Mausbedienung der echten
WorkspaceBridge-/Vorschau-Komponenten, verzögerte synthetische API-Antworten und
synthetische WAV-Bytes. Keine native IPC, Aufnahme, Modelle oder Nutzermeetings
im Browserfixture. Keine relevanten Konsolen-/Seitenfehler oder fremden Anfragen;
Workspace ohne horizontalen Seitenüberlauf. Zwei Bilder tatsächlich angesehen.
Eigene Browser und Server beendet. Ein vorher falscher englischer Tabselektor
wurde nur im privaten Helfer berichtigt; dessen fehlgeschlagener Lauf bleibt erhalten.

Referenzumgebung: vorhandenes **google/gemma-4-e4b Q4_K_M**, **6.326.936.720
Modellbytes**, LM Studio **0.4.26+4**, Windows 11 Pro **26200**, Intel **i7-13700**,
rund **64 GiB** physischer RAM. Verstrichene Sekunden bis zur validierten
dauerhaften Speicherung und Wiederöffnung, keine Geschwindigkeitsfreigabe.
Keine eigenen Builds, Tests oder Browser während der Inferenz; lesende
GitHub-Abfragen waren erlaubt. Fremde Last war nicht isoliert. Working Set,
Private Bytes und Grafikspeicher wurden nicht neu gemessen; Dateigröße ist
keine RAM-Messung. Die eigene ausgewählte Instanz war danach freigegeben.

Unveränderter DE/EN-Korpus einschließlich I’ll und Budgetfragen. Fünf Kriterien:
positive Aufgabe, unverbindliche Aussage nicht als Aufgabe, erhaltene Impulse,
menschlicher Impulsstatus nach Protokolländerung und bytegleiche Wiederöffnung.
Höchstens eine vollständig validierte Wiederholung pro Anfrage bleibt zulässig.
Keine Cloud, Downloads oder Nutzermeetings. Der englische Referenzpatch
93335102 bleibt Bestandteil des PR.

## Erhaltene Korrekturen und Reviewreichweite

Die vorige Testkorrektur bleibt erhalten: ausschließlich der Zwei-Sekunden-UI-Poll
wird im Testhelfer gesteuert. Andere Assertionstimer und die Fünf-Sekunden-Testfrist
bleiben real und unverändert. Derselbe verzögerte Vorherfall überschritt diese
Frist, danach bestand er; ursprüngliche Assertions blieben erhalten. Keine
produktive Provider-, Capture- oder Inferenzfrist wurde dafür verändert.

Kartenmenü mit nativem Top Layer und zugänglichem Ersatzweg, Capturekontrollen,
Quellenautorität, verbindliche/unverbindliche Aussagen, Impulserhaltung,
Notizversionen, Layouts und SemVer-Updater bleiben erhalten. Historische
Browser-/Gerätenachweise werden diesem neuen Stand nicht als frisch zugeordnet.

Alle 16 ursprünglichen Sourcery-Anmerkungen und die bisherigen CodeRabbit-Threads
sind einzeln geprüft und beantwortet. Die sieben aktuellen Threads werden nach
der Veröffentlichung dieser Korrekturen beantwortet. Auf a8e5554 bestanden zwei
frische CODE-Runden und alle 22 CI-Checks (16 erfolgreich, sechs regulär
übersprungen). Seine vollständige CodeRabbit-Runde war mit diesen sieben
Hinweisen ausdrücklich **nicht sauber**, Risiko Moderate. Alle 100 ausgewählten
Dateien entsprachen dem vollständig paginierten tatsächlichen 102-Dateien-Diff
mit den unveränderten zwei Lockdateifiltern; keine unverfügbaren Änderungen.
Dokumentation **82.71 Prozent** bei unveränderter **80-Prozent-Grenze**.
Alle Risiko-, Architektur-, Zusatz-, Nitpick-, Outside-diff- und operativen Texte
wurden gelesen. Der neue Stand braucht eigene saubere Reviews und eigene grüne
CI. Alte/null Security-Flags sind kein aktueller formaler Securitynachweis.

## Grenzen

Aufnahmen, Journale, Originaltranskripte und menschliche Bestätigungen bleiben
versioniert erhalten. Private Vorbereitung bleibt privat. Automatische Auswertung
bleibt lokal; Cloud benötigt ausdrückliche Textfreigabe je Auswertung und erhält
weder Audio noch lokale Pfade oder Sitzungsobjekte. Keine Weiterleitungen,
System-Proxys, automatischen Anbieterwechsel oder fremden Modellentladungen.
Prompts, Verträge, Belegbudget, Fristen, Ollama-TTL 300 s/Erneuerung alle 60 s,
Pakete, Lizenzen, Schutz-/Reviewerregeln, Dokumentationsgrenze, Version und ganzer
TEST_REPORT bleiben unverändert.

Physische Aufnahme-, Berechtigungs-, Geräteverlust- und Zweistundenprüfungen sind
auf später verschoben. Keine neue echte Parakeet-Dauer-, GPU-, kombinierte
Zweistundenlast-, unabhängige installierte Prozess-, Signatur-, Notarisierungs-
oder forensische Freigabe. Virtueller Ton mit Startunterbrechungen bestätigt keine
lückenlose physische Aufnahme. Cloudprüfungen ohne Zugangsdaten bleiben
Simulationen. Unbelegte Zuständige/Termine bleiben unbekannt; Abschnittszeiten
sind keine Wortzeiten. Technische Belegprüfung beweist keine inhaltliche Deutung.
Ergebnisse bleiben überprüfbare KI-Entwürfe.

[Unveränderliche vorige Validierung und historische Nachweise](https://github.com/Puma7/meeting-for-handy/blob/a8e5554db6592bedcd9596df3acdb613f7a5d2f6/docs/validation/pr41-review-followup-20261104.md).
Frühere Fehlläufe und offene Ursachen bleiben erhalten. Diese Nacharbeit erstellt
keinen installierten Programmstand oder Release.
