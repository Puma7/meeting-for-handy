import { useEffect, useMemo, useRef, useState } from "react";
import {
  FolderClock,
  RefreshCw,
  Search,
  Pin,
  ArrowRight,
  Trash2,
} from "lucide-react";
import { errorMessage } from "../api";
import { clock, trackName } from "../domain";
import SourceAudioPreview from "../workspace/SourceAudioPreview";
import HistoryQuestions, {
  type HistoryModel,
  type HistoryQuestionResult,
} from "./HistoryQuestions";
import type {
  DesktopApi,
  HistoryCitation,
  Locale,
  MeetingSummary,
  SearchPage,
  SearchScope,
} from "../types";

type HistorySearchState = {
  query: string;
  from: string;
  through: string;
  pinnedOnly: boolean;
  transcriptOnly: boolean;
  offset: number;
};

export type HistoryQuestionReturn = HistoryQuestionResult & { key: string };

const PAGE_SIZE = 25;

export const initialHistorySearch: HistorySearchState = {
  query: "",
  from: "",
  through: "",
  pinnedOnly: false,
  transcriptOnly: false,
  offset: 0,
};

function lastPageOffset(total: number) {
  return Math.floor(Math.max(0, total - 1) / PAGE_SIZE) * PAGE_SIZE;
}

/** Use local calendar days, including the shorter or longer days around a clock change. */
function dayBoundary(value: string, nextDay = false) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return NaN;
  const date = new Date(`${value}T00:00:00`);
  if (nextDay) date.setDate(date.getDate() + 1);
  return date.getUTCFullYear() >= 1 && date.getUTCFullYear() <= 9999
    ? date.getTime()
    : NaN;
}

function inScope(meeting: MeetingSummary, scope: SearchScope | undefined) {
  if (scope?.pinnedOnly && !meeting.userMetadata?.pinned) return false;
  const started = Date.parse(meeting.startedAt);
  return (
    (scope?.fromMs === undefined || started >= scope.fromMs) &&
    (scope?.beforeMs === undefined || started < scope.beforeMs)
  );
}

/** Preserve SQLite's pinned-descending, stored-date-descending, ID-ascending
 * order without locale-dependent collation or changing the supplied summaries. */
function historyOrder(a: MeetingSummary, b: MeetingSummary): number {
  const pinned =
    Number(b.userMetadata?.pinned ?? false) -
    Number(a.userMetadata?.pinned ?? false);
  if (pinned) return pinned;
  if (a.startedAt !== b.startedAt) return a.startedAt > b.startedAt ? -1 : 1;
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

/** Renders source-bound search hits and deletion actions without restoring blocked meeting rows. */
export default function HistorySearch({
  api,
  locale,
  onOpen,
  imported = [],
  nativeMeetings,
  onRefresh,
  onErase,
  hiddenMeetingIds,
  initialState = initialHistorySearch,
  onStateChange,
  historyModel,
  questionReturn,
  onOpenCitation,
}: {
  api: DesktopApi;
  onErase?: (meeting: MeetingSummary) => void;
  hiddenMeetingIds?: ReadonlySet<string>;
  locale: Locale;
  onOpen: (id: string, sourceId: string | null) => void;
  /** Imported legacy meetings are read in place and are not part of the search index. */
  imported?: MeetingSummary[];
  /** Authoritative native summaries remain complete when the derived search cache is not. */
  nativeMeetings?: MeetingSummary[];
  onRefresh?: () => void;
  initialState?: HistorySearchState;
  onStateChange?: (state: HistorySearchState) => void;
  historyModel?: HistoryModel;
  questionReturn?: HistoryQuestionReturn;
  onOpenCitation?: (
    citation: HistoryCitation,
    result: HistoryQuestionReturn,
  ) => void;
}) {
  const de = locale === "de";
  const [state, setState] = useState(initialState);
  const { query, from, through, pinnedOnly, transcriptOnly, offset } = state;
  function updateSearch(patch: Partial<HistorySearchState>) {
    const next = { ...state, offset: 0, ...patch };
    setState(next);
    onStateChange?.(next);
  }
  const [result, setResult] = useState<{
      key: string;
      page: SearchPage;
    } | null>(null),
    [error, setError] = useState<string | null>(null),
    [busy, setBusy] = useState(false),
    [refresh, setRefresh] = useState(0);
  const generation = useRef(0);
  const hasQuery = Boolean(query.trim());
  const fromMs = from ? dayBoundary(from) : undefined;
  const beforeMs = through ? dayBoundary(through, true) : undefined;
  const invalidDates =
    Number.isNaN(fromMs) ||
    Number.isNaN(beforeMs) ||
    (fromMs !== undefined && beforeMs !== undefined && fromMs >= beforeMs);
  const evidenceOnly = transcriptOnly && hasQuery;
  const scope = useMemo(
    () =>
      fromMs !== undefined ||
      beforeMs !== undefined ||
      pinnedOnly ||
      evidenceOnly
        ? { fromMs, beforeMs, pinnedOnly, transcriptOnly: evidenceOnly }
        : undefined,
    [fromMs, beforeMs, pinnedOnly, evidenceOnly],
  );
  const requestKey = JSON.stringify([query, offset, scope]);
  const questionKey = JSON.stringify([
    from,
    through,
    pinnedOnly,
    evidenceOnly,
    historyModel?.endpoint,
    historyModel?.model,
  ]);
  const authoritative = !hasQuery && nativeMeetings !== undefined;
  const native = authoritative
    ? (nativeMeetings ?? [])
        .filter(
          (item) => !hiddenMeetingIds?.has(item.id) && inScope(item, scope),
        )
        .sort(historyOrder)
    : [];
  const viewOffset = authoritative
    ? Math.min(offset, lastPageOffset(native.length))
    : offset;
  const indexedPage = result?.key === requestKey ? result.page : null;
  const indexedLastOffset = indexedPage
    ? lastPageOffset(indexedPage.total)
    : offset;
  const recoveringPage =
    !authoritative && !invalidDates && !busy && offset > indexedLastOffset;
  const visiblePage: SearchPage | null =
    invalidDates || recoveringPage
      ? null
      : authoritative
        ? {
            items: native
              .slice(viewOffset, viewOffset + PAGE_SIZE)
              .map((item) => ({
                ...item,
                pinned: item.userMetadata?.pinned ?? false,
                audioRemoved: item.userMetadata?.audioRemoved ?? false,
                metadataRevision: item.userMetadata?.revision ?? 0,
                sourceId: null,
                startMs: null,
                endMs: null,
                excerpt: "",
              })),
            total: native.length,
            offset: viewOffset,
            limit: PAGE_SIZE,
          }
        : indexedPage;
  const visibleImports =
    !hasQuery && !invalidDates
      ? imported.filter(
          (item) => !hiddenMeetingIds?.has(item.id) && inScope(item, scope),
        )
      : [];
  const loading = !authoritative && !invalidDates && (busy || recoveringPage);
  useEffect(() => {
    if (!recoveringPage) return;
    // Deletion or an index refresh can remove the current last page.
    const next = { ...state, offset: indexedLastOffset };
    setState(next);
    onStateChange?.(next);
  }, [recoveringPage, indexedLastOffset, state, onStateChange]);
  useEffect(() => {
    const update = () => setRefresh((v) => v + 1);
    window.addEventListener("library-index-updated", update);
    return () => window.removeEventListener("library-index-updated", update);
  }, []);
  useEffect(() => {
    const current = ++generation.current;
    const searchMeetings = api.searchMeetings;
    if (authoritative || invalidDates || !searchMeetings) {
      setBusy(false);
      setError(null);
      return () => {
        generation.current++;
      };
    }
    setBusy(true);
    setError(null);
    const timer = setTimeout(
      () => {
        const search = scope
          ? searchMeetings.call(api, query, offset, PAGE_SIZE, scope)
          : searchMeetings.call(api, query, offset, PAGE_SIZE);
        void search
          .then((value) => {
            if (generation.current === current)
              setResult({ key: requestKey, page: value });
          })
          .catch((e) => {
            if (generation.current === current) setError(errorMessage(e));
          })
          .finally(() => {
            if (generation.current === current) setBusy(false);
          });
      },
      query ? 250 : 0,
    );
    return () => {
      generation.current++;
      clearTimeout(timer);
    };
  }, [
    api,
    query,
    offset,
    refresh,
    authoritative,
    invalidDates,
    scope,
    requestKey,
  ]);
  return (
    <section className="history-search">
      <div className="section-heading">
        <h2>
          <FolderClock size={18} />
          {de ? "Meetings durchsuchen" : "Search meetings"}
        </h2>
        <button
          className="secondary"
          title={
            de
              ? "Gespeicherte Meetings und Suchergebnisse neu laden."
              : "Reload saved meetings and search results."
          }
          disabled={loading}
          onClick={() => {
            onRefresh?.();
            setRefresh((v) => v + 1);
          }}
        >
          <RefreshCw size={15} />
          {de ? "Aktualisieren" : "Refresh"}
        </button>
      </div>
      <label className="search-field">
        <span>
          <Search size={15} />
          {de
            ? "Suche in Meetingnamen und Transkripten"
            : "Search meeting names and transcripts"}
        </span>
        <input
          title={
            de
              ? "Titel und gespeicherte Transkripte durchsuchen."
              : "Search titles and saved transcripts."
          }
          type="search"
          maxLength={200}
          value={query}
          onChange={(e) => updateSearch({ query: e.target.value })}
          placeholder={
            de ? "Begriff, Name oder Thema …" : "Word, name or topic …"
          }
        />
      </label>
      <fieldset className="search-filters">
        <legend>{de ? "Suche eingrenzen" : "Narrow the search"}</legend>
        <label>
          {de ? "Von" : "From"}
          <input
            type="date"
            value={from}
            onChange={(e) => updateSearch({ from: e.target.value })}
          />
        </label>
        <label>
          {de ? "Bis einschließlich" : "Through"}
          <input
            type="date"
            value={through}
            onChange={(e) => updateSearch({ through: e.target.value })}
          />
        </label>
        <label className="search-filter-check">
          <input
            type="checkbox"
            checked={pinnedOnly}
            onChange={(e) => updateSearch({ pinnedOnly: e.target.checked })}
          />
          {de ? "Nur VIP-Meetings" : "VIP meetings only"}
        </label>
        <label className="search-filter-check">
          <input
            type="checkbox"
            checked={evidenceOnly}
            disabled={!hasQuery}
            onChange={(e) => updateSearch({ transcriptOnly: e.target.checked })}
          />
          {de ? "Nur Treffer im Transkript" : "Transcript matches only"}
        </label>
      </fieldset>
      {api.askMeetingHistory && onOpenCitation && (
        <HistoryQuestions
          key={questionKey}
          ask={api.askMeetingHistory}
          locale={locale}
          model={historyModel}
          choices={nativeMeetings
            ?.filter((item) => inScope(item, scope))
            .sort(historyOrder)}
          scope={scope}
          invalidDates={invalidDates}
          hiddenMeetingIds={hiddenMeetingIds}
          initialResult={
            questionReturn?.key === questionKey ? questionReturn : undefined
          }
          onOpen={(citation, result) =>
            onOpenCitation(citation, { ...result, key: questionKey })
          }
        />
      )}
      {invalidDates && (
        <p role="alert" className="inline-error">
          {de ? "Wähle einen gültigen Zeitraum." : "Choose a valid date range."}
        </p>
      )}
      <div className="search-state" role="status">
        {loading
          ? de
            ? "Suchergebnisse werden geladen …"
            : "Loading search results …"
          : visiblePage
            ? `${visiblePage.total.toLocaleString(locale)} ${authoritative ? "Meetings" : de ? "Treffer" : "results"}`
            : ""}
      </div>
      {!authoritative && !invalidDates && error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <div className="session-list" aria-busy={loading}>
        {visiblePage?.items
          .filter((item) => !hiddenMeetingIds?.has(item.id))
          .map((item, i) => (
            <article
              className="session-row search-result"
              key={item.id + ":" + item.sourceId + ":" + i}
            >
              <div>
                <strong>
                  {item.pinned && <Pin size={14} aria-label="VIP" />}
                  {item.title}
                </strong>
                <p>
                  {new Date(item.startedAt).toLocaleString(locale)} ·{" "}
                  {clock(item.durationMs)}
                </p>
                {item.excerpt && (
                  <p className="search-excerpt">{item.excerpt}</p>
                )}
                {item.sourceId && (
                  <small>
                    {de ? "Originalabschnitt" : "Original section"}:{" "}
                    {clock(item.startMs ?? 0)}–{clock(item.endMs ?? 0)}
                    {/(?:^|[/\\])system-/.test(item.sourceId) &&
                      ` · ${trackName("system", locale)}`}
                    {/(?:^|[/\\])microphone-/.test(item.sourceId) &&
                      ` · ${trackName("microphone", locale)}`}
                    {api.getTranscriptAudio &&
                      !item.audioRemoved &&
                      item.startMs !== null &&
                      item.endMs !== null &&
                      item.endMs - item.startMs > 30_000 &&
                      (de
                        ? " · Die Audiovorschau spielt die ersten 30 Sekunden des Abschnitts."
                        : " · Audio preview plays the first 30 seconds of the section.")}
                  </small>
                )}
              </div>
              <div className="session-row-actions">
                {api.getTranscriptAudio &&
                  item.sourceId &&
                  !item.audioRemoved && (
                    <SourceAudioPreview
                      sourceId={item.sourceId}
                      locale={locale}
                      load={(sourceId) =>
                        api.getTranscriptAudio!(item.id, sourceId)
                      }
                    />
                  )}
                {onErase && !item.id.startsWith("legacy-") && (
                  <button
                    type="button"
                    className="icon-button erasure-button"
                    aria-label={`${de ? "Meeting dauerhaft löschen" : "Permanently delete meeting"}: ${item.title}`}
                    title={
                      de
                        ? "Meeting dauerhaft löschen"
                        : "Permanently delete meeting"
                    }
                    onClick={() => onErase(item)}
                  >
                    <Trash2 size={16} />
                  </button>
                )}
                <button
                  className="secondary"
                  title={
                    de
                      ? "Das Meeting am gefundenen Abschnitt öffnen, falls vorhanden."
                      : "Open the meeting at the matching section when available."
                  }
                  onClick={() => onOpen(item.id, item.sourceId)}
                >
                  <ArrowRight size={15} />
                  {de ? "Öffnen" : "Open"}
                </button>
              </div>
            </article>
          ))}
        {!loading && visiblePage?.total === 0 && (
          <p className="empty">
            {authoritative
              ? scope
                ? de
                  ? "Keine Meetings für diese Filter."
                  : "No meetings match these filters."
                : de
                  ? "Keine Meetings vorhanden."
                  : "No meetings yet."
              : query
                ? de
                  ? "Keine Treffer. Prüfe bei Bedarf den Suchindex unter Einstellungen."
                  : "No matches. Check the search index in Settings if needed."
                : de
                  ? "Der lokale Suchindex wird beim Start aktualisiert. Vorhandene Sitzungen bleiben erhalten."
                  : "The local index updates at startup. Existing sessions are preserved."}
          </p>
        )}
      </div>
      {visibleImports.length > 0 && (
        <div className="session-list">
          <h3>
            {de
              ? "Importierte ältere Meetings · nicht durchsuchbar"
              : "Imported earlier meetings · not searchable"}
          </h3>
          {visibleImports.map((item) => (
            <article className="session-row" key={item.id}>
              <div>
                <strong>{item.title}</strong>
                <p>
                  {new Date(item.startedAt).toLocaleString(locale)} ·{" "}
                  {clock(item.durationMs)}
                </p>
              </div>
              <button
                className="secondary"
                title={
                  de
                    ? "Die gespeicherte Sitzung öffnen."
                    : "Open the saved session."
                }
                onClick={() => onOpen(item.id, null)}
              >
                <ArrowRight size={15} />
                {de ? "Öffnen" : "Open"}
              </button>
            </article>
          ))}
        </div>
      )}
      {visiblePage && visiblePage.total > PAGE_SIZE && (
        <div className="search-pages">
          <button
            className="secondary"
            disabled={loading || viewOffset === 0}
            title={
              de ? "Die vorherige Seite anzeigen." : "Show the previous page."
            }
            onClick={() =>
              updateSearch({ offset: Math.max(0, viewOffset - PAGE_SIZE) })
            }
          >
            {de ? "Zurück" : "Previous"}
          </button>
          <span>
            {viewOffset + 1}–
            {Math.min(viewOffset + PAGE_SIZE, visiblePage.total)}
          </span>
          <button
            className="secondary"
            disabled={loading || viewOffset + PAGE_SIZE >= visiblePage.total}
            title={de ? "Die nächste Seite anzeigen." : "Show the next page."}
            onClick={() => updateSearch({ offset: viewOffset + PAGE_SIZE })}
          >
            {de ? "Weiter" : "Next"}
          </button>
        </div>
      )}
    </section>
  );
}
