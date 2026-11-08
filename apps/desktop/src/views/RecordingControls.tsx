import { Mic, MicOff, Pause, Play } from "lucide-react";
import type { CaptureControl, Locale } from "../types";

/**
 * Renders pause/mute from acknowledged native state and emits requested changes without
 * optimistic success. Microphone controls apply only to this app's track.
 */
export default function RecordingControls({
  control,
  locale,
  disabled,
  onPause,
  onMute,
}: {
  control: CaptureControl;
  locale: Locale;
  disabled: boolean;
  onPause: (paused: boolean) => void;
  onMute: (muted: boolean) => void;
}) {
  const de = locale === "de";
  // Shown on hover instead of a permanent paragraph below the buttons.
  const help = de
    ? "Pause und App-Stummschaltung ersetzen betroffenen Ton durch Stille. Je nach Plattform können Geräte geöffnet bleiben; die Aufnahmeanzeige des Betriebssystems kann weiterhin sichtbar sein. Teams, Zoom und andere Apps werden dadurch nicht stummgeschaltet."
    : "Pause and app mute replace affected audio with silence. Depending on the platform, devices can remain open and the operating system may still show its recording indicator. This does not mute Teams, Zoom or other apps.";
  return (
    <div
      className="capture-control-panel"
      aria-label={
        de ? "Laufende Aufnahme steuern" : "Control current recording"
      }
    >
      <div className="capture-control-buttons">
        <button
          className="secondary"
          disabled={disabled}
          aria-pressed={control.paused}
          title={help}
          onClick={() => onPause(!control.paused)}
        >
          {control.paused ? <Play size={16} /> : <Pause size={16} />}
          {control.paused
            ? de
              ? "Aufnahme fortsetzen"
              : "Resume recording"
            : de
              ? "Aufnahme pausieren"
              : "Pause recording"}
        </button>
        {control.hasMicrophone && (
          <button
            className="secondary"
            disabled={disabled}
            aria-pressed={control.microphoneMuted}
            title={help}
            onClick={() => onMute(!control.microphoneMuted)}
          >
            {control.microphoneMuted ? <MicOff size={16} /> : <Mic size={16} />}
            {control.microphoneMuted
              ? de
                ? "App-Mikrofon einschalten"
                : "Unmute app microphone"
              : de
                ? "App-Mikrofon stummschalten"
                : "Mute app microphone"}
          </button>
        )}
      </div>
      {/* Also reachable without hover: keyboard and touch users open it explicitly. */}
      <details className="capture-control-help">
        <summary
          title={
            de
              ? "Nachlesen, welche Tonspuren beim Pausieren oder Stummschalten weiterlaufen."
              : "See which audio tracks continue when you pause or mute."
          }
        >
          {de
            ? "Was bewirken Pause und Stummschaltung?"
            : "What do pause and mute do?"}
        </summary>
        <p>{help}</p>
      </details>
      {(control.paused ||
        (control.microphoneMuted && control.hasMicrophone)) && (
        <p role="status" className="notice warning">
          {control.paused
            ? de
              ? "Aufnahme pausiert. Gesprächston wird nicht gespeichert; die Zeitachse läuft weiter."
              : "Recording paused. Conversation audio is not saved; the timeline continues."
            : de
              ? "App-Mikrofon stumm. Der Computerton wird weiterhin aufgenommen."
              : "App microphone muted. System audio is still being recorded."}
        </p>
      )}
    </div>
  );
}
