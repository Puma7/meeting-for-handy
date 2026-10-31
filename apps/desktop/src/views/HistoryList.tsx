import { FolderClock, RefreshCw, Trash2 } from "lucide-react";
import { clock } from "../domain";
import type { Locale, MeetingSummary } from "../types";
import type { MessageKey } from "../i18n";

/**
 * Shows fallback history for a core without full-text search. Opening a row passes its
 * original meeting ID, including read-only imports, rather than looking up a title.
 */
/** Renders explicit native meeting actions while imported originals remain read-only. */
export default function HistoryList({
  history,
  locale,
  t,
  busy,
  connected,
  onRefresh,
  onOpen,
  onErase,
  statusName,
}: {
  history: MeetingSummary[];
  locale: Locale;
  t: Record<MessageKey, string>;
  busy: boolean;
  connected: boolean;
  onRefresh: () => void;
  onOpen: (id: string) => void;
  onErase?: (meeting: MeetingSummary) => void;
  statusName: (status: string) => string;
}) {
  return (
    <section className="history-panel">
      <div className="section-heading">
        <h2>{t.storage}</h2>
        <button
          className="secondary"
          disabled={busy || !connected}
          onClick={onRefresh}
        >
          <RefreshCw size={15} />
          {t.refresh}
        </button>
      </div>
      {history.length ? (
        <div className="session-list">
          {history.map((item) => (
            <div className="session-row" key={item.id}>
              <div>
                <strong>{item.title}</strong>
                <p>
                  {new Date(item.startedAt).toLocaleString(
                    locale === "de" ? "de-DE" : "en-GB",
                  )}{" "}
                  · {clock(item.durationMs)} · {statusName(item.status)}
                </p>
              </div>
              <div className="session-row-actions">
                {onErase && !item.id.startsWith("legacy-") && (
                  <button
                    type="button"
                    className="icon-button erasure-button"
                    disabled={busy}
                    aria-label={`${locale === "de" ? "Meeting dauerhaft löschen" : "Permanently delete meeting"}: ${item.title}`}
                    title={
                      locale === "de"
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
                  disabled={busy}
                  onClick={() => onOpen(item.id)}
                >
                  {t.open}
                </button>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="empty">
          <FolderClock size={30} />
          <p>{t.emptyHistory}</p>
        </div>
      )}
    </section>
  );
}
