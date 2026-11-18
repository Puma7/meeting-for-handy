import { useEffect, useState } from "react";
import { RefreshCw, X } from "lucide-react";
import CancelJobButton from "./CancelJobButton";
import { errorMessage } from "../api";
import { clock } from "../domain";
import { startAdaptivePoll } from "../jobPolling";
import type { DesktopApi, LibraryProgress, Locale } from "../types";
export default function LibraryStatus({
  api,
  connected,
  locale,
}: {
  api: DesktopApi;
  connected: boolean;
  locale: Locale;
}) {
  const [job, setJob] = useState<LibraryProgress | null>(null),
    [error, setError] = useState<string | null>(null),
    [hidden, setHidden] = useState<string | null>(null);
  const de = locale === "de";
  useEffect(() => {
    if (job?.status === "complete")
      window.dispatchEvent(new Event("library-index-updated"));
  }, [job?.jobId, job?.status]);
  useEffect(() => {
    if (!connected || !api.getLibraryProgress) return;
    let disposed = false,
      running = false;
    const stop = startAdaptivePoll(async () => {
      try {
        const next = await api.getLibraryProgress?.();
        running = next?.status === "running";
        if (!disposed) {
          setJob(next ?? null);
          setError(null);
        }
      } catch (e) {
        if (!disposed) setError(errorMessage(e));
      }
      return running;
    });
    return () => {
      disposed = true;
      stop();
    };
  }, [api, connected]);
  // A finished update needs no permanent bar; running, failed and cancelled jobs stay visible.
  if (!job || job.jobId === hidden || job.status === "complete") return null;
  const running = job.status === "running";
  const label = de ? "Lokaler Suchindex" : "Local search index";
  return (
    <section
      className="notes-progress notes-statusbar library-status"
      data-status={job.status}
      role="status"
      aria-label={label}
    >
      <div className="notes-statusbar-row">
        <div className="notes-statusbar-summary">
          <strong>
            {running && <RefreshCw size={16} className="spinning" />}
            {label} ·{" "}
            {running
              ? de
                ? "wird aktualisiert"
                : "updating"
              : job.status === "cancelled"
                ? de
                  ? "abgebrochen"
                  : "cancelled"
                : de
                  ? "fehlgeschlagen"
                  : "failed"}
          </strong>
        </div>
        <strong>{clock(job.elapsedMs)}</strong>
        {running ? (
          api.cancelLibraryJob && (
            <CancelJobButton
              key={job.jobId}
              requested={job.cancelRequested}
              label={de ? "Indexierung abbrechen" : "Cancel indexing"}
              cancel={() => api.cancelLibraryJob!(job.jobId)}
            />
          )
        ) : (
          <button
            className="icon-button"
            aria-label={de ? "Indexhinweis schließen" : "Dismiss index status"}
            onClick={() => setHidden(job.jobId)}
          >
            <X size={15} />
          </button>
        )}
      </div>
      <progress
        aria-label={label}
        value={job.totalTasks ? job.confirmedTasks : undefined}
        max={job.totalTasks || undefined}
      />
      <p className="muted">
        {job.confirmedTasks}/{job.totalTasks || "?"}{" "}
        {de ? "Ordnereinträge geprüft" : "directory entries checked"}
        {job.skipped > 0
          ? ` · ${job.skipped} ${de ? "nicht lesbare Einträge unverändert übersprungen" : "unreadable entries preserved and skipped"}`
          : ""}
      </p>
      {(error || job.error) && (
        <p className="inline-error" role="alert">
          {error ?? job.error}
        </p>
      )}
    </section>
  );
}
