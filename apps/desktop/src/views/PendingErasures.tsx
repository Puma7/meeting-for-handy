import { useEffect, useState } from "react";
import { errorMessage } from "../api";
import type {
  DesktopApi,
  Locale,
  MeetingSummary,
  PendingErasure,
} from "../types";

/** Keeps interrupted explicit deletion discoverable even after its original manifest is gone. */
export default function PendingErasures({
  api,
  locale,
  revision,
  onResume,
}: {
  api: DesktopApi;
  locale: Locale;
  revision: number;
  onResume: (meeting: MeetingSummary) => void;
}) {
  const [rows, setRows] = useState<PendingErasure[]>([]),
    [error, setError] = useState<string | null>(null);
  const de = locale === "de";
  useEffect(() => {
    let current = true;
    void api
      .listPendingErasures?.()
      .then((value) => {
        if (current) {
          setRows(value);
          setError(null);
        }
      })
      .catch((failure) => {
        if (current) setError(errorMessage(failure));
      });
    return () => {
      current = false;
    };
  }, [api, revision]);
  if (!rows.length && !error) return null;
  return (
    <section
      className="notice warning pending-erasures"
      aria-label={de ? "Unterbrochene Löschungen" : "Interrupted deletions"}
    >
      <h3>
        {rows.length > 0
          ? de
            ? "Löschung noch nicht vollständig abgeschlossen"
            : "Deletion is not yet complete"
          : de
            ? "Unterbrochene Löschungen konnten nicht gelesen werden"
            : "Interrupted deletions could not be read"}
      </h3>
      {rows.length > 0 && (
        <p>
          {de
            ? "Verbleibende Daten können erneut geprüft und nach Bestätigung entfernt werden."
            : "Inspect remaining data and confirm again to finish removing it."}
        </p>
      )}
      {error && <p role="alert">{error}</p>}
      {rows.map((row) => (
        <div className="session-row" key={row.meetingId}>
          <span>
            {de ? "Bestätigt am " : "Confirmed on "}
            {new Date(row.confirmedAt).toLocaleString(locale)}
          </span>
          <button
            className="secondary"
            onClick={() =>
              onResume({
                id: row.meetingId,
                title: de
                  ? "Unterbrochener Löschauftrag"
                  : "Interrupted deletion",
                startedAt: row.confirmedAt,
                durationMs: 0,
                status: "erasure-pending",
                modelId: "",
              })
            }
          >
            {de ? "Prüfen und fortsetzen" : "Inspect and continue"}
          </button>
        </div>
      ))}
    </section>
  );
}
