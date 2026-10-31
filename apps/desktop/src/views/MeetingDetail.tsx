import { ArrowLeft, Check, Copy, Sparkles, Trash2 } from "lucide-react";
import { meetingLabel } from "../domain";
import type { Meeting, Locale, TranscriptFormat } from "../types";
import type { MessageKey } from "../i18n";
import TranscriptBody from "./TranscriptBody";
import SessionWarnings from "./SessionWarnings";
import MeetingFiles from "./MeetingFiles";
/**
 * Renders one historical meeting's literal source sections. Callers bind copy/export
 * actions to that meeting's ID; displayed section times are not word timestamps.
 */
export default function MeetingDetail({
  meeting,
  locale,
  t,
  busy,
  copied,
  onBack,
  onErase,
  onCopy,
  onAnalyse,
  onOpenFolder,
  onExport,
  savedFile,
  selectedFile,
  speakers,
}: {
  meeting: Meeting;
  locale: Locale;
  t: Record<MessageKey, string>;
  busy: boolean;
  copied: boolean;
  onBack: () => void;
  onErase?: () => void;
  onCopy: () => void;
  onAnalyse: () => void;
  onOpenFolder: () => void;
  onExport: (format: TranscriptFormat) => void;
  savedFile: string | null;
  selectedFile?: string | null;
  speakers?: Record<string, string>;
}) {
  const parts = [...meeting.parts]
    .sort(
      (a, b) =>
        a.startSample - b.startSample || a.source.localeCompare(b.source),
    )
    .filter((part) => (part.text ?? "").trim());
  return (
    <section className="meeting-detail">
      <button className="text-button" onClick={onBack}>
        <ArrowLeft size={15} />
        {t.backHistory}
      </button>
      <div className="detail-heading">
        <div>
          <h2>{meeting.title}</h2>
          <p>{meetingLabel(meeting, locale)}</p>
        </div>
        <div className="detail-actions">
          {onErase && !meeting.id.startsWith("legacy-") && (
            <button
              type="button"
              className="danger"
              disabled={busy}
              onClick={onErase}
            >
              <Trash2 size={16} />
              {locale === "de" ? "Meeting löschen" : "Delete meeting"}
            </button>
          )}
          <button
            className="secondary"
            disabled={busy || !parts.length}
            title={t.copyTooltip}
            onClick={onCopy}
          >
            {copied ? <Check size={16} /> : <Copy size={16} />}
            <span>{copied ? t.copied : t.copy}</span>
          </button>
          <button
            className="primary"
            disabled={busy || meeting.status !== "complete" || !parts.length}
            onClick={onAnalyse}
          >
            <Sparkles size={16} />
            {t.analyzeMeeting}
          </button>
        </div>
      </div>
      {meeting.status !== "complete" && (
        <div className="notice warning">{t.pendingNotes}</div>
      )}
      <div className="meeting-files">
        <MeetingFiles
          t={t}
          disabled={busy}
          onOpenFolder={onOpenFolder}
          onExport={onExport}
          savedFile={savedFile}
        />
      </div>
      <SessionWarnings warnings={meeting.warnings} locale={locale} t={t} />
      <TranscriptBody
        parts={parts}
        locale={locale}
        t={t}
        selectedFile={selectedFile}
        speakers={speakers}
      />
    </section>
  );
}
