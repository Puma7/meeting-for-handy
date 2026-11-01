import { useState, useEffect, useRef } from "react";
import { RefreshCw, Download, ExternalLink } from "lucide-react";
import { errorMessage } from "../api";
import { clock } from "../domain";
import { requestJobStatusRefresh, startAdaptivePoll } from "../jobPolling";
import type { DesktopApi, Locale, UpdateProgress } from "../types";
/**
 * Displays explicit update checks/downloads and native job progress. The backend
 * authorizes installation; platforms without in-app downloads use the official
 * release-page action.
 */
export default function UpdatePanel({
  api,
  locale,
  footer = false,
  shortcut = false,
  onOpenSettings,
  blocked = false,
  hidden: suppressed = false,
}: {
  api: DesktopApi;
  locale: Locale;
  footer?: boolean;
  /** Explicit check in the sidebar; existing full settings and progress stay authoritative. */
  shortcut?: boolean;
  onOpenSettings?: () => void;
  blocked?: boolean;
  /** The full update panel is on screen; keep the dismissal but do not poll or render. */
  hidden?: boolean;
}) {
  const de = locale === "de";
  const [job, setJob] = useState<UpdateProgress | null>(null),
    [error, setError] = useState<string | null>(null),
    [request, setRequest] = useState(false),
    [jobsRunning, setJobsRunning] = useState(false),
    [dismissed, setDismissed] = useState<string | null>(null);
  // A check or download request can run before its status becomes visible.
  const requesting = useRef(false);
  useEffect(() => {
    if (suppressed || !api.getUpdateProgress) return;
    let active = true,
      busy = false;
    const stop = startAdaptivePoll(async () => {
      try {
        const value = await api.getUpdateProgress!();
        // Archive maintenance and indexing also block installation in the backend.
        const others =
          footer || shortcut
            ? []
            : await Promise.all([
                api.getMaintenanceProgress?.() ?? null,
                api.getLibraryProgress?.() ?? null,
              ]);
        const othersRunning = others.some((p) => p?.status === "running");
        busy =
          value.status === "checking" ||
          value.status === "downloading" ||
          othersRunning;
        if (active) {
          setJob(value);
          setJobsRunning(othersRunning);
          setError(null);
        }
      } catch (e) {
        if (active) setError(errorMessage(e));
      }
      return busy || requesting.current;
    });
    return () => {
      active = false;
      stop();
    };
  }, [api, footer, shortcut, suppressed]);
  if (!api.checkForUpdate || suppressed) return null;
  const installBlocked = blocked || jobsRunning;
  const running = job?.status === "checking" || job?.status === "downloading";
  // Only the verified Windows setup is downloaded in the app; other systems use the release page.
  const windows = (job?.platform ?? "windows") === "windows";
  const canDownload = job?.canDownload ?? windows;
  const releasePage =
    job?.releaseUrl ?? "https://github.com/Puma7/meeting-for-handy/releases";
  /**
   * Tracks one requested update operation and wakes progress pollers while retaining
   * command failures in the panel.
   */
  async function action(run: () => Promise<unknown>) {
    if (requesting.current) return;
    requesting.current = true;
    setRequest(true);
    setError(null);
    try {
      const pending = run();
      requestJobStatusRefresh();
      await pending;
      if (api.getUpdateProgress) setJob(await api.getUpdateProgress());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      requesting.current = false;
      setRequest(false);
    }
  }
  /**
   * Requests the backend's fixed official release page, or opens the displayed release URL
   * in a browser-only preview without granting WebView opener permissions.
   */
  async function openReleasePage() {
    setError(null);
    try {
      // The WebView has no opener permission; the app opens only its official release page.
      if (api.openReleasePage) await api.openReleasePage();
      else window.open(releasePage, "_blank", "noopener,noreferrer");
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  const externalPackage = job?.status === "available" && !canDownload;
  const title = de ? "App aktualisieren" : "Update app";
  const labels: Record<string, string> = de
    ? {
        "": "Updates noch nicht geprüft",
        idle: "Updates noch nicht geprüft",
        checking: "Offizielles Update wird geprüft",
        downloading: "Windows-Installer wird geladen",
        ready: "Installer vollständig geladen und geprüft",
        available: "Neue Version verfügbar",
        current: "Du verwendest die aktuelle veröffentlichte Version",
        failed: "Update fehlgeschlagen",
        cancelled: "Download abgebrochen",
      }
    : {
        "": "Updates not checked yet",
        idle: "Updates not checked yet",
        checking: "Checking the official release",
        downloading: "Downloading the Windows installer",
        ready: "Installer downloaded and verified",
        available: "New version available",
        current: "You have the current published version",
        failed: "Update failed",
        cancelled: "Download cancelled",
      };
  const activityLabel =
    job?.status === "downloading"
      ? labels.downloading
      : de
        ? "Update wird geprüft…"
        : "Checking for updates…";
  const shortcutLabel =
    request || running
      ? activityLabel
      : de
        ? "Update prüfen"
        : "Check for updates";
  // A cancelled download was the user's own choice; a failure stays until dismissed.
  const failureKey = `${job?.attempt ?? ""}:${job?.version ?? ""}:${job?.error ?? ""}`;
  if (shortcut)
    return (
      <div className="sidebar-update">
        <button
          className="secondary"
          title={shortcutLabel}
          aria-label={shortcutLabel}
          disabled={request || running}
          onClick={() => void action(api.checkForUpdate!)}
        >
          <RefreshCw
            size={16}
            className={request || running ? "spinning" : undefined}
          />
          <span className="sidebar-update-label">{shortcutLabel}</span>
        </button>
        <span className="sr-only" role="status" aria-live="polite">
          {request || running ? activityLabel : ""}
        </span>
        {job?.status === "current" && (
          <p role="status">{de ? "App ist aktuell." : "App is up to date."}</p>
        )}
        {(job?.status === "available" || job?.status === "ready") && (
          <p role="status">
            {de ? "Update verfügbar." : "Update available."}{" "}
            {onOpenSettings && (
              <button className="text-button" onClick={onOpenSettings}>
                {de ? "Update öffnen" : "Open update"}
              </button>
            )}
          </p>
        )}
        {error && error !== job?.error && <p role="alert">{error}</p>}
      </div>
    );
  if (
    footer &&
    !running &&
    (job?.status !== "failed" || dismissed === failureKey)
  )
    return null;
  const content = (
    <>
      <div className="notes-statusbar-row">
        <strong>
          {running && <RefreshCw className="spinning" size={16} />}{" "}
          {job ? labels[job.status] : title}
          {job?.version ? ` · ${job.version}` : ""}
        </strong>
        {running && <span>{clock(job?.elapsedMs ?? 0)}</span>}
      </div>
      {running && (
        <progress
          aria-label={title}
          value={job?.status === "downloading" ? job.receivedBytes : undefined}
          max={job?.totalBytes || undefined}
        />
      )}
      {job?.status === "downloading" && (
        <p>
          {(job.receivedBytes / 1048576).toFixed(1)} /{" "}
          {(job.totalBytes / 1048576).toFixed(1)} MB ·{" "}
          {Math.floor((job.receivedBytes / Math.max(1, job.totalBytes)) * 100)}%
        </p>
      )}
      {(error || job?.error) && <p role="alert">{error ?? job?.error}</p>}
      {!footer && (
        <>
          <p>
            {windows
              ? de
                ? "Updates werden nur auf deinen Klick geprüft. Der Windows-Installer wird mit Größe und GitHub-SHA256 geprüft; das Setup ist weiterhin unsigniert. Die Installation startet erst nach deinem zweiten Klick und beendet nur diese App, wenn keine Aufnahme, Erkennung oder Auswertung läuft."
                : "Updates are checked only when you click. The Windows installer is verified against its size and GitHub SHA256; setup remains unsigned. Installation requires another click and closes only this app when recording, recognition and analysis are finished."
              : de
                ? "Updates werden nur auf deinen Klick geprüft. Auf diesem System lädt und installiert Meeting for Handy Updates nicht selbst: Lade das passende Paket von der offiziellen Release-Seite und installiere es dort. Aufnahmen bleiben erhalten."
                : "Updates are checked only when you click. On this system Meeting for Handy does not download or install updates itself: get the matching package from the official release page and install it there. Recordings are kept."}
          </p>
          <button
            className="secondary"
            disabled={request || running}
            onClick={() => void action(api.checkForUpdate!)}
          >
            <RefreshCw size={15} />
            {de ? "Nach Updates suchen" : "Check for updates"}
          </button>
          {externalPackage && (
            <p>
              {job?.assetName
                ? de
                  ? `Paket für dieses System: ${job?.assetName}. Öffne die offizielle Release-Seite, um es herunterzuladen.`
                  : `Package for this system: ${job?.assetName}. Open the official release page to download it.`
                : de
                  ? "Diese Version listet noch kein Paket für dieses System. Die offizielle Release-Seite zeigt den aktuellen Stand."
                  : "This release lists no package for this system yet. The official release page shows the current state."}
            </p>
          )}
          {job?.status === "available" && canDownload && (
            <button
              className="secondary"
              disabled={request}
              onClick={() => void action(api.downloadUpdate!)}
            >
              <Download size={15} />
              {de ? "Windows-Installer laden" : "Download Windows installer"}
            </button>
          )}
          {job?.status === "ready" && (
            <button
              className="primary"
              disabled={request || installBlocked}
              onClick={() => {
                if (
                  window.confirm(
                    de
                      ? "Geprüften Windows-Installer starten und Meeting for Handy schließen? Vorhandene Aufnahmen und ältere Programmordner bleiben erhalten."
                      : "Start the verified Windows installer and close Meeting for Handy? Recordings and older program folders are retained.",
                  )
                )
                  void action(api.installUpdate!);
              }}
            >
              {de
                ? "Installer starten und App schließen"
                : "Start installer and close app"}
            </button>
          )}
          {installBlocked && job?.status === "ready" && (
            <p>
              {de
                ? "Bitte zuerst Aufnahme, Erkennung, Auswertung, Archivpflege und Indexierung abschließen."
                : "Finish recording, recognition, analysis, archive maintenance and indexing first."}
            </p>
          )}
          <p>
            <button
              className={externalPackage ? "secondary" : "text-button"}
              onClick={() => void openReleasePage()}
            >
              <ExternalLink size={14} />
              {externalPackage
                ? de
                  ? "Offizielle Release-Seite öffnen"
                  : "Open official release page"
                : de
                  ? "Offizielle Downloads und Hinweise"
                  : "Official downloads and release notes"}
            </button>
          </p>
        </>
      )}
      {job?.status === "downloading" && (
        // Cancellation only sets a flag, so it stays available while the download request runs.
        <button
          className="secondary"
          onClick={() =>
            void Promise.resolve(api.cancelUpdate!()).catch((e) =>
              setError(errorMessage(e)),
            )
          }
        >
          {de ? "Download abbrechen" : "Cancel download"}
        </button>
      )}
      {footer && job?.status === "failed" && (
        <button className="secondary" onClick={() => setDismissed(failureKey)}>
          {de ? "Ausblenden" : "Dismiss"}
        </button>
      )}
    </>
  );
  return footer ? (
    <section
      className="notes-progress notes-statusbar"
      role="status"
      aria-label={title}
    >
      {content}
    </section>
  ) : (
    <section className="legacy-panel">
      <h2>{title}</h2>
      {content}
    </section>
  );
}
