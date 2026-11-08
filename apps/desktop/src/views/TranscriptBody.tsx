import { AudioLines, ArrowDownUp, Search } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { clock, trackName } from "../domain";
import type { AudioPart, Locale } from "../types";
import type { MessageKey } from "../i18n";
import { useTranscriptOrder } from "../useTranscriptOrder";

/**
 * Renders all retained transcript sections and searches their literal text without markup.
 * Sections can be read oldest or newest first; search highlights use original indices.
 */
export default function TranscriptBody({
  parts,
  locale,
  t,
  selectedFile,
  speakers = {},
}: {
  parts: AudioPart[];
  locale: Locale;
  t: Record<MessageKey, string>;
  selectedFile?: string | null;
  /** Confirmed speaker labels by source ID; shown beside, never inside, the original text. */
  speakers?: Record<string, string>;
}) {
  const de = locale === "de";
  const [query, setQuery] = useState("");
  const needle = query.trim();
  // Search filters the displayed sections only; the transcript itself is never changed.
  const matching = needle
    ? parts.filter((part) => searchPattern(needle).test(part.text ?? ""))
    : parts;
  const [newestFirst, chooseOrder] = useTranscriptOrder();
  const shown = newestFirst ? [...matching].reverse() : matching;
  const scrolledFile = useRef<string | null>(null);
  useEffect(() => {
    if (!selectedFile) {
      scrolledFile.current = null;
      return;
    }
    if (scrolledFile.current === selectedFile) return;
    const target = document.getElementById(
      "transcript-part-" + encodeURIComponent(selectedFile),
    );
    if (target) {
      target.scrollIntoView?.({ block: "center" });
      scrolledFile.current = selectedFile;
    }
  }, [selectedFile, parts]);
  return (
    <>
      {parts.length > 0 && (
        <div className="transcript-search">
          <label>
            <Search size={14} aria-hidden="true" />
            <input
              title={t.transcriptSearchTooltip}
              type="search"
              value={query}
              placeholder={de ? "Im Transkript suchen" : "Search transcript"}
              aria-label={de ? "Im Transkript suchen" : "Search transcript"}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          {parts.length > 1 && (
            <button
              type="button"
              className="secondary transcript-order"
              aria-pressed={newestFirst}
              title={
                de
                  ? "Reihenfolge der Abschnitte umschalten"
                  : "Switch the order of the sections"
              }
              onClick={() => chooseOrder(!newestFirst)}
            >
              <ArrowDownUp size={14} aria-hidden="true" />
              {de ? "Neueste oben" : "Newest first"}
            </button>
          )}
          {needle && (
            <span role="status" className="muted">
              {de
                ? `${shown.length} von ${parts.length} Abschnitten`
                : `${shown.length} of ${parts.length} sections`}
            </span>
          )}
        </div>
      )}
      <div
        className="transcript-content"
        role="region"
        aria-label={t.live}
        tabIndex={0}
        aria-live={needle ? "off" : "polite"}
      >
        {shown.length ? (
          shown.map((part, index) => (
            <article
              className="transcript-part"
              id={
                part.file
                  ? "transcript-part-" + encodeURIComponent(part.file)
                  : undefined
              }
              data-highlighted={
                (Boolean(selectedFile) && part.file === selectedFile) ||
                undefined
              }
              key={part.file ?? part.source + part.startSample + index}
            >
              <div className="part-meta">
                <span title={t.sourceTimes}>
                  {clock(part.startSample / 16)}–
                  {clock((part.startSample + part.sampleCount) / 16)}
                </span>
                <span>{trackName(part.source, locale)}</span>
                {part.file && speakers[part.file] && (
                  <span
                    className="speaker-label"
                    title={
                      de
                        ? "Von dir bestätigte Sprecherzuordnung"
                        : "Speaker label you confirmed"
                    }
                  >
                    {speakers[part.file]}
                  </span>
                )}
                {part.status !== "complete" && <span>{t.pending}</span>}
              </div>
              <p>{highlight(part.text ?? "", needle)}</p>
            </article>
          ))
        ) : needle ? (
          <p className="muted">
            {de
              ? "Kein Abschnitt enthält diesen Text."
              : "No section contains this text."}
          </p>
        ) : (
          <div className="empty">
            <AudioLines size={30} />
            <p>{t.emptyTranscript}</p>
          </div>
        )}
      </div>
    </>
  );
}
/** Marks case-insensitive matches; the text is rendered literally, never as markup. */
function highlight(text: string, needle: string): ReactNode {
  if (!needle) return text;
  const pieces: ReactNode[] = [];
  let from = 0;
  for (const match of text.matchAll(searchPattern(needle, true))) {
    const at = match.index;
    if (at > from) pieces.push(text.slice(from, at));
    pieces.push(<mark key={at}>{match[0]}</mark>);
    from = at + match[0].length;
  }
  if (from < text.length) pieces.push(text.slice(from));
  return pieces;
}
/** Escape all pattern syntax: transcript searches remain literal and use original indices. */
function searchPattern(needle: string, all = false) {
  return new RegExp(
    needle.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
    all ? "giu" : "iu",
  );
}
