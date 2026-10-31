import { useEffect, useRef, useState } from "react";
import {
  FolderClock,
  RefreshCw,
  Search,
  Pin,
  ArrowRight,
  Trash2,
} from "lucide-react";
import { errorMessage } from "../api";
import { clock } from "../domain";
import type { DesktopApi, Locale, MeetingSummary, SearchPage } from "../types";

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
}) {
  const de = locale === "de";
  const [query, setQuery] = useState(""),
    [offset, setOffset] = useState(0),
    [page, setPage] = useState<SearchPage | null>(null),
    [error, setError] = useState<string | null>(null),
    [busy, setBusy] = useState(false),
    [refresh, setRefresh] = useState(0);
  const generation = useRef(0);
  const authoritative = !query.trim() && nativeMeetings !== undefined;
  const native = authoritative
    ? (nativeMeetings ?? [])
        .filter((item) => !hiddenMeetingIds?.has(item.id))
        .sort(historyOrder)
    : [];
  const viewOffset = authoritative
    ? Math.min(offset, Math.max(0, Math.floor((native.length - 1) / 25) * 25))
    : offset;
  const visiblePage: SearchPage | null = authoritative
    ? {
        items: native.slice(viewOffset, viewOffset + 25).map((item) => ({
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
        limit: 25,
      }
    : page;
  const loading = !authoritative && busy;
  useEffect(() => {
    const update = () => setRefresh((v) => v + 1);
    window.addEventListener("library-index-updated", update);
    return () => window.removeEventListener("library-index-updated", update);
  }, []);
  useEffect(() => {
    const current = ++generation.current;
    if (authoritative) {
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
        void api
          .searchMeetings?.(query, offset, 25)
          .then((value) => {
            if (generation.current === current) setPage(value);
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
  }, [api, query, offset, refresh, authoritative]);
  return (
    <section className="history-search">
      <div className="section-heading">
        <h2>
          <FolderClock size={18} />
          {de ? "Meetings durchsuchen" : "Search meetings"}
        </h2>
        <button
          className="secondary"
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
          type="search"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setOffset(0);
          }}
          placeholder={
            de ? "Begriff, Name oder Thema …" : "Word, name or topic …"
          }
        />
      </label>
      <div className="search-state" role="status">
        {loading
          ? de
            ? "Suchergebnisse werden geladen …"
            : "Loading search results …"
          : visiblePage
            ? `${visiblePage.total.toLocaleString(locale)} ${authoritative ? "Meetings" : de ? "Treffer" : "results"}`
            : ""}
      </div>
      {!authoritative && error && (
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
                  </small>
                )}
              </div>
              <div className="session-row-actions">
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
              ? de
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
      {!query.trim() && imported.length > 0 && (
        <div className="session-list">
          <h3>
            {de
              ? "Importierte ältere Meetings · nicht durchsuchbar"
              : "Imported earlier meetings · not searchable"}
          </h3>
          {imported.map((item) => (
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
                onClick={() => onOpen(item.id, null)}
              >
                <ArrowRight size={15} />
                {de ? "Öffnen" : "Open"}
              </button>
            </article>
          ))}
        </div>
      )}
      {visiblePage && visiblePage.total > 25 && (
        <div className="search-pages">
          <button
            className="secondary"
            disabled={loading || viewOffset === 0}
            onClick={() => setOffset(Math.max(0, viewOffset - 25))}
          >
            {de ? "Zurück" : "Previous"}
          </button>
          <span>
            {viewOffset + 1}–{Math.min(viewOffset + 25, visiblePage.total)}
          </span>
          <button
            className="secondary"
            disabled={loading || viewOffset + 25 >= visiblePage.total}
            onClick={() => setOffset(viewOffset + 25)}
          >
            {de ? "Weiter" : "Next"}
          </button>
        </div>
      )}
    </section>
  );
}
