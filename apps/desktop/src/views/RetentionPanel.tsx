import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { Archive } from "lucide-react";
import { createPortal } from "react-dom";
import { errorMessage } from "../api";
import AudioCopiesPanel from "./AudioCopiesPanel";
import ArchivePanel from "./ArchivePanel";
import MaintenanceStatus from "./MaintenanceStatus";
import type {
  DesktopApi,
  Locale,
  RetentionPolicy,
  RetentionPlan,
  LibraryProgress,
  HandyModel,
} from "../types";
type Rule =
  | "audioArchiveAfterDays"
  | "archiveAfterDays"
  | "convertAfterDays"
  | "maxVisibleMeetings"
  | "deleteArchivesAfterDays";
/**
 * Session-only preview consent shared across settings views. Retaining these IDs
 * through navigation does not persist program paths or confirmation text.
 */
interface PreviewValidity {
  invalidated: Set<string>;
  observed: Set<string>;
  requireExplicitPreview: boolean;
  allowedPreviewId: string | null;
  generation: number;
  listeners: Set<() => void>;
}
const previewValidityByApi = new WeakMap<DesktopApi, PreviewValidity>();
/**
 * Returns the API-session preview-consent state shared across settings remounts; no paths
 * or confirmation text are persisted.
 */
function previewValidity(api: DesktopApi): PreviewValidity {
  let validity = previewValidityByApi.get(api);
  if (!validity) {
    validity = {
      invalidated: new Set<string>(),
      observed: new Set<string>(),
      requireExplicitPreview: false,
      allowedPreviewId: null,
      generation: 0,
      listeners: new Set<() => void>(),
    };
    previewValidityByApi.set(api, validity);
  }
  return validity;
}
/**
 * Checks session-local consent for a preview ID. Backend plan, file and fingerprint
 * validation is still required before maintenance can change data.
 */
function previewIsValid(validity: PreviewValidity, id: string): boolean {
  return (
    !validity.invalidated.has(id) &&
    (!validity.requireExplicitPreview || validity.allowedPreviewId === id)
  );
}
/**
 * Edits revisioned local policy while keeping saving, preview and single-action consent
 * separate. Native grant changes revoke old previews across mounted consumers.
 */
export default function RetentionPanel({
  api,
  locale,
  models = [],
  statusContainer,
}: {
  api: DesktopApi;
  locale: Locale;
  models?: HandyModel[];
  /** Shows the same single job observer outside an ancestor's collapsed settings. */
  statusContainer?: HTMLElement | null;
}) {
  const de = locale === "de";
  const latestJob = useRef<string | null>(null);
  const previewSession = useMemo(() => previewValidity(api), [api]);
  /**
   * Registers this view for shared preview invalidations and returns the unmount cleanup
   * used by the external-store hook.
   */
  const subscribe = useCallback(
    (listener: () => void) => {
      previewSession.listeners.add(listener);
      return () => {
        previewSession.listeners.delete(listener);
      };
    },
    [previewSession],
  );
  const previewGeneration = useSyncExternalStore(
    subscribe,
    () => previewSession.generation,
  );
  // A remounted status bar reports the last finished preview; only a current one is shown.
  const policyRevision = useRef<number | null>(null);
  const [policy, setPolicy] = useState<RetentionPolicy | null>(null),
    [draft, setDraft] = useState<RetentionPolicy | null>(null),
    [storedPlan, setPlan] = useState<RetentionPlan | null>(null),
    [job, setJob] = useState<LibraryProgress | null>(null),
    [error, setError] = useState<string | null>(null),
    [busy, setBusy] = useState(false),
    [page, setPage] = useState(0),
    [selected, setSelected] = useState<number | null>(null),
    [confirmation, setConfirmation] = useState(""),
    [accepted, setAccepted] = useState(false);
  useEffect(() => {
    latestJob.current = null;
    setPlan(null);
    setSelected(null);
    setAccepted(false);
    setConfirmation("");
  }, [previewSession, previewGeneration]);
  useEffect(() => {
    if (!api.getRetentionPolicy) return;
    let active = true;
    api
      .getRetentionPolicy()
      .then((value) => {
        if (active) {
          setPolicy(value);
          setDraft(value);
        }
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [api]);
  useEffect(() => {
    policyRevision.current = policy?.revision ?? null;
  }, [policy]);
  /**
   * Loads only current, unrevoked preview receipts with the saved policy revision and a
   * valid 24-hour age. Stale successes or failures cannot restore previous consent.
   */
  const progress = useCallback(
    (value: LibraryProgress) => {
      setJob(value);
      if (value.kind === "cleanup-preview") {
        previewSession.observed.add(value.jobId);
        if (!previewIsValid(previewSession, value.jobId)) return;
      }
      latestJob.current = value.jobId;
      if (value.kind === "cleanup-preview" && value.status === "complete")
        void api
          .getRetentionPlan?.(value.jobId)
          .then((p) => {
            if (
              !previewIsValid(previewSession, value.jobId) ||
              latestJob.current !== value.jobId ||
              p.id !== value.jobId
            )
              return;
            const age = Date.now() - Date.parse(p.createdAt);
            if (
              p.policyRevision !== policyRevision.current ||
              !(age >= 0 && age <= 24 * 60 * 60 * 1000)
            )
              return;
            setSelected(null);
            setAccepted(false);
            setConfirmation("");
            setPlan(p);
            setPage(0);
          })
          .catch((e) => {
            if (
              !previewIsValid(previewSession, value.jobId) ||
              latestJob.current !== value.jobId
            )
              return;
            setError(errorMessage(e));
          });
    },
    [api, previewSession],
  );
  if (!api.getRetentionPolicy) return null;
  const plan =
    storedPlan && previewIsValid(previewSession, storedPlan.id)
      ? storedPlan
      : null;
  const candidate = selected === null ? null : plan?.candidates[selected];
  const deleting = candidate?.action === "delete-archived-meeting";
  const expectedDelete = candidate ? `DELETE ${candidate.meetingId}` : "";
  const running = job?.status === "running";
  const changed = JSON.stringify(draft) !== JSON.stringify(policy);
  const actionNames: Record<string, string> = de
    ? {
        "archive-audio": "Nur Audio verlustfrei archivieren",
        "archive-meeting": "Ganzes Meeting archivieren",
        "convert-audio": "Zusätzliche Audiokopie umwandeln",
        "delete-archived-meeting":
          "Archiv endgültig löschen · extra Bestätigung nötig",
      }
    : {
        "archive-audio": "Archive audio only, without loss",
        "archive-meeting": "Archive the whole meeting",
        "convert-audio": "Convert an additional audio copy",
        "delete-archived-meeting":
          "Permanently delete archive · extra confirmation required",
      };
  /**
   * Saves the draft against the current policy revision without starting a preview or
   * applying any candidate action.
   */
  async function save() {
    if (!policy || !draft || !api.saveRetentionPolicy) return;
    setBusy(true);
    setSelected(null);
    setAccepted(false);
    setConfirmation("");
    setError(null);
    try {
      const next = await api.saveRetentionPolicy(policy.revision, draft);
      setPolicy(next);
      setDraft(next);
      setPlan(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  /**
   * Requests a new read-only plan and, after native grant changes, accepts only a new job
   * ID started in the current invalidation generation.
   */
  async function preview() {
    if (!api.previewRetention) return;
    setBusy(true);
    setSelected(null);
    setAccepted(false);
    setConfirmation("");
    setError(null);
    const generation = previewSession.generation;
    const knownIds = new Set(previewSession.observed);
    try {
      const id = await api.previewRetention();
      if (previewSession.requireExplicitPreview) {
        if (
          generation !== previewSession.generation ||
          knownIds.has(id) ||
          previewSession.invalidated.has(id)
        ) {
          previewSession.invalidated.add(id);
          return;
        }
        previewSession.allowedPreviewId = id;
      }
      if (api.getMaintenanceProgress) {
        const value = await api.getMaintenanceProgress();
        if (value) progress(value);
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  /**
   * Requests a native-approved FFmpeg selection. Cancellation leaves the draft/consent
   * unchanged; every approval, including the same path, revokes old previews.
   */
  async function chooseFfmpeg() {
    if (!api.chooseFfmpegExecutable || !draft || busy || running) return;
    setBusy(true);
    setError(null);
    try {
      const selected = await api.chooseFfmpegExecutable();
      if (selected) {
        // A confirmed native choice may approve new bytes even at the same path.
        // An earlier preview and its consent cannot authorize this selection.
        invalidatePreview();
        setDraft({ ...draft, ffmpegExecutable: selected });
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  /**
   * Clears only the draft program selection and preview consent. The program and
   * recordings remain intact, and saving policy is a separate action.
   */
  function clearFfmpeg() {
    if (!draft || busy || running) return;
    setDraft({ ...draft, ffmpegExecutable: null });
    setError(null);
    invalidatePreview();
  }
  /**
   * Revokes all previous preview consent for this API session, requires a newly requested
   * plan and notifies every mounted consumer, including after an originating picker view
   * unmounts.
   */
  function invalidatePreview() {
    // Keep rejected job IDs blocked when an old completion is reported again;
    // only a new preview job may establish a new plan and confirmation.
    for (const id of [plan?.id, latestJob.current])
      if (id) previewSession.observed.add(id);
    for (const id of previewSession.observed)
      previewSession.invalidated.add(id);
    // A chooser can finish after its original panel unmounts, before that panel
    // ever learned a preview ID. Require a newly requested ID in every consumer.
    previewSession.requireExplicitPreview = true;
    previewSession.allowedPreviewId = null;
    previewSession.generation++;
    for (const listener of previewSession.listeners) listener();
    latestJob.current = null;
    setPlan(null);
    setSelected(null);
    setAccepted(false);
    setConfirmation("");
  }
  /**
   * Submits the selected still-valid plan row with its separate confirmation. The native
   * service revalidates the immutable receipt before applying a change.
   */
  async function apply() {
    if (!plan || selected === null || !candidate || !api.applyRetention) return;
    // An already queued click may run before the invalidation render commits.
    if (!previewIsValid(previewSession, plan.id)) return;
    setBusy(true);
    setError(null);
    try {
      await api.applyRetention(
        plan.id,
        selected,
        deleting ? confirmation : `APPLY ${plan.id} ${selected}`,
      );
      setPlan(null);
      setSelected(null);
      setConfirmation("");
      setAccepted(false);
      if (api.getMaintenanceProgress) {
        const value = await api.getMaintenanceProgress();
        if (value) progress(value);
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  /**
   * Renders a nullable opt-in rule with its numeric input bounds, editing only the policy
   * draft.
   */
  // Hover explanations for each rule; the visible label stays short.
  const ruleHelp: Record<Rule, string> = de
    ? {
        convertAfterDays:
          "Die Aufräumvorschau schlägt nach dieser Zahl von Tagen eine zusätzliche, kleinere Audiokopie (FLAC oder Opus) vor. Das Original bleibt erhalten. Benötigt FFmpeg.",
        audioArchiveAfterDays:
          "Die Aufräumvorschau schlägt nach dieser Zahl von Tagen vor, nur die Audiodateien verlustfrei zu archivieren. Transkript und Auswertungen bleiben in der Historie.",
        archiveAfterDays:
          "Die Aufräumvorschau schlägt nach dieser Zahl von Tagen vor, das ganze Meeting verlustfrei zu archivieren. Archive lassen sich wiederherstellen.",
        maxVisibleMeetings:
          "Liegen mehr Meetings in der Historie, schlägt die Aufräumvorschau die ältesten zum Archivieren vor.",
        deleteArchivesAfterDays:
          "Die Aufräumvorschau schlägt endgültiges Löschen eines Archivs erst nach dieser Zahl von Tagen vor. Gelöscht wird nur nach ausdrücklicher Bestätigung.",
      }
    : {
        convertAfterDays:
          "After this many days, the retention preview proposes an additional smaller audio copy (FLAC or Opus). The original is kept. Requires FFmpeg.",
        audioArchiveAfterDays:
          "After this many days, the retention preview proposes archiving only the audio files without loss. Transcript and analyses stay in history.",
        archiveAfterDays:
          "After this many days, the retention preview proposes archiving the whole meeting without loss. Archives can be restored.",
        maxVisibleMeetings:
          "When history holds more meetings, the retention preview proposes archiving the oldest ones.",
        deleteArchivesAfterDays:
          "The retention preview proposes permanent deletion of an archive only after this many days. Nothing is deleted without explicit confirmation.",
      };
  function rule(key: Rule, label: string, initial: number) {
    if (!draft) return null;
    const number = draft[key];
    const minimum =
      key === "maxVisibleMeetings" || key === "deleteArchivesAfterDays"
        ? 1
        : draft.untouchedDays;
    return (
      <div className="retention-rule">
        <label title={ruleHelp[key]}>
          <input
            type="checkbox"
            checked={number !== null}
            disabled={busy || running}
            onChange={(e) =>
              setDraft({
                ...draft,
                [key]: e.target.checked ? Math.max(initial, minimum) : null,
              })
            }
          />
          {label}
        </label>
        {number !== null && (
          <input
            type="number"
            min={minimum}
            max={key === "maxVisibleMeetings" ? 1000000 : 36500}
            aria-label={label}
            value={number}
            disabled={busy || running}
            onChange={(e) =>
              setDraft({ ...draft, [key]: Number(e.target.value) })
            }
          />
        )}
      </div>
    );
  }
  return (
    <section className="legacy-panel retention-panel">
      <h2>
        <Archive size={19} />
        {de ? "Aufbewahrung und Archiv" : "Retention and archive"}
      </h2>
      <p>
        {de
          ? "Alle Regeln beginnen ausgeschaltet. Eine Vorschau ändert keine Dateien. VIP-, laufende, unvollständige oder nicht sicher lesbare Meetings bleiben geschützt. Das endgültige Löschen eines Archivs braucht eine zusätzliche ausdrückliche Bestätigung."
          : "Every rule starts disabled. A preview changes no files. VIP, active, incomplete and unreadable meetings are protected. Permanent archive deletion requires a separate explicit confirmation."}
      </p>
      {error &&
        (statusContainer ? (
          createPortal(<p role="alert">{error}</p>, statusContainer)
        ) : (
          <p role="alert">{error}</p>
        ))}
      {draft && (
        <>
          <label>
            {de
              ? "Meetings zunächst unangetastet lassen · Tage"
              : "Leave meetings untouched initially · days"}
            <input
              type="number"
              min={1}
              max={36500}
              value={draft.untouchedDays}
              disabled={busy || running}
              onChange={(e) =>
                setDraft({ ...draft, untouchedDays: Number(e.target.value) })
              }
            />
          </label>
          <div className="retention-rules">
            {rule(
              "convertAfterDays",
              de
                ? "Zusätzliche Audiokopie umwandeln nach Tagen"
                : "Convert an additional audio copy after days",
              60,
            )}
            {rule(
              "audioArchiveAfterDays",
              de
                ? "Nur Audio archivieren nach Tagen · Text bleibt"
                : "Archive audio only after days · keep text",
              90,
            )}
            {rule(
              "archiveAfterDays",
              de
                ? "Ganzes Meeting archivieren nach Tagen"
                : "Archive the whole meeting after days",
              180,
            )}
            {rule(
              "maxVisibleMeetings",
              de
                ? "Höchstens so viele Meetings in der normalen Historie"
                : "Maximum meetings in normal history",
              1000,
            )}
            {rule(
              "deleteArchivesAfterDays",
              de
                ? "Endgültiges Löschen vorschlagen nach Tagen im Archiv"
                : "Propose permanent deletion after days in archive",
              365,
            )}
          </div>
          <label>
            {de ? "Optionales Audioformat" : "Optional audio format"}
            <select
              value={draft.codec}
              disabled={busy || running}
              onChange={(e) =>
                setDraft({ ...draft, codec: e.target.value as "flac" | "opus" })
              }
            >
              <option value="flac">
                FLAC · {de ? "verlustfrei" : "lossless"}
              </option>
              <option value="opus">
                Opus · {de ? "kompakter, verlustbehaftet" : "smaller, lossy"}
              </option>
            </select>
          </label>
          <p className="muted">
            {de
              ? "ZIP-Archive erhalten die Originaldateien verlustfrei. Zusätzliche FLAC-/Opus-Kopien ersetzen niemals ungeprüft die einzige Originalaufnahme. Opus kann die spätere Erkennungsqualität beeinflussen."
              : "ZIP archives preserve original files without loss. Additional FLAC/Opus copies never replace the only original without verification. Opus may affect later recognition quality."}
          </p>
          <label>
            {de
              ? "Separat installiertes FFmpeg · nur für optionale Formatumwandlung"
              : "Separately installed FFmpeg · only for optional format conversion"}
            <input value={draft.ffmpegExecutable ?? ""} readOnly />
          </label>
          <button
            className="secondary"
            disabled={busy || running || !api.chooseFfmpegExecutable}
            title={
              de
                ? "Die ausgewählte Programmdatei wird geprüft. Wähle und bestätige sie nach jedem App-Neustart sowie nach einem Austausch erneut, bevor du sie verwendest. Der gespeicherte Pfad allein ist keine neue Freigabe. Abbrechen lässt die bisherige Wahl unverändert."
                : "The selected program is checked. Choose and confirm it again after each app restart or replacement before using it. The saved path alone does not grant renewed permission. Cancelling preserves the previous selection."
            }
            onClick={() => void chooseFfmpeg()}
          >
            {de
              ? "FFmpeg im Dateidialog auswählen …"
              : "Choose FFmpeg in file dialog …"}
          </button>
          <button
            className="secondary"
            disabled={busy || running || !draft.ffmpegExecutable}
            title={
              de
                ? "Das Entfernen ändert nur die Programmauswahl. Programmdatei und Aufnahmen bleiben erhalten. Übernimm die Änderung mit „Regeln speichern“, bevor du eine neue Vorschau erstellst."
                : "Clearing changes only the program selection. The program file and recordings are retained. Apply the change with Save rules before creating a new preview."
            }
            onClick={clearFfmpeg}
          >
            {de ? "FFmpeg-Auswahl entfernen" : "Clear FFmpeg selection"}
          </button>
          <details className="field-help">
            <summary>
              {de
                ? "Hinweise zur FFmpeg-Auswahl"
                : "About the FFmpeg selection"}
            </summary>
            <p>
              {de
                ? "Die ausgewählte Programmdatei wird geprüft. Wähle und bestätige sie nach jedem App-Neustart sowie nach einem Austausch erneut, bevor du sie verwendest. Der gespeicherte Pfad allein ist keine neue Freigabe. Abbrechen lässt die bisherige Wahl unverändert."
                : "The selected program is checked. Choose and confirm it again after each app restart or replacement before using it. The saved path alone does not grant renewed permission. Cancelling preserves the previous selection."}
            </p>
            <p>
              {de
                ? "Das Entfernen ändert nur die Programmauswahl. Programmdatei und Aufnahmen bleiben erhalten. Übernimm die Änderung mit „Regeln speichern“, bevor du eine neue Vorschau erstellst."
                : "Clearing changes only the program selection. The program file and recordings are retained. Apply the change with Save rules before creating a new preview."}
            </p>
          </details>
          <button
            className="secondary"
            disabled={busy || running || !changed}
            onClick={() => void save()}
          >
            {de ? "Regeln speichern" : "Save rules"}
          </button>
          <button
            className="secondary"
            disabled={busy || running || changed}
            onClick={() => void preview()}
          >
            {de ? "Aufräumvorschau erstellen" : "Create retention preview"}
          </button>
          {changed && (
            <p>
              {de
                ? "Bitte geänderte Regeln vor der Vorschau speichern."
                : "Save changed rules before creating a preview."}
            </p>
          )}
        </>
      )}
      {statusContainer === null ? null : statusContainer ? (
        createPortal(
          <MaintenanceStatus api={api} locale={locale} onProgress={progress} />,
          statusContainer,
        )
      ) : (
        <MaintenanceStatus api={api} locale={locale} onProgress={progress} />
      )}
      {plan && (
        <div className="retention-preview">
          <h3>
            {de
              ? "Vorschau · noch keine Dateien geändert"
              : "Preview · no files changed"}
          </h3>
          <p>
            {plan.scanned} {de ? "geprüft" : "checked"} · {plan.protected}{" "}
            {de ? "geschützt" : "protected"} · {plan.unreadable}{" "}
            {de ? "nicht sicher lesbar" : "unreadable"} ·{" "}
            {plan.candidates.length} {de ? "Vorschläge" : "proposals"}
          </p>
          {plan.limited && (
            <p>
              {de
                ? "Die Vorschau ist auf 500 Vorschläge begrenzt. Weitere Schritte benötigen eine neue Vorschau."
                : "This preview is limited to 500 proposals. Further actions need a new preview."}
            </p>
          )}
          <ul>
            {plan.candidates
              .slice(page * 25, (page + 1) * 25)
              .map((candidate, i) => (
                <li key={candidate.meetingId + candidate.action + i}>
                  <strong>{candidate.title}</strong> · {candidate.ageDays}{" "}
                  {de ? "Tage" : "days"}
                  <p>{actionNames[candidate.action]}</p>
                  {api.applyRetention && (
                    <button
                      className="secondary"
                      disabled={busy || running || changed}
                      onClick={() => {
                        setSelected(page * 25 + i);
                        setConfirmation("");
                        setAccepted(false);
                      }}
                    >
                      {de ? "Schritt prüfen" : "Review action"}
                    </button>
                  )}
                </li>
              ))}
          </ul>
          {plan.candidates.length > 25 && (
            <div className="search-pages">
              <button
                disabled={page === 0}
                onClick={() => setPage((n) => n - 1)}
              >
                {de ? "Zurück" : "Previous"}
              </button>
              <span>
                {page * 25 + 1}–
                {Math.min((page + 1) * 25, plan.candidates.length)}
              </span>
              <button
                disabled={(page + 1) * 25 >= plan.candidates.length}
                onClick={() => setPage((n) => n + 1)}
              >
                {de ? "Weiter" : "Next"}
              </button>
            </div>
          )}
        </div>
      )}
      {candidate && (
        <section
          className="retention-confirm"
          aria-label={
            de ? "Aufräumschritt bestätigen" : "Confirm retention action"
          }
        >
          <h3>{candidate.title}</h3>
          <p>{actionNames[candidate.action]}</p>
          <p>
            {deleting
              ? de
                ? "Alle zu diesem archivierten Meeting gehörenden geprüften Archivfassungen, zurückbehaltenen Quellkopien und Zuordnungen werden unwiderruflich gelöscht. Eine Wiederherstellung aus diesen Dateien ist danach unmöglich. Andere Meetings bleiben erhalten."
                : "All verified archive versions, retained source copies and annotations belonging to this archived meeting will be permanently deleted. These files cannot be restored afterwards. Other meetings are preserved."
              : candidate.action === "archive-meeting"
                ? de
                  ? "Das vollständige Meeting wird geprüft und aus der normalen Historie ins lokale Archiv verschoben. Zusätzlich zum ZIP bleibt vorerst eine Quellkopie erhalten; dadurch wird noch kein Speicher freigegeben."
                  : "The complete meeting is verified and moved from normal history into the local archive. A source copy is initially retained alongside the ZIP; this step does not yet free storage."
                : candidate.action === "archive-audio"
                  ? de
                    ? "Originalaudio wird erst nach vollständiger Prüfung im verlustfreien ZIP archiviert. Der Text bleibt in der Historie; für erneute Erkennung lässt sich das Audio wiederherstellen."
                    : "Original audio is moved to a lossless ZIP only after complete verification. Text remains in history; restore audio for recognition again."
                  : de
                    ? "Zusätzliche geprüfte Audiokopien werden mit dem gewählten lokalen Werkzeug erstellt. Originalaudio und bisherige Texte bleiben erhalten. Opus ist verlustbehaftet."
                    : "Additional verified audio copies are created with the selected local tool. Original audio and existing text are retained. Opus is lossy."}
          </p>
          {deleting ? (
            <label>
              {de
                ? "Zur Bestätigung exakt eingeben"
                : "Type exactly to confirm"}
              : <code>{expectedDelete}</code>
              <input
                aria-label={
                  de
                    ? "Endgültiges Löschen bestätigen"
                    : "Confirm permanent deletion"
                }
                value={confirmation}
                disabled={busy || running}
                onChange={(e) => setConfirmation(e.target.value)}
              />
            </label>
          ) : (
            <label>
              <input
                type="checkbox"
                checked={accepted}
                disabled={busy || running}
                onChange={(e) => setAccepted(e.target.checked)}
              />
              {de
                ? "Diesen einzelnen beschriebenen Schritt ausführen"
                : "Perform this single described action"}
            </label>
          )}
          <button
            className="secondary"
            disabled={
              busy ||
              running ||
              (deleting ? confirmation !== expectedDelete : !accepted)
            }
            onClick={() => void apply()}
          >
            {deleting
              ? de
                ? "Unwiderruflich löschen"
                : "Permanently delete"
              : de
                ? "Bestätigten Schritt ausführen"
                : "Run confirmed action"}
          </button>
          <button
            className="text-button"
            disabled={busy || running}
            onClick={() => setSelected(null)}
          >
            {de ? "Abbrechen" : "Cancel"}
          </button>
        </section>
      )}
      <ArchivePanel
        api={api}
        locale={locale}
        job={job}
        statusContainer={statusContainer}
      />
      <AudioCopiesPanel
        api={api}
        locale={locale}
        models={models}
        job={job}
        statusContainer={statusContainer}
      />
    </section>
  );
}
