# Kurze, belegte Meeting-Ergebnisse

Diese Nacharbeit verbessert die allgemeine Auswertung; sie korrigiert keine
einzelne Nutzersitzung und verändert weder Originaltranskripte noch Aufnahmen.

## Verhalten

- Prompt `handymeeting-workspace-11` verlangt einzelne kurze Handlungstitel
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
- Neue Auswertungen erhalten eine eigene Promptidentität und `workspace-v11`.
  Vorherige `workspace-v9`- und `workspace-v10`-Fassungen bleiben lesbar und werden nicht mit der
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

Die anschließende Codeprüfung fand zwei zusätzliche gültige Randfälle:
negative Pronomen wie „nichts“/„nothing“ bei neuen Aufgaben und punktgetrennte
Uhrzeiten wie „9.05 Uhr“. Beide wurden vor der Korrektur reproduziert. Die
gemeinsame Aufnahmeprüfung weist jetzt auch diese negativen Pronomen zurück;
bejahte Prüfaufträge mit eingebetteten Fragen bleiben zulässig. Expliziter
Uhrzeitkontext (`Uhr`, `am`/`pm`, `um`/`at`) hat bei punktgetrennten Zahlen
Vorrang vor einer Kalenderinterpretation. Echte Daten neben einer Uhrzeit
werden weiterhin formatiert; Zitate und gespeicherte Termine bleiben wörtlich.
Die Terminanzeige berücksichtigt dafür auch den Originalbeleg, wenn das Modell
aus „9.05 Uhr“ nur „9.05“ als Terminwert übernimmt.
Ein weiterer gültiger Reviewhinweis betraf großgeschriebene Bitten, Vermutungen
und unbestimmte Personen als vermeintliche Namen. Diese Präfixe werden jetzt
auch vor alten Aufgabenmarkern abgefangen; echte benannte und pronominale
Zuweisungen bleiben zulässig. Die betroffenen Ablauf- und Regressionfunktionen
sind dokumentiert; die unveränderte Dokumentationsgrenze bleibt 80 Prozent.

Eine weitere gezielte Runde reproduzierte drei verbleibende Zuordnungsfehler:
bejahte Schutz-/Prüfzusagen mit „nothing“ oder „nichts“ wurden verworfen,
Sprecherpräfixe verdeckten Bitten und unbestimmte Personen, und ausdrückliche
Zukunftsaussagen mit „wahrscheinlich“/„probably“ galten als verbindlich.
Die Prüfung trennt jetzt auch diese Fälle. Unsicherheit ausschließlich vor
einem Datum erhält die Aufgabe ohne bestätigten Termin; unabhängige
Verneinungen und Bedingungen sperren weiterhin. Regressionen prüfen die
klassische Ausgabe und die atomare Workspace-Veröffentlichung einschließlich
unveränderter Originalbelege. Es bleibt eine begrenzte sprachliche Prüfung,
keine Garantie für die Deutung jedes frei formulierten Gesprächs.

Die erneute echte deutsche Referenz deckte zusätzlich eine falsche Zuordnung
auf: Ein unverbindliches Thema erhielt einen vorhandenen Beleg einer anderen
Aufgabe. Dieser Versuch gilt ausdrücklich als nicht bestanden. Die neue
Promptfassung verlangt deshalb einen eigenen inhaltlich passenden Beleg pro
Handlung und Frage, verbietet erfundene Klärungsaufgaben und doppelte Impulse.
Eine eigene Promptidentität verhindert unbemerkte Fortschreibung älterer
Fassungen. Die technische Literalprüfung allein beweist weiterhin keine
semantische Richtigkeit; auch kleine lokale Modelle können falsch zuordnen.
Die aktuelle deutsche Qualitätsreferenz und die unveränderte englische
Basisreferenz bestanden mit Prompt 11: keine zusätzlichen unverbindlichen
Aufgaben, erhaltene manuelle Markierungen und bytegleiche Wiederöffnung.
Die manuelle Sichtung des deutschen Ergebnisses zeigt allerdings noch zwei
ähnlich formulierte Rückfragen zum selben Thema. Die Referenzkriterien prüfen
keine vollständige semantische Deduplizierung; das bleibt eine Qualitätsgrenze.

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
