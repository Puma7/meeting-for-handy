import { useEffect, useRef, useState } from "react";
import type { DesktopApi, Locale } from "../types";
import { errorMessage } from "../api";

type Result = Awaited<ReturnType<NonNullable<DesktopApi["checkMeetingSetup"]>>>;

export default function SetupProbe({
  run,
  cancel,
  disabled,
  locale,
}: {
  run: () => Promise<Result>;
  cancel?: () => Promise<boolean>;
  disabled: boolean;
  locale: Locale;
}) {
  const de = locale === "de";
  const [running, setRunning] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [result, setResult] = useState<Result | null>(null);
  const [error, setError] = useState("");
  const runningRef = useRef(false);
  const cancelRef = useRef(cancel);
  cancelRef.current = cancel;
  useEffect(
    () => () => {
      if (runningRef.current) void cancelRef.current?.().catch(() => {});
    },
    [],
  );
  const verdict = (value: boolean | null) =>
    value === null
      ? de
        ? "übersprungen"
        : "skipped"
      : value
        ? de
          ? "bestanden"
          : "passed"
        : de
          ? "bitte Einrichtung prüfen"
          : "check setup";
  return (
    <div>
      <button
        type="button"
        className="secondary"
        disabled={disabled || running}
        onClick={async () => {
          runningRef.current = true;
          setResult(null);
          setError("");
          setRunning(true);
          try {
            setResult(await run());
          } catch (failure) {
            setError(errorMessage(failure));
          } finally {
            runningRef.current = false;
            setRunning(false);
            setCancelling(false);
          }
        }}
      >
        {de ? "Mit Testdaten prüfen" : "Check with sample data"}
      </button>
      {running && cancel && (
        <button
          type="button"
          className="secondary"
          disabled={cancelling}
          onClick={async () => {
            setCancelling(true);
            try {
              if (!(await cancel())) setCancelling(false);
            } catch (failure) {
              setCancelling(false);
              setError(errorMessage(failure));
            }
          }}
        >
          {de ? "Funktionsprobe abbrechen" : "Cancel setup check"}
        </button>
      )}
      <p>
        {de
          ? "Prüft die Speicherung und Transkriptionssoftware mit einem Testton, das lokale Textmodell mit Beispieltext. Mikrofon und Worterkennung prüfst du mit einer kurzen Aufnahme."
          : "Checks storage and transcription software with a test tone, and the local text model with sample text. Use a short recording to check your microphone and word recognition."}
      </p>
      <div role="status">
        {running &&
          (cancelling
            ? de
              ? "Funktionsprobe wird beendet …"
              : "Stopping setup check …"
            : de
              ? "Funktionsprobe läuft …"
              : "Setup check in progress …")}
        {result && (
          <ul>
            <li>
              {de ? "Audiospeicherung" : "Audio storage"} ·{" "}
              {verdict(result.audioRoundtrip)}
            </li>
            <li>
              {de
                ? "Transkriptionssoftware (Testton)"
                : "Transcription software (test tone)"}{" "}
              · {verdict(result.transcriptionWorker)}
            </li>
            <li>
              {de ? "Lokale Textauswertung" : "Local text analysis"} ·{" "}
              {verdict(result.textAnswer)}
            </li>
          </ul>
        )}
      </div>
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
