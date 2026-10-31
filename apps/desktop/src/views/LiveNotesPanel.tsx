import {
  Check,
  Copy,
  Maximize2,
  Minimize2,
  Play,
  RefreshCw,
  Settings2,
  Square,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { errorMessage } from "../api";
import { clock } from "../domain";
import type {
  DesktopApi,
  LiveItem,
  LiveNotesStatus,
  LiveView,
  Locale,
} from "../types";
import LocalModelSetup from "./LocalModelSetup";

export interface LiveConnection {
  endpoint: string;
  model: string;
  mode: "local" | "cloud";
  apiKey: string;
  useSavedKey: boolean;
}

export interface LivePreferences {
  /** Start with every recording; only ever for local models. */
  auto: boolean;
  intervalMinutes: number;
}

export const INTERVAL_CHOICES = [1, 2, 3, 5, 10, 15];

type Props = {
  api: DesktopApi;
  locale: Locale;
  connected: boolean;
  meetingId: string | null;
  meetingTitle: string;
  /** Recording or catch-up still produces text. */
  meetingActive: boolean;
  connection: LiveConnection;
  preferences: LivePreferences;
  setPreferences: (next: LivePreferences) => void;
  onUseLocalModel: (endpoint: string, model: string) => void;
  onOpenTextModelSettings: () => void;
  focus: boolean;
  setFocus: (focus: boolean) => void;
  /** Shows nothing and only keeps the automatic start running, e.g. on other pages. */
  headless?: boolean;
  connectionLocked?: boolean;
};

const POLL_RUNNING_MS = 1500;
const POLL_IDLE_MS = 5000;
/** A failed automatic start is retried this often, for at most ten minutes. */
const AUTO_START_RETRY_MS = 15_000;
const AUTO_START_ATTEMPTS = 40;

function trackLabel(track: string, de: boolean): string {
  if (track === "microphone") return de ? "Mikrofon" : "Microphone";
  return de ? "Computerton" : "Computer audio";
}

/** Compares routes like the native client: trailing slash and `localhost` loopback alias. */
function normalizeEndpoint(endpoint: string): string {
  const trimmed = endpoint.trim().replace("//localhost", "//127.0.0.1");
  return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
}

function timeOf(item: LiveItem): number {
  return Math.min(...item.evidence.map((e) => e.startMs));
}

function time(iso: string | null, locale: Locale): string {
  if (!iso) return "—";
  const date = new Date(iso);
  return Number.isNaN(date.getTime())
    ? "—"
    : date.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
}

/** Plain text for reading out or pasting. Model text is data and stays plain text. */
export function liveNotesText(
  view: LiveView,
  title: string,
  locale: Locale,
): string {
  const de = locale === "de";
  // Every statement keeps its source identifiers and verbatim quotes, also outside the app.
  const line = (item: LiveItem) => {
    const extra = [item.owner, item.due].filter(Boolean).join(", ");
    const evidence = item.evidence.map(
      (e) =>
        `  [${e.sourceId} · ${clock(e.startMs)} · ${trackLabel(e.track, de)}] „${e.quote}“`,
    );
    return [`- ${item.text}${extra ? ` (${extra})` : ""}`, ...evidence].join(
      "\n",
    );
  };
  const block = (heading: string, items: LiveItem[]) =>
    items.length ? `\n${heading}\n${items.map(line).join("\n")}\n` : "";
  return (
    `${de ? "Live-Zusammenfassung" : "Live summary"} · ${title} · ${de ? "Einzelergebnisse vom" : "Individual notes as of"} ${time(view.updatedAt, locale)}\n` +
    `${de ? "Kurzfassung" : "Overview"}: ${view.overviewWindow} ${de ? "von" : "of"} ${view.windows} ${de ? "Textfenstern" : "text windows"}${view.overviewUpdatedAt ? ` · ${time(view.overviewUpdatedAt, locale)}` : ""}\n` +
    (overviewPending(view)
      ? `${de ? "Kurzfassung ist noch nicht vollständig. Neuere Einzelergebnisse stehen unten." : "The overview is incomplete. Newer individual notes are included below."}\n`
      : "") +
    block(de ? "Bisheriger Verlauf" : "Summary so far", view.overview) +
    // Accepted points also count when the overview failed or is not up to date yet.
    block(
      de ? "Wichtige Punkte und Erkenntnisse" : "Important points and insights",
      view.points,
    ) +
    block(de ? "Entscheidungen" : "Decisions", view.decisions) +
    block(de ? "Aufgaben" : "Tasks", view.tasks) +
    block(de ? "Aufgeworfene Fragen" : "Questions raised", view.questions) +
    `\n${de ? "KI-Entwurf aus der laufenden Mitschrift. Belege bestätigen Originalzitate, nicht automatisch die Deutung." : "AI draft from the running transcript. Evidence confirms the quotes, not necessarily the interpretation."}\n`
  );
}

/** Older stored replies still expose coverage, even without the newer explicit pending flag. */
function overviewPending(view: LiveView): boolean {
  return view.overviewPending ?? view.overviewWindow < view.windows;
}

function Items({
  items,
  locale,
  empty,
  newestFirst = false,
}: {
  items: LiveItem[];
  locale: Locale;
  empty: string;
  newestFirst?: boolean;
}) {
  const de = locale === "de";
  if (!items.length) return <p className="muted">{empty}</p>;
  const shown = newestFirst ? [...items].reverse() : items;
  return (
    <ul className="live-items">
      {shown.map((item, index) => (
        <li key={`${item.window}-${index}-${item.text}`}>
          <span className="live-item-text">{item.text}</span>
          {(item.owner || item.due) && (
            <span className="live-item-meta">
              {[item.owner, item.due].filter(Boolean).join(" · ")}
            </span>
          )}
          <span className="live-item-meta">
            {clock(timeOf(item))} ·{" "}
            {item.tracks.map((t) => trackLabel(t, de)).join(", ")}
          </span>
          <details className="live-evidence">
            <summary>{de ? "Belege" : "Evidence"}</summary>
            {item.evidence.map((e, i) => (
              <blockquote key={`${e.sourceId}-${i}`}>
                <span className="muted">
                  [{e.sourceId}] {clock(e.startMs)} · {trackLabel(e.track, de)}
                </span>{" "}
                {e.quote}
              </blockquote>
            ))}
          </details>
        </li>
      ))}
    </ul>
  );
}

/**
 * Running, source-linked summary beside the recording. Updates come from the native worker;
 * this panel only starts, stops and displays them. Cloud use needs a fresh text release for
 * every start and never starts automatically.
 */
export default function LiveNotesPanel({
  api,
  locale,
  connected,
  meetingId,
  meetingTitle,
  meetingActive,
  connection,
  preferences,
  setPreferences,
  onUseLocalModel,
  onOpenTextModelSettings,
  focus,
  setFocus,
  headless = false,
  connectionLocked = false,
}: Props) {
  const de = locale === "de";
  const [loaded, setStatus] = useState<LiveNotesStatus | null>(null),
    [error, setError] = useState<string | null>(null),
    [busy, setBusy] = useState(false),
    [cloudConsent, setCloudConsent] = useState(false),
    [setupOpen, setSetupOpen] = useState(false),
    [copied, setCopied] = useState(false);
  const sequence = useRef(0),
    mounted = useRef(true),
    // Counts meeting and connection changes; an answer to a call made before a change is
    // ignored, so a late answer for one meeting never shows up under another.
    generation = useRef(0),
    autoStarted = useRef<string | null>(null),
    autoAttempts = useRef(new Map<string, number>());
  const [autoRetry, setAutoRetry] = useState(0);
  // Only the state of the meeting shown now is displayed or acted on.
  const status = loaded?.meetingId === meetingId ? loaded : null;
  const available = Boolean(api.getLiveNotes && api.startLiveNotes);
  const endpointValid = endpointAllowed(connection.mode, connection.endpoint);
  const modelReady = Boolean(connection.model.trim()) && endpointValid;

  useEffect(() => {
    // Restored on every setup: StrictMode runs setup, cleanup and setup again.
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  // Runs before the browser paints, so no answer for the previous meeting can show up first.
  useLayoutEffect(() => {
    generation.current += 1;
    setCloudConsent(false);
    setStatus(null);
    setError(null);
    setBusy(false);
  }, [meetingId, connection.mode, connection.endpoint]);
  // Whether the answer to a call made in `call` may still change what this panel shows.
  const current = useCallback(
    (call: number) => mounted.current && call === generation.current,
    [],
  );

  const poll = useCallback(async () => {
    if (!meetingId || !api.getLiveNotes || !connected) return null;
    const id = ++sequence.current,
      call = generation.current;
    try {
      const next = await api.getLiveNotes(meetingId);
      if (current(call) && id === sequence.current) setStatus(next);
      return next;
    } catch (e) {
      if (current(call) && id === sequence.current) setError(errorMessage(e));
      return null;
    }
  }, [api, meetingId, connected, current]);

  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const tick = async () => {
      const next = await poll();
      if (stopped) return;
      timer = setTimeout(
        () => void tick(),
        next?.running ? POLL_RUNNING_MS : POLL_IDLE_MS,
      );
    };
    void tick();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [poll]);

  const start = useCallback(async (): Promise<boolean> => {
    if (!meetingId || !api.startLiveNotes) return false;
    const call = generation.current;
    setBusy(true);
    setError(null);
    let started = false;
    try {
      const next = await api.startLiveNotes({
        meetingId,
        endpoint: connection.endpoint.trim(),
        model: connection.model.trim(),
        mode: connection.mode,
        cloudConsent: connection.mode === "cloud" && cloudConsent,
        apiKey: connection.useSavedKey ? "" : connection.apiKey,
        useSavedKey: connection.useSavedKey,
        intervalMs: preferences.intervalMinutes * 60_000,
        language: locale,
      });
      if (current(call)) setStatus(next);
      started = true;
    } catch (e) {
      if (current(call)) setError(errorMessage(e));
    } finally {
      if (current(call)) setBusy(false);
    }
    // The release covers this start only.
    if (current(call)) setCloudConsent(false);
    return started;
  }, [
    api,
    meetingId,
    connection,
    cloudConsent,
    preferences.intervalMinutes,
    locale,
    current,
  ]);

  // Automatic start: local models only, once per meeting, never after an explicit stop.
  useEffect(() => {
    if (
      !preferences.auto ||
      !meetingId ||
      !meetingActive ||
      !connected ||
      connection.mode !== "local" ||
      !modelReady ||
      status?.status !== "off" ||
      autoStarted.current === meetingId
    )
      return;
    autoStarted.current = meetingId;
    void start().then((started) => {
      // The previous meeting's summary may still be finishing its backlog: try again later,
      // a bounded number of times, instead of giving up on this meeting's automatic start.
      if (started || !mounted.current) return;
      const attempts = (autoAttempts.current.get(meetingId) ?? 0) + 1;
      autoAttempts.current.set(meetingId, attempts);
      if (attempts >= AUTO_START_ATTEMPTS) return;
      // Not cleared by a re-render: the retry must survive changing props.
      setTimeout(() => {
        if (!mounted.current) return;
        if (autoStarted.current === meetingId) autoStarted.current = null;
        setAutoRetry((n) => n + 1);
      }, AUTO_START_RETRY_MS);
    });
  }, [
    preferences.auto,
    meetingId,
    meetingActive,
    connected,
    connection.mode,
    modelReady,
    status?.status,
    start,
    autoRetry,
  ]);

  async function stop() {
    if (!api.stopLiveNotes) return;
    const call = generation.current;
    setBusy(true);
    try {
      await api.stopLiveNotes();
      await poll();
    } catch (e) {
      if (current(call)) setError(errorMessage(e));
    } finally {
      if (current(call)) setBusy(false);
    }
  }

  async function refresh() {
    const call = generation.current;
    try {
      await api.refreshLiveNotes?.();
      await poll();
    } catch (e) {
      if (current(call)) setError(errorMessage(e));
    }
  }

  async function changeInterval(minutes: number) {
    setPreferences({ ...preferences, intervalMinutes: minutes });
    if (status?.running) {
      const call = generation.current;
      try {
        await api.setLiveNotesInterval?.(minutes * 60_000);
      } catch (e) {
        if (current(call)) setError(errorMessage(e));
      }
    }
  }

  async function copy() {
    // Only a summary of the meeting shown now gets this meeting's title.
    if (!status?.view || status.meetingId !== meetingId) return;
    const call = generation.current;
    try {
      await api.copyText(liveNotesText(status.view, meetingTitle, locale));
      if (current(call)) {
        setCopied(true);
        setTimeout(() => mounted.current && setCopied(false), 1500);
      }
    } catch (e) {
      if (current(call)) setError(errorMessage(e));
    }
  }

  if (!available || headless) return null;
  const view = status?.view ?? null;
  const running = status?.running ?? false;
  const staleOverview = view !== null && overviewPending(view);
  const hasContent = Boolean(
    view &&
      (view.overview.length ||
        view.points.length ||
        view.decisions.length ||
        view.tasks.length ||
        view.questions.length),
  );
  const nextMinutes =
    status?.nextUpdateInMs != null
      ? Math.max(1, Math.ceil(status.nextUpdateInMs / 60_000))
      : null;
  const stateText = (() => {
    if (status?.running && status.waitingForTranscript)
      return de
        ? "Wartet auf Spracherkennung · Aufnahme und Transkript haben Vorrang"
        : "Waiting for recognition · recording and transcription take priority";
    switch (status?.status) {
      case "starting":
      case "waiting":
        return `${de ? "Läuft" : "Running"} · ${de ? "Stand" : "as of"} ${time(view?.updatedAt ?? null, locale)}${
          status.pendingChars > 0 && nextMinutes
            ? ` · ${de ? `nächste Aktualisierung in ca. ${nextMinutes} Min.` : `next update in about ${nextMinutes} min`}`
            : ""
        }`;
      case "updating":
        return de ? "Aktualisiert …" : "Updating …";
      case "finished":
      case "partial":
        if (staleOverview)
          return de
            ? "Einzelergebnisse gespeichert; Kurzfassung ist noch nicht vollständig."
            : "Individual notes saved; the overview is not yet complete.";
        return de
          ? "Abgeschlossen: das ganze Meeting ist zusammengefasst."
          : "Finished: the whole meeting is summarized.";
      case "stopped":
        return de ? "Angehalten" : "Stopped";
      case "failed":
        return de ? "Angehalten nach Fehlern" : "Stopped after errors";
      default:
        return de ? "Aus" : "Off";
    }
  })();
  // While running, the route of the active run is shown, never the editable settings.
  const activeRoute =
    running && view?.runId
      ? { mode: view.mode, model: view.model, endpoint: view.endpoint }
      : modelReady
        ? {
            mode: connection.mode,
            model: connection.model,
            endpoint: connection.endpoint,
          }
        : null;
  const settingsChanged = Boolean(
    running &&
      view?.runId &&
      (view.mode !== connection.mode ||
        view.model !== connection.model.trim() ||
        normalizeEndpoint(view.endpoint) !==
          normalizeEndpoint(connection.endpoint)),
  );
  const canStart =
    connected &&
    Boolean(meetingId) &&
    !running &&
    !busy &&
    modelReady &&
    (connection.mode === "local" || cloudConsent);

  return (
    <aside
      className="live-notes-panel"
      aria-label={de ? "Live-Zusammenfassung" : "Live summary"}
    >
      <div className="section-heading">
        <div>
          <h2>{de ? "Live-Zusammenfassung" : "Live summary"}</h2>
          <p className="live-state" role="status">
            {status?.status === "updating" && (
              <RefreshCw size={13} className="spinning" />
            )}{" "}
            {stateText}
          </p>
        </div>
        <div className="detail-actions">
          <button
            className="icon-button bordered"
            title={
              focus
                ? de
                  ? "Transkript wieder zeigen"
                  : "Show transcript again"
                : de
                  ? "Nur Zusammenfassung zeigen"
                  : "Show summary only"
            }
            aria-label={
              focus
                ? de
                  ? "Transkript wieder zeigen"
                  : "Show transcript again"
                : de
                  ? "Nur Zusammenfassung zeigen"
                  : "Show summary only"
            }
            aria-pressed={focus}
            onClick={() => setFocus(!focus)}
          >
            {focus ? <Minimize2 size={15} /> : <Maximize2 size={15} />}
          </button>
          <button
            className="icon-button bordered"
            title={de ? "Kopieren" : "Copy"}
            aria-label={de ? "Zusammenfassung kopieren" : "Copy summary"}
            disabled={!hasContent}
            onClick={() => void copy()}
          >
            {copied ? <Check size={15} /> : <Copy size={15} />}
          </button>
        </div>
      </div>

      <div className="live-model-line muted">
        {activeRoute ? (
          <>
            <span
              className={activeRoute.mode === "cloud" ? "chip warn" : "chip ok"}
            >
              {activeRoute.mode === "cloud" ? "Cloud" : de ? "Lokal" : "Local"}
            </span>{" "}
            {activeRoute.model}
            {activeRoute.mode === "cloud" && ` · ${activeRoute.endpoint}`}
            {settingsChanged && (
              <span role="status">
                {de
                  ? " · Geänderte Einstellungen gelten ab dem nächsten Start."
                  : " · Changed settings apply from the next start."}
              </span>
            )}
          </>
        ) : (
          <span role={connection.model.trim() ? "alert" : undefined}>
            {!connection.model.trim()
              ? de
                ? "Noch kein Textmodell gewählt."
                : "No text model chosen yet."
              : connection.mode === "local"
                ? de
                  ? "Lokale Verarbeitung erlaubt nur Adressen auf diesem Computer (127.0.0.1, localhost oder [::1]) mit HTTP oder HTTPS."
                  : "Local processing only allows addresses on this computer (127.0.0.1, localhost or [::1]) over HTTP or HTTPS."
                : de
                  ? "Cloud benötigt eine HTTPS-Adresse ohne Zugangsdaten oder Zusatzparameter."
                  : "Cloud needs an HTTPS address without credentials or extra parameters."}
          </span>
        )}
        <button
          className="link-button"
          onClick={() => setSetupOpen((open) => !open)}
        >
          <Settings2 size={13} />
          {de ? "Lokales Modell einrichten" : "Set up local model"}
        </button>
        {connection.mode === "cloud" && !modelReady && (
          <button className="link-button" onClick={onOpenTextModelSettings}>
            {de ? "Alle Textmodell-Einstellungen" : "All text model settings"}
          </button>
        )}
      </div>

      {setupOpen && (
        <LocalModelSetup
          api={api}
          locale={locale}
          connected={connected}
          disabled={connectionLocked}
          onUse={(endpoint, model) => {
            if (connectionLocked) return;
            onUseLocalModel(endpoint, model);
            setSetupOpen(false);
          }}
        />
      )}

      {connection.mode === "cloud" && !running && modelReady && (
        <label className="live-consent">
          <input
            type="checkbox"
            checked={cloudConsent}
            onChange={(e) => setCloudConsent(e.target.checked)}
          />
          {de
            ? `Transkripttext dieses Meetings während der Aufnahme an ${connection.endpoint} senden. Keine Audiodateien. Gilt nur für diesen Start.`
            : `Send this meeting's transcript text to ${connection.endpoint} while recording. No audio. Applies to this start only.`}
        </label>
      )}

      {connection.mode === "local" && (
        <p className="muted live-memory-note">
          {de
            ? "Lokales Textmodell benötigt zusätzlichen Arbeitsspeicher. Bei knappem Speicher kann der Ton stocken – dann erst nach der Aufnahme auswerten."
            : "A local text model needs additional memory. Low memory can make audio stutter; run analysis after recording if memory is tight."}
        </p>
      )}
      <div className="live-controls">
        {running ? (
          <>
            <button
              className="secondary"
              disabled={busy}
              onClick={() => void stop()}
            >
              <Square size={14} />
              {de ? "Beenden" : "Stop"}
            </button>
            <button
              className="secondary"
              disabled={busy || status?.status === "updating"}
              onClick={() => void refresh()}
            >
              <RefreshCw size={14} />
              {de ? "Jetzt aktualisieren" : "Update now"}
            </button>
          </>
        ) : (
          <button
            className="primary"
            disabled={!canStart}
            onClick={() => void start()}
          >
            <Play size={14} />
            {staleOverview
              ? de
                ? "Auswertung erneuern"
                : "Retry summary"
              : de
                ? "Live-Zusammenfassung starten"
                : "Start live summary"}
          </button>
        )}
      </div>
      <details className="help-disclosure live-options">
        <summary>
          {de ? "Optionen und Hinweise" : "Options and guidance"}
        </summary>
        <div className="live-controls">
          <label className="live-interval">
            {de ? "Alle" : "Every"}
            <select
              value={preferences.intervalMinutes}
              onChange={(e) => void changeInterval(Number(e.target.value))}
            >
              {INTERVAL_CHOICES.map((minutes) => (
                <option key={minutes} value={minutes}>
                  {minutes} {de ? "Min." : "min"}
                </option>
              ))}
            </select>
          </label>
          <label className="live-auto">
            <input
              type="checkbox"
              checked={preferences.auto}
              onChange={(e) =>
                setPreferences({ ...preferences, auto: e.target.checked })
              }
            />
            {de
              ? "Bei jeder Aufnahme automatisch starten (nur lokal)"
              : "Start with every recording (local only)"}
          </label>
        </div>

        {connection.mode === "local" && (
          <p className="muted live-memory-detail">
            {de
              ? "Das lokale Textmodell läuft neben der Aufnahme und belegt auch in kleinen Größen (etwa 4B) mehrere GB Arbeitsspeicher. Wird der Speicher knapp, werden Zusammenfassung und Erkennung langsam und der Ton kann stocken; die Auslastung zeigt die Seitenleiste. Bei wenig Arbeitsspeicher die Auswertung besser nach der Aufnahme starten."
              : "The local text model runs next to the recording and takes several GB of memory even in small sizes (around 4B). When memory runs short, the summary and recognition slow down and the audio can stutter; the sidebar shows the load. With little memory, run the analysis after the recording instead."}
          </p>
        )}
        {(connection.mode !== "cloud" || modelReady) && (
          <button className="link-button" onClick={onOpenTextModelSettings}>
            {de ? "Alle Textmodell-Einstellungen" : "All text model settings"}
          </button>
        )}
      </details>
      {(error || status?.error) && (
        <p className="notice warning" role="alert">
          {error ?? status?.error}
        </p>
      )}
      {view?.lastRequest && view.lastRequest.request.result !== "stored" && (
        <p className="notice warning" role="status">
          {de
            ? "Der letzte Modellversuch wurde nicht gespeichert: "
            : "The last model attempt was not saved: "}
          {view.lastRequest.request.result === "incomplete"
            ? de
              ? "Antwort unvollständig. "
              : "Incomplete response. "
            : view.lastRequest.request.result === "persistence"
              ? de
                ? "Ergebnis konnte nicht gespeichert werden. "
                : "Result could not be saved. "
              : view.lastRequest.request.result === "validation"
                ? de
                  ? "Antwort oder Quellenprüfung fehlgeschlagen. "
                  : "Response or evidence validation failed. "
                : view.lastRequest.request.result === "cancelled"
                  ? de
                    ? "Abgebrochen. "
                    : "Cancelled. "
                  : de
                    ? "Verbindung oder Dienstantwort fehlgeschlagen. "
                    : "Connection or service response failed. "}
          {view.lastRequest.request.httpStatus &&
            `HTTP ${view.lastRequest.request.httpStatus}. `}
          {de
            ? "Vorhandene Einzelergebnisse und Kurzfassungen bleiben erhalten."
            : "Existing notes and overviews are retained."}
        </p>
      )}
      {view && view.damagedRuns > 0 && (
        <p className="notice warning" role="status">
          {de
            ? `${view.damagedRuns} gespeicherte Live-Zusammenfassung(en) sind nicht prüfbar oder einem anderen Meeting zugeordnet. Sie bleiben unverändert erhalten.`
            : `${view.damagedRuns} saved live summary run(s) failed verification or belong to another meeting. They are kept unchanged.`}
        </p>
      )}

      {!hasContent ? (
        <p className="muted live-empty">
          {running
            ? de
              ? `Sobald bestätigter Text vorliegt, entsteht hier die Zusammenfassung – spätestens alle ${preferences.intervalMinutes} Minuten, früher nach längeren Redebeiträgen.`
              : `As soon as confirmed text exists the summary appears here – at least every ${preferences.intervalMinutes} minutes, sooner after long contributions.`
            : de
              ? "Verlauf, Entscheidungen und Aufgaben mit Originalbelegen. Wähle ein Textmodell und starte die Zusammenfassung."
              : "Summary, decisions and tasks with original evidence. Choose a text model and start the summary."}
        </p>
      ) : (
        view && (
          <div className="live-sections">
            <section>
              <h3>{de ? "Bisheriger Verlauf" : "Summary so far"}</h3>
              {staleOverview && (
                <p className="notice warning" role="status">
                  {de
                    ? "Kurzfassung ist noch nicht vollständig. Neuere Einzelergebnisse stehen in den Listen darunter. Frühere Fassungen bleiben erhalten."
                    : "The overview is incomplete. Newer individual notes are in the lists below. Earlier versions are preserved."}
                </p>
              )}
              <p className="muted">
                {de ? "Kurzfassung" : "Overview"}: {view.overviewWindow}{" "}
                {de ? "von" : "of"} {view.windows}{" "}
                {de ? "Textfenstern" : "text windows"}
                {view.overviewUpdatedAt &&
                  ` · ${de ? "Stand" : "as of"}: ${time(view.overviewUpdatedAt, locale)}`}
              </p>
              <Items
                items={view.overview}
                locale={locale}
                empty={de ? "Noch keine Kurzfassung." : "No summary yet."}
              />
            </section>
            <section>
              <h3>
                {de ? "Entscheidungen" : "Decisions"} ({view.decisions.length})
              </h3>
              <Items
                items={view.decisions}
                locale={locale}
                empty={
                  de
                    ? "Keine ausdrücklichen Entscheidungen."
                    : "No explicit decisions."
                }
              />
            </section>
            <section>
              <h3>
                {de ? "Aufgaben" : "Tasks"} ({view.tasks.length})
              </h3>
              <Items
                items={view.tasks}
                locale={locale}
                empty={
                  de ? "Keine zugesagten Aufgaben." : "No committed tasks."
                }
              />
            </section>
            <section>
              <h3>
                {de ? "Aufgeworfene Fragen" : "Questions raised"} (
                {view.questions.length})
              </h3>
              <Items
                items={view.questions}
                locale={locale}
                empty={
                  de ? "Keine Fragen aufgeworfen." : "No questions raised."
                }
              />
            </section>
            <details className="live-points">
              <summary>
                {de
                  ? "Wichtige Punkte und Erkenntnisse"
                  : "Important points and insights"}{" "}
                ({view.points.length})
              </summary>
              <Items
                items={view.points}
                locale={locale}
                newestFirst
                empty={de ? "Noch keine Punkte." : "No points yet."}
              />
            </details>
            <p className="muted live-draft-note">
              {de
                ? "KI-Entwurf. Belege bestätigen Originalzitate, nicht automatisch die Deutung. Abschnittszeiten sind keine Wortzeiten."
                : "AI draft. Evidence confirms the quotes, not necessarily the interpretation. Section times are not word timestamps."}
              {view.rejected > 0 &&
                (de
                  ? ` ${view.rejected} KI-Aussage(n) ohne gültigen Beleg wurden verworfen.`
                  : ` ${view.rejected} AI statement(s) without valid evidence were discarded.`)}
            </p>
          </div>
        )
      )}
    </aside>
  );
}

/**
 * Mirrors the backend transport rules so the start button is only offered for an endpoint the
 * backend accepts: no credentials or extra parameters; local is loopback HTTP(S), cloud HTTPS.
 */
export function endpointAllowed(mode: string, endpoint: string): boolean {
  let url: URL;
  try {
    url = new URL(endpoint.trim());
  } catch {
    return false;
  }
  if (url.username || url.password || url.search || url.hash) return false;
  if (mode === "cloud") return url.protocol === "https:";
  if (mode !== "local" || !["http:", "https:"].includes(url.protocol))
    return false;
  const host = url.hostname;
  return (
    host === "localhost" || host === "[::1]" || /^127(\.\d{1,3}){3}$/.test(host)
  );
}
