export const de = {
  audioQuality: "Aufnahme prüfen",
  audioQualityHelp:
    "Bereits gesicherte Daten bleiben erhalten. Eine Unterbrechung meldet der Audiotreiber meist, wenn der Rechner kurz so ausgelastet war, dass die App den Ton nicht rechtzeitig abholen konnte, zum Beispiel weil ein lokales Textmodell fast den ganzen Arbeitsspeicher belegt. Direkt beim Aufnahmebeginn melden manche Treiber sie auch ohne Verlust. Hör dir die genannte Stelle an (ältere Aufnahmen nennen keine Stelle; dann die ganze Spur), bevor du dich auf die Vollständigkeit verlässt.",
  technicalDetails: "Technische Meldungen anzeigen",
  openFolder: "Sitzungsordner öffnen",
  openFolderHelp:
    "Öffnet den lokalen Ordner mit Audiospuren, Textjournalen und gespeicherten Auswertungen im Dateimanager.",
  exportTranscript: "Transkript speichern",
  exportFormat: "Exportformat",
  exportTranscriptHelp:
    "Speichert den vollständigen aktuell gesicherten Text beider Spuren als neue Text-, Markdown- oder JSON-Datei im Unterordner exports. Frühere Exporte bleiben erhalten; Zeiten bleiben Abschnittszeiten.",
  exportSaved: "Transkript gespeichert:",
  savedFilesHelp:
    "Audio und Textjournale werden bereits automatisch lokal gesichert. Jeder Export ist eine zusätzliche Momentaufnahme.",
  captureSetup: "Aufnahme einrichten",
  analysisSetup: "Auswertung einrichten",
  connectionSettings: "Verbindung und Zugangsschlüssel",
  setupHelp: "Hilfe zur lokalen Auswertung",
  analysisDetails: "Details zur Auswertung",
  meetingDetail: "Sitzungsdetails",
  chooseMeeting: "Meeting für die Auswertung",
  selectMeeting: "Bitte ein Meeting auswählen",
  analyzeMeeting: "Dieses Meeting auswerten",
  backHistory: "Zur Historie",
  localServiceHelp:
    "Für Zusammenfassungen brauchst du zusätzlich einen lokalen Textmodelldienst, zum Beispiel LM Studio. Lade dort ein Textmodell und starte den lokalen Server. Übernimm dessen API-Adresse; die Standardadresse von LM Studio steht unten. Handy übernimmt weiterhin die Spracherkennung.",
  localEndpoint: "Adresse des lokalen Textmodelldienstes",
  refreshTextModels: "Textmodelle aktualisieren",
  loadingTextModels: "Textmodelle werden abgefragt …",
  selectTextModel: "Lokales Textmodell auswählen",
  noTextModels: "Der lokale Dienst bietet noch kein Textmodell an.",
  textModelsHelp:
    "Prüfe, ob der lokale Server in LM Studio oder deinem anderen Textmodelldienst läuft und dort ein Modell bereitsteht. Danach die Textmodelle aktualisieren.",
  manualTextModel: "Modellnamen manuell eingeben",
  textModelUnavailable:
    "Die bisherige Modellwahl wird von diesem Dienst nicht angeboten. Bitte ausdrücklich ein verfügbares Modell auswählen oder den Namen manuell bestätigen.",
  notesTask: "Auswertung",
  notesNotStarted: "Auswertung noch nicht gestartet.",
  notesCheckingStatus:
    "Auswertung läuft. Der aktuelle Fortschritt wird abgefragt …",
  startAnalysis: "Auswertung starten",
  analysisReady:
    "Vorlage und Textmodell sind bereit. Klicke auf „Auswertung starten“, um die Verarbeitung zu beginnen.",
  analysisChooseModel:
    "Wähle ein Textmodell und prüfe den lokalen Modelldienst. Danach kannst du die Auswertung starten.",
  analysisFailureHelp:
    "Prüfe die Verbindung und das gewählte Textmodell. Öffne das zugehörige Meeting, um die Auswertung erneut zu starten oder gesicherte Schritte fortzusetzen.",
  cancelAnalysis: "Auswertung abbrechen",
  cancelRequested: "Abbruch angefordert …",
  cancelHelp:
    "Gesicherte Schritte bleiben erhalten. Der Textmodelldienst kann bereits gestartete Berechnungen noch beenden.",
  notesCancelled:
    "Auswertung abgebrochen. Gesicherte Schritte bleiben erhalten.",
  notesComplete: "Auswertung gespeichert.",
  notesFailed:
    "Auswertung fehlgeschlagen. Bereits gesicherte Schritte bleiben erhalten.",
  cancelUnavailable:
    "Der Abbruch wurde nicht bestätigt. Der aktuelle Status wird geprüft.",
  elapsed: "Vergangene Zeit",
  confirmedTasks: "geprüfte und gespeicherte Teilaufgaben",
  inferenceUnknown:
    "Die Rechenzeit des Modells ist nicht vorhersehbar. Der Fortschritt zählt ausschließlich gesicherte Teilaufgaben.",
  stagePreparing: "Transkript und Arbeitsaufträge vorbereiten",
  stageRequesting: "Textmodell verarbeitet den aktuellen Abschnitt",
  stageValidating: "Originalzitate und Quellen prüfen",
  stageSaving: "Arbeitsstand lokal speichern",
  progressUnavailable: "Der Fortschritt ist vorübergehend nicht erreichbar.",
  openAnalysisMeeting: "Zugehöriges Meeting öffnen",
  resumeAnalysis: "Gesicherte Auswertung fortsetzen",
  unfinishedAnalyses: "Unfertige Auswertungen",
  skippedAnalyses:
    "Einige gespeicherte Auswertungen werden nicht angezeigt, weil sie die Prüfung nicht bestehen, zu einer anderen Sitzung gehören oder ein unbekanntes Speicherformat haben. Die Dateien bleiben unverändert im Sitzungsordner erhalten:",
  earlierPrompt: "Frühere Promptfassung",
  readOnly: "nur lesbar",
  earlierPromptPending:
    "Dieser Arbeitsstand bleibt erhalten, wird aber nicht fortgesetzt. Bitte bei Bedarf eine neue Auswertung starten.",
  earlierPromptResult:
    "Diese Auswertung ist nur lesbar. Sie wird weder fortgesetzt noch mit neuen Ergebnissen gemischt.",
  rejectedItems:
    "KI-Aussagen wurden verworfen, weil ihr Beleg oder Umfang die Prüfung nicht bestand. Sie werden nicht angezeigt.",
  resumeAnalysisHelp:
    "Dieser Arbeitsstand verwendet seine ursprünglichen Quellen, Arbeitsaufträge, API-Adresse und sein ursprüngliches Modell. Bereits bestätigte Teilaufgaben werden wiederverwendet. Schlüssel und eine Cloud-Freigabe werden nicht aus der gespeicherten Sitzung übernommen.",
  resumeHelp:
    "Verwendet den ursprünglichen Transkriptstand, die Vorlage und das Modell dieses Laufs.",
  copyMarkdown: "Alle Ergebnisse als Markdown kopieren",
  markdownHelp:
    "Enthält alle Tabs sowie wörtliche Belege und Verweise auf das Originaltranskript. Aktiver HTML-Inhalt wird entschärft.",
  wrongMeeting:
    "Diese Auswertung gehört zu einer anderen Sitzung und wird hier nicht verwendet.",
  pendingNotes:
    "Bitte zuerst die offenen Transkriptabschnitte nachholen oder ein vollständig erkanntes Meeting auswählen.",
  dismiss: "Hinweis schließen",
  preferencesError:
    "Die gespeicherten Einstellungen sind nicht lesbar oder nicht beschreibbar. Vorhandene Werte bleiben erhalten.",
  handyVersionMissing:
    "Handy erkannt; die Installation liefert keine Versionsangabe",
  firstModelLoad:
    "Die Liste enthält die vom Dienst gemeldeten Modellkennungen. Wähle ein Modell für Textantworten. Manche Dienste laden es erst bei der ersten Anfrage; dabei kann die erste Auswertung länger dauern.",
  meeting: "Meeting",
  history: "Historie",
  historyHelp:
    "Gespeicherte Meetings finden, Transkripte öffnen und Ergebnisse weiterbearbeiten.",
  notes: "Nachbearbeitung",
  settings: "Einstellungen",
  companion: "Braucht die Sprachmodelle aus Handy, läuft aber als eigene App",
  refresh: "Aktualisieren",
  connecting: "Verbindung wird hergestellt …",
  disconnected: "Desktop-Verbindung nicht verfügbar",
  retry: "Erneut versuchen",
  title: "Meetingtitel",
  model: "Streaming-Modell aus Handy",
  system: "Computerton",
  microphone: "Mikrofon zusätzlich aufnehmen",
  noDevices: "Keine passenden Geräte gefunden",
  noModels: "Kein kompatibles installiertes Streaming-Modell gefunden",
  modelUnavailable:
    "Das zuvor gewählte Modell ist nicht mehr installiert oder kompatibel. Bitte wähle ausdrücklich ein anderes Streaming-Modell.",
  choose: "Bitte auswählen",
  start: "Aufnahme starten",
  stop: "Meeting beenden",
  ready: "Bereit zur Aufnahme",
  recording: "Audioaufnahme läuft",
  stopping: "Aufnahme wird beendet …",
  stoppingDetail:
    "Meeting wird beendet. Gespeichertes Audio bleibt erhalten; offene Abschnitte werden verarbeitet …",
  live: "Transkript",
  copy: "Transkript kopieren",
  copied: "Kopiert",
  emptyTranscript:
    "Hier erscheint der Text, sobald die lokale Erkennung Abschnitte liefert.",
  emptyHistory:
    "Noch keine gespeicherten Meetings. Starte eine Aufnahme. Ältere Prototyp-Sitzungen lassen sich unter Einstellungen importieren.",
  open: "Öffnen",
  duration: "Dauer",
  sourceTimes:
    "Zeitangaben bezeichnen Aufnahmeabschnitte, keine exakten Wortzeiten.",
  saved: "Gesichertes Audio",
  lag: "Erkennungsrückstand",
  warnings: "Hinweise zur Sitzung",
  sourceSystem: "Computerton",
  sourceMicrophone: "Mikrofon",
  preserve: "Audio wird unabhängig von der Erkennung lokal gesichert.",
  installation: "Handy-Installation",
  executable: "Handy-Programmdatei (optional)",
  executableHelp:
    "Handy wird automatisch erkannt. Eine andere Installation kann über den Dateidialog ausgewählt werden. Die Modellliste stammt aus dem installierten Handy.",
  version: "Version",
  notFound: "Noch nicht erkannt",
  compatibility:
    "Neue Handy-Modelle erscheinen nach Aktualisierung der Liste. Streaming-Unterstützung und kompatible Erkennungslaufzeit werden zusätzlich geprüft.",
  language: "Sprache",
  theme: "Darstellung",
  light: "Hell",
  dark: "Dunkel",
  systemTheme: "System",
  storage: "Sitzungen",
  noMeeting: "Bitte zuerst eine Sitzung mit Transkript öffnen.",
  processing: "Verarbeitung",
  local: "Lokal (Standard)",
  cloud: "Cloud · nur Text",
  endpoint: "API-Adresse",
  textModel: "Textmodell",
  apiKey: "API-Schlüssel (optional)",
  apiKeyHelp:
    "Schlüssel werden nicht im Browser gespeichert. Die Cloud erhält ausschließlich freigegebenen Text, keine Audiodateien.",
  template: "Vorlage",
  templateName: "Name der Vorlage",
  sessionKey: "Schlüssel nur für diese App-Sitzung",
  credentialStored:
    "Für diese Adresse liegt ein Schlüssel im geschützten Betriebssystem-Speicher.",
  credentialMissing: "Für diese Adresse ist kein Schlüssel gespeichert.",
  credentialUnavailable:
    "Der geschützte Schlüsselspeicher ist nicht verfügbar. Vorhandene Einträge werden nicht ersetzt.",
  useSavedKey: "Gespeicherten Schlüssel für diese Adresse verwenden",
  replaceCredential:
    "Den vorhandenen, lesbaren Schlüssel ausdrücklich ersetzen",
  confirmRemoveCredential:
    "Ich möchte den gespeicherten Schlüssel für diese Adresse entfernen",
  removeCredential: "Gespeicherten Schlüssel entfernen",
  saveCredential: "Schlüssel im Betriebssystem speichern",
  credentialHelp:
    "Speichern und Entfernen erfolgen nur durch diese Schaltflächen. Ein gespeicherter Schlüssel erteilt keine Cloudfreigabe; Transkripttext muss weiterhin für jeden Lauf bestätigt werden.",
  agentControl: "Lokaler Agentenzugriff",
  agentConnected: "Verbindung aktiv",
  agentDisconnected: "Verbindung beendet",
  agentScope:
    "Eine Freigabe gilt nur für die angefragte Aufnahme und deren Live-Transkript. Sie erlaubt keine Cloudübertragung oder Anbieterwechsel. Aufnahmeende bei Zeitlimit oder Verbindungsende.",
  agentOwnedSession: "Eigene Agentensitzung",
  agentExpires: "endet spätestens",
  agentDuration: "Höchstdauer",
  agentWithMicrophone: "Computerton und Mikrofon angefragt",
  agentSystemOnly: "Nur Computerton angefragt",
  agentMicrophoneConsent:
    "Mein Mikrofon ausdrücklich für diese Anfrage freigeben",
  agentChooseDevices:
    "Bitte zuerst unter Meeting ein verfügbares Streaming-Modell und die Computerausgabe auswählen.",
  agentChooseMicrophone:
    "Bitte zuerst unter Meeting ein verfügbares Mikrofon auswählen.",
  agentBusy:
    "Eine Aufnahme oder deren Abschluss läuft bereits. Die neue Anfrage kann erst danach freigegeben werden.",
  agentDeny: "Anfrage ablehnen",
  agentApprove: "Diese Aufnahme erlauben",
  agentRevoke: "Agentenzugriff widerrufen",
  agentRevoked:
    "Zugriff widerrufen. Eine manuell gestartete Aufnahme bleibt unberührt.",
  duplicateTemplate: "Vorlage duplizieren",
  duplicateName: "Kopie",
  newTemplateHelp:
    "Erstellt eine neue Vorlage mit den drei Standardaufträgen. Danach kannst du Schritte hinzufügen, entfernen und ändern.",
  duplicateTemplateHelp:
    "Erstellt eine unabhängige Kopie aller Arbeitsaufträge. Gespeicherte Auswertungen bleiben unverändert.",
  renameTemplateHelp: "Ändert nur den Vorlagennamen für künftige Auswertungen.",
  deleteTemplateHelp:
    "Diese Vorlage aus der Auswahl entfernen? Bereits gespeicherte Auswertungen und ihre ursprünglichen Arbeitsaufträge bleiben erhalten.",
  duplicateTemplateName:
    "Dieser Vorlagenname ist bereits vergeben. Bitte einen anderen Namen wählen.",
  templateSaveFailed:
    "Die Vorlage konnte nicht gespeichert werden. Deine Eingaben bleiben geöffnet. Nach einem Konflikt die Bibliothek aktualisieren und die Änderung erneut anwenden.",
  templateLoadFailed:
    "Die Vorlagenbibliothek konnte nicht geladen werden. Bitte erneut laden; vorhandene Daten werden nicht ersetzt.",
  refreshTemplates: "Vorlagen neu laden",
  importBrowserTemplates: "Frühere Browser-Vorlagen übernehmen",
  browserTemplatesHelp:
    "Übernimmt die alten lokalen Browser-Vorlagen in den App-Speicher. Bestehende Vorlagen und Auswertungen bleiben erhalten; Konflikte werden als zusätzliche Kopien übernommen.",
  templatesPortable:
    "Vorlagen werden versioniert im lokalen App-Speicher gesichert. Gespeicherte Auswertungen behalten ihren eigenen Stand.",
  templateLimit:
    "Maximal 100 Vorlagen und 100 Schritte pro Vorlage. Namen bis 100 Zeichen, Arbeitsaufträge bis 8000 Zeichen.",
  lastTemplate: "Mindestens eine Vorlage muss erhalten bleiben.",
  editCurrentTemplate: "Aktuelle Vorlage bearbeiten",
  inspectInstructions: "Gesicherte Arbeitsaufträge ansehen",
  snapshotInstructions: "Gesicherter Arbeitsauftrag · schreibgeschützt",
  historicalTemplateHelp:
    "Du siehst eine gespeicherte Auswertung. Ihre Arbeitsaufträge bleiben unverändert. Zum Anpassen eine eigene Vorlage duplizieren oder zur aktuellen Vorlage wechseln.",
  stepKindHelp:
    "Zusammenfassung bündelt Themen. Entscheidungen verlangt ausdrückliche Beschlüsse. To-dos ergänzt Zuständige und Termine nur bei Belegen. Freie Analyse untersucht deine eigene Fragestellung. Die Belegpflicht gilt bei allen Ausgabeformen.",
  tabKeyboardHelp:
    "Tab auswählen: Pfeiltasten, Pos1 oder Ende. Arbeitsauftrag über Bearbeiten oder F2 öffnen.",
  previousSource: "Vorheriger Originalabschnitt",
  nextSource: "Nächster Originalabschnitt",
  sourceSection: "Originalabschnitt",
  sourceSnapshot:
    "Originaltranskriptstand dieser Auswertung. Das hervorgehobene Zitat ist wörtlich belegt; Abschnittszeiten sind keine Wortzeiten.",
  noSystemHelp:
    "Keine Computerausgabe verfügbar. Prüfe das Ausgabegerät und die Audiofreigabe deines Systems; danach Audiogeräte aktualisieren.",
  noMicrophoneHelp:
    "Kein Mikrofon verfügbar. Prüfe Verbindung und Mikrofonfreigabe oder nimm nur den Computerton auf.",
  newTemplate: "Neue Vorlage",
  renameTemplate: "Vorlage umbenennen",
  removeTemplate: "Vorlage entfernen",
  addTab: "Analyseschritt hinzufügen",
  editTab: "Arbeitsauftrag bearbeiten",
  generate: "Auswertung erstellen",
  generating: "Auswertung läuft …",
  edit: "Bearbeiten",
  close: "Schließen",
  cancel: "Abbrechen",
  save: "Speichern",
  tabName: "Name des Tabs",
  instruction: "Arbeitsauftrag",
  instructionHelp:
    "Beschreibe, was dieser Schritt im Originaltranskript untersuchen soll. Jeder Ergebnispunkt benötigt ein vorhandenes Quellenzitat. Gesicherte Auswertungen behalten ihre ursprünglichen Arbeitsaufträge.",
  kind: "Ausgabeform",
  summary: "Zusammenfassung",
  decisions: "Entscheidungen",
  tasks: "To-dos",
  analysis: "Freie Analyse",
  earlier: "Gespeicherte Auswertung",
  currentTemplate: "Aktuelle Vorlage",
  noResults:
    "Noch keine Auswertung. Jeder Tab erhält einen eigenen Arbeitsauftrag mit Belegen aus dem Originaltranskript.",
  copyResults: "Alle Ergebnisse kopieren",
  noItems: "Keine ausdrücklich belegten Einträge.",
  evidence: "Originalstelle",
  unknown: "nicht genannt",
  owner: "Zuständig",
  due: "Termin",
  draft:
    "KI-Entwurf. Wörtliche Belege bestätigen die Quelle, nicht automatisch die inhaltliche Deutung.",
  cloudTitle: "Transkripttext für diese Auswertung freigeben?",
  cloudBody:
    "Der Text der ausgewählten Sitzung und die Arbeitsaufträge werden an diese API-Adresse gesendet. Audio und lokale Dateipfade werden nicht übertragen. Diese Freigabe gilt nur für den folgenden Lauf.",
  approve: "Text senden und auswerten",
  legacy: "Vorhandene Sitzungen importieren",
  legacyHelp:
    "Die neue Version überschreibt keine alten Prototyp-Sitzungen. Import und Überprüfung erfolgen getrennt.",
  templateError:
    "Die gespeicherten Vorlagen sind nicht lesbar. Sie werden nicht überschrieben. Sichere den Browser-Speicher und prüfe die Datei vor einer Wiederherstellung.",
  invalidTemplate:
    "Bitte Namen, Ausgabeform und Arbeitsauftrag vollständig ausfüllen.",
  removeTab: "Tab entfernen",
  moveLeft: "Nach links",
  moveRight: "Nach rechts",
  copyTooltip:
    "Kopiert den vollständigen bisher gespeicherten Text beider Audiospuren, auch außerhalb des sichtbaren Bereichs.",
  startTooltip:
    "Beginnt die lokale Aufnahme der gewählten Computerausgabe und optional des Mikrofons. Die Erkennung läuft getrennt.",
  stopTooltip:
    "Beendet beide Aufnahmen und schließt die Audiodateien ab. Erkennungsrückstand kann weiterverarbeitet werden.",
  modelTooltip:
    "Zeigt ausschließlich installierte, streamingfähige Modelle aus Handy. Neue Modellformate benötigen zusätzlich eine kompatible Erkennungslaufzeit.",
  emptyTitle: "Mein Meeting",
  connectHelp:
    "Diese Ansicht benötigt den Rust-Desktopkern. Bitte Meeting for Handy als Desktop-App starten.",
  busy: "Bitte warten …",
  pending: "Noch nicht vollständig erkannt",
  complete: "Abgeschlossen",
  locallySaved: "Lokal gespeichert",
  resultTemplate: "Arbeitsauftrag dieses Laufs",
  sourceMissing: "Die Originalstelle ist in dieser Auswertung nicht vorhanden.",
  refreshDevices: "Audiogeräte aktualisieren",
} as const;
export const extraDe = {
  resume: "Offene Abschnitte nachholen",
  manifest: "Vorhandene session.json",
  importSession: "Sitzung schreibgeschützt öffnen",
} as const;
export const extraEn: Record<keyof typeof extraDe, string> = {
  resume: "Process pending sections",
  manifest: "Existing session.json",
  importSession: "Open session read-only",
};
export type MessageKey = keyof typeof de;
export const en: Record<MessageKey, string> = {
  audioQuality: "Check the recording",
  audioQualityHelp:
    "Previously saved data is retained. The audio driver usually reports an interruption when the computer was briefly so busy that the app could not collect the audio in time, for example because a local text model takes up almost all memory. Some drivers also report one right at the start of a recording without losing anything. Listen to the named position (older recordings name none; then check the whole track) before relying on the track being complete.",
  technicalDetails: "Show technical messages",
  openFolder: "Open session folder",
  openFolderHelp:
    "Opens the local audio tracks, transcript journals and saved analyses in your file manager.",
  exportTranscript: "Save transcript",
  exportFormat: "Export format",
  exportTranscriptHelp:
    "Saves the complete currently stored text of both tracks as a new text, Markdown or JSON file in exports. Earlier exports are retained; times describe sections.",
  exportSaved: "Transcript saved:",
  savedFilesHelp:
    "Audio and transcript journals are already saved locally. Each export is an additional snapshot.",
  captureSetup: "Recording setup",
  analysisSetup: "Analysis setup",
  connectionSettings: "Connection and access keys",
  setupHelp: "Help with local analysis",
  analysisDetails: "Analysis details",
  meetingDetail: "Session details",
  chooseMeeting: "Meeting to analyse",
  selectMeeting: "Please select a meeting",
  analyzeMeeting: "Analyse this meeting",
  backHistory: "Back to history",
  localServiceHelp:
    "Summaries need an additional local text-model service, such as LM Studio. Load a text model there and start its local server. Use that server's API address; LM Studio's default is shown below. Handy continues to provide speech recognition.",
  localEndpoint: "Local text-model service address",
  refreshTextModels: "Refresh text models",
  loadingTextModels: "Reading available text models …",
  selectTextModel: "Select a local text model",
  noTextModels: "The local service does not offer a text model yet.",
  textModelsHelp:
    "Check that the local server in LM Studio or your other text-model service is running and has a model available. Then refresh text models.",
  manualTextModel: "Enter the model name manually",
  textModelUnavailable:
    "This service does not offer the previous model selection. Explicitly choose an available model or confirm its name manually.",
  notesTask: "Analysis",
  notesNotStarted: "Analysis has not started yet.",
  notesCheckingStatus: "Analysis is running. Checking its current progress …",
  startAnalysis: "Start analysis",
  analysisReady:
    "The template and text model are ready. Click Start analysis to begin processing.",
  analysisChooseModel:
    "Choose a text model and check the local model service. You can then start the analysis.",
  analysisFailureHelp:
    "Check the connection and selected text model. Open the associated meeting to retry or resume saved steps.",
  cancelAnalysis: "Cancel analysis",
  cancelRequested: "Cancellation requested …",
  cancelHelp:
    "Saved steps are preserved. The text-model service may still finish calculations it has already started.",
  notesCancelled: "Analysis cancelled. Saved steps are preserved.",
  notesComplete: "Analysis saved.",
  notesFailed: "Analysis failed. Previously saved steps are preserved.",
  cancelUnavailable:
    "Cancellation was not confirmed. The current status is being checked.",
  handyVersionMissing:
    "Handy detected; this installation does not report a version",
  firstModelLoad:
    "The list contains model identifiers reported by the service. Choose a model for text replies. Some services load it on the first request, so the first analysis can take longer.",
  elapsed: "Elapsed time",
  confirmedTasks: "validated and saved subtasks",
  inferenceUnknown:
    "The model's remaining computation time is unknown. Progress counts only durably saved subtasks.",
  stagePreparing: "Preparing transcript and instructions",
  stageRequesting: "The text model is processing the current section",
  stageValidating: "Checking original quotes and sources",
  stageSaving: "Saving the local checkpoint",
  progressUnavailable: "Progress is temporarily unavailable.",
  openAnalysisMeeting: "Open the associated meeting",
  resumeAnalysis: "Resume saved analysis",
  unfinishedAnalyses: "Unfinished analyses",
  skippedAnalyses:
    "Some saved analyses are not shown because they fail verification, belong to another meeting or use an unknown storage format. The files remain unchanged in the meeting folder:",
  earlierPrompt: "Earlier prompt version",
  readOnly: "read-only",
  earlierPromptPending:
    "This work state is kept but will not be continued. Start a new analysis if needed.",
  earlierPromptResult:
    "This analysis is read-only. It is neither continued nor mixed with new results.",
  rejectedItems:
    "AI statements were discarded because their evidence or volume failed verification. They are not shown.",
  resumeAnalysisHelp:
    "This run keeps its original sources, instructions, API address and model. Already confirmed subtasks are reused. Credentials and cloud approval are not restored from the saved session.",
  resumeHelp:
    "Uses this run's original transcript snapshot, template and model.",
  copyMarkdown: "Copy all results as Markdown",
  markdownHelp:
    "Includes every tab, verbatim evidence and original-transcript references. Active HTML content is escaped.",
  wrongMeeting:
    "This analysis belongs to another session and will not be used here.",
  pendingNotes:
    "First process pending transcript sections or choose a fully transcribed meeting.",
  dismiss: "Dismiss notice",
  preferencesError:
    "Saved preferences cannot be read or written. Existing values are preserved.",
  meeting: "Meeting",
  history: "History",
  historyHelp:
    "Find saved meetings, open transcripts and continue working with their results.",
  notes: "Post-processing",
  settings: "Settings",
  companion: "Needs the speech models from Handy but runs as its own app",
  refresh: "Refresh",
  connecting: "Connecting …",
  disconnected: "Desktop connection unavailable",
  retry: "Try again",
  title: "Meeting title",
  model: "Streaming model from Handy",
  system: "System audio",
  microphone: "Also record microphone",
  noDevices: "No matching audio devices found",
  noModels: "No compatible installed streaming model found",
  modelUnavailable:
    "The previously selected model is no longer installed or compatible. Please explicitly choose another streaming model.",
  choose: "Please select",
  start: "Start recording",
  stop: "End meeting",
  ready: "Ready to record",
  recording: "Audio recording is running",
  stopping: "Stopping recording …",
  stoppingDetail:
    "Ending the meeting. Saved audio is preserved; pending sections are being processed …",
  live: "Transcript",
  copy: "Copy transcript",
  copied: "Copied",
  emptyTranscript:
    "Text appears here when local recognition produces transcript sections.",
  emptyHistory:
    "No saved meetings yet. Start a recording. Older prototype sessions can be imported in Settings.",
  open: "Open",
  duration: "Duration",
  sourceTimes: "Times identify recording sections, not exact word timestamps.",
  saved: "Saved audio",
  lag: "Recognition backlog",
  warnings: "Session notices",
  sourceSystem: "System audio",
  sourceMicrophone: "Microphone",
  preserve: "Audio is saved locally, independently of recognition.",
  installation: "Handy installation",
  executable: "Handy executable (optional)",
  executableHelp:
    "Handy is detected automatically. Choose another installation through the file dialog. Model lists come from the installed Handy.",
  version: "Version",
  notFound: "Not detected yet",
  compatibility:
    "New Handy models appear after refreshing the list. Streaming support and a compatible recognition runtime are checked separately.",
  language: "Language",
  theme: "Appearance",
  light: "Light",
  dark: "Dark",
  systemTheme: "System",
  storage: "Sessions",
  noMeeting: "First open a session containing a transcript.",
  processing: "Processing",
  local: "Local (default)",
  cloud: "Cloud · text only",
  endpoint: "API address",
  textModel: "Text model",
  apiKey: "API key (optional)",
  apiKeyHelp:
    "Keys are not stored in the browser. Cloud processing receives only approved text, never audio files.",
  template: "Template",
  templateName: "Template name",
  sessionKey: "Key for this app session only",
  credentialStored:
    "A key for this address is stored in the protected operating-system store.",
  credentialMissing: "No key is stored for this address.",
  credentialUnavailable:
    "The protected key store is unavailable. Existing entries will not be replaced.",
  useSavedKey: "Use the stored key for this address",
  replaceCredential: "Explicitly replace the existing readable key",
  confirmRemoveCredential: "I want to remove the stored key for this address",
  removeCredential: "Remove stored key",
  saveCredential: "Save key in the operating system",
  credentialHelp:
    "Only these buttons save or remove keys. A stored key does not grant cloud permission; transcript text still requires confirmation for every run.",
  agentControl: "Local agent access",
  agentConnected: "Connection active",
  agentDisconnected: "Connection ended",
  agentScope:
    "Approval applies only to the requested recording and its live transcript. It does not permit cloud transmission or provider changes. Recording ends at the time limit or when the connection closes.",
  agentOwnedSession: "Agent-owned session",
  agentExpires: "ends no later than",
  agentDuration: "Maximum duration",
  agentWithMicrophone: "System audio and microphone requested",
  agentSystemOnly: "System audio only requested",
  agentMicrophoneConsent: "Explicitly allow my microphone for this request",
  agentChooseDevices:
    "First choose an available streaming model and system output under Meeting.",
  agentChooseMicrophone: "First select an available microphone under Meeting.",
  agentBusy:
    "A recording or its completion is already running. The new request can be approved afterwards.",
  agentDeny: "Deny request",
  agentApprove: "Allow this recording",
  agentRevoke: "Revoke agent access",
  agentRevoked: "Access revoked. A manually started recording is unaffected.",
  duplicateTemplate: "Duplicate template",
  duplicateName: "Copy",
  newTemplateHelp:
    "Creates a new template with the three default tasks. You can then add, remove and change steps.",
  duplicateTemplateHelp:
    "Creates an independent copy of every instruction. Saved analyses remain unchanged.",
  renameTemplateHelp: "Changes only the template name for future analyses.",
  deleteTemplateHelp:
    "Remove this template from the selection? Saved analyses and their original instructions are preserved.",
  duplicateTemplateName:
    "This template name is already used. Please choose a different name.",
  templateSaveFailed:
    "The template could not be saved. Your input remains open. After a conflict, reload the library and apply the change again.",
  templateLoadFailed:
    "The template library could not be loaded. Please retry; existing data will not be replaced.",
  refreshTemplates: "Reload templates",
  importBrowserTemplates: "Import previous browser templates",
  browserTemplatesHelp:
    "Copies the old local browser templates into app storage. Existing templates and analyses are preserved; conflicts are imported as additional copies.",
  templatesPortable:
    "Templates are versioned in local app storage. Saved analyses keep their own snapshot.",
  templateLimit:
    "Up to 100 templates and 100 steps per template. Names may contain 100 characters and instructions 8000 characters.",
  lastTemplate: "At least one template must remain.",
  editCurrentTemplate: "Edit current template",
  inspectInstructions: "View saved instructions",
  snapshotInstructions: "Saved instruction · read only",
  historicalTemplateHelp:
    "This is a saved analysis. Its instructions remain unchanged. Duplicate it as your own template or return to the current template to make changes.",
  stepKindHelp:
    "Summary groups topics. Decisions requires explicit decisions. To-dos adds owners and dates only with evidence. Custom analysis explores your own question. Every output type requires source evidence.",
  tabKeyboardHelp:
    "Select tabs with arrow keys, Home or End. Open instructions with Edit or F2.",
  previousSource: "Previous original section",
  nextSource: "Next original section",
  sourceSection: "Original section",
  sourceSnapshot:
    "Original transcript snapshot for this analysis. Highlighted quotes are verbatim evidence; section times are not word timestamps.",
  noSystemHelp:
    "No system output is available. Check your output device and system audio permission, then refresh the audio devices.",
  noMicrophoneHelp:
    "No microphone is available. Check its connection and microphone permission, or record system audio only.",
  newTemplate: "New template",
  renameTemplate: "Rename template",
  removeTemplate: "Remove template",
  addTab: "Add analysis step",
  editTab: "Edit instructions",
  generate: "Generate analysis",
  generating: "Analysing …",
  edit: "Edit",
  close: "Close",
  cancel: "Cancel",
  save: "Save",
  tabName: "Tab name",
  instruction: "Instructions",
  instructionHelp:
    "Describe what this step should analyse in the original transcript. Each result needs a verbatim source quote. Saved analyses keep their original instructions.",
  kind: "Output type",
  summary: "Summary",
  decisions: "Decisions",
  tasks: "Tasks",
  analysis: "Custom analysis",
  earlier: "Saved analysis",
  currentTemplate: "Current template",
  noResults:
    "No analysis yet. Each tab has its own instructions and references to the original transcript.",
  copyResults: "Copy all results",
  noItems: "No explicitly evidenced items.",
  evidence: "Original source",
  unknown: "not specified",
  owner: "Owner",
  due: "Due",
  draft:
    "AI draft. Verbatim evidence confirms the source, not necessarily the interpretation.",
  cloudTitle: "Approve transcript text for this analysis?",
  cloudBody:
    "The selected session's text and analysis instructions will be sent to this API address. Audio and local file paths are excluded. Approval applies only to the following run.",
  approve: "Send text and analyse",
  legacy: "Import existing sessions",
  legacyHelp:
    "The new version does not overwrite old prototype sessions. Import and validation are separate steps.",
  templateError:
    "Stored templates cannot be read. They will not be overwritten. Preserve the browser storage before attempting recovery.",
  invalidTemplate: "Please complete the name, output type and instructions.",
  removeTab: "Remove tab",
  moveLeft: "Move left",
  moveRight: "Move right",
  copyTooltip:
    "Copies the full saved transcript from both tracks, including sections outside the visible area.",
  startTooltip:
    "Starts local recording of the chosen system output and optional microphone. Recognition runs independently.",
  stopTooltip:
    "Stops both recordings and closes their audio files. Recognition backlog can be processed afterward.",
  modelTooltip:
    "Shows installed streaming models from Handy. New model formats also require a compatible recognition runtime.",
  emptyTitle: "My meeting",
  connectHelp:
    "This view requires the Rust desktop core. Start Meeting for Handy as a desktop application.",
  busy: "Please wait …",
  pending: "Recognition incomplete",
  complete: "Complete",
  locallySaved: "Saved locally",
  resultTemplate: "Instructions for this run",
  sourceMissing: "The original source is unavailable in this analysis.",
  refreshDevices: "Refresh audio devices",
};
