import { useEffect, useState, type ReactNode } from "react";
import type { SavedWorkspace, WorkspaceHistoryListing } from "../types";
import { formatMeetingDue } from "./shell/meetingDisplay.mjs";
import { formatTime } from "./shell/data.mjs";

/** Render a bounded page while keeping every saved result reachable. */
function Pages<T>({
  title,
  rows,
  renderRow,
  english,
}: {
  title: string;
  rows: T[];
  renderRow: (row: T, index: number) => ReactNode;
  english: boolean;
}) {
  const [page, setPage] = useState(0);
  const last = Math.max(0, Math.ceil(rows.length / 25) - 1);
  const shown = Math.min(page, last);
  return (
    <section aria-label={title}>
      <h3>
        {title} <small>({rows.length})</small>
      </h3>
      {rows
        .slice(shown * 25, (shown + 1) * 25)
        .map((row, i) => renderRow(row, shown * 25 + i))}
      {last > 0 && (
        <nav aria-label={title}>
          <button disabled={shown === 0} onClick={() => setPage(shown - 1)}>
            {english ? "Previous" : "Zurück"}
          </button>
          <span>
            {shown + 1} / {last + 1}
          </span>
          <button disabled={shown === last} onClick={() => setPage(shown + 1)}>
            {english ? "Next" : "Weiter"}
          </button>
        </nav>
      )}
    </section>
  );
}

/** Read a selected immutable generation without changing the live adapter, context or recording. */
export default function SavedWorkspaceHistory({
  meetingId,
  currentRunId,
  list,
  load,
  language = "de",
}: {
  meetingId: string;
  currentRunId?: string | null;
  list: (meetingId: string) => Promise<WorkspaceHistoryListing>;
  load: (
    meetingId: string,
    runId: string,
    version: string,
  ) => Promise<SavedWorkspace>;
  language?: "de" | "en";
}) {
  const english = language === "en";
  const [listing, setListing] = useState<WorkspaceHistoryListing | null>(null);
  const [selected, setSelected] = useState("");
  const [saved, setSaved] = useState<SavedWorkspace | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const key = (e: { runId: string; promptVersion: string }) =>
    `${e.runId}/${e.promptVersion}`;
  useEffect(() => {
    let active = true;
    setListing(null);
    setSelected("");
    setSaved(null);
    setError("");
    void list(meetingId)
      .then((value) => {
        if (!active) return;
        setListing(value);
        const first =
          value.entries.find((entry) => entry.runId !== currentRunId) ||
          value.entries[0];
        if (first) setSelected(key(first));
      })
      .catch((reason) => {
        if (active) setError(String(reason));
      });
    return () => {
      active = false;
    };
    // currentRunId can change while polling; an explicitly selected history must stay selected.
  }, [list, meetingId]);
  useEffect(() => {
    const entry = listing?.entries.find((e) => key(e) === selected);
    let active = true;
    setSaved(null);
    setError("");
    if (!entry) {
      setLoading(false);
      return;
    }
    setLoading(true);
    void load(meetingId, entry.runId, entry.promptVersion)
      .then((value) => {
        if (!active) return;
        if (
          value.runId !== entry.runId ||
          value.promptVersion !== entry.promptVersion
        )
          throw new Error(
            english
              ? "The saved view does not match the selection."
              : "Die gespeicherte Ansicht passt nicht zur Auswahl.",
          );
        setSaved(value);
      })
      .catch((reason) => {
        if (active) setError(String(reason));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [load, meetingId, selected, listing, english]);
  const state = saved?.workspace;
  const labels: Record<string, string> = english
    ? {
        task: "Task",
        fact: "Detail",
        decision: "Decision",
        impulse: "Question",
        agenda: "Preparation",
        open: "Open",
        recorded: "Recorded",
        unclear: "Unclear",
        completed: "Completed",
        withdrawn: "Withdrawn",
        discussed: "Discussed",
        touched: "Touched",
      }
    : {
        task: "Aufgabe",
        fact: "Detail",
        decision: "Beschluss",
        impulse: "Gesprächsimpuls",
        agenda: "Vorbereitung",
        open: "Offen",
        recorded: "Festgehalten",
        unclear: "Unklar",
        completed: "Erledigt",
        withdrawn: "Zurückgenommen",
        discussed: "Besprochen",
        touched: "Angerissen",
      };
  const evidence = (items: { sourceId: string; quote: string }[]) =>
    items.length > 0 && (
      <details>
        <summary>{english ? "Original evidence" : "Originalbelege"}</summary>
        {items.map((e, i) => (
          <blockquote key={i}>{e.quote}</blockquote>
        ))}
      </details>
    );
  return (
    <div className="saved-workspace-history">
      <p>
        {english
          ? "Read only · separate from the current analysis."
          : "Nur lesbar · getrennt von der aktuellen Auswertung."}
      </p>
      {error && <p role="alert">{error}</p>}
      {!listing && !error && (
        <p role="status">
          {english
            ? "Loading saved views …"
            : "Gespeicherte Fassungen werden geladen …"}
        </p>
      )}
      {listing && (
        <>
          {listing.unreadableEntries > 0 && (
            <p role="alert">
              {english
                ? "Some saved views could not be read. Their files are retained."
                : "Einige gespeicherte Fassungen konnten nicht gelesen werden. Ihre Dateien bleiben erhalten."}
            </p>
          )}
          {listing.entries.length === 0 ? (
            <p>
              {english
                ? "No saved workspace yet."
                : "Noch keine gespeicherte Meetingbegleitung."}
            </p>
          ) : (
            <label>
              {english ? "Saved view" : "Gespeicherte Fassung"}
              <select
                value={selected}
                onChange={(e) => setSelected(e.target.value)}
              >
                {listing.entries.map((entry, i) => (
                  <option key={key(entry)} value={key(entry)}>
                    {i + 1} ·{" "}
                    {new Date(
                      entry.updatedAt || entry.createdAt,
                    ).toLocaleString(english ? "en-US" : "de-DE")}{" "}
                    · {entry.model} · {entry.entityCount}{" "}
                    {english ? "items" : "Einträge"}
                  </option>
                ))}
              </select>
            </label>
          )}
        </>
      )}
      {loading && (
        <p role="status">
          {english ? "Loading selection …" : "Auswahl wird geladen …"}
        </p>
      )}
      {state && (
        <div key={selected}>
          <Pages
            title={english ? "Saved items" : "Gespeicherte Einträge"}
            rows={state.entities}
            english={english}
            renderRow={(e) => (
              <article key={e.id}>
                <h4>
                  {e.text}{" "}
                  <small>
                    {labels[e.kind] || e.kind} · {labels[e.status] || e.status}
                    {e.humanLocked
                      ? english
                        ? " · manual"
                        : " · manuell"
                      : ""}
                  </small>
                </h4>
                {e.kind === "task" && (
                  <p>
                    {english ? "Owner:" : "Zuständig:"} {e.owner || "—"} ·{" "}
                    {english ? "Due:" : "Termin:"}{" "}
                    {(e.due !== null
                      ? formatMeetingDue(
                          e.due,
                          language,
                          e.evidence.map((v) => v.quote),
                        )
                      : null) || "—"}
                  </p>
                )}
                {evidence(e.evidence)}
              </article>
            )}
          />
          <Pages
            title={
              english ? "Changes in this view" : "Änderungen dieser Fassung"
            }
            rows={state.events}
            english={english}
            renderRow={(e) => (
              <article key={e.id}>
                <h4>
                  {e.text}{" "}
                  <small>
                    {labels[e.status] || e.status}
                    {e.human ? (english ? " · manual" : " · manuell") : ""}
                  </small>
                </h4>
                {evidence(e.evidence)}
              </article>
            )}
          />
          <Pages
            title={
              english
                ? "Preparation and released notes"
                : "Vorbereitung und freigegebene Notizen"
            }
            rows={[...state.context.agenda, ...state.context.sharedNotes]}
            english={english}
            renderRow={(e, i) => <p key={i}>{e.text}</p>}
          />
          <Pages
            title={
              english
                ? "Original transcript of this view"
                : "Originaltranskript dieser Fassung"
            }
            rows={state.originals}
            english={english}
            renderRow={(e) => (
              <article key={e.id}>
                <small>
                  {e.source === "microphone"
                    ? english
                      ? "Microphone"
                      : "Mikrofon"
                    : english
                      ? "Computer audio"
                      : "Computerton"}{" "}
                  · {formatTime(e.startMs / 1000)}–{formatTime(e.endMs / 1000)}
                </small>
                <p>{e.text}</p>
              </article>
            )}
          />
        </div>
      )}
    </div>
  );
}
