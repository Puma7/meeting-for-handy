import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Trash2 } from "lucide-react";
import { errorMessage } from "../api";
import { clock } from "../domain";
import type {
  DesktopApi,
  ErasurePreview,
  Locale,
  MeetingSummary,
} from "../types";

type Stage = "preparing" | "ready" | "deleting" | "failed";

/** Confirms one immutable meeting preview; cancellation never implies undo of removed files. */
export default function EraseMeetingDialog({
  api,
  meeting,
  locale,
  onStart,
  onDeleted,
  onStopped,
  onClose,
}: {
  api: DesktopApi;
  meeting: MeetingSummary;
  locale: Locale;
  onStart: () => void;
  onDeleted: (preservedAtomicRemnants: number) => void;
  onStopped: () => void;
  onClose: () => void;
}) {
  const de = locale === "de",
    word = de ? "LÖSCHEN" : "DELETE";
  const [stage, setStage] = useState<Stage>("preparing");
  const [preview, setPreview] = useState<ErasurePreview | null>(null);
  const [confirmation, setConfirmation] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState({ done: 0, total: 0 });
  const [cancelling, setCancelling] = useState(false);
  const [statusUnavailable, setStatusUnavailable] = useState(false);
  const form = useRef<HTMLFormElement>(null),
    alive = useRef(true);
  // Capture before the first layout effect transfers focus into this dialog.
  const opener = useRef(
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null,
  );
  const job = useRef<string | null>(null),
    previewJob = useRef("");
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const generation = useRef(0),
    destructive = useRef(false);
  const cancelRequested = useRef(false),
    submitting = useRef(false);
  const callbacks = useRef({ onStart, onDeleted, onStopped, onClose });
  callbacks.current = { onStart, onDeleted, onStopped, onClose };
  const safeAction = useRef<HTMLButtonElement>(null);
  // Starting removal replaces the focused submit control. Keep focus in the
  // dialog, including the brief period when cancellation disables all controls.
  useLayoutEffect(() => {
    if (!form.current?.contains(document.activeElement)) {
      const target = safeAction.current?.disabled
        ? form.current
        : safeAction.current;
      target?.focus();
    }
  }, [stage, cancelling, statusUnavailable]);

  /** Only an exact current job's completed status may confirm success. */
  async function watch(
    id: string,
    deleting: boolean,
    sequence: number,
    failures = 0,
  ) {
    if (!alive.current || generation.current !== sequence) return;
    try {
      const status = await api.getMaintenanceProgress?.();
      if (!alive.current || generation.current !== sequence) return;
      if (!status || status.jobId !== id)
        throw new Error(
          de
            ? "Der Auftragsstatus ist nicht bestätigt. Bitte erneut prüfen."
            : "The job status is unconfirmed. Check again.",
        );
      if (
        !["running", "complete", "failed", "cancelled"].includes(status.status)
      )
        throw new Error(
          de
            ? "Der Auftragsstatus ist unbekannt. Bitte erneut prüfen."
            : "The job status is unknown. Check again.",
        );
      setProgress({ done: status.confirmedTasks, total: status.totalTasks });
      setStatusUnavailable(false);
      if (status.status === "running") {
        timer.current = setTimeout(
          () => void watch(id, deleting, sequence),
          400,
        );
        return;
      }
      job.current = null;
      setCancelling(false);
      if (!deleting && cancelRequested.current) {
        callbacks.current.onClose();
        return;
      }
      if (status.status !== "complete")
        throw new Error(
          status.error ??
            (de ? "Vorgang abgebrochen." : "Operation cancelled."),
        );
      if (deleting) {
        destructive.current = false;
        callbacks.current.onDeleted(preview?.preservedAtomicRemnants ?? 0);
      } else {
        const value = await api.getMeetingErasurePreview!(id);
        if (!alive.current || generation.current !== sequence) return;
        if (value.meetingId !== meeting.id)
          throw new Error(
            de
              ? "Die Vorschau gehört zu einem anderen Meeting."
              : "The preview belongs to a different meeting.",
          );
        previewJob.current = id;
        setPreview(value);
        setStage("ready");
      }
    } catch (failure) {
      if (!alive.current || generation.current !== sequence) return;
      setError(errorMessage(failure));
      if (deleting && job.current) {
        // A failed status read is not evidence that destructive work stopped.
        if (failures < 2) {
          timer.current = setTimeout(
            () => void watch(id, deleting, sequence, failures + 1),
            800,
          );
        } else {
          // Stop background retries, request cancellation, and keep the exact job
          // reserved until a later explicit status check proves a terminal state.
          setStatusUnavailable(true);
          setCancelling(false);
          cancelRequested.current = true;
          void api.cancelMaintenance?.(id).catch(() => undefined);
        }
        return;
      }
      setStage("failed");
      if (destructive.current) {
        destructive.current = false;
        callbacks.current.onStopped();
      }
    }
  }
  /** A failed cancellation acknowledgement never ends observation of an existing job. */
  async function cancelAndObserve(
    id: string,
    deleting: boolean,
    sequence: number,
  ) {
    try {
      await api.cancelMaintenance?.(id);
    } catch (failure) {
      if (alive.current && generation.current === sequence) {
        setError(errorMessage(failure));
        setCancelling(false);
      }
    }
    if (alive.current && generation.current === sequence) {
      setStatusUnavailable(false);
      void watch(id, deleting, sequence);
    }
  }
  /** Cancels the native job even if its ID arrives after the dialog was closed. */
  async function startPreview() {
    const sequence = ++generation.current;
    setStage("preparing");
    setError(null);
    setPreview(null);
    setConfirmation("");
    setCancelling(false);
    setStatusUnavailable(false);
    cancelRequested.current = false;
    submitting.current = true;
    try {
      const id = await api.previewMeetingErasure!(meeting.id);
      if (!alive.current || generation.current !== sequence) {
        await api.cancelMaintenance?.(id);
        return;
      }
      job.current = id;
      submitting.current = false;
      if (cancelRequested.current) await cancelAndObserve(id, false, sequence);
      else void watch(id, false, sequence);
    } catch (failure) {
      if (alive.current && generation.current === sequence) {
        submitting.current = false;
        setCancelling(false);
        setStage("failed");
        setError(errorMessage(failure));
      }
    }
  }
  useEffect(() => {
    alive.current = true;
    form.current?.querySelector<HTMLButtonElement>("button")?.focus();
    timer.current = setTimeout(() => void startPreview(), 0);
    return () => {
      alive.current = false;
      ++generation.current;
      clearTimeout(timer.current);
      if (job.current)
        void api.cancelMaintenance?.(job.current).catch(() => undefined);
      // The parent removes inert and restores its fallback during passive cleanup.
      // Restore the real opener afterwards, unless another dialog has taken over.
      requestAnimationFrame(() => {
        if (
          !alive.current &&
          opener.current?.isConnected &&
          !document.querySelector('[role="dialog"],[role="alertdialog"]')
        )
          opener.current.focus();
      });
    };
    // The parent keys the dialog by meeting ID; API and callbacks are stable for its lifetime.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Submits exactly the displayed confirmed capability and observes only its own job. */
  async function remove() {
    if (
      stage !== "ready" ||
      !preview ||
      preview.meetingId !== meeting.id ||
      confirmation !== word
    )
      return;
    const sequence = ++generation.current;
    cancelRequested.current = false;
    submitting.current = true;
    destructive.current = true;
    callbacks.current.onStart();
    setStage("deleting");
    setStatusUnavailable(false);
    setError(null);
    setProgress({ done: 0, total: preview.files });
    try {
      const id = await api.eraseMeetingNow!(
        previewJob.current,
        meeting.id,
        confirmation,
      );
      if (!alive.current || generation.current !== sequence) {
        await api.cancelMaintenance?.(id);
        return;
      }
      job.current = id;
      submitting.current = false;
      if (cancelRequested.current) await cancelAndObserve(id, true, sequence);
      else void watch(id, true, sequence);
    } catch (failure) {
      if (alive.current && generation.current === sequence) {
        submitting.current = false;
        destructive.current = false;
        setCancelling(false);
        callbacks.current.onStopped();
        setStage("failed");
        setError(errorMessage(failure));
      }
    }
  }
  /** Requests cancellation without representing an irreversible partial removal as undo. */
  async function stop() {
    if (stage !== "preparing" && stage !== "deleting") {
      callbacks.current.onClose();
      return;
    }
    cancelRequested.current = true;
    setCancelling(true);
    if (!job.current) {
      if (!submitting.current) callbacks.current.onClose();
      return;
    }
    const id = job.current;
    const sequence = ++generation.current;
    clearTimeout(timer.current);
    await cancelAndObserve(id, stage === "deleting", sequence);
  }
  /** Reinspect the existing native job without releasing admission or starting another erasure. */
  function retryStatus() {
    if (!job.current) return;
    clearTimeout(timer.current);
    setStatusUnavailable(false);
    setError(null);
    void watch(job.current, stage === "deleting", ++generation.current);
  }
  return (
    <div className="modal-backdrop">
      <form
        ref={form}
        tabIndex={-1}
        className="dialog small erasure-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="erase-meeting-title"
        aria-describedby="erase-meeting-scope"
        onSubmit={(event) => {
          event.preventDefault();
          void remove();
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            void stop();
          }
          if (event.key === "Tab") {
            const controls = Array.from(
              form.current?.querySelectorAll<HTMLElement>(
                "button:not(:disabled),input:not(:disabled),summary,[tabindex='0']",
              ) ?? [],
            );
            const first = controls[0],
              last = controls[controls.length - 1];
            if (!controls.length) {
              event.preventDefault();
              return;
            }
            if (event.shiftKey && document.activeElement === first) {
              event.preventDefault();
              last?.focus();
            } else if (!event.shiftKey && document.activeElement === last) {
              event.preventDefault();
              first?.focus();
            }
          }
        }}
      >
        <h2 id="erase-meeting-title">
          {de ? "Meeting dauerhaft löschen?" : "Permanently delete meeting?"}
        </h2>
        <p className="erasure-meeting">
          <strong>{meeting.title}</strong>
          <br />
          {new Date(meeting.startedAt).toLocaleString(locale)}
          {meeting.status !== "erasure-pending" && (
            <> · {clock(meeting.durationMs)}</>
          )}
        </p>
        <p id="erase-meeting-scope">
          {de
            ? "Entfernt beide Audiospuren, Transkripte, Auswertungen und zugehörige Kopien in HandyMeeting. Es gibt keine Wiederherstellung."
            : "Removes both audio tracks, transcripts, analyses and related copies stored by HandyMeeting. This cannot be undone."}
        </p>
        <p className="muted">
          {de
            ? "Appverwaltete Transkript-Sicherungskopien werden mitgelöscht. Externe Exporte, Zwischenablage und externe Sicherungskopien werden nicht gelöscht. Physische Datenträgerspuren lassen sich hier nicht garantiert beseitigen."
            : "App-managed transcript backups are also erased. External exports, clipboard and external backups are not erased. Physical storage traces cannot be guaranteed erased here."}
        </p>
        {stage === "ready" && preview && (
          <>
            <p>
              {de
                ? `${preview.files.toLocaleString(locale)} zugeordnete Dateien · ${(preview.bytes / 1048576).toLocaleString(locale, { maximumFractionDigits: 1 })} MB. Der Suchindex wird neu aufgebaut.`
                : `${preview.files.toLocaleString(locale)} attributed files · ${(preview.bytes / 1048576).toLocaleString(locale, { maximumFractionDigits: 1 })} MB. The search index will be rebuilt.`}
            </p>
            {!!preview.preservedAtomicRemnants && (
              <p role="alert">
                {de
                  ? `${preview.preservedAtomicRemnants} unterbrochene Datensätze sind keinem Meeting sicher zuzuordnen und bleiben erhalten. Zugeordnete Meetingdaten können trotzdem gelöscht werden.`
                  : `${preview.preservedAtomicRemnants} interrupted records cannot be assigned to a meeting safely and will be preserved. Attributed meeting data can still be removed.`}
              </p>
            )}
            <label>
              {de
                ? `Zur Bestätigung ${word} eingeben`
                : `Type ${word} to confirm`}
              <input
                value={confirmation}
                onChange={(event) => setConfirmation(event.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
            </label>
          </>
        )}
        {(stage === "preparing" || stage === "deleting") && (
          <div role="status" aria-live="polite">
            {cancelling
              ? de
                ? "Stopp angefordert …"
                : "Stop requested …"
              : stage === "preparing"
                ? de
                  ? "Dateien und Kopien werden geprüft …"
                  : "Checking files and copies …"
                : de
                  ? "Meeting wird unwiderruflich entfernt …"
                  : "Permanently removing meeting …"}
            {progress.total > 0 && (
              <p>
                {progress.done.toLocaleString(locale)} /{" "}
                {progress.total.toLocaleString(locale)}
              </p>
            )}
          </div>
        )}
        {error && (
          <p role="alert" className="inline-error">
            {error}
          </p>
        )}
        {stage === "failed" && (
          <p>
            {de
              ? "Bereits entfernte Dateien bleiben gelöscht. Mit einer neuen Vorschau können verbleibende Daten geprüft und entfernt werden."
              : "Files already removed stay deleted. Create a new preview to inspect and remove remaining data."}
          </p>
        )}
        <div className="dialog-actions">
          {statusUnavailable && stage === "deleting" && (
            <button type="button" className="secondary" onClick={retryStatus}>
              {de ? "Auftragsstatus erneut prüfen" : "Check job status again"}
            </button>
          )}
          <button
            ref={safeAction}
            type="button"
            className="secondary"
            disabled={cancelling}
            onClick={() => void stop()}
          >
            {stage === "deleting"
              ? de
                ? "Restliche Löschung stoppen"
                : "Stop remaining deletion"
              : stage === "preparing"
                ? de
                  ? "Prüfung abbrechen"
                  : "Cancel check"
                : de
                  ? "Schließen"
                  : "Close"}
          </button>
          {stage === "failed" && (
            <button
              type="button"
              className="secondary"
              onClick={() => void startPreview()}
            >
              {de ? "Erneut prüfen" : "Check again"}
            </button>
          )}
          {stage === "ready" && (
            <button
              type="submit"
              className="danger"
              disabled={confirmation !== word}
            >
              <Trash2 size={15} />
              {de ? "Jetzt endgültig löschen" : "Permanently delete now"}
            </button>
          )}
        </div>
      </form>
    </div>
  );
}
