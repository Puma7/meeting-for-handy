import { ChevronLeft, ChevronRight, X } from "lucide-react";
import { useState } from "react";
import { clock, trackName } from "../domain";
import type { TranscriptSource, Locale } from "../types";
import type { MessageKey } from "../i18n";
export type SourceSelection = {
  sources: TranscriptSource[];
  index: number;
  quote: string;
};
export default function SourceDialog({
  selection,
  locale,
  t,
  onClose,
}: {
  selection: SourceSelection;
  locale: Locale;
  t: Record<MessageKey, string>;
  onClose: () => void;
}) {
  const [index, setIndex] = useState(selection.index);
  const source = selection.sources[index];
  const quote = index === selection.index ? selection.quote : "";
  if (!source) return null;
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <section
        className="dialog source-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="source-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="section-heading">
          <h2 id="source-title">{t.evidence}</h2>
          <button
            className="icon-button"
            aria-label={t.close}
            title={t.close}
            onClick={onClose}
          >
            <X size={19} />
          </button>
        </div>
        <p className="muted">{t.sourceSnapshot}</p>
        <div className="source-navigation">
          <button
            className="secondary"
            aria-label={t.previousSource}
            title={t.previousSource}
            disabled={index === 0}
            onClick={() => setIndex(index - 1)}
          >
            <ChevronLeft size={17} />
          </button>
          <label>
            {t.sourceSection}
            <select
              value={index}
              onChange={(event) => setIndex(Number(event.target.value))}
            >
              {selection.sources.map((item, position) => (
                <option key={item.id} value={position}>
                  {position + 1}. {clock(item.startMs)}–{clock(item.endMs)} ·{" "}
                  {trackName(item.source, locale)}
                </option>
              ))}
            </select>
          </label>
          <button
            className="secondary"
            aria-label={t.nextSource}
            title={t.nextSource}
            disabled={index === selection.sources.length - 1}
            onClick={() => setIndex(index + 1)}
          >
            <ChevronRight size={17} />
          </button>
        </div>
        <p className="source-text">
          {quote
            ? source.text.split(quote).map((part, position, pieces) => (
                <span key={position}>
                  {part}
                  {position < pieces.length - 1 && <mark>{quote}</mark>}
                </span>
              ))
            : source.text}
        </p>
        <details className="source-identifier">
          <summary>{locale === "de" ? "Quellenkennung" : "Source ID"}</summary>
          <code>{source.id}</code>
        </details>
      </section>
    </div>
  );
}
