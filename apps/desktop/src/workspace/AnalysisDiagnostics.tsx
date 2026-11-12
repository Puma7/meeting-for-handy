import { useEffect, useState } from "react";
import type { LiveNotesStatus, Locale } from "../types";

const phases = new Set([
  "off",
  "starting",
  "waiting",
  "updating",
  "finished",
  "partial",
  "stopped",
  "failed",
]);
const requestResults = new Set([
  "transport",
  "http",
  "contextLimit",
  "envelope",
  "incomplete",
  "validation",
  "persistence",
  "stored",
  "cancelled",
]);
const appVersion =
  /^\d{1,6}\.\d{1,6}\.\d{1,6}(?:-(?:alpha|beta|rc)\.\d{1,6})?$/;

/** Build a new record from safe scalar fields. Never serialize provider or meeting objects. */
export function analysisDiagnostics(
  status: LiveNotesStatus | null,
  pendingAudioMs: number | undefined,
  version: string | null,
  userAgent: string,
) {
  const count = (value: number | null | undefined) =>
    typeof value === "number" && Number.isSafeInteger(value) && value >= 0
      ? value
      : null;
  const request = status?.view?.lastRequest?.request;
  return {
    schema: 1,
    version: version && appVersion.test(version) ? version : null,
    platform: /Windows/.test(userAgent)
      ? "windows"
      : /Macintosh/.test(userAgent)
        ? "macos"
        : /Linux/.test(userAgent)
          ? "linux"
          : null,
    mode:
      status?.view?.mode === "local" || status?.view?.mode === "cloud"
        ? status.view.mode
        : null,
    phase:
      status?.running && status.preparingModel
        ? "modelPreparation"
        : status?.running && status.waitingForTranscript
          ? "transcriptPriority"
          : status?.running && status.waitingForMemory
            ? "memoryPriority"
            : status && phases.has(status.status)
              ? status.status
              : "unknown",
    pendingAudioMs: count(pendingAudioMs),
    pendingTextCharacters: count(status?.pendingChars),
    updateElapsedMs: count(status?.updateElapsedMs),
    responseBytes: count(status?.responseBytes),
    responseIdleMs: count(status?.responseIdleMs),
    lastRequest: request
      ? {
          result: requestResults.has(request.result) ? request.result : null,
          elapsedMs: count(request.elapsedMs),
          httpStatus: count(request.httpStatus),
          promptTokens: count(request.promptTokens),
          completionTokens: count(request.completionTokens),
        }
      : null,
    hasError: Boolean(status?.error),
  };
}

export default function AnalysisDiagnostics({
  status,
  pendingAudioMs,
  locale,
  readVersion,
  copy,
}: {
  status: LiveNotesStatus | null;
  pendingAudioMs?: number;
  locale: Locale;
  readVersion: () => Promise<string>;
  copy: (text: string) => Promise<void>;
}) {
  const [snapshot] = useState(() =>
    analysisDiagnostics(status, pendingAudioMs, null, navigator.userAgent),
  );
  const [version, setVersion] = useState<string | null>(null);
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">(
    "idle",
  );
  useEffect(() => {
    let open = true;
    void readVersion().then(
      (value) => {
        if (open && appVersion.test(value)) setVersion(value);
      },
      () => {},
    );
    return () => {
      open = false;
    };
  }, [readVersion]);
  const de = locale === "de";
  const report = JSON.stringify({ ...snapshot, version }, null, 2);
  return (
    <>
      <p>
        {de
          ? "Vorschau der technischen Angaben. Kopieren legt diesen Stand in die Zwischenablage."
          : "Preview the technical details. Copy puts this snapshot on the clipboard."}
      </p>
      <pre className="workspace-diagnostics-report">{report}</pre>
      <button
        type="button"
        onClick={async () => {
          try {
            await copy(report);
            setCopyState("copied");
          } catch {
            setCopyState("failed");
          }
        }}
      >
        {de ? "Diagnose kopieren" : "Copy diagnostics"}
      </button>
      <p role="status">
        {copyState === "copied" &&
          (de ? "Diagnose kopiert." : "Diagnostics copied.")}
        {copyState === "failed" &&
          (de ? "Kopieren fehlgeschlagen." : "Could not copy diagnostics.")}
      </p>
    </>
  );
}
