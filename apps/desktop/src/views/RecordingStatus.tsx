import { useEffect, useRef, useState } from "react";
import { Cpu, TriangleAlert } from "lucide-react";
import { clock } from "../domain";
import type { AudioPart, DesktopApi, Locale, SystemLoad } from "../types";

/** Refresh interval of the processor and memory reading while recording. */
export const SYSTEM_LOAD_MS = 2000;
/** Share of processor or memory at which the reading is highlighted. */
export const HIGH_LOAD_PERCENT = 90;
/** Length of the excerpt of the most recently recognized words. */
export const RECENT_TEXT_CHARS = 100;

type Props = {
  locale: Locale;
  paused: boolean;
  durationMs: number;
  parts: AudioPart[];
  warningCount: number;
  /** The microphone was meant to record, but its track does not. */
  microphoneSilent: boolean;
  microphoneMuted: boolean;
  /** Audio not yet recognized; null when unknown. */
  backlogMs: number | null;
  getSystemLoad?: DesktopApi["getSystemLoad"];
  onOpen: () => void;
};

/** The end of the most recent section with text, cut at a word where possible. */
export function recentText(parts: AudioPart[], limit = RECENT_TEXT_CHARS) {
  const latest = parts
    .filter((part) => (part.text ?? "").trim())
    .reduce<AudioPart | null>(
      (best, part) =>
        !best ||
        part.startSample + part.sampleCount >
          best.startSample + best.sampleCount
          ? part
          : best,
      null,
    );
  const text = (latest?.text ?? "").trim().replace(/\s+/g, " ");
  if (text.length <= limit) return text;
  const tail = text.slice(-limit);
  const space = tail.indexOf(" ");
  return "…" + (space > 0 && space < 20 ? tail.slice(space + 1) : tail);
}

/** Reads the whole computer's load while mounted and the window is visible. */
function useSystemLoad(getSystemLoad?: DesktopApi["getSystemLoad"]) {
  const [load, setLoad] = useState<SystemLoad | null>(null);
  useEffect(() => {
    if (!getSystemLoad) return;
    let disposed = false,
      running = false;
    const read = () => {
      if (disposed || running || document.visibilityState === "hidden") return;
      running = true;
      void getSystemLoad()
        .then((value) => {
          if (!disposed) setLoad(value);
        })
        .catch(() => {
          if (!disposed) setLoad(null);
        })
        .finally(() => {
          running = false;
        });
    };
    read();
    const timer = window.setInterval(read, SYSTEM_LOAD_MS);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [getSystemLoad]);
  return load;
}

/**
 * Compact recording health for every page: running time, whole-computer load, audio
 * notices, recognition backlog and the latest recognized words. It only reads state.
 */
export default function RecordingStatus({
  locale,
  paused,
  durationMs,
  parts,
  warningCount,
  microphoneSilent,
  microphoneMuted,
  backlogMs,
  getSystemLoad,
  onOpen,
}: Props) {
  const de = locale === "de";
  const load = useSystemLoad(getSystemLoad);
  const number = (value: number, digits: number) =>
    value.toLocaleString(de ? "de-DE" : "en-US", {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    });
  const cpu = load?.cpuPercent ?? null;
  const used = load?.memoryUsedBytes ?? null;
  const total = load?.memoryTotalBytes ?? null;
  const memoryPercent = used !== null && total ? (used / total) * 100 : null;
  const gb = (bytes: number) => number(bytes / 1024 ** 3, 1);
  const text = recentText(parts);
  const behind = backlogMs !== null && backlogMs >= 30_000;
  const recordingNotices = de
    ? `${warningCount} ${warningCount === 1 ? "Hinweis" : "Hinweise"} zur Aufnahme`
    : `${warningCount} recording ${warningCount === 1 ? "notice" : "notices"}`;
  const audioNote = microphoneSilent
    ? de
      ? "Mikrofon nimmt nicht auf"
      : "Microphone is not recording"
    : microphoneMuted
      ? de
        ? "Mikrofon stummgeschaltet"
        : "Microphone muted"
      : warningCount > 0
        ? recordingNotices
        : de
          ? "Keine Störung gemeldet"
          : "No problem reported";
  const warning = microphoneSilent || warningCount > 0 || behind;
  const backlogNote = de ? "Erkennung im Rückstand" : "Transcription behind";
  const checkLabel = de ? "Aufnahme prüfen" : "Check recording";
  const warningTitle = `${checkLabel}: ${[
    microphoneSilent || microphoneMuted || warningCount > 0 ? audioNote : null,
    behind ? backlogNote : null,
  ]
    .filter(Boolean)
    .join(" · ")}`;
  const warningMessage = `${checkLabel}: ${[
    microphoneSilent
      ? de
        ? "Mikrofon nimmt nicht auf"
        : "Microphone is not recording"
      : null,
    warningCount > 0 ? recordingNotices : null,
    behind ? backlogNote : null,
  ]
    .filter(Boolean)
    .join(" · ")}`;
  const previousWarnings = useRef({ microphoneSilent, warningCount, behind });
  const [announcement, setAnnouncement] = useState("");
  // Keep the region mounted even in the collapsed sidebar. Only newly reported
  // reasons announce; initial state, time/load polling and removals stay quiet.
  useEffect(() => {
    const previous = previousWarnings.current;
    if (
      (microphoneSilent && !previous.microphoneSilent) ||
      warningCount > previous.warningCount ||
      (behind && !previous.behind)
    )
      setAnnouncement(warningMessage);
    else if (
      microphoneSilent !== previous.microphoneSilent ||
      warningCount !== previous.warningCount ||
      behind !== previous.behind
    )
      setAnnouncement("");
    previousWarnings.current = { microphoneSilent, warningCount, behind };
  }, [microphoneSilent, warningCount, behind, warningMessage]);
  const status = paused
    ? de
      ? "Aufnahme pausiert"
      : "Recording paused"
    : de
      ? "Aufnahme läuft"
      : "Recording";
  return (
    <section
      className="sidebar-recording"
      data-paused={paused || undefined}
      data-warning={warning || undefined}
      aria-label={de ? "Aufnahmestatus" : "Recording status"}
    >
      <button
        type="button"
        className="sidebar-recording-head"
        title={
          warning
            ? warningTitle
            : de
              ? "Zur laufenden Aufnahme"
              : "Go to the recording"
        }
        onClick={onOpen}
      >
        {warning ? (
          <TriangleAlert
            className="recording-warning"
            size={17}
            aria-label={de ? "Aufnahme prüfen" : "Check recording"}
          />
        ) : (
          <span className="recording-pulse" aria-hidden="true" />
        )}
        <span className="sidebar-recording-label">{status}</span>
        <span className="sidebar-recording-time">{clock(durationMs)}</span>
      </button>
      <p
        role="status"
        aria-live="polite"
        aria-atomic="true"
        className="sr-only"
      >
        {announcement}
      </p>
      <dl className="sidebar-recording-facts">
        <div
          title={
            de
              ? "Auslastung des ganzen Computers, nicht nur dieser App"
              : "Load of the whole computer, not only of this app"
          }
        >
          <dt>{de ? "Rechner" : "Computer"}</dt>
          <dd>
            <span data-high={(cpu ?? 0) >= HIGH_LOAD_PERCENT || undefined}>
              CPU {cpu === null ? "—" : `${number(cpu, 0)} %`}
            </span>
            <span
              data-high={(memoryPercent ?? 0) >= HIGH_LOAD_PERCENT || undefined}
            >
              <span>
                RAM{" "}
                {memoryPercent === null ? "—" : `${number(memoryPercent, 0)} %`}
              </span>
            </span>
            <span>GPU —</span>
          </dd>
        </div>
        <div>
          <dt>{de ? "Ton" : "Audio"}</dt>
          <dd data-high={microphoneSilent || warningCount > 0 || undefined}>
            {audioNote}
          </dd>
        </div>
        <div>
          <dt>{de ? "Transkript" : "Transcript"}</dt>
          <dd data-high={behind || undefined}>
            {backlogMs === null
              ? "—"
              : backlogMs > 0
                ? de
                  ? `${clock(Math.ceil(backlogMs / 1000) * 1000)} noch offen`
                  : `${clock(Math.ceil(backlogMs / 1000) * 1000)} pending`
                : de
                  ? "Aktuell"
                  : "Up to date"}
          </dd>
        </div>
      </dl>
      <details className="sidebar-resource-details">
        <summary
          aria-label={de ? "Rechnerdetails" : "Computer details"}
          title={de ? "Rechnerdetails" : "Computer details"}
        >
          <Cpu size={17} aria-hidden="true" />
          {de ? "Rechnerdetails" : "Computer details"}
        </summary>
        <div className="sidebar-resource-content">
          <p>
            {de
              ? "Auslastung des ganzen Computers"
              : "Load of the whole computer"}
          </p>
          <p data-high={(cpu ?? 0) >= HIGH_LOAD_PERCENT || undefined}>
            CPU:{" "}
            {cpu === null
              ? de
                ? "nicht gemessen"
                : "not measured"
              : `${number(cpu, 0)} %${cpu >= HIGH_LOAD_PERCENT ? (de ? " · hohe Auslastung" : " · high load") : ""}`}
          </p>
          <p>
            {de ? "Arbeitsspeicher" : "Memory"}:{" "}
            {used === null || !total
              ? de
                ? "nicht gemessen"
                : "not measured"
              : `${gb(used)}/${gb(total)} GiB`}
          </p>
          <p>
            {de
              ? "GPU-Auslastung wird derzeit nicht gemessen"
              : "GPU usage is currently not measured"}
          </p>
        </div>
      </details>
      <p className="sidebar-recording-text">
        {text
          ? de
            ? `„${text}“`
            : `“${text}”`
          : de
            ? "Noch kein Text erkannt."
            : "No text recognized yet."}
      </p>
    </section>
  );
}
