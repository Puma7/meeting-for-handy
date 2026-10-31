import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { errorMessage } from "../api";
import type {
  ArchiveEntry,
  ArchivePage,
  DesktopApi,
  LibraryProgress,
  Locale,
  UnfinishedDeletion,
} from "../types";
export default function ArchivePanel({
  api,
  locale,
  job,
  statusContainer,
}: {
  api: DesktopApi;
  locale: Locale;
  job: LibraryProgress | null;
  /** Keeps asynchronous command and catalog failures visible outside collapsed settings. */
  statusContainer?: HTMLElement | null;
}) {
  const de = locale === "de";
  const [page, setPage] = useState<ArchivePage | null>(null),
    [offset, setOffset] = useState(0),
    [error, setError] = useState<string | null>(null),
    [selected, setSelected] = useState<ArchiveEntry | null>(null),
    [compact, setCompact] = useState<ArchiveEntry | null>(null),
    [busy, setBusy] = useState(false),
    [unfinished, setUnfinished] = useState<UnfinishedDeletion[]>([]),
    [resuming, setResuming] = useState<UnfinishedDeletion | null>(null),
    [typed, setTyped] = useState(""),
    [logError, setLogError] = useState<string | null>(null);
  // Only the newest request may update the page; older pages arrive late after quick paging.
  const generation = useRef(0);
  const load = useCallback(async () => {
    if (!api.listArchives) return;
    const current = ++generation.current;
    try {
      const next = await api.listArchives(offset, 25);
      // An unreadable deletion log must not hide the archive list or block restoration.
      let stoppedError: string | null = null;
      const stopped =
        (await api.listUnfinishedDeletions?.().catch((e) => {
          stoppedError = errorMessage(e);
          return [];
        })) ?? [];
      if (current !== generation.current) return;
      setPage(next);
      setUnfinished(stopped);
      setLogError(stoppedError);
      setError(null);
    } catch (e) {
      if (current === generation.current) setError(errorMessage(e));
    }
  }, [api, offset]);
  useEffect(() => {
    void load();
  }, [load, job?.jobId, job?.status]);
  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );
  if (!api.listArchives) return null;
  const running = job?.status === "running";
  async function restore() {
    if (!selected || !api.restoreArchive) return;
    setBusy(true);
    setError(null);
    try {
      await api.restoreArchive(selected.archiveId, true);
      setSelected(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function compactCopy() {
    if (!compact || !api.compactArchive) return;
    setBusy(true);
    setError(null);
    try {
      await api.compactArchive(compact.archiveId, true);
      setCompact(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const unfinishedNotice =
    unfinished.length > 0 && api.resumePermanentDeletion ? (
      <div className="notice warning archive-interruption">
        <strong>
          {de
            ? "Unterbrochene endgültige Löschung"
            : "Stopped permanent deletion"}
        </strong>
        <p>
          {de
            ? "Diese bereits bestätigten Löschungen wurden nicht abgeschlossen. Fortsetzen löscht nur noch die protokollierten, unveränderten Dateien; geänderte Dateien stoppen den Vorgang und bleiben erhalten."
            : "These confirmed deletions did not finish. Continuing deletes only the logged, unchanged files; a changed file stops the run and is kept."}
        </p>
        <ul>
          {unfinished.map((item) => (
            <li key={item.logId}>
              <code>{item.meetingId}</code> · {item.deletedFiles}/
              {item.totalFiles} · {item.confirmedAt}{" "}
              <button
                className="secondary"
                disabled={busy || running}
                onClick={() => {
                  setResuming(item);
                  setTyped("");
                }}
              >
                {de ? "Fortsetzen prüfen" : "Review continuation"}
              </button>
            </li>
          ))}
        </ul>
        {resuming && (
          <div>
            <label>
              {de
                ? `Zur Bestätigung „DELETE ${resuming.meetingId}“ eingeben`
                : `Type "DELETE ${resuming.meetingId}" to confirm`}
              <input value={typed} onChange={(e) => setTyped(e.target.value)} />
            </label>
            <button
              className="danger"
              disabled={
                busy || running || typed !== `DELETE ${resuming.meetingId}`
              }
              onClick={() => {
                const target = resuming;
                setBusy(true);
                setError(null);
                void api.resumePermanentDeletion!(target.logId, typed)
                  .then(() => setResuming(null))
                  .catch((e) => setError(errorMessage(e)))
                  .finally(() => setBusy(false));
              }}
            >
              {de ? "Löschung fortsetzen" : "Continue deletion"}
            </button>
          </div>
        )}
      </div>
    ) : null;
  return (
    <section className="retention-preview">
      <h3>{de ? "Lokale Archive" : "Local archives"}</h3>
      <p>
        {de
          ? "Wiederherstellung prüft alle Dateien erneut und überschreibt keine vorhandene Sitzung. Die Archivkopie bleibt erhalten. Endgültiges Löschen ist ausschließlich über eine neue Aufräumvorschau möglich."
          : "Restoration verifies every file again and never overwrites an existing meeting. The archive copy is retained. Permanent deletion is available only through a fresh retention preview."}
      </p>
      {error &&
        (statusContainer ? (
          createPortal(<p role="alert">{error}</p>, statusContainer)
        ) : (
          <p role="alert">{error}</p>
        ))}
      {logError &&
        (statusContainer ? (
          createPortal(<p role="alert">{logError}</p>, statusContainer)
        ) : (
          <p role="alert">{logError}</p>
        ))}
      {unfinishedNotice &&
        (statusContainer
          ? createPortal(unfinishedNotice, statusContainer)
          : unfinishedNotice)}
      <button
        className="text-button"
        disabled={busy || running}
        onClick={() => void load()}
      >
        {de ? "Archive aktualisieren" : "Refresh archives"}
      </button>
      {page && (
        <>
          <ul>
            {page.items.map((item) => {
              const receipt = item.receipt;
              if (!receipt || item.unreadable === "receipt") {
                return (
                  <li key={item.archiveId}>
                    <strong>
                      {de ? "Nicht lesbares Archiv" : "Unreadable archive"}
                    </strong>{" "}
                    <code>{item.archiveId}</code>
                    <p role="status">
                      {de
                        ? "Der Archivbeleg ist nicht lesbar, möglicherweise stammt er aus einer neueren Version. Alle Dateien bleiben unverändert erhalten. Wiederherstellen ist hier nicht möglich; bitte mit der passenden Version öffnen oder den lokalen Archivordner prüfen."
                        : "The archive receipt cannot be read; it may come from a newer version. All files are kept unchanged. Restoring is not available here; open it with a matching version or inspect the local archive folder."}
                    </p>
                  </li>
                );
              }
              if (item.unreadable === "labels") {
                return (
                  <li key={item.archiveId}>
                    <strong>{item.title}</strong>
                    <p role="status">
                      {de
                        ? "Die gespeicherten Meetingbezeichnungen sind nicht lesbar. Archiv und Bezeichnungen bleiben unverändert erhalten. Wiederherstellen ist erst nach Prüfung der Bezeichnungen möglich."
                        : "The saved meeting labels cannot be read. The archive and the labels are kept unchanged. Restoring is available only after the labels have been checked."}
                    </p>
                  </li>
                );
              }
              return (
                <li key={item.archiveId}>
                  <strong>{item.title}</strong>
                  <p>
                    {receipt.kind === "audio-only"
                      ? de
                        ? "Nur Audio"
                        : "Audio only"
                      : de
                        ? "Ganzes Meeting"
                        : "Whole meeting"}{" "}
                    · {Math.ceil(receipt.archiveBytes / 1024)} KiB ·{" "}
                    {receipt.createdAt}
                  </p>
                  {item.sourceCopyRetained && (
                    <p className="muted">
                      {de
                        ? "Geprüftes ZIP und zusätzliche Originalkopie vorhanden"
                        : "Verified ZIP and additional original copy retained"}
                    </p>
                  )}
                  {item.sourceCopyRetained && api.compactArchive && (
                    <button
                      className="secondary"
                      disabled={busy || running}
                      onClick={() => {
                        setCompact(item);
                        setSelected(null);
                      }}
                    >
                      {de
                        ? "Archiv platzsparend aufbewahren"
                        : "Keep archive compactly"}
                    </button>
                  )}
                  <button
                    className="secondary"
                    disabled={
                      busy ||
                      running ||
                      (receipt.kind === "whole-meeting" && item.sessionPresent)
                    }
                    onClick={() => {
                      setSelected(item);
                      setCompact(null);
                    }}
                  >
                    {de ? "Wiederherstellung prüfen" : "Review restoration"}
                  </button>
                  {item.sessionPresent && receipt.kind === "whole-meeting" && (
                    <p>
                      {de
                        ? "Meeting ist bereits in der normalen Historie vorhanden."
                        : "Meeting already exists in normal history."}
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
          {!page.items.length && (
            <p className="muted">
              {offset
                ? de
                  ? "Keine Archive auf dieser Seite."
                  : "No archives on this page."
                : de
                  ? "Noch keine Archive. Archive entstehen erst, wenn eine Aufbewahrungsregel ein Meeting archiviert."
                  : "No archives yet. Archives are created only when a retention rule archives a meeting."}
            </p>
          )}
          {/* Paging only when there is more than one page. */}
          {(offset > 0 || page.more) && (
            <div className="search-pages">
              <button
                disabled={!offset || busy || running}
                onClick={() => setOffset((n) => Math.max(0, n - 25))}
              >
                {de ? "Zurück" : "Previous"}
              </button>
              <button
                disabled={!page.more || busy || running}
                onClick={() => setOffset((n) => n + 25)}
              >
                {de ? "Weiter" : "Next"}
              </button>
            </div>
          )}
        </>
      )}
      {selected && (
        <section className="retention-confirm">
          <h4>{selected.title}</h4>
          <p>
            {de
              ? "Diese Archivkopie prüfen und wieder in der normalen Historie bzw. als Originalaudio bereitstellen? Bestehende Dateien werden nicht ersetzt."
              : "Verify and restore this archive to normal history or restore its original audio? Existing files will not be replaced."}
          </p>
          <button
            className="secondary"
            disabled={busy || running}
            onClick={() => void restore()}
          >
            {de ? "Geprüfte Kopie wiederherstellen" : "Restore verified copy"}
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
      {compact && (
        <section className="retention-confirm">
          <h4>{compact.title}</h4>
          <p>
            {de
              ? "Das vollständige verlustfreie ZIP erneut prüfen und anschließend nur die überflüssige Rohkopie entfernen? Das ZIP mit Originalaudio, Text und Auswertungen bleibt erhalten. Neue oder geänderte Dateien verhindern diesen Schritt."
              : "Verify the complete lossless ZIP again, then remove only the redundant raw copy? The ZIP containing original audio, text and analyses is retained. New or changed files block this action."}
          </p>
          <button
            className="secondary"
            disabled={busy || running}
            onClick={() => void compactCopy()}
          >
            {de
              ? "Geprüftes ZIP behalten, Rohkopie entfernen"
              : "Keep verified ZIP, remove raw duplicate"}
          </button>
          <button
            className="text-button"
            disabled={busy || running}
            onClick={() => setCompact(null)}
          >
            {de ? "Abbrechen" : "Cancel"}
          </button>
        </section>
      )}
    </section>
  );
}
