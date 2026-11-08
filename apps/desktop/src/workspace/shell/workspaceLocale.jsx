import { createContext, useContext } from "react";

export const WorkspaceLanguage = createContext("de");

const english = {
  "Vollständigen Stand anzeigen": "Show complete version",
  "Diese Auswertung verwendet ältere Prompts und bleibt unverändert lesbar. Eine neue lokale Auswertung starten, um mit der aktuellen Fassung weiterzuarbeiten.":
    "This analysis uses older prompts and remains readable. Start a new local analysis to continue with the current version.",
  Beleg: "Evidence",
  "Die Änderung wurde nicht übernommen. Bitte aktuellen Auswertungsstand abwarten und erneut versuchen.":
    "The change was not applied. Wait for the current analysis state and try again.",
  "Freigegebene Vorbereitung und Notizen sind zu groß. Bitte Text kürzen oder weniger Themen freigeben; private Entwürfe bleiben erhalten.":
    "Shared preparation and notes are too long. Shorten the text or share fewer topics; private drafts are retained.",
  durchsuchen: "search",
  Protokollnotiz: "Minutes note",
  " Die Höhe bleibt nur für diese Ansicht erhalten; der frühere Browserstand wird bewahrt.":
    " The height is kept only for this view; the previous browser state is retained.",
  Vorbereitung: "Preparation",
  Transkript: "Transcript",
  Überblick: "Overview",
  Details: "Details",
  Impulse: "Prompts",
  Verlauf: "History",
  Offen: "Open",
  "Belegt · KI": "Source-linked · AI",
  "Angerissen · KI": "Mentioned · AI",
  Besprochen: "Discussed",
  "Laut Gespräch erledigt": "Reported complete",
  Zurückgenommen: "Withdrawn",
  "Zuordnung prüfen": "Check attribution",
  "Dieser Layoutname ist bereits vergeben.":
    "This layout name is already in use.",
  "Layout konnte nicht gespeichert werden.": "The layout could not be saved.",
  "Für diese Änderung zuerst den gespeicherten Meetingstand öffnen.":
    "Open the saved meeting analysis before making this change.",
  "Notizstand gesichert": "Notes saved",
  "Live-Überblick": "Live overview",
  "Jetzt relevant": "Relevant now",
  "Änderungen im Gespräch": "Changes in the conversation",
  Gesprächsverlauf: "Conversation history",
  Gesprächsimpulse: "Conversation prompts",
  "Zur Standardansicht": "Standard view",
  "Frühere Auswertungen": "Earlier analyses",
  "Öffnet frühere gespeicherte Auswertungen dieses Meetings zum Nachlesen.":
    "Open earlier saved analyses for this meeting.",
  "Transkript kopieren": "Copy transcript",
  "Transkript kopiert": "Transcript copied",
  "Kopiert das bisherige Transkript beider Spuren, unabhängig vom Filter.":
    "Copy the transcript so far from both tracks, regardless of filters.",
  Aufnahme: "Recording",
  "Aufnahme beenden und sichern": "Stop and save the recording",
  "Aufnahmehinweise in der Standardansicht öffnen":
    "Open recording notices in the standard view",
  "KI-Auswertung": "AI analysis",
  "Aktualisiert die KI-Auswertung, sobald der laufende Durchgang fertig ist.":
    "Refresh AI analysis when the current update is complete.",
  "Startet die lokale KI-Auswertung des bisherigen Transkripts.":
    "Start local AI analysis of the transcript so far.",
  Vorläufig: "Provisional",
  "Diese Wörter können sich während der Erkennung noch ändern.":
    "These words can still change during transcription.",
  "Aufnahme prüfen": "Check recording",
  "Aufnahme pausiert": "Recording paused",
  "Aufnahme läuft": "Recording in progress",
  "Erkennung holt nach": "Transcription catching up",
  "Aufnahme beenden": "Stop recording",
  "Aufnahme fortsetzen": "Resume recording",
  "Aufnahme pausieren": "Pause recording",
  "Mikrofon aktivieren": "Enable microphone",
  "Mikrofon stummschalten": "Mute microphone",
  "Textmodell einrichten": "Set up text model",
  "Öffnet die Einstellungen für Anbieter und Modell der Meeting-Auswertung.":
    "Open the provider and model settings for meeting analysis.",
  "Auswertung läuft in der bisherigen Ansicht.":
    "Analysis is running in the standard view.",
  "Cloud-Modell in den Einstellungen ausgewählt":
    "Cloud model selected in settings",
  "Gespeicherte Auswertung wird geladen …": "Loading saved analysis …",
  "Gespeicherte Auswertungen": "Saved analyses",
  "Gespeicherte Auswertungen konnten nicht geprüft werden.":
    "Saved analyses could not be verified.",
  "Ein früherer lesbarer Stand wird angezeigt.":
    "An earlier readable revision is displayed.",
  "Gespeicherte Ergebnisse sind derzeit nicht verfügbar.":
    "Saved results are currently unavailable.",
  "Die Dateien bleiben unverändert erhalten. Prüfe sie in der Standardansicht.":
    "Files are kept unchanged. Review them in the standard view.",
  Intervall: "Interval",
  "Erster Lauf": "First pass",
  "Wartezeit bis zur ersten automatischen Auswertung. Danach folgt bei neuem Transkript der nächste Lauf mit kurzer Pause.":
    "Delay before the first automatic update. Subsequent updates follow new transcript text after a short pause.",
  Auswertungsintervall: "Analysis interval",
  "Legt fest, wie oft die KI den Überblick aktualisiert.":
    "Choose how often AI updates the overview.",
  "2 Min.": "2 min",
  "3 Min.": "3 min",
  "Nur Auswertung beenden": "Stop analysis only",
  "Stoppt die KI-Auswertung. Eine laufende Aufnahme geht weiter.":
    "Stop AI analysis while recording continues.",
  "Erweiterte Auswertung nur lokal verfügbar":
    "Advanced analysis requires a local model",
  "Lokale Auswertung starten": "Start local analysis",
  "Helles Design aktivieren": "Switch to light theme",
  "Dunkles Design aktivieren": "Switch to dark theme",
  "Der lokale Notizstand ist nicht lesbar oder beschreibbar. Vorhandene Daten bleiben erhalten; neue Änderungen nur in diesem Fenster.":
    "Local notes cannot be read or written. Existing data is retained. New changes remain in this window.",
  Meetingbereiche: "Meeting sections",
  "Überblick-Layout": "Overview layout",
  "Zwischen deinen gespeicherten Kachelanordnungen wechseln":
    "Switch between your saved card layouts",
  "Meine Vorbereitung": "My preparation",
  "Mein Ziel": "My goal",
  "Private Vorbereitung": "Private preparation",
  "Nur freigegebene Gesprächspunkte und Protokollnotizen gehen an das Modell.":
    "Only shared topics and meeting notes are sent to the model.",
  "Originalstände gesichert": "Original revisions saved",
  "kein Textmodell aktiv": "no text model active",
  "· KI-Entwürfe mit Originalbelegen": "· AI drafts with original evidence",
  Originalbeleg: "Original evidence",
  "Früherer Transkriptstand ·": "Earlier transcript version ·",
  "· Abschnittszeiten": "· Section times",
  "Im Transkript ansehen": "View in transcript",
  "Der Originalabschnitt ist in diesem Stand nicht verfügbar.":
    "The original section is unavailable in this revision.",
  "Eigene Gesprächspunkte vor dem Meeting ergänzen.":
    "Add your topics before the meeting.",
  "Noch nicht abgeglichen": "Not yet reviewed",
  Freigeben: "Share",
  "Wieder öffnen": "Reopen",
  "Als besprochen markieren": "Mark as discussed",
  "Neuer Gesprächspunkt": "New topic",
  "Eigenen Gesprächspunkt ergänzen …": "Add your own topic …",
  "Gesprächspunkt hinzufügen": "Add topic",
  "Nur freigegebene Gesprächspunkte werden automatisch abgeglichen.":
    "Only shared topics are automatically reviewed.",
  "Wichtige Details": "Key details",
  Anzahl: "Count",
  "Anzahl Gesprächsimpulse": "Number of conversation prompts",
  von: "of",
  "Suchen …": "Search …",
  Transkriptspur: "Transcript track",
  "Beide Spuren": "Both tracks",
  Computerton: "System audio",
  Mikrofon: "Microphone",
  Transkriptzeitraum: "Transcript range",
  Gesamt: "All",
  Letzte: "Last",
  "Min.": "min",
  "Individueller Zeitraum": "Custom range",
  "Neueste zuerst": "Newest first",
  "Älteste zuerst": "Oldest first",
  "Neueste zuerst · Reihenfolge umkehren": "Newest first · reverse order",
  "Älteste zuerst · Reihenfolge umkehren": "Oldest first · reverse order",
  "Verlauf nach Status filtern": "Filter history by status",
  "Alle Status": "All statuses",
  "Von (Min.)": "From (min)",
  "Bis (Min.)": "To (min)",
  "Das Ende muss nach dem Anfang liegen.": "The end must be after the start.",
  "Bisheriger Stand": "Current state",
  "Zuständig:": "Owner:",
  "nicht benannt": "unspecified",
  "· Termin:": "· Due:",
  "Für diese Auswahl kein Transkriptabschnitt.":
    "No transcript section matches this selection.",
  "Für diese Auswahl kein gespeicherter Eintrag.":
    "No saved item matches this selection.",
  "Hier erscheinen belegte Entwicklungen und Statusänderungen. Der Wortlaut steht im Transkript.":
    "Source-linked developments and status changes appear here. Read the wording in the transcript.",
  "Noch keine geprüfte lokale Auswertung.": "No verified local analysis yet.",
  Notizblock: "Notepad",
  "Notizbereich Höhe ändern": "Resize notes area",
  "Notizen nach oben größer ziehen · Doppelklick: Standardhöhe":
    "Drag up to enlarge notes · double-click for default height",
  "Nach oben größer ziehen. Pfeil hoch oder runter ändert die Höhe. Pos1: klein, Ende: groß. Doppelklick: Standardhöhe.":
    "Drag up to enlarge. Up and down arrows resize. Home makes it small, End makes it large. Double-click restores default height.",
  Notizen: "Notes",
  Privat: "Private",
  "Fürs Protokoll": "For the minutes",
  "Nicht im Protokoll": "Excluded from minutes",
  "Eigene Protokollnotiz": "My minutes note",
  "Chat wird gesondert angeschlossen. Die lokale Kachelauswertung ist davon unabhängig.":
    "Meeting chat will be connected separately. Local card analysis works independently.",
  "Meeting fragen": "Ask meeting",
  "Private Notiz": "Private note",
  "Was möchtest du für dich festhalten?":
    "What would you like to note privately?",
  "Was soll im Protokoll stehen?": "What should appear in the minutes?",
  "Enter: neue Zeile": "Enter for a new line",
  "Nur für diese Ansicht": "Only in this view",
  "Im Browser gesichert": "Saved in the browser",
  "Dialog schließen": "Close dialog",
  "Bitte einen Layoutnamen eingeben.": "Enter a layout name.",
  "Standardlayout gespeichert.": "Default layout saved.",
  "Layout gespeichert.": "Layout saved.",
  "Modul abgelegt.": "Module placed.",
  "am Spaltenende": "at the end of the column",
  "Verschieben abgebrochen.": "Move cancelled.",
  "Dein Überblick": "Your overview",
  "wird verschoben": "is being moved",
  "Layouts verwalten": "Manage layouts",
  Layoutname: "Layout name",
  "Als neues Layout speichern": "Save as a new layout",
  "Aktuelles Layout umbenennen": "Rename current layout",
  "Als Standard öffnen": "Open by default",
  "Anzahl Spalten": "Number of columns",
  Spalten: "Columns",
  Module: "Modules",
  "Module im Überblick": "Overview modules",
  "Ausblenden ändert keine Meetinginhalte. Notizen bleiben unten.":
    "Hiding a module preserves meeting contents. Notes stay at the bottom.",
  "Voreinstellung wiederherstellen": "Restore preset",
  "Letzte Layoutänderung rückgängig": "Undo last layout change",
  "Hilfe zum Anordnen": "Layout help",
  "Deinen Überblick anordnen": "Arrange your overview",
  "Am Griff ziehen. Die blaue Linie zeigt den Einfügeplatz ober- oder unterhalb einer Karte.":
    "Drag the handle. The blue line shows where the card will be inserted.",
  "Spaltentrenner und Kartenunterkante verändern die Größe. Die unterste Karte füllt automatisch bis zu den Notizen. Tastatur: Griff aktivieren, Pfeiltasten, Enter zum Ablegen, Escape zum Abbrechen.":
    "Drag column dividers or card edges to resize. The bottom card fills the space above notes. Keyboard: activate a handle, move with arrow keys, Enter to drop, Escape to cancel.",
  "Griff aktivieren, mit Pfeiltasten verschieben, Enter zum Ablegen, Escape zum Abbrechen.":
    "Activate the handle, move with arrow keys, Enter to drop, Escape to cancel.",
  "Anordnung nur für diese Ansicht. Der frühere Browserstand bleibt unverändert.":
    "Layout changes apply to this view. The previous browser state is retained.",
  "Hier in Spalte": "Drop into column",
  ablegen: "here",
  "Hier einfügen:": "Insert here:",
  Verschieben: "Move",
  "Nach oben": "Move up",
  "Nach unten": "Move down",
  "Spalte nach links": "Column to the left",
  "Spalte nach rechts": "Column to the right",
  "Höhe · bis zu den Notizen": "Height · down to notes",
  Höhe: "Height",
  Modul: "Module",
  "Bereits sichtbare Module tauschen den Platz.":
    "Visible modules swap places.",
  Gemerkt: "Saved",
  Merken: "Save",
  Fertig: "Done",
  Anordnen: "Arrange",
  "Verlauf-Layout": "History layout",
  "Regie-Layout": "Control layout",
  Aktualisieren: "Refresh",
  Vorgemerkt: "Queued",
  Ziele: "Goals",
  " · manuell": " · manual",
  " · Standard": " · Default",
  " · Kompakt": " · Compact",
  " · Groß": " · Large",
  vergrößern: "expand",
  verschieben: "move",
  anordnen: "arrange",
  Kartenhöhe: "card height",
  "Modul austauschen": "replace module",
  "aus Überblick entfernen": "remove from overview",
  "Höhe ändern": "resize height",
  offen: "open",
  Abschnitte: "sections",
  bestätigt: "confirmed",
  Freigaben: "shared",
  "Belege bis": "Evidence through",
  "Zeichen noch nicht ausgewertet": "characters not yet analysed",
  "Diese Stelle liegt hinter der Vorschau der ersten 30 Sekunden.":
    "This section starts beyond the first thirty seconds of the preview.",
  "Audiovorschau des Originalabschnitts":
    "Audio preview of the original section",
  "Audio wird geladen …": "Loading audio …",
  "Audiovorschau laden": "Load audio preview",
  "Für diesen Abschnitt ist keine lokale Audiovorschau verfügbar. Archiviertes Audio kann nach dem Wiederherstellen erneut geprüft werden.":
    "Local audio preview is unavailable for this section. Restore archived audio and try again.",
  "verschoben.": "moved.",
  "aufgenommen. Mit Pfeiltasten verschieben, Enter zum Ablegen, Escape zum Abbrechen.":
    "picked up. Move with arrow keys, Enter to drop, Escape to cancel.",
  oberhalb: "above",
  unterhalb: "below",
  Spalte: "column",
  "{module} in Spalte {n} abgelegt.": "{module} dropped in column {n}.",
  "Überblick Spalte": "Overview column",
  und: "and",
  "Breite ändern": "resize width",
  "frühere Einträge nur im Verlauf": "earlier entries in history only",
  "Früherer Transkriptstand · ": "Earlier transcript version · ",
};

export function workspaceText(language, text) {
  return language === "en" &&
    Object.prototype.hasOwnProperty.call(english, text)
    ? english[text]
    : text;
}

export function useWorkspaceText() {
  const language = useContext(WorkspaceLanguage);
  return (text) => workspaceText(language, text);
}
