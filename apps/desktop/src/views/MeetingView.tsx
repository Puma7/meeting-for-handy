import {
  AudioLines,
  Check,
  Copy,
  RefreshCw,
  Square,
  Sparkles,
} from "lucide-react";
import type {
  AppState,
  AudioDevice,
  HandyModel,
  Meeting,
  AudioPart,
  TranscriptFormat,
  DesktopApi,
} from "../types";
import type { MessageKey, extraDe } from "../i18n";
import { clock } from "../domain";
import TranscriptBody from "./TranscriptBody";
import { useEffect, useState, type ReactNode } from "react";
import SessionWarnings from "./SessionWarnings";
import MeetingFiles from "./MeetingFiles";
import RecordingControls from "./RecordingControls";
import AudioSources from "./AudioSources";
type Props = {
  t: Record<MessageKey, string>;
  x: Record<keyof typeof extraDe, string>;
  app: AppState | null;
  meeting: Meeting | null;
  recording: boolean;
  isStopping: boolean;
  busy: boolean;
  /** A microphone reconnect is pending; Stop stays available and cancels it. */
  reconnecting?: boolean;
  connected: boolean;
  title: string;
  defaultTitle: string;
  onAnalyse: () => void;
  onOpenFolder: () => void;
  onExport: (format: TranscriptFormat) => void;
  onPause: (paused: boolean) => void;
  onMute: (muted: boolean) => void;
  onReconnectMicrophone?: () => void;
  controlsAvailable: boolean;
  getAudioLevels?: DesktopApi["getAudioLevels"];
  savedFile: string | null;
  modelId: string;
  systemId: string;
  micId: string;
  micEnabled: boolean;
  models: HandyModel[];
  devices: AudioDevice[];
  copied: boolean;
  backlog: number | null;
  recentParts: AudioPart[];
  transcriptLoading?: boolean;
  loadFullTranscript?: () => Promise<void>;
  locale: "de" | "en";
  setTitle: (v: string) => void;
  setModelId: (v: string) => void;
  setSystemId: (v: string) => void;
  setMicId: (v: string) => void;
  setMicEnabled: (v: boolean) => void;
  refreshHandy: () => void;
  /** Re-reads the audio devices, for example after connecting a headset. */
  refreshDevices?: () => void;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  savedByTrack: (track: "system" | "microphone") => number;
  resume: () => void;
  copyFullTranscript: () => Promise<void>;
  /** Live summary shown beside the transcript. */
  liveNotes?: ReactNode;
  /** Only the live summary is shown; the transcript stays available behind a toggle. */
  liveFocus?: boolean;
  onLiveFocus?: (focus: boolean) => void;
};
/**
 * Shows live section/capture progress and delegates mutations to parent callbacks. Native
 * acknowledgement and ownership determine the pause/mute controls.
 */
export default function MeetingView({
  t,
  x,
  app,
  meeting,
  recording,
  isStopping,
  busy,
  reconnecting = false,
  connected,
  title,
  defaultTitle,
  onAnalyse,
  onOpenFolder,
  onExport,
  onPause,
  onMute,
  onReconnectMicrophone,
  controlsAvailable,
  getAudioLevels,
  savedFile,
  modelId,
  systemId,
  micId,
  micEnabled,
  models,
  devices,
  copied,
  backlog,
  recentParts,
  transcriptLoading,
  loadFullTranscript,
  locale,
  setTitle,
  setModelId,
  setSystemId,
  setMicId,
  setMicEnabled,
  refreshHandy,
  refreshDevices,
  start,
  stop,
  savedByTrack,
  resume,
  copyFullTranscript,
  liveNotes,
  liveFocus = false,
  onLiveFocus,
}: Props) {
  const [setupOpen, setSetupOpen] = useState(!meeting && !recording);
  const agentOwnsCapture = Boolean(
    app?.agent?.activeCapture &&
      app.agent.ownedSessionId === app.activeMeeting?.id,
  );
  useEffect(() => {
    if (meeting) setSetupOpen(false);
  }, [recording, meeting?.id]);
  return (
    <>
      {!!app?.handy?.error && (
        <div className="notice warning">{app.handy.error}</div>
      )}
      {connected && !devices.some((item) => item.source === "system") && (
        <div className="notice warning" role="status">
          {t.noSystemHelp}
        </div>
      )}
      {connected &&
        micEnabled &&
        !devices.some((item) => item.source === "microphone") && (
          <div className="notice warning" role="status">
            {t.noMicrophoneHelp}
          </div>
        )}
      <section
        className={`setup-panel${recording && !setupOpen ? " recording-compact" : ""}`}
        aria-label={t.meeting}
      >
        <details
          className="setup-details"
          open={setupOpen}
          onToggle={(event) => setSetupOpen(event.currentTarget.open)}
        >
          <summary>
            <strong>{t.captureSetup}</strong>
            <span>
              {models.find((model) => model.id === modelId)?.name ?? t.choose}
            </span>
          </summary>
          <div className="setup-grid">
            <label className="full">
              {t.title}
              <input
                value={title}
                disabled={recording || busy}
                placeholder={defaultTitle}
                maxLength={200}
                onChange={(event) => setTitle(event.target.value)}
              />
            </label>
            <label className="full">
              {t.model}
              <div className="inline-control">
                <select
                  title={t.modelTooltip}
                  value={modelId}
                  disabled={recording || busy || !models.length}
                  onChange={(event) => setModelId(event.target.value)}
                >
                  <option value="">
                    {models.length ? t.choose : t.noModels}
                  </option>
                  {models.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name}
                    </option>
                  ))}
                </select>
                <button
                  className="icon-button bordered"
                  title={t.refresh}
                  aria-label={t.refresh}
                  disabled={busy || recording || !connected}
                  onClick={refreshHandy}
                >
                  <RefreshCw size={17} />
                </button>
              </div>
            </label>
          </div>
        </details>
        <AudioSources
          compact={recording && !setupOpen}
          t={t}
          locale={locale}
          devices={devices}
          systemId={systemId}
          micId={micId}
          micEnabled={micEnabled}
          recording={recording}
          busy={busy}
          connected={connected}
          isStopping={isStopping}
          activeMeetingId={app?.activeMeeting?.id ?? null}
          displayedMeetingId={meeting?.id ?? null}
          captureControl={app?.captureControl}
          getAudioLevels={getAudioLevels}
          setSystemId={setSystemId}
          setMicId={setMicId}
          setMicEnabled={setMicEnabled}
          onRefresh={refreshDevices}
          canReconnectMicrophone={app?.microphoneReconnectAvailable}
          onReconnectMicrophone={onReconnectMicrophone}
        />
        <div className="record-controls full">
          <span
            className={
              recording && !app?.captureControl?.paused
                ? "state recording"
                : "state"
            }
          >
            <span className="state-dot" />
            {isStopping
              ? t.stopping
              : app?.transitioning ||
                  (!recording &&
                    ["catching-up", "streaming"].includes(
                      app?.activeMeeting?.status ?? "",
                    ))
                ? t.busy
                : recording
                  ? app?.captureControl?.paused
                    ? locale === "de"
                      ? "Aufnahme pausiert"
                      : "Recording paused"
                    : t.recording
                  : connected && modelId && systemId
                    ? t.ready
                    : t.choose}
          </span>
          {recording || isStopping ? (
            <button
              className="danger"
              disabled={(busy && !reconnecting) || isStopping}
              title={t.stopTooltip}
              onClick={() => void stop()}
            >
              {isStopping ? (
                <RefreshCw size={16} className="spinning" />
              ) : (
                <Square size={16} />
              )}
              {isStopping ? t.stopping : t.stop}
            </button>
          ) : (
            <button
              className="primary"
              disabled={
                !connected ||
                busy ||
                !modelId ||
                !systemId ||
                (micEnabled && !micId)
              }
              title={t.startTooltip}
              onClick={() => void start()}
            >
              <AudioLines size={18} />
              {busy ? t.busy : t.start}
            </button>
          )}
        </div>
        {recording && controlsAvailable && app?.captureControl && (
          <RecordingControls
            control={app.captureControl}
            locale={locale}
            disabled={busy || isStopping || !connected || agentOwnsCapture}
            onPause={onPause}
            onMute={onMute}
          />
        )}
        {recording && agentOwnsCapture && (
          <p className="notice warning" role="status">
            {locale === "de"
              ? "Diese Aufnahme gehört zur freigegebenen Agentensitzung. Entziehe zuerst die Agentenfreigabe, bevor du eine eigene Aufnahme steuerst."
              : "This recording belongs to the approved agent session. Revoke agent access before controlling a new manual recording."}
          </p>
        )}
      </section>
      {meeting && (
        <div
          className={
            recording ? "session-progress compact" : "session-progress"
          }
        >
          <div>
            <span>{t.duration}</span>
            <strong>{clock(meeting.durationMs)}</strong>
          </div>
          <div>
            <span>
              {t.saved} · {t.sourceSystem}
            </span>
            <strong>{clock(savedByTrack("system"))}</strong>
          </div>
          {meeting.parts.some((item) => item.source === "microphone") && (
            <div>
              <span>
                {t.saved} · {t.sourceMicrophone}
              </span>
              <strong>{clock(savedByTrack("microphone"))}</strong>
            </div>
          )}
          <div>
            <span>{t.lag}</span>
            <strong>{backlog === null ? "—" : clock(backlog)}</strong>
          </div>
        </div>
      )}
      {meeting && (
        <SessionWarnings
          warnings={meeting.warnings}
          locale={locale}
          t={t}
          announceNew={recording}
        />
      )}
      {meeting &&
        !recording &&
        meeting.status !== "complete" &&
        !meeting.readOnly && (
          <div className="recovery-actions">
            <button
              className="secondary"
              disabled={busy || !connected}
              onClick={resume}
            >
              <RefreshCw size={15} />
              {x.resume}
            </button>
          </div>
        )}
      <div className="meeting-workspace-frame">
        {liveNotes && onLiveFocus && (
          <div
            className="meeting-view-choice"
            role="group"
            aria-label={locale === "de" ? "Meetingansicht" : "Meeting view"}
          >
            <button
              className="secondary"
              aria-pressed={!liveFocus}
              onClick={() => onLiveFocus(false)}
            >
              {locale === "de" ? "Transkript" : "Transcript"}
            </button>
            <button
              className="secondary"
              aria-pressed={liveFocus}
              onClick={() => onLiveFocus(true)}
            >
              {locale === "de" ? "Zusammenfassung" : "Summary"}
            </button>
          </div>
        )}
        <div
          className={`meeting-workspace${liveNotes ? " with-live" : ""}${
            liveNotes && liveFocus ? " live-focus" : ""
          }`}
        >
          <section className="transcript-panel">
            <div className="section-heading">
              <div>
                <h2>{meeting?.title ?? t.live}</h2>
              </div>
              <div className="detail-actions">
                <button
                  className="secondary"
                  disabled={
                    busy ||
                    !meeting?.parts.some((item) => (item.text ?? "").trim())
                  }
                  title={t.copyTooltip}
                  onClick={() => void copyFullTranscript()}
                >
                  {copied ? <Check size={16} /> : <Copy size={16} />}
                  <span>{copied ? t.copied : t.copy}</span>
                </button>
                {meeting?.status === "complete" && (
                  <button
                    className="primary"
                    disabled={busy || !recentParts.length}
                    onClick={onAnalyse}
                  >
                    <Sparkles size={16} />
                    {t.analyzeMeeting}
                  </button>
                )}
              </div>
            </div>
            {meeting && (
              <div className="meeting-files">
                <MeetingFiles
                  t={t}
                  disabled={busy || !connected}
                  onOpenFolder={onOpenFolder}
                  onExport={onExport}
                  savedFile={savedFile}
                />
              </div>
            )}
            {meeting?.livePreviewLimited && loadFullTranscript && (
              <button
                className="text-button"
                disabled={transcriptLoading}
                onClick={() => void loadFullTranscript()}
              >
                {transcriptLoading
                  ? locale === "de"
                    ? "Frühere Abschnitte werden geladen …"
                    : "Loading earlier sections …"
                  : locale === "de"
                    ? "Gesamtes Transkript laden"
                    : "Load complete transcript"}
              </button>
            )}
            <TranscriptBody parts={recentParts} locale={locale} t={t} />
          </section>
          {liveNotes}
        </div>
      </div>
    </>
  );
}
