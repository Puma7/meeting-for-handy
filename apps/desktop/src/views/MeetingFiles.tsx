import { Download, FolderOpen } from "lucide-react";
import type { MessageKey } from "../i18n";
import { useState } from "react";
import type { TranscriptFormat } from "../types";

/**
 * Offers explicit TXT/Markdown/JSON selection and displays the caller's save receipt. The
 * caller binds folder/export callbacks to the selected meeting.
 */
export default function MeetingFiles({
  t,
  disabled,
  onOpenFolder,
  onExport,
  savedFile,
}: {
  t: Record<MessageKey, string>;
  disabled: boolean;
  onOpenFolder: () => void;
  onExport: (format: TranscriptFormat) => void;
  savedFile: string | null;
}) {
  const [format, setFormat] = useState<TranscriptFormat>("md");
  return (
    <>
      <label className="transcript-format">
        {t.exportFormat}
        <select
          title={t.exportFormatTooltip}
          value={format}
          disabled={disabled}
          onChange={(event) =>
            setFormat(event.target.value as TranscriptFormat)
          }
        >
          <option value="md">Markdown (.md)</option>
          <option value="txt">Text (.txt)</option>
          <option value="json">JSON (.json)</option>
        </select>
      </label>
      <button
        className="secondary"
        disabled={disabled}
        title={t.openFolderHelp}
        onClick={onOpenFolder}
      >
        <FolderOpen size={16} />
        <span>{t.openFolder}</span>
      </button>
      <button
        className="secondary"
        disabled={disabled}
        title={t.exportTranscriptHelp}
        onClick={() => onExport(format)}
      >
        <Download size={16} />
        <span>{t.exportTranscript}</span>
      </button>
      {savedFile && (
        <p className="export-feedback" role="status">
          {t.exportSaved} {savedFile}
        </p>
      )}
      <p className="sr-only file-save-help">{t.savedFilesHelp}</p>
    </>
  );
}
