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
- 279 Oberflächenprüfungen, TypeScript und Produktionsbuild bestehen.
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

Der Referenzlauf lädt oder entlädt keine Modellinstanz, lädt kein Modell herunter
und verwendet keine Nutzermeetings. Er belegt keine neue Residenz- oder
Freigabezusage für fremde Instanzen. Physische Aufnahmeunterbrechungen,
Gerätetests, GPU-Auslastung und ein gemeinsamer Zweistundenlauf sind damit
nicht nachgewiesen oder behoben. `TEST_REPORT.md`, Paketversionen,
Abhängigkeitssperren und Schutzkonfigurationen bleiben unverändert.
