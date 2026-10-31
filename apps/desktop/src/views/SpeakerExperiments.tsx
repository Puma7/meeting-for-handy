import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { RefreshCw, Square } from "lucide-react";
import { errorMessage } from "../api";
import { clock } from "../domain";
import { startAdaptivePoll } from "../jobPolling";
import type {
  DesktopApi,
  Locale,
  Meeting,
  HandyModel,
  SpeakerProgress,
  SavedSpeakerSections,
} from "../types";
/**
 * Checks a selected model before enabling optional, meeting-bound speaker experiments.
 * Setup-only mode needs no recording and does not query meeting jobs or saved sections.
 */
export default function SpeakerExperiments({
  api,
  locale,
  meeting,
  models = [],
  recording = false,
  footer = false,
  embedded = false,
  checkedModels = {},
  onModelChecked,
  onCheckingChange,
  setupOnly = false,
  selectedModelId,
  onSelectedModelChange,
  checkingBlocked = false,
  statusContainer,
}: {
  api: DesktopApi;
  locale: Locale;
  meeting?: Meeting | null;
  models?: HandyModel[];
  recording?: boolean;
  footer?: boolean;
  embedded?: boolean;
  checkedModels?: Record<string, boolean>;
  onModelChecked?: (modelId: string, supported: boolean) => void;
  onCheckingChange?: (checking: boolean) => void;
  setupOnly?: boolean;
  selectedModelId?: string;
  onSelectedModelChange?: (id: string) => void;
  checkingBlocked?: boolean;
  /** Setup errors remain visible when the optional settings disclosure is closed. */
  statusContainer?: HTMLElement | null;
}) {
  const de = locale === "de";
  const [localModel, setLocalModel] = useState(""),
    [job, setJob] = useState<SpeakerProgress | null>(null),
    [saved, setSaved] = useState<SavedSpeakerSections | null>(null),
    [actionError, setError] = useState<string | null>(null),
    // A transient polling failure clears on the next successful poll; action errors stay.
    [pollError, setPollError] = useState<string | null>(null),
    [busy, setBusy] = useState(false),
    [hidden, setHidden] = useState<string | null>(null),
    [archiveNotice, setArchiveNotice] = useState<string | null>(null),
    [archiveNeeded, setArchiveNeeded] = useState(false);
  const archivePrefix = "[SPEAKER_CHECK_ARCHIVE_REQUIRED]";
  const error = (actionError ?? pollError)?.replace(archivePrefix, "").trim();
  const model = selectedModelId ?? localModel;
  const setModel = onSelectedModelChange ?? setLocalModel;
  useEffect(() => {
    if (setupOnly || !api.getSpeakerProgress) return;
    let active = true,
      running = false;
    const stop = startAdaptivePoll(async () => {
      try {
        const next = await api.getSpeakerProgress!();
        running = next?.status === "running";
        if (active) {
          setJob(next);
          setPollError(null);
        }
      } catch (e) {
        if (active) setPollError(errorMessage(e));
      }
      return running;
    });
    return () => {
      active = false;
      stop();
    };
  }, [api, setupOnly]);
  useEffect(() => {
    if (setupOnly || footer || !meeting || !api.getSpeakerSections) return;
    let active = true;
    setSaved(null);
    api
      .getSpeakerSections(meeting.id)
      .then((value) => {
        if (active) setSaved(value);
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [api, meeting?.id, footer, setupOnly, job?.status]);
  if (!api.startSpeakerExperiment) return null;
  const running = job?.status === "running";
  const label = de ? "Sprecherexperiment" : "Speaker experiment";
  const supported = checkedModels[model] === true;
  /** Runs one native capability check under the shared recording/analysis admission gates. */
  async function checkModel() {
    if (
      !model ||
      !api.checkSpeakerModel ||
      busy ||
      recording ||
      checkingBlocked
    )
      return;
    setBusy(true);
    onCheckingChange?.(true);
    setError(null);
    setArchiveNotice(null);
    try {
      const found = await api.checkSpeakerModel(model);
      setArchiveNeeded(false);
      onModelChecked?.(model, found);
      if (!found)
        setError(
          de
            ? "Dieses Modell unterstützt keine lokale Sprechertrennung. Wähle ein Modell mit Diarisierungsfunktion. Manuelle Zuordnungen bleiben verfügbar."
            : "This model does not support local speaker separation. Choose a model with diarization support. Manual labels remain available.",
        );
    } catch (error) {
      const message = errorMessage(error);
      if (message.startsWith(archivePrefix)) setArchiveNeeded(true);
      setError(message);
    } finally {
      setBusy(false);
      onCheckingChange?.(false);
    }
  }
  /** Archives generated proofs only after the user asks; every record is retained and
   * native admission prevents overlap with capture, analysis or another model check. */
  async function archiveChecks() {
    if (
      !api.archiveSpeakerModelChecks ||
      busy ||
      running ||
      recording ||
      checkingBlocked
    )
      return;
    setBusy(true);
    onCheckingChange?.(true);
    try {
      const moved = await api.archiveSpeakerModelChecks();
      setArchiveNeeded(false);
      setError(null);
      setArchiveNotice(
        moved
          ? de
            ? "Prüfnachweise archiviert und erhalten. Bitte die Modellprüfung erneut starten."
            : "Check records archived and retained. Run the model check again."
          : de
            ? "Keine Prüfnachweise zu archivieren. Bitte die Modellprüfung erneut starten."
            : "No check records to archive. Run the model check again.",
      );
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
      onCheckingChange?.(false);
    }
  }
  /** Starts one checked-model job for the current meeting and refreshes its owned progress. */
  async function start(mode: "live" | "post-meeting") {
    if (!meeting || !model || !supported || busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.startSpeakerExperiment!(meeting.id, model, mode);
      setHidden(null);
      if (api.getSpeakerProgress) setJob(await api.getSpeakerProgress());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const status = job && (
    <>
      <div className="notes-statusbar-row">
        <strong>
          {running && <RefreshCw size={16} className="spinning" />}
          {label} ·{" "}
          {job.mode === "live"
            ? de
              ? "Live · experimentell"
              : "Live · experimental"
            : de
              ? "Audio nach dem Meeting · Beta"
              : "Post-meeting audio · beta"}{" "}
          ·{" "}
          {running
            ? job.stage === "waiting"
              ? de
                ? "wartet auf geschlossene Abschnitte"
                : "waiting for closed sections"
              : job.stage === "loading"
                ? de
                  ? "Modell wird geprüft und geladen"
                  : "checking and loading model"
                : de
                  ? "arbeitet"
                  : "working"
            : job.status === "complete"
              ? de
                ? "abgeschlossen"
                : "complete"
              : job.status === "cancelled"
                ? de
                  ? "abgebrochen"
                  : "cancelled"
                : de
                  ? "fehlgeschlagen"
                  : "failed"}
        </strong>
        <span>{clock(job.elapsedMs)}</span>
      </div>
      {running && (
        <progress
          aria-label={label}
          max={job.totalTasks || undefined}
          value={job.totalTasks ? job.confirmedTasks : undefined}
        />
      )}
      <p>
        {job.confirmedTasks}
        {job.totalTasks ? ` / ${job.totalTasks}` : ""}{" "}
        {de
          ? "bestätigt gespeicherte Audioabschnitte"
          : "confirmed saved audio sections"}
        {job.totalTasks
          ? ` · ${Math.floor((job.confirmedTasks / job.totalTasks) * 100)}%`
          : ""}
      </p>
      {(error || job.error) && <p role="alert">{error ?? job.error}</p>}
      {running && (
        <button
          className="secondary"
          disabled={job.cancelRequested}
          onClick={() =>
            void api
              .cancelSpeakerExperiment?.(job.jobId)
              .catch((e) => setError(errorMessage(e)))
          }
        >
          <Square size={14} />
          {de
            ? "Nur Sprecherexperiment abbrechen"
            : "Cancel only the speaker experiment"}
        </button>
      )}
    </>
  );
  if (footer) {
    if (!job || job.jobId === hidden) return null;
    return (
      <section
        className="notes-progress notes-statusbar"
        role="status"
        aria-label={label}
      >
        {status}
        {!running && (
          <button onClick={() => setHidden(job.jobId)}>
            {de ? "Hinweis schließen" : "Dismiss"}
          </button>
        )}
      </section>
    );
  }
  if (!setupOnly && !meeting) return null;
  const checkFeedback = (
    <>
      {setupOnly && error && <p role="alert">{error}</p>}
      {archiveNeeded && api.archiveSpeakerModelChecks && (
        <div>
          <p className="muted">
            {de
              ? "Die bisherigen Prüfnachweise benötigen Aufmerksamkeit. Du kannst sie zusammen archivieren und eine neue Prüfung beginnen. Alle Dateien bleiben erhalten; Aufnahmen und Auswertungen werden nicht verändert."
              : "The existing check records need attention. You can archive them together and start a new check. Every file is retained; recordings and analyses are unchanged."}
          </p>
          <button
            className="secondary"
            disabled={busy || running || recording || checkingBlocked}
            onClick={() => void archiveChecks()}
          >
            {de ? "Prüfnachweise archivieren" : "Archive check records"}
          </button>
        </div>
      )}
      {archiveNotice && <p role="status">{archiveNotice}</p>}
    </>
  );
  const Container = embedded ? "section" : "details";
  return (
    <Container className="speaker-options">
      {embedded ? (
        <h3>
          {de ? "Stimmen im Audio unterscheiden" : "Separate voices in audio"}
        </h3>
      ) : (
        <summary>
          {de
            ? "Zusätzliche lokale Sprecherfunktionen · Beta / experimentell"
            : "Additional local speaker features · beta / experimental"}
        </summary>
      )}
      <p className="muted">
        {de
          ? "Ein Modell muss zuerst seine Sprecherunterstützung nachweisen. Die einmalige CPU-Prüfung erfolgt vor der Aufnahme; es wird kein Audio geöffnet oder Modell heruntergeladen."
          : "A model must first pass its speaker capability check. This one-time CPU check runs before recording; no audio is opened and no model is downloaded."}
      </p>
      <label>
        {de
          ? "Installiertes Modell für die Sprecherprüfung"
          : "Installed model for speaker capability check"}
        <select
          value={model}
          disabled={busy || running || checkingBlocked}
          onChange={(e) => setModel(e.target.value)}
        >
          <option value="">
            {de ? "Installiertes Modell wählen" : "Choose an installed model"}
          </option>
          {models
            .filter((m) => m.isDownloaded)
            .map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
                {checkedModels[m.id] === true
                  ? " ✓"
                  : checkedModels[m.id] === false
                    ? de
                      ? " · nicht unterstützt"
                      : " · unsupported"
                    : ""}
              </option>
            ))}
        </select>
      </label>
      <button
        className="secondary"
        disabled={
          busy ||
          running ||
          recording ||
          checkingBlocked ||
          !model ||
          !api.checkSpeakerModel
        }
        onClick={() => void checkModel()}
      >
        {busy ? <RefreshCw size={14} className="spinning" /> : null}
        {de ? "Sprecherunterstützung prüfen" : "Check speaker support"}
      </button>
      {supported && (
        <p role="status">
          {de
            ? "Sprecherunterstützung geprüft. Die Auswertung bleibt ein optionaler Entwurf."
            : "Speaker support checked. Analysis remains an optional draft."}
        </p>
      )}
      {setupOnly && statusContainer
        ? createPortal(checkFeedback, statusContainer)
        : checkFeedback}
      {!setupOnly && meeting && (
        <>
          <div>
            <h3>
              {de ? "Nach dem Meeting · Beta" : "After the meeting · beta"}
            </h3>
            <p>
              {de
                ? "Prüft abgeschlossene Abschnitte auf unterschiedliche Stimmen. Originaltexte werden nicht ersetzt."
                : "Checks closed audio sections for different voices. Original transcripts are retained."}
            </p>
            <button
              className="secondary"
              disabled={
                busy ||
                running ||
                recording ||
                meeting.status !== "complete" ||
                !supported
              }
              onClick={() => void start("post-meeting")}
            >
              {de
                ? "Lokale Sprechertrennung prüfen"
                : "Run local speaker separation"}
            </button>
          </div>
          <div>
            <h3>
              {de
                ? "Während der Aufnahme · experimentell"
                : "During recording · experimental"}
            </h3>
            <p>
              {de
                ? "Verarbeitet neue geschlossene Audioabschnitte im Hintergrund, mit entsprechender Verzögerung. Kein Eingriff in die Audiosicherung."
                : "Processes newly closed audio sections in the background, with a corresponding delay. Audio saving remains independent."}
            </p>
            <button
              className="secondary"
              disabled={busy || running || !recording || !supported}
              onClick={() => void start("live")}
            >
              {de
                ? "Live-Stimmenabschnitte prüfen"
                : "Check live voice sections"}
            </button>
          </div>
          <p className="notice warning">
            {de
              ? "Stimme 1 usw. sind neutrale Modell-Entwürfe pro Abschnitt und Spur, keine Personennamen. Nummern können zwischen Abschnitten wechseln. Kein automatischer Abgleich mit Namensnennungen und keine behaupteten Wortzeiten."
              : "Voice 1 etc. are neutral model drafts per section and track, not personal identities. Numbers can change between sections. They are not automatically matched to names, and no word timing is claimed."}
          </p>
          {error && <p role="alert">{error}</p>}
          {job?.meetingId === meeting.id && status}
          {job?.meetingId === meeting.id && running && job.lastSection && (
            <div>
              <h3>
                {de
                  ? "Letzter bestätigter Stimmenabschnitt"
                  : "Latest confirmed voice section"}
              </h3>
              {job.lastSection.turns.length ? (
                <ul>
                  {job.lastSection.turns.slice(0, 100).map((turn, i) => (
                    <li key={i}>
                      {clock(job.lastSection!.startMs + turn.startMs)}–
                      {clock(job.lastSection!.startMs + turn.endMs)} ·{" "}
                      {de ? "Stimme" : "Voice"} {turn.speakerId}{" "}
                      {de ? "(nur dieser Abschnitt)" : "(this section only)"}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>
                  {de
                    ? "Keine verwertbaren Stimmenzeiten in diesem Abschnitt."
                    : "No usable timed voice turns in this section."}
                </p>
              )}
            </div>
          )}
          {saved && (
            <div>
              <h3>
                {de
                  ? "Letzte gespeicherte Fassung · letzte 25 Abschnitte"
                  : "Latest saved revision · last 25 sections"}
              </h3>
              <p>{saved.modelId}</p>
              {saved.sections.map((section) => (
                <article key={section.sourceId}>
                  <strong>
                    {de ? "Originalabschnitt" : "Original section"}{" "}
                    {clock(section.startMs)}–
                    {clock(section.startMs + section.durationMs)}
                  </strong>
                  {section.turns.length ? (
                    <ul>
                      {section.turns.slice(0, 100).map((turn, i) => (
                        <li key={i}>
                          {clock(section.startMs + turn.startMs)}–
                          {clock(section.startMs + turn.endMs)} ·{" "}
                          {de ? "Stimme" : "Voice"} {turn.speakerId}{" "}
                          {de
                            ? "(nur dieser Abschnitt)"
                            : "(this section only)"}
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p>
                      {de
                        ? "Keine verwertbaren zeitlichen Stimmenabschnitte vom Modell geliefert."
                        : "The model returned no usable timed voice sections."}
                    </p>
                  )}
                </article>
              ))}
            </div>
          )}
        </>
      )}
    </Container>
  );
}
