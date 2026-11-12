import type { Locale } from "../types";
import type { WorkspaceState } from "./WorkspaceBridge";

const RESULT_LIMIT = 12;
type Filter = "open" | "completed" | "decisions" | "questions" | "sources";
type Lookup = { filter: Filter; query: string; visible: number };
export const initialMeetingQuestionLookup: Lookup = {
  filter: "open",
  query: "",
  visible: RESULT_LIMIT,
};

/** Return current saved entries and exact quotes only. This never calls a provider. */
export function meetingAnswers(
  workspace: WorkspaceState | undefined,
  filter: Filter,
  query: string,
  visible = RESULT_LIMIT,
) {
  const excluded = new Set(workspace?.contextExcludedIds);
  const superseded = new Set(workspace?.supersededSources);
  const originals = (workspace?.originals ?? []).filter(
    (source) => !superseded.has(source.id) && source.text.trim(),
  );
  const byId = new Map(originals.map((source) => [source.id, source]));
  const needle = query.trim().toLowerCase();
  const results =
    filter === "sources"
      ? needle
        ? originals
            .filter((source) => source.text.toLowerCase().includes(needle))
            .map((source) => ({
              id: source.id,
              text: "",
              evidence: [{ sourceId: source.id, quote: source.text }],
            }))
        : []
      : (workspace?.entities ?? [])
          .filter((item) => {
            if (excluded.has(item.id)) return false;
            if (filter === "open" || filter === "completed")
              return item.kind === "task" && item.status === filter;
            return filter === "decisions"
              ? item.kind === "decision" && item.status === "recorded"
              : item.kind === "impulse" && item.status === "open";
          })
          .map((item) => ({
            id: item.id,
            text: item.text,
            evidence: item.evidence.filter(
              ({ sourceId, quote }) =>
                quote.trim() && byId.get(sourceId)?.text.includes(quote),
            ),
          }))
          .filter((item) => item.evidence.length > 0);
  return { total: results.length, items: results.slice(0, visible) };
}

export default function MeetingQuestions({
  workspace,
  locale,
  openSource,
  lookup,
  onLookupChange,
}: {
  workspace?: WorkspaceState;
  locale: Locale;
  openSource: (id: string) => void;
  lookup: Lookup;
  onLookupChange: (lookup: Lookup) => void;
}) {
  const de = locale === "de";
  const { filter, query, visible } = lookup;
  const { items, total } = meetingAnswers(workspace, filter, query, visible);
  const choices: Record<Filter, string> = {
    open: de ? "Offene Aufgaben" : "Open tasks",
    completed: de ? "Erledigte Aufgaben" : "Completed tasks",
    decisions: de ? "Beschlüsse" : "Decisions",
    questions: de ? "Offene Fragen" : "Open questions",
    sources: de ? "Originalbelege durchsuchen" : "Search original evidence",
  };
  return (
    <>
      <p>
        {de
          ? "Diese Abfrage durchsucht den gespeicherten Auswertungsstand dieses Meetings auf diesem Gerät."
          : "This lookup searches the saved analysis of this meeting on this device."}
      </p>
      <label>
        {de ? "Was möchtest du finden?" : "What do you want to find?"}
        <select
          value={filter}
          onChange={(event) =>
            onLookupChange({
              ...lookup,
              filter: event.target.value as Filter,
              visible: RESULT_LIMIT,
            })
          }
        >
          {Object.entries(choices).map(([id, label]) => (
            <option key={id} value={id}>
              {label}
            </option>
          ))}
        </select>
      </label>
      {filter === "sources" && (
        <label>
          {de ? "Wort oder genaue Wortfolge" : "Word or exact phrase"}
          <input
            value={query}
            maxLength={160}
            onChange={(event) =>
              onLookupChange({
                ...lookup,
                query: event.target.value,
                visible: RESULT_LIMIT,
              })
            }
          />
        </label>
      )}
      {items.length > 0 ? (
        <>
          <p role="status">
            {de
              ? `${items.length} von ${total} Treffern`
              : `${items.length} of ${total} results`}
          </p>
          <ul className="meeting-answers">
            {items.map((item) => (
              <li key={item.id}>
                {item.text && <strong>{item.text}</strong>}
                {item.evidence.map(({ sourceId, quote }, index) => (
                  <button
                    type="button"
                    className="text-button"
                    key={`${sourceId}-${index}`}
                    onClick={() => openSource(sourceId)}
                  >
                    „{quote}“ ↗
                  </button>
                ))}
              </li>
            ))}
          </ul>
          {items.length < total && (
            <button
              type="button"
              className="text-button"
              onClick={() =>
                onLookupChange({ ...lookup, visible: visible + RESULT_LIMIT })
              }
            >
              {de ? "Weitere Treffer anzeigen" : "Show more results"}
            </button>
          )}
        </>
      ) : (
        <p role="status">
          {filter === "sources" && !query.trim()
            ? de
              ? "Gib einen Suchbegriff ein."
              : "Enter a search term."
            : de
              ? "Im gespeicherten Auswertungsstand gibt es dazu keinen belegten Treffer."
              : "The saved analysis has no grounded result for this lookup."}
        </p>
      )}
    </>
  );
}
