<p align="center"><img src="assets/handymeeting.svg" width="76" alt="Meeting for Handy-Logo"></p>
<h1 align="center">Meeting for Handy</h1>
<p align="center"><strong>Dein Meeting. Dein Computer. Dein Transkript.</strong></p>
<p align="center">Lokale Aufnahme, Live-Transkript und Meeting-Notizen mit Originalzitaten.</p>

<p align="center">
<a href="https://github.com/Puma7/meeting-for-handy/releases/latest">Download</a> ·
<a href="README.md">English</a> ·
<a href="docs/USER-GUIDE.md">Benutzerhandbuch (englisch)</a> ·
<a href="https://github.com/Puma7/meeting-for-handy/releases">Änderungen</a> ·
<a href="https://github.com/Puma7/meeting-for-handy/issues">Feedback</a>
</p>

Meeting for Handy ist eine eigenständige **Rust-/Tauri-Companion-App für [Handy](https://github.com/cjpais/Handy)**. Sie nimmt den Computerton von Zoom, Teams oder anderen Meeting-Apps und auf Wunsch dein Mikrofon als getrennte Spur auf und transkribiert lokal, während das Meeting läuft. Eine Live-Zusammenfassung hält währenddessen mit; danach entstehen belegte Zusammenfassungen, Entscheidungen und To-dos – ohne Audio in die Cloud zu schicken.

## Download

**[Aktueller Release](https://github.com/Puma7/meeting-for-handy/releases/latest)** – unter *Assets* das passende Paket wählen. Was sich in jeder Version geändert hat, steht auf der [Release-Seite](https://github.com/Puma7/meeting-for-handy/releases).

| System | Paket | Voraussetzung |
| --- | --- | --- |
| Windows 10/11 · x64 | Setup-EXE oder ZIP ohne Installation | WebView2 und Microsoft Visual C++ x64 |
| Ubuntu 22.04+ / Debian 12–13 · x64 | DEB | PulseAudio oder PipeWire-Pulse |
| Mac · Apple Silicon | `osx-arm64`-DMG | macOS 15+, Aufnahmeberechtigungen |
| Mac · Intel | `osx-x64`-DMG | macOS 15+, Aufnahmeberechtigungen |

Unter Windows installiert das Setup nach `%LOCALAPPDATA%\Programs\Meeting for Handy` und legt einen Startmenü-Eintrag an. Ein neueres Setup aktualisiert die App an derselben Stelle; Aufnahmen und Einstellungen bleiben erhalten. Die App kann auch selbst nach Updates suchen.

Die Pakete sind nicht codesigniert (macOS: ad hoc signiert, nicht notarisiert), daher kann das Betriebssystem vor dem ersten Start warnen. Schutzfunktionen dafür nicht abschalten. Echte Mac-/Linux-Geräteaufnahmen und ein vollständiger Zweistundentest der Rust-App stehen noch aus; zuerst mit einem unkritischen Meeting testen. [Messungen und offene Grenzen](TEST_REPORT.md).

## Was die App kann

| Aufnehmen | Verstehen | Kontrolle behalten |
| --- | --- | --- |
| Getrennte Spuren für Computerton und optional Mikrofon, mit Live-Pegeln | Lokale Streaming-Spracherkennung mit kompatiblen Handy-Modellen | Die Audiosicherung läuft weiter, auch wenn die Erkennung ausfällt |
| Live-Transkript mit Suche, Pause und Mikrofon stummschalten | **Live-Zusammenfassung** im Meeting: Verlauf, Entscheidungen, Aufgaben, Fragen | Lokale Historie mit Suche, Namen, VIP-Markierung, Archiv und Audiokopien |
| Export als TXT, Markdown oder JSON und Sitzungsordner per Klick | Nachbearbeitung mit eigenen Vorlagen und Tabs | Originaltranskripte und jede Auswertungsfassung bleiben erhalten |

Jede Aussage einer Zusammenfassung verweist auf einen vorhandenen Transkriptabschnitt und ein wörtliches Zitat; unbekannte Zuständige und Termine bleiben unbekannt. Ergebnisse bleiben KI-Entwürfe zum Prüfen.

![Transkript und lokale Dateien in Meeting for Handy](assets/screenshots/transcript-light.png)
*Echte App-Komponenten mit künstlichen Demo-Inhalten. Deutsch und Englisch sind enthalten.*

![Nachbearbeitung mit Originalbelegen](assets/screenshots/analysis-light.png)

## Das erste Meeting

1. **Handy** installieren und dort ein kompatibles **Streaming-Sprachmodell** herunterladen. Meeting for Handy liest Handys Modellliste; Modellgewichte sind nicht im Download enthalten.
2. Das passende Paket installieren (Windows-Setup oder ZIP, Linux-DEB über den Paketmanager, Mac-App aus der DMG in „Programme“).
3. **Meeting → Aufnahme einrichten:** Namen, Sprachmodell und die Tonausgabe der Meeting-App wählen. Für eine eigene Spur das Mikrofon einschalten.
4. Kurz testen und beide Pegel prüfen. Die Tonausgabe kann auch andere Programme enthalten.
5. Meeting beenden, Transkript lesen und **Dieses Meeting auswerten** wählen. **Sitzungsordner öffnen** zeigt Audio, Journale, Exporte und Auswertungen.

Für Zusammenfassungen braucht es zusätzlich ein lokales Textmodell: **Ollama** oder **LM Studio**. Die App findet beide auf diesem Computer, schlägt passend zum Arbeitsspeicher Gemma 4 E2B/E4B vor und kann den Vorschlag über Ollama laden. Ein Sprachmodell allein schreibt keine Notizen.

Live-Zusammenfassungen halten das gewählte lokale Modell bis zum gespeicherten Abschluss reserviert; bei fehlgeschlagenem Abschluss bleibt es für die Wiederaufnahme verfügbar. LM Studio benötigt native REST-v1-Unterstützung und eine bestätigte eigene Modellinstanz.
Für die erste eigene Ladung bei der Vorbereitung einer Auswertung oder Live-Zusammenfassung darf der ausgewählte LM-Studio-Modellschlüssel keine geladene Instanz haben. Eine bereits geladene fremde Instanz wird weder übernommen noch entladen; die App lädt dann keine zweite Kopie. Eine bereits bestätigte eigene Reservierung wird wiederverwendet.
Ollama verwendet eine gemeinsame serverseitige Lebensdauer statt eigener Instanzen je Client. Beim letzten eigenen Abschluss endet die Erneuerung ohne Entladeanfrage oder Zurücksetzen einer alten Lebensdauer. Das Modell kann daher bis zum Ablauf der bestehenden endlichen Frist warm bleiben (normalerweise bis zu 300 Sekunden nach der letzten Anfrage); andere Clients können sie verlängern. LM Studio gibt weiterhin nur die exakt bestätigte eigene Instanz frei.
Native Modellreservierungen verwenden von Bestandsabfrage bis Ladung, Auswertung und Freigabe dieselbe konkrete Loopback-Adresse. Dabei bedeutet localhost 127.0.0.1; für einen ausschließlich über IPv6 erreichbaren Dienst muss [::1] ausdrücklich eingerichtet werden. Ist die Ablaufzeit eines bereits geladenen Ollama-Modells unlesbar, stoppt die Vorbereitung vor jeder Modelländerung; seine Aufbewahrung wird nicht geraten.
Siehe [Anbietervoraussetzungen, Referenzprüfung und Grenzen](docs/validation/local-model-residency-20261030.md).

## Datenschutz und Grenzen

- **Audio bleibt lokal.** Die Spracherkennung läuft in eigenen lokalen Prozessen und nutzt Handys Modelldateien.
- **Lokale Textmodelle sind Standard.** Ein Cloud-Anbieter ist optional und braucht für jeden Lauf deine ausdrückliche Freigabe; er erhält Text, Quellenkennungen und Arbeitsaufträge, nie Audio oder Dateipfade. Es gibt keinen automatischen Cloud-Ersatz und keinen System-Proxy.
- **Schlüssel gelten nur für die App-Sitzung**, außer du legst sie in Credential Manager, Keychain oder Secret Service ab.
- **Wiederherstellung:** Gesicherte Schreibpunkte überstehen getestete Prozessabbrüche; die letzten unbestätigten Sekunden können fehlen. Das ist kein Schutz bei Stromausfall. Aufnahmen, Transkripte und Auswertungen werden nie automatisch gelöscht.
- **Zwei Spuren sind keine Personenerkennung.** Namen werden nur nach deiner Bestätigung zugeordnet; optionale Sprecherexperimente liefern neutrale Kennungen je Abschnitt.
- **Keine versteckte Aufnahme.** Lokale Agenten brauchen für jede Aufnahme eine sichtbare Freigabe und eine eigene Mikrofonfreigabe ([Agentenprotokoll](docs/RUST_AGENT_PROTOCOL.md)).

Die Daten liegen im lokalen Anwendungsdatenordner `HandyMeeting/RustBeta` (Name aus Kompatibilitätsgründen beibehalten). Dateien sind nicht verschlüsselt; ein Synchronisierungsdienst kann sie hochladen.

## Im Handy-Ökosystem

Die Companion-Richtung entstand aus [Handy Discussion #1923](https://github.com/cjpais/Handy/discussions/1923). Meeting for Handy liest installierte Modelle und nutzt die transcribe.cpp-Bindings mit festgelegter nativer Laufzeit; Handy-Einstellungen, Schlüssel und Modelle werden nie verändert. Meeting for Handy ist **kein offizielles Handy-Produkt**. [Vorschlag für eine Companion-Schnittstelle](docs/HANDY_UPSTREAM_PROPOSAL.md).

## Entwicklung

[Build und Prüfungen](docs/RUST_BUILD.md) · [Veröffentlichen](docs/RELEASING.md) · [CI](docs/CI.md) · [Lokale Dateien](docs/MEETING_FILES.md) · [Geräteprüfplan](docs/NATIVE_VALIDATION.md) · [Roadmap](docs/OPTIONAL_FEATURE_ROADMAP.md) · [Mitwirken](CONTRIBUTING.md) · [Sicherheit](SECURITY.md)

| Ordner | Inhalt |
| --- | --- |
| `crates/` | Rust-Bibliotheken: Aufnahme und Sitzungen (`meeting-core`), Notizen und Live-Zusammenfassung (`meeting-notes`), Handy-Erkennung (`handy-bridge`) |
| `apps/desktop/` | Tauri-App: Rust-Backend (`src-tauri/`) und React-Oberfläche (`src/`) |
| `scripts/` | Build-, Paket-, Installer- und Release-Skripte mit ihren Tests |
| `docs/` | Dokumentation; `docs/evidence/` enthält Messprotokolle, `docs/legacy/` die Unterlagen des C#-Prototyps |
| `licenses/` | Original-Lizenztexte aller mitgelieferten Abhängigkeiten |

**Releases:** Auf GitHub einen Release mit neuem Tag (`v1.2.3`) erstellen – Version, Pakete und Release-Notizen entstehen automatisch ([Ablauf](docs/RELEASING.md)).

**Alter C#-Prototyp:** `src/`, `verify.ps1`, `build.ps1`, `setup-native.ps1`, `global.json`, `NuGet.config`, `System.Numerics.Tensors.*`, `licenses/nuget/`, `licenses/nuget-packages.json`, `licenses/Avalonia-MIT.txt` und `licenses/Inter-OFL.txt` gehören zu den früheren C#-Prototypen (0.4/0.5). Die CI prüft sie noch mit, sie werden aber nicht mehr ausgeliefert. **Mit Version 2.0 werden sie entfernt**; danach enthält das Repository nur noch das Rust-Projekt.

MIT-Lizenz. Dank an Handy, transcribe.cpp, Tauri und die verwendeten Bibliotheken; ihre Hinweise stehen in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) und `licenses/`.
