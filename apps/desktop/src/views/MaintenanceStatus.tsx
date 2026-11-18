import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import CancelJobButton from "./CancelJobButton";
import { clock } from "../domain";
import { errorMessage } from "../api";
import { startAdaptivePoll } from "../jobPolling";
import type { DesktopApi, Locale, LibraryProgress } from "../types";
export default function MaintenanceStatus({
  api,
  locale,
  onProgress,
  hidden: suppressed = false,
}: {
  api: DesktopApi;
  locale: Locale;
  onProgress?: (job: LibraryProgress) => void;
  /** Another instance shows the same job on this page; keep dismissals but do not poll. */
  hidden?: boolean;
}) {
  const [job, setJob] = useState<LibraryProgress | null>(null),
    [error, setError] = useState<string | null>(null),
    [hidden, setHidden] = useState<string | null>(null);
  const de = locale === "de";
  useEffect(() => {
    if (suppressed || !api.getMaintenanceProgress) return;
    let active = true,
      running = false;
    const stop = startAdaptivePoll(async () => {
      try {
        const value = await api.getMaintenanceProgress!();
        running = value?.status === "running";
        if (active) {
          setJob(value);
          setError(null);
        }
      } catch (e) {
        if (active) setError(errorMessage(e));
      }
      return running;
    });
    return () => {
      active = false;
      stop();
    };
  }, [api, suppressed]);
  useEffect(() => {
    if (job) onProgress?.(job);
  }, [job?.jobId, job?.status, onProgress]);
  if (suppressed || !job || job.jobId === hidden) return null;
  const running = job.status === "running";
  const title = de ? "Aufbewahrung und Archiv" : "Retention and archive";
  const operation =
    (de
      ? {
          "cleanup-preview": "Vorschau",
          "retention-apply": "Bestätigter Aufräumschritt",
          "archive-restore": "Originaldateien wiederherstellen",
          "archive-compact": "Überflüssige Rohkopie entfernen",
          "prepare-transcription": "Neue Transkriptionsquelle vorbereiten",
        }
      : {
          "cleanup-preview": "Preview",
          "retention-apply": "Confirmed retention action",
          "archive-restore": "Restore original files",
          "archive-compact": "Remove redundant raw copy",
          "prepare-transcription": "Prepare new transcription source",
        })[job.kind] ?? (de ? "Aufräumschritte" : "Maintenance");
  return (
    <section
      className="notes-progress notes-statusbar"
      role="status"
      aria-label={title}
    >
      <div className="notes-statusbar-row">
        <strong>
          {running && <RefreshCw size={16} className="spinning" />}
          {title} · {operation} ·{" "}
          {running
            ? de
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
          aria-label={title}
          value={job.totalTasks ? job.confirmedTasks : undefined}
          max={job.totalTasks || undefined}
        />
      )}
      <p>
        {job.confirmedTasks} / {job.totalTasks || "?"}{" "}
        {de ? "bestätigte Prüfschritte" : "confirmed check steps"}
        {job.totalTasks
          ? ` · ${Math.floor((job.confirmedTasks / job.totalTasks) * 100)}%`
          : ""}
      </p>
      {(job.error || error) && <p role="alert">{error ?? job.error}</p>}
      {running ? (
        api.cancelMaintenance && (
          <CancelJobButton
            key={job.jobId}
            requested={job.cancelRequested}
            label={
              de ? "Aufbewahrungsauftrag abbrechen" : "Cancel retention job"
            }
            cancel={() => api.cancelMaintenance!(job.jobId)}
          />
        )
      ) : (
        <button className="text-button" onClick={() => setHidden(job.jobId)}>
          {de ? "Hinweis schließen" : "Dismiss"}
        </button>
      )}
    </section>
  );
}
