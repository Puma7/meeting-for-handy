import {
  RefreshCw,
  Square,
  X,
  ArrowUpRight,
  Sparkles,
  CheckCircle2,
  CircleAlert,
} from "lucide-react";
import { clock } from "../domain";
import type { NotesProgress } from "../types";
import type { MessageKey } from "../i18n";
import type { JobFeedback } from "../useNotesJob";

type Props = {
  progress: NotesProgress | null;
  elapsedMs: number;
  cancelPending: boolean;
  feedback: JobFeedback | null;
  pollError: string | null;
  meetingTitle: string;
  t: Record<MessageKey, string>;
  onCancel: () => void;
  onOpen: () => void;
  onDismiss: () => void;
  onResume: (() => void) | null;
  awaitingProgress?: boolean;
  setup?: {
    loading: boolean;
    message: string;
    startDisabled: boolean;
    onStart: () => void;
  };
};
export default function NotesProgressPanel({
  progress,
  elapsedMs,
  cancelPending,
  feedback,
  pollError,
  meetingTitle,
  t,
  onCancel,
  onOpen,
  onDismiss,
  onResume,
  setup,
  awaitingProgress,
}: Props) {
  if (!progress && !feedback) {
    if (awaitingProgress)
      return (
        <section
          className="notes-progress notes-statusbar"
          data-status="checking"
          aria-label={t.notesTask}
          role="status"
        >
          <div className="notes-statusbar-summary">
            <strong>
              <RefreshCw size={16} className="spinning" aria-hidden="true" />
              {t.notesCheckingStatus}
            </strong>
          </div>
          <progress aria-label={t.notesCheckingStatus} />
          {pollError && (
            <p className="inline-error" role="alert">
              {t.progressUnavailable} {pollError}
            </p>
          )}
        </section>
      );
    if (!setup) return null;
    return (
      <section
        className="notes-progress notes-statusbar"
        data-status="idle"
        aria-label={t.notesTask}
        role="status"
      >
        <div className="notes-statusbar-row">
          <div className="notes-statusbar-summary">
            <strong>
              {setup.loading && (
                <RefreshCw size={16} className="spinning" aria-hidden="true" />
              )}
              {t.notesNotStarted}
            </strong>
            <p>{setup.message}</p>
          </div>
          <button
            className="primary"
            disabled={setup.startDisabled}
            onClick={setup.onStart}
          >
            <Sparkles size={16} aria-hidden="true" />
            {t.startAnalysis}
          </button>
        </div>
      </section>
    );
  }
  const running = progress?.status === "running";
  const state = running ? "running" : (feedback?.kind ?? progress?.status);
  const complete = state === "complete";
  const failed = state === "failed";
  const total = Math.max(0, progress?.totalTasks ?? 0);
  const confirmed = complete
    ? total
    : Math.min(Math.max(0, progress?.confirmedTasks ?? 0), total);
  const percent = complete
    ? 100
    : total > 0
      ? Math.floor((confirmed * 100) / total)
      : null;
  const status =
    state === "cancelled"
      ? t.notesCancelled
      : failed
        ? t.notesFailed
        : complete
          ? t.notesComplete
          : t.notesTask;
  const stage =
    progress?.stage === "requesting"
      ? t.stageRequesting
      : progress?.stage === "validating"
        ? t.stageValidating
        : progress?.stage === "saving"
          ? t.stageSaving
          : t.stagePreparing;
  const cancelling = running && (cancelPending || progress?.cancelRequested);
  const message = feedback?.message ?? progress?.error;
  return (
    <section
      className="notes-progress notes-statusbar"
      data-status={state}
      aria-label={t.notesTask}
      role="status"
    >
      <div className="notes-statusbar-row">
        <div className="notes-statusbar-summary">
          <strong>
            {running ? (
              <RefreshCw size={16} className="spinning" aria-hidden="true" />
            ) : complete ? (
              <CheckCircle2 size={16} aria-hidden="true" />
            ) : (
              <CircleAlert size={16} aria-hidden="true" />
            )}
            {running ? (cancelling ? t.cancelRequested : stage) : status}
          </strong>
          <p>{meetingTitle}</p>
        </div>
        <div
          className="notes-statusbar-time"
          aria-label={t.elapsed}
          aria-live="off"
        >
          <strong>{clock(elapsedMs)}</strong>
        </div>
        {running ? (
          <button
            className="secondary"
            disabled={Boolean(cancelling)}
            onClick={onCancel}
            title={t.cancelHelp}
          >
            <Square size={15} aria-hidden="true" />
            <span>{cancelling ? t.cancelRequested : t.cancelAnalysis}</span>
          </button>
        ) : (
          <button
            className="icon-button"
            onClick={onDismiss}
            aria-label={t.dismiss}
          >
            <X size={17} />
          </button>
        )}
      </div>
      {(running || total > 0 || complete) && (
        <div className="notes-statusbar-progress">
          {percent === null ? (
            <progress aria-label={t.stagePreparing} />
          ) : (
            <progress
              value={confirmed || (complete ? 1 : 0)}
              max={total || 1}
              aria-label={t.confirmedTasks}
            />
          )}
          <span>{percent === null ? t.stagePreparing : percent + "%"}</span>
        </div>
      )}
      <div className="notes-statusbar-details">
        <span>
          {total > 0 && (
            <>
              {confirmed}/{total} {t.confirmedTasks}
            </>
          )}
          {progress?.stepName && (
            <>
              {total > 0 ? " · " : ""}
              {progress.stepName}
              {progress.totalSteps > 0
                ? " · " + progress.stepIndex + "/" + progress.totalSteps
                : ""}
            </>
          )}
        </span>
        <button className="text-button" onClick={onOpen}>
          <ArrowUpRight size={14} />
          {t.openAnalysisMeeting}
        </button>
        {!running && onResume && (
          <button className="secondary" onClick={onResume} title={t.resumeHelp}>
            <RefreshCw size={15} />
            {t.resumeAnalysis}
          </button>
        )}
      </div>
      {running && (
        <p className="notes-statusbar-help">
          {cancelling ? t.cancelHelp : t.inferenceUnknown}
        </p>
      )}
      {running && feedback?.kind === "cancel-unavailable" && (
        <p className="inline-error">{t.cancelUnavailable}</p>
      )}
      {pollError && (
        <p className="inline-error" role="alert">
          {t.progressUnavailable} {pollError}
        </p>
      )}
      {message && (
        <p className="inline-error" role="alert">
          {message}
        </p>
      )}
      {failed && (
        <p className="notes-statusbar-help">{t.analysisFailureHelp}</p>
      )}
    </section>
  );
}
