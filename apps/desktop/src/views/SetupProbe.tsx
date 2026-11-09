import { useState } from "react";
import type { DesktopApi, Locale } from "../types";
import { errorMessage } from "../api";

type Result = Awaited<ReturnType<NonNullable<DesktopApi["checkMeetingSetup"]>>>;

export default function SetupProbe({
  run,
  disabled,
  locale,
}: {
  run: () => Promise<Result>;
  disabled: boolean;
  locale: Locale;
}) {
  const de = locale === "de";
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<Result | null>(null);
  const [error, setError] = useState("");
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
          setResult(null);
          setError("");
          setRunning(true);
          try {
            setResult(await run());
          } catch (failure) {
            setError(errorMessage(failure));
          } finally {
            setRunning(false);
          }
        }}
      >
        {de
          ? "Synthetische Funktionsprobe starten"
          : "Run synthetic setup check"}
      </button>
      <p>
        {de
          ? "Prüft mit erzeugtem Audio die Speicherung und Transkriptionssoftware sowie mit Testtext das gewählte lokale Textmodell. Aufnahmegeräte werden dabei nicht geöffnet."
          : "Checks audio storage and transcription software with generated audio, plus the selected local text model with sample text. It does not open recording devices."}
      </p>
      <div role="status">
        {running &&
          (de ? "Funktionsprobe läuft …" : "Setup check in progress …")}
        {result && (
          <ul>
            <li>
              {de ? "Audiospeicherung" : "Audio storage"} ·{" "}
              {verdict(result.audioRoundtrip)}
            </li>
            <li>
              {de ? "Transkriptionssoftware" : "Transcription software"} ·{" "}
              {verdict(result.recognition)}
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
