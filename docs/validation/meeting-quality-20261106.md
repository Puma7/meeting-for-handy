# Kurze, belegte Meeting-Ergebnisse

Diese Nacharbeit verbessert die allgemeine Auswertung; sie korrigiert keine
einzelne Nutzersitzung und verändert weder Originaltranskripte noch Aufnahmen.

## Verhalten

- Prompt `handymeeting-workspace-10` verlangt einzelne kurze Handlungstitel
  statt fortlaufend verlängerter Sammelabsätze. Die produktive Anfrage und die
  native Antwortprüfung begrenzen neue Ergebnistitel auf 180 Zeichen.
- Ausdrückliche Zuweisungen wie „Jana holt die Technik ab“ erreichen nun die
  Aufgabenphase. Fragen, Bedingungen, Verneinungen und unverbindliche Vorschläge
  bleiben durch die vorhandenen Prüfungen gesperrt.
- Indirekte offene Fragen benötigen kein Fragezeichen. Der erste ausdrücklich
  erkannte, bisher unbeantwortete Klärungsbedarf bindet den zusätzlichen
  Frageauftrag an seinen tatsächlichen Originalbeleg. Ein beliebiger anderer
  Impuls kann diesen Auftrag nicht erfüllen. Das ist keine Garantie, dass ein
  Modell jede denkbare offene Frage semantisch vollständig erkennt.
- Eindeutige Kalenderangaben erscheinen in der deutschen Anzeige als Tag.Monat,
  in US-Englisch als Monat/Tag. Gesprochene Ordnungszahlen, Monatsnamen und
  eindeutig zuordenbare numerische Daten werden kalendarisch geprüft. Eine
  fehlende Jahreszahl bleibt fehlend; relative, ungültige oder mehrdeutige
  Angaben werden nicht anhand der Systemuhr geraten. Unsicherheiten bleiben
  sichtbar. Gespeicherte Terminbelege und wörtliche Zitate bleiben unverändert.
- Wenn ein historischer Eintrag seinen bisherigen Absatz nur erweitert,
  zeigt der Verlauf zunächst die Ergänzung. Der vollständige gespeicherte
  Stand bleibt aufklappbar. Statuswechsel, menschliche Änderungen und
  inhaltliche Korrekturen bleiben als vollständige Ereignisse sichtbar.
- Neue Auswertungen erhalten eine eigene Promptidentität und `workspace-v10`.
  Vorherige `workspace-v9`-Fassungen bleiben lesbar und werden nicht mit der
  neuen Promptfassung fortgeschrieben. Freigegebene menschliche Eingaben und
  bestätigte Zustände werden über die bestehenden geprüften Regeln erhalten.

## Nachweise und Grenzen

Native und gerenderte Komponentenregressionen reproduzieren die vorher
fehlende Aufgabenaufnahme, wiederholte Absätze, fehlende Datumsdarstellung
und einen sachfremden Ersatzimpuls. Sie prüfen außerdem unveränderte Belege,
Suchbarkeit der angezeigten Daten, Wiederöffnung und getrennte Promptstände.
Die bestehende englische Referenzvorlage einschließlich `I’ll` bleibt erhalten.

Die echten lokalen Referenzen verwenden ausschließlich künstliche Gespräche,
LM Studio 0.4.28+2 und das vorhandene Gemma 4 E4B Q4_K_M. Die zusätzliche
deutsche und englische Qualitätsvorlage verlangt drei getrennte benannte
Aufgaben mit wörtlichen Terminen, einen belegten indirekten Frageimpuls sowie
erhaltene manuelle Markierungen und bytegleiche Wiederöffnung. Der erste
deutsche Versuch lieferte einen sachfremden Impuls und wurde deshalb nicht als
Qualitätsnachweis akzeptiert; die zusätzliche Belegbindung behebt diesen Fall.
Private Versuche und Messbelege bleiben lokal.

Gemessen wird auf Windows x64 / Intel Core i7-13700 mit rund 63,7 GiB nutzbarem
RAM. Keine eigenen Compiler liefen während der Modellreferenzen. Laufzeiten
gelten für zwei synthetische Textfenster, nicht für eine physische Aufnahme.
Working Set und Private Bytes werden getrennt mit 2-Sekunden-Stichproben
beobachtet; Grafikspeicher wird nicht gemessen und Fremdlast nicht isoliert.
Die ersten deutschen Prozessmessungen erfassten nur einen Teil der
LM-Studio-Prozesse und sind kein vollständiger Modell-Speichernachweis.

Aufnahmepipeline, Cloudfreigabe, Netzwerk- und Eigentümerschutz, Abhängigkeiten,
Version 1.1.0-beta.1, Reviewerregeln und der gesamte `TEST_REPORT.md` bleiben
unverändert. Keine Nutzermeetings im Referenztest, keine Downloads oder Cloud.
Keine neue Geräte-, GPU-, Zweistunden-, Installations- oder Releasefreigabe.
Ergebnisse bleiben überprüfbare KI-Entwürfe; wörtliche Belege beweisen nicht
automatisch die Richtigkeit ihrer inhaltlichen Deutung.
