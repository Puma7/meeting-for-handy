import { useWorkspaceText, WorkspaceLanguage } from "./workspaceLocale.jsx";
import { workspaceStorageKey } from "../workspaceStorage";
import { Fragment, useEffect, useRef, useState } from "react";
import {
  Activity,
  ArrowLeft,
  ArrowDownUp,
  Bookmark,
  RotateCcw,
  Check,
  ChevronDown,
  ChevronUp,
  FileText,
  Lightbulb,
  ListChecks,
  Maximize2,
  Moon,
  Sun,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Square,
} from "lucide-react";
import { CustomOverview } from "./CustomOverview.jsx";
import { Notes } from "./Notes.jsx";
import SourceAudioPreview from "../SourceAudioPreview";
import { IconButton, Modal, Panel, Status } from "./ui.jsx";
import { usePanelSettings } from "./usePanelSettings.jsx";
import { formatTime } from "./data.mjs";
import {
  initialLiveState,
  loadLiveState,
  sharedWorkspaceContext,
  stableVisibleItems,
  workspaceStatusText,
  restoreSharedContext,
  sameSharedContext,
  acknowledgeSharedTopics,
  sharedContextSize,
  SHARED_CONTEXT_LIMIT,
  CONTEXT_BUDGET_MESSAGE,
} from "./liveWorkspace.mjs";

const tabs = [
  ["preparation", "Vorbereitung"],
  ["transcript", "Transkript"],
  ["overview", "Überblick"],
  ["details", "Details"],
  ["impulses", "Impulse"],
  ["history", "Verlauf"],
];
const statusName = {
  open: "Offen",
  recorded: "Belegt · KI",
  touched: "Angerissen · KI",
  discussed: "Besprochen",
  completed: "Laut Gespräch erledigt",
  withdrawn: "Zurückgenommen",
  unclear: "Zuordnung prüfen",
};

/** Present native/action failures consistently while preserving non-Error rejection values. */
function workspaceErrorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

/** Provide the application language to the isolated live-workspace renderer. */
export function LiveWorkspace(props) {
  return (
    <WorkspaceLanguage.Provider value={props.language ?? "de"}>
      <WorkspaceContents {...props} />
    </WorkspaceLanguage.Provider>
  );
}

/** Coordinate native progress, durable shared inputs and local UI state without exposing private notes. */
function WorkspaceContents({
  adapter,
  meeting,
  onBack,
  selectedFile,
  initialEndpoint = "",
  initialModel = "",
  mode = "local",
  theme = "light",
  onThemeChange,
  onModelSettings,
  onStopRecording,
  stopping = false,
  onTogglePause,
  paused = false,
  onToggleMute,
  muted = false,
  captureError,
  actionError,
  onDismissError,
  notices,
  language = "de",
}) {
  const t = useWorkspaceText();
  const layoutCatalogKey = "handymeeting.workspace.layout-catalog.v1";
  const [catalog, setCatalog] = useState(() => {
    const fallback = { defaultId: null, custom: [], names: {} };
    try {
      const raw = localStorage.getItem(layoutCatalogKey);
      if (!raw) return fallback;
      const saved = JSON.parse(raw);
      if (
        !Array.isArray(saved.custom) ||
        !saved.names ||
        saved.custom.some(
          ({ id, base }) =>
            !/^custom-[a-f0-9-]+$/.test(id) ||
            !["regie", "verlauf"].includes(base) ||
            typeof saved.names[id] !== "string",
        )
      )
        return fallback;
      return saved;
    } catch {
      return fallback;
    }
  });
  const [layoutId, setLayoutId] = useState(() => {
    if (
      ["verlauf", "regie", ...catalog.custom.map(({ id }) => id)].includes(
        catalog.defaultId,
      )
    )
      return catalog.defaultId;
    try {
      return localStorage.getItem("handymeeting.workspace.layout-preset.v1") ===
        "regie"
        ? "regie"
        : "verlauf";
    } catch {
      return "verlauf";
    }
  });
  const variant =
    catalog.custom.find(({ id }) => id === layoutId)?.base ?? layoutId;
  const layoutChoices = [
    { id: "verlauf", label: catalog.names.verlauf || t("Verlauf-Layout") },
    { id: "regie", label: catalog.names.regie || t("Regie-Layout") },
    ...catalog.custom.map(({ id }) => ({ id, label: catalog.names[id] })),
  ];
  const layoutPrefix = "handymeeting.workspace.user-layout.v1.";
  /** Select a named layout while retaining independently saved arrangements. */
  function chooseLayout(id) {
    setLayoutId(id);
    if (id === "regie" || id === "verlauf") {
      try {
        localStorage.setItem("handymeeting.workspace.layout-preset.v1", id);
      } catch {
        // The layout remains selectable for this window.
      }
    }
  }
  /** Apply explicit user layout actions and report invalid or conflicting names. */
  function manageLayout(action, name, layout) {
    if (
      action !== "default" &&
      layoutChoices.some(
        ({ id, label }) =>
          (action === "copy" || id !== layoutId) &&
          label.toLocaleLowerCase() === name.toLocaleLowerCase(),
      )
    )
      return t("Dieser Layoutname ist bereits vergeben.");
    const id = action === "copy" ? `custom-${crypto.randomUUID()}` : layoutId;
    const next = {
      defaultId: action === "default" ? id : catalog.defaultId,
      custom:
        action === "copy"
          ? [...catalog.custom, { id, base: variant }]
          : catalog.custom,
      names:
        action === "default" ? catalog.names : { ...catalog.names, [id]: name },
    };
    try {
      if (action === "copy")
        localStorage.setItem(
          `${layoutPrefix}${id}.layout.v1`,
          JSON.stringify(layout),
        );
      localStorage.setItem(layoutCatalogKey, JSON.stringify(next));
    } catch {
      return t("Layout konnte nicht gespeichert werden.");
    }
    setCatalog(next);
    if (action === "copy") chooseLayout(id);
    return null;
  }
  const storageKey = workspaceStorageKey(meeting.id);
  const [loaded] = useState(() => {
    try {
      const raw = localStorage.getItem(storageKey);
      return { ...loadLiveState(raw), hasSaved: Boolean(raw) };
    } catch {
      return { state: initialLiveState(), blocked: true, hasSaved: true };
    }
  });
  const [state, setState] = useState(loaded.state),
    [storageBlocked, setStorageBlocked] = useState(loaded.blocked);
  const [storageReady, setStorageReady] = useState(false);
  const hydrated = useRef(false),
    blocked = useRef(storageBlocked);
  blocked.current = storageBlocked;
  const [tab, setTab] = useState(selectedFile ? "transcript" : "overview"),
    [focusedFile, setFocusedFile] = useState(selectedFile),
    [modal, setModal] = useState(null);
  const endpoint = initialEndpoint;
  const model = initialModel;
  const dark =
    theme === "dark" ||
    (theme === "system" &&
      window.matchMedia?.("(prefers-color-scheme: dark)")?.matches);
  const [minutes, setMinutes] = useState(3);
  const [status, setStatus] = useState(null),
    [error, setError] = useState(null),
    [busy, setBusy] = useState(false);
  const [pollError, setPollError] = useState(null);
  const [viewportHeight, setViewportHeight] = useState(window.innerHeight),
    [panelViews, setPanelViews] = useState({});
  const current = useRef(state),
    context = useRef(null),
    contextRun = useRef(null),
    contextQueue = useRef(Promise.resolve()),
    latestStatus = useRef(null),
    polling = useRef(false),
    alive = useRef(true);
  current.current = state;
  const workspace = status?.view?.workspace;
  const entities = workspace?.entities || [],
    originals = workspace?.originals || [];
  const lastEvidenceEnd = originals.reduce(
    (end, source) =>
      workspace?.supersededSources?.includes(source.id)
        ? end
        : Math.max(end, source.endMs),
    0,
  );
  /** Hydrate authoritative native progress and context, then synchronize only committed inputs when idle. */
  async function poll() {
    if (polling.current) return;
    polling.current = true;
    try {
      let next;
      try {
        next = await adapter.status(meeting.id);
      } catch (e) {
        if (alive.current) setPollError(workspaceErrorMessage(e));
        return;
      }
      if (!alive.current) return;
      setPollError(null);
      const runId = next.view?.runId ?? null;
      const native = runId ? next.workspaceContext : null;
      const sameRun = runId === contextRun.current;
      if (
        native &&
        sameRun &&
        native.revision < (context.current?.revision ?? 0)
      )
        return;
      latestStatus.current = next;
      setStatus(next);
      if (
        !hydrated.current ||
        (native &&
          (!sameRun || native.revision > (context.current?.revision ?? -1)))
      ) {
        const restored = restoreSharedContext(current.current, native, runId);
        current.current = restored;
        setState(restored);
        hydrated.current = true;
        setStorageReady(true);
      }
      contextRun.current = native ? runId : null;
      context.current = native ?? null;
      if (native) await syncContext();
    } catch (e) {
      if (alive.current) setError(workspaceErrorMessage(e));
    } finally {
      polling.current = false;
    }
  }
  /** Publish released inputs using the current native revision, preserving concurrent edits. */
  function syncContext(extra) {
    const runId = contextRun.current;
    const send = async () => {
      if (
        !context.current ||
        !runId ||
        runId !== contextRun.current ||
        !alive.current ||
        blocked.current ||
        !hydrated.current
      )
        return false;
      // Older browser caches may contain an inadmissible pending note. Retain its
      // text locally, restore the last native note as the shared snapshot, and let
      // independent topic revocations proceed instead of retrying invalid writes.
      if (
        current.current.protocolShare?.pending &&
        sharedContextSize(sharedWorkspaceContext(current.current)) >
          SHARED_CONTEXT_LIMIT
      ) {
        const local = current.current;
        const pendingText = local.protocolShare.text;
        const saved = {
          ...local,
          protocolShare: {
            text: context.current.sharedNotes.map((note) => note.text).join(""),
            revision: Math.max(
              1,
              ...context.current.sharedNotes.map((note) => note.revision),
            ),
            pending: false,
          },
          history: local.history.some(
            (entry) =>
              entry.noteKind === "protocol" && entry.content === pendingText,
          )
            ? local.history
            : [
                ...local.history,
                {
                  id: `note-${Date.now()}`,
                  at: new Date().toISOString(),
                  label: t("Notizstand gesichert"),
                  noteKind: "protocol",
                  content: pendingText,
                },
              ],
        };
        current.current = saved;
        setState(saved);
        setError(t(CONTEXT_BUDGET_MESSAGE));
      }
      const updating = latestStatus.current?.status === "updating";
      if (updating && !extra) return false;
      const next = extra
        ? {
            ...context.current,
            overrides: [...(context.current.overrides || []), extra],
          }
        : sharedWorkspaceContext(
            current.current,
            context.current.revision,
            extra
              ? [...(context.current.overrides || []), extra]
              : context.current.overrides || [],
          );
      if (sameSharedContext(next, context.current)) {
        if (
          current.current.protocolShare?.pending ||
          current.current.topics.some((topic) => topic.pending)
        ) {
          const saved = {
            ...acknowledgeSharedTopics(current.current, context.current.agenda),
            ...(current.current.protocolShare
              ? {
                  protocolShare: {
                    ...current.current.protocolShare,
                    pending: false,
                  },
                }
              : {}),
          };
          current.current = saved;
          setState(saved);
        }
        return true;
      }
      if (sharedContextSize(next) > SHARED_CONTEXT_LIMIT) {
        if (extra) throw new Error(t(CONTEXT_BUDGET_MESSAGE));
        setError(t(CONTEXT_BUDGET_MESSAGE));
        return false;
      }
      const accepted = await adapter.context(next, runId);
      // State-derived inputs remain pending for a later poll; an extra human
      // override must be explicitly retried against the then-current revision.
      if (accepted === false) return false;
      if (contextRun.current !== runId || !alive.current) return false;
      context.current = { ...next, revision: next.revision + 1 };
      const saved = {
        ...acknowledgeSharedTopics(current.current, next.agenda),
        nativeContextRevision: context.current.revision,
        nativeContextRunId: runId,
        ...(current.current.protocolShare?.pending &&
        sameSharedContext(
          { sharedNotes: sharedWorkspaceContext(current.current).sharedNotes },
          { sharedNotes: next.sharedNotes },
        )
          ? {
              protocolShare: {
                ...current.current.protocolShare,
                pending: false,
              },
            }
          : {}),
      };
      current.current = saved;
      setState(saved);
      return true;
    };
    const pending = contextQueue.current.then(send, send);
    contextQueue.current = pending.catch(() => {});
    return pending;
  }
  useEffect(() => {
    alive.current = true;
    void poll();
    const timer = setInterval(() => {
      void poll();
    }, 2000);
    const resize = () => setViewportHeight(window.innerHeight);
    window.addEventListener("resize", resize);
    return () => {
      alive.current = false;
      clearInterval(timer);
      window.removeEventListener("resize", resize);
    };
  }, [adapter, meeting.id]);
  useEffect(() => {
    if (storageReady && !storageBlocked)
      try {
        localStorage.setItem(storageKey, JSON.stringify(state));
      } catch {
        setStorageBlocked(true);
      }
  }, [state, storageKey, storageBlocked, storageReady]);
  const mutate = (action) => setState((previous) => action(previous));
  /** Reject newly over-budget topic sharing while allowing local drafts and reductions. */
  function changeTopics(action) {
    const next = action(current.current);
    const size = sharedContextSize(sharedWorkspaceContext(next));
    if (
      size > SHARED_CONTEXT_LIMIT &&
      size > sharedContextSize(sharedWorkspaceContext(current.current))
    ) {
      setError(t(CONTEXT_BUDGET_MESSAGE));
      return;
    }
    current.current = next;
    setState(next);
  }
  /** Run one user-requested native action and refresh its authoritative status. */
  async function action(fn) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await fn();
      await poll();
    } catch (e) {
      setError(workspaceErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  /** Append a human status override using the entity's current stable revision. */
  async function override(item, value) {
    if (!context.current) {
      setError(
        t("Für diese Änderung zuerst den gespeicherten Meetingstand öffnen."),
      );
      return;
    }
    try {
      const published = await syncContext({
        id: item.id,
        expectedRevision: item.revision,
        status: value,
        saved: Boolean(state.impulseStates[item.id]?.saved),
      });
      if (!alive.current) return;
      if (published === false)
        setError(
          t(
            "Die Änderung wurde nicht übernommen. Bitte aktuellen Auswertungsstand abwarten und erneut versuchen.",
          ),
        );
      else setError(null);
      await poll();
    } catch (e) {
      setError(workspaceErrorMessage(e));
    }
  }
  useEffect(() => {
    if (selectedFile) openTranscript(selectedFile);
  }, [selectedFile]);

  /** Navigate to the original transcript slice referenced by a selected source. */
  function openTranscript(file) {
    setFocusedFile(file);
    setPanelViews((previous) => ({
      ...previous,
      transcript: {
        ...previous.transcript,
        query: "",
        track: "all",
        minutes: "all",
        fromMinute: "",
        toMinute: "",
      },
    }));
    setTab("transcript");
    setModal(null);
  }

  const ctx = {
    _mutate: mutate,
    changeTopics,
    state,
    storageKey,
    storageBlocked,
    viewportHeight,
    format: "window",
    compact: false,
    snapshot: workspace?.revision || 0,
    chatAvailable: false,
    panelViews,
    focusedFile,
    openTranscript,
    setPanelView: (id, patch) =>
      setPanelViews((v) => ({ ...v, [id]: { ...v[id], ...patch } })),
    openModal: setModal,
    onSource: (id) =>
      setModal((previous) => ({ type: "source", id, back: previous })),
    setNote: (kind, text) =>
      mutate((v) => ({
        ...v,
        notes: { ...v.notes, [kind]: text },
        ...(kind === "protocol"
          ? {
              protocolShare: v.protocolShare ?? {
                text: v.notes.protocol,
                revision: v.protocolRevision || 1,
                pending: false,
              },
            }
          : {}),
        protocolRevision:
          (v.protocolRevision || 1) + (kind === "protocol" ? 1 : 0),
      })),
    commitNote: (kind) => {
      const v = current.current;
      const saved = {
        ...v,
        ...(kind === "protocol"
          ? {
              protocolShare: {
                text: v.notes.protocol,
                revision: v.protocolRevision || 1,
                pending: true,
              },
            }
          : {}),
        history:
          v.history.at(-1)?.content === v.notes[kind] &&
          v.history.at(-1)?.noteKind === kind
            ? v.history
            : [
                ...v.history,
                {
                  id: `note-${Date.now()}`,
                  at: new Date().toISOString(),
                  label: t("Notizstand gesichert"),
                  noteKind: kind,
                  content: v.notes[kind],
                },
              ],
      };
      const rejected =
        kind === "protocol" &&
        sharedContextSize(sharedWorkspaceContext(saved)) > SHARED_CONTEXT_LIMIT;
      if (rejected) {
        saved.protocolShare = v.protocolShare;
        setError(t(CONTEXT_BUDGET_MESSAGE));
      }
      current.current = saved;
      setState(saved);
      if (kind === "protocol" && !rejected)
        void syncContext().catch((error) =>
          setError(workspaceErrorMessage(error)),
        );
    },
  };
  /** Open an original source quote with explicit recognition-revision context. */
  function quote(item) {
    return (
      <div className="live-evidence">
        {item.evidence?.map((ev, index) => (
          <button
            type="button"
            className="quote text-button"
            key={`${ev.sourceId}-${index}`}
            onClick={() => ctx.onSource(ev.sourceId)}
          >
            „{ev.quote}“ <span aria-hidden="true">↗</span>
          </button>
        ))}
      </div>
    );
  }
  /** Render a module with the same native state and source actions as its dedicated tab. */
  function renderPanel(panel, expanded = false) {
    if (panel === "topics")
      return (
        <LiveTopics
          ctx={ctx}
          entities={entities}
          quote={quote}
          override={override}
          full={expanded}
          onExpand={() =>
            setModal({ type: "panel", panel, title: t("Vorbereitung") })
          }
        />
      );
    return (
      <LivePanel
        key={panel}
        ctx={ctx}
        panel={panel}
        workspace={workspace}
        meeting={meeting}
        quote={quote}
        override={override}
        full={expanded}
        onExpand={() =>
          setModal({
            type: "panel",
            panel,
            title: {
              summary: t("Live-Überblick"),
              relevant: t("Jetzt relevant"),
              changes: t("Änderungen im Gespräch"),
              impulses: t("Gesprächsimpulse"),
              details: t("Details"),
              transcript: t("Transkript"),
            }[panel],
          })
        }
      />
    );
  }
  const source =
    modal?.type === "source" && originals.find((s) => s.id === modal.id);
  const sourceParts =
    source && !workspace?.supersededSources?.includes(source.id)
      ? meeting.parts.filter(
          (part) =>
            part.file &&
            part.source === source.source &&
            Math.floor(part.startSample / 16) <= source.startMs &&
            (part.startSample + part.sampleCount) / 16 >= source.endMs &&
            (part.committedText ?? part.text ?? "").includes(source.text),
        )
      : [];
  const classicRunning = Boolean(status?.running && !status.workspaceContext);
  return (
    <div
      className={`prototype theme-${dark ? "dark" : "light"} format-window live-workspace`}
    >
      <div className="prototype-toolbar">
        <button className="text-button" onClick={onBack}>
          <ArrowLeft size={14} />
          {t("Zur Standardansicht")}
        </button>
        <div className="live-update-bar">
          {(onStopRecording ||
            ["recording", "streaming", "catching-up"].includes(
              meeting.status,
            )) && (
            <span className="workspace-capture-state">
              {onStopRecording ||
              ["recording", "streaming"].includes(meeting.status)
                ? captureError
                  ? t("Aufnahme prüfen")
                  : paused
                    ? t("Aufnahme pausiert")
                    : t("Aufnahme läuft")
                : t("Erkennung holt nach")}
            </span>
          )}
          {meeting.modelId === "parakeet-ultra" &&
            ["recording", "streaming", "catching-up"].includes(
              meeting.status,
            ) && (
              <span className="workspace-capture-state">
                {language === "de"
                  ? "Abschnittstranskription"
                  : "Section transcription"}
                {meeting.pendingAudioMs > 0 &&
                  ` · ${language === "de" ? "offenes Audio" : "audio pending"} ${formatTime(meeting.pendingAudioMs / 1000)}`}
              </span>
            )}
          {onStopRecording && (
            <button
              type="button"
              className="subtle-button"
              disabled={stopping}
              onClick={onStopRecording}
            >
              {t("Aufnahme beenden")}
            </button>
          )}
          {onTogglePause && (
            <button
              type="button"
              className="subtle-button"
              disabled={stopping}
              onClick={onTogglePause}
            >
              {paused ? t("Aufnahme fortsetzen") : t("Aufnahme pausieren")}
            </button>
          )}
          {onToggleMute && (
            <button
              type="button"
              className="subtle-button"
              disabled={stopping}
              onClick={onToggleMute}
            >
              {muted ? t("Mikrofon aktivieren") : t("Mikrofon stummschalten")}
            </button>
          )}
          {(meeting.captureIncomplete || meeting.warnings?.length > 0) && (
            <button type="button" className="subtle-button" onClick={onBack}>
              {t("Aufnahme prüfen")}
            </button>
          )}
          {onModelSettings && (
            <IconButton
              label={t("Textmodell einrichten")}
              title={t(
                "Öffnet die Einstellungen für Anbieter und Modell der Meeting-Auswertung.",
              )}
              onClick={onModelSettings}
            >
              <Settings2 size={16} />
            </IconButton>
          )}
          <span role="status">
            {classicRunning
              ? t("Auswertung läuft in der bisherigen Ansicht.")
              : storageReady
                ? mode === "cloud" && !status?.running
                  ? t("Cloud-Modell in den Einstellungen ausgewählt")
                  : workspaceStatusText(status, language)
                : t("Gespeicherte Auswertung wird geladen …")}
            {lastEvidenceEnd > 0 &&
              ` · ${t("Belege bis")} ${formatTime(lastEvidenceEnd / 1000)}`}
            {status?.running &&
              status.pendingChars > 0 &&
              ` · ${status.pendingChars.toLocaleString(language)} ${t("Zeichen noch nicht ausgewertet")}`}
          </span>
          <label>
            {t("Intervall")}{" "}
            <select
              aria-label={t("Auswertungsintervall")}
              title={t("Legt fest, wie oft die KI den Überblick aktualisiert.")}
              disabled={!storageReady || classicRunning}
              value={minutes}
              onChange={(e) => {
                const value = Number(e.target.value);
                setMinutes(value);
                if (status?.running)
                  void action(() => adapter.interval(value * 60000));
              }}
            >
              <option value={2}>{t("2 Min.")}</option>
              <option value={3}>{t("3 Min.")}</option>
            </select>
          </label>
          {status?.running ? (
            <>
              <button
                disabled={busy || classicRunning || status.refreshRequested}
                className="subtle-button"
                onClick={() => action(() => adapter.refresh())}
              >
                <RefreshCw size={13} />
                {status.refreshRequested ? t("Vorgemerkt") : t("Aktualisieren")}
              </button>
              <IconButton
                label={t("Nur Auswertung beenden")}
                title={t(
                  "Stoppt die KI-Auswertung. Eine laufende Aufnahme geht weiter.",
                )}
                disabled={busy}
                onClick={() => action(() => adapter.stop())}
              >
                <Square size={14} />
              </IconButton>
            </>
          ) : (
            <button
              className="subtle-button"
              disabled={
                busy ||
                !storageReady ||
                storageBlocked ||
                mode !== "local" ||
                !endpoint.trim() ||
                !model.trim()
              }
              onClick={() =>
                action(async () => {
                  const shared = sharedWorkspaceContext(state);
                  if (sharedContextSize(shared) > SHARED_CONTEXT_LIMIT)
                    throw new Error(t(CONTEXT_BUDGET_MESSAGE));
                  await adapter.start({
                    meetingId: meeting.id,
                    endpoint,
                    model,
                    mode: "local",
                    cloudConsent: false,
                    apiKey: "",
                    useSavedKey: false,
                    intervalMs: minutes * 60000,
                    language,
                    workspace: shared,
                  });
                })
              }
            >
              {mode === "cloud"
                ? t("Erweiterte Auswertung nur lokal verfügbar")
                : t("Lokale Auswertung starten")}
            </button>
          )}
        </div>
        <IconButton
          label={
            dark
              ? t("Helles Design aktivieren")
              : t("Dunkles Design aktivieren")
          }
          onClick={() => onThemeChange?.(dark ? "light" : "dark")}
        >
          {dark ? <Sun size={16} /> : <Moon size={16} />}
        </IconButton>
      </div>
      <div className="prototype-stage">
        <div className="meeting-app">
          <div className="app-interior">
            <div className="meeting-workspace">
              {notices && (
                <div className="workspace-native-notices">{notices}</div>
              )}
              {captureError && (
                <div className="state-banner warning" role="alert">
                  {captureError}
                </div>
              )}
              {(error || pollError || status?.error) && (
                <div className="state-banner warning" role="status">
                  {error || pollError || status?.error}
                </div>
              )}
              {status?.view?.damagedRuns > 0 && (
                <div
                  className="state-banner warning"
                  role="status"
                  aria-label={t("Gespeicherte Auswertungen")}
                >
                  {t("Gespeicherte Auswertungen konnten nicht geprüft werden.")}{" "}
                  {status.view.workspace
                    ? t("Ein früherer lesbarer Stand wird angezeigt.")
                    : t(
                        "Gespeicherte Ergebnisse sind derzeit nicht verfügbar.",
                      )}{" "}
                  {t(
                    "Die Dateien bleiben unverändert erhalten. Prüfe sie in der Standardansicht.",
                  )}
                </div>
              )}
              {storageBlocked && (
                <div className="state-banner warning">
                  {t(
                    "Der lokale Notizstand ist nicht lesbar oder beschreibbar. Vorhandene Daten bleiben erhalten; neue Änderungen nur in diesem Fenster.",
                  )}
                </div>
              )}
              {actionError && (
                <div role="alert" className="state-banner warning">
                  <span>{actionError}</span>
                  <button type="button" onClick={onDismissError}>
                    {language === "en"
                      ? "Dismiss message"
                      : "Meldung schließen"}
                  </button>
                </div>
              )}
              <nav
                className="tabs"
                role="tablist"
                aria-label={t("Meetingbereiche")}
              >
                {tabs.map(([id, name]) => (
                  <button
                    role="tab"
                    key={id}
                    aria-selected={tab === id}
                    onClick={() => setTab(id)}
                  >
                    {t(name)}
                  </button>
                ))}
              </nav>
              <main
                role="tabpanel"
                aria-label={t(tabs.find(([id]) => id === tab)?.[1])}
                className={`workspace-content tab-${tab}`}
              >
                {tab === "overview" ? (
                  <CustomOverview
                    key={layoutId}
                    variant={variant}
                    storageKey={`${layoutPrefix}${layoutId}`}
                    mergeRelevant
                    manageLayout={manageLayout}
                    layoutLabel={
                      layoutChoices.find(({ id }) => id === layoutId).label
                    }
                    isDefaultLayout={catalog.defaultId === layoutId}
                    layoutPicker={
                      <select
                        aria-label={t("Überblick-Layout")}
                        title={t(
                          "Zwischen deinen gespeicherten Kachelanordnungen wechseln",
                        )}
                        value={layoutId}
                        onChange={(event) => chooseLayout(event.target.value)}
                      >
                        {layoutChoices.map(({ id, label }) => (
                          <option key={id} value={id}>
                            {label}
                            {catalog.defaultId === id ? t(" · Standard") : ""}
                          </option>
                        ))}
                      </select>
                    }
                    renderPanel={renderPanel}
                    onExpand={(panel, title) =>
                      setModal({ type: "panel", panel, title })
                    }
                  />
                ) : tab === "preparation" ? (
                  <fieldset
                    disabled={!storageReady}
                    className="preparation-layout workspace-editors"
                  >
                    {renderPanel("topics", true)}
                    <Panel title={t("Meine Vorbereitung")}>
                      <label>
                        {t("Mein Ziel")}
                        <textarea
                          rows={4}
                          value={state.preparation.goal}
                          onChange={(e) =>
                            mutate((v) => ({
                              ...v,
                              preparation: {
                                ...v.preparation,
                                goal: e.target.value,
                              },
                            }))
                          }
                        />
                      </label>
                      <label>
                        {t("Private Vorbereitung")}
                        <textarea
                          rows={6}
                          value={state.preparation.agenda}
                          onChange={(e) =>
                            mutate((v) => ({
                              ...v,
                              preparation: {
                                ...v.preparation,
                                agenda: e.target.value,
                              },
                            }))
                          }
                        />
                      </label>
                      <p className="muted">
                        {t(
                          "Nur freigegebene Gesprächspunkte und Protokollnotizen gehen an das Modell.",
                        )}
                      </p>
                    </Panel>
                  </fieldset>
                ) : (
                  renderPanel(
                    {
                      history: "changes",
                      transcript: "transcript",
                      details: "details",
                      impulses: "impulses",
                    }[tab],
                    true,
                  )
                )}
              </main>
              {storageReady && <Notes ctx={ctx} />}
              <div className="app-footnote">
                {originals.length} {t("Originalstände gesichert")}
                {workspace?.contextExcludedIds?.length
                  ? ` · ${workspace.contextExcludedIds.length} ${t("frühere Einträge nur im Verlauf")}`
                  : ""}{" "}
                · {status?.view?.model || t("kein Textmodell aktiv")}{" "}
                {t("· KI-Entwürfe mit Originalbelegen")}
              </div>
            </div>
          </div>
        </div>
      </div>
      {modal?.type === "panel" && (
        <Modal title={modal.title} wide onClose={() => setModal(null)}>
          {renderPanel(modal.panel, true)}
        </Modal>
      )}
      {modal?.type === "source" && (
        <Modal
          title={t("Originalbeleg")}
          onClose={() => setModal(modal.back || null)}
        >
          {source ? (
            <>
              <div className="source-meta">
                {source.source === "microphone"
                  ? t("Mikrofon")
                  : t("Computerton")}{" "}
                ·{" "}
                {workspace?.supersededSources?.includes(source.id) &&
                  t("Früherer Transkriptstand · ")}
                {formatTime(source.startMs / 1000)}–
                {formatTime(source.endMs / 1000)} {t("· Abschnittszeiten")}
              </div>
              <blockquote className="source-quote">{source.text}</blockquote>
              {sourceParts.length === 1 && (
                <div className="source-actions">
                  <button onClick={() => openTranscript(sourceParts[0].file)}>
                    {t("Im Transkript ansehen")}
                  </button>
                  {adapter.sourceAudio && (
                    <SourceAudioPreview
                      key={source.id}
                      file={sourceParts[0].file}
                      load={adapter.sourceAudio}
                      offsetSeconds={Math.max(
                        0,
                        source.startMs / 1000 -
                          sourceParts[0].startSample / 16000,
                      )}
                    />
                  )}
                </div>
              )}
            </>
          ) : (
            <p>
              {t("Der Originalabschnitt ist in diesem Stand nicht verfügbar.")}
            </p>
          )}
        </Modal>
      )}
    </div>
  );
}

/** Edit local preparation topics and explicitly control which topics enter model context. */
function LiveTopics({ ctx, entities, quote, override, full, onExpand }) {
  const t = useWorkspaceText();
  const [view, patch] = usePanelSettings(ctx, "topics", {
    draft: "",
    filter: "all",
  });
  const topics = ctx.state.topics.map((topic) => ({
    ...topic,
    native: topic.shared ? entities.find((e) => e.id === topic.id) : undefined,
  }));
  const done = topics.filter(
    (topic) => topic.native?.status === "discussed",
  ).length;
  /** Apply a preparation edit locally while retaining its explicit sharing state. */
  function change(fn) {
    ctx.changeTopics(fn);
  }
  return (
    <Panel
      title={t("Vorbereitung")}
      subtitle={`${topics.length - done} ${t("offen")}`}
      icon={ListChecks}
      onExpand={full ? undefined : onExpand}
    >
      <div className="topic-list scroll-region">
        {!topics.length && (
          <p className="empty-inline">
            {t("Eigene Gesprächspunkte vor dem Meeting ergänzen.")}
          </p>
        )}
        {topics.map((topic) => (
          <article className="topic-row" key={topic.id}>
            <div className="topic-line">
              <div className="result-title">
                {" "}
                <button
                  className="topic-name"
                  aria-expanded={view.expanded === topic.id}
                  onClick={() =>
                    patch({
                      expanded: view.expanded === topic.id ? null : topic.id,
                    })
                  }
                >
                  {topic.text}
                </button>
                <Status
                  kind={topic.native?.status === "discussed" ? "done" : "open"}
                >
                  {topic.native
                    ? `${t(statusName[topic.native.status])}${topic.native.humanLocked ? t(" · manuell") : ""}`
                    : t("Noch nicht abgeglichen")}
                </Status>{" "}
              </div>
              <label>
                <input
                  type="checkbox"
                  checked={Boolean(topic.shared)}
                  onChange={(e) =>
                    change((v) => ({
                      ...v,
                      topics: v.topics.map((x) =>
                        x.id === topic.id
                          ? { ...x, shared: e.target.checked, pending: true }
                          : x,
                      ),
                    }))
                  }
                />
                {t("Freigeben")}
              </label>
              {topic.pending && (
                <span className="heading-meta" role="status">
                  {t("Vorgemerkt")}
                </span>
              )}
            </div>
            {view.expanded === topic.id && topic.native && (
              <>
                {quote(topic.native)}
                <button
                  className="text-button"
                  onClick={() =>
                    override(
                      topic.native,
                      topic.native.status === "discussed"
                        ? "open"
                        : "discussed",
                    )
                  }
                >
                  {topic.native.status === "discussed"
                    ? t("Wieder öffnen")
                    : t("Als besprochen markieren")}
                </button>
              </>
            )}
          </article>
        ))}
      </div>
      {full && (
        <form
          className="add-topic"
          onSubmit={(e) => {
            e.preventDefault();
            if (!view.draft?.trim()) return;
            const text = view.draft.trim();
            change((v) => ({
              ...v,
              topics: [
                ...v.topics,
                {
                  id: `e_agenda_${Date.now()}`,
                  revision: 1,
                  text,
                  shared: false,
                },
              ],
            }));
            patch({ draft: "" });
          }}
        >
          <input
            aria-label={t("Neuer Gesprächspunkt")}
            maxLength={180}
            value={view.draft || ""}
            onChange={(e) => patch({ draft: e.target.value })}
            placeholder={t("Eigenen Gesprächspunkt ergänzen …")}
          />
          <IconButton type="submit" label={t("Gesprächspunkt hinzufügen")}>
            <Plus size={16} />
          </IconButton>
        </form>
      )}
      <div className="panel-foot">
        {t("Nur freigegebene Gesprächspunkte werden automatisch abgeglichen.")}
      </div>
    </Panel>
  );
}

/** Render source-linked overview, details, prompts or history without inventing missing evidence. */
function LivePanel({
  ctx,
  panel,
  workspace,
  meeting,
  quote,
  override,
  full,
  onExpand,
}) {
  const t = useWorkspaceText();
  const [view, patch] = usePanelSettings(ctx, panel, {
    query: "",
    ascending: true,
    count: 5,
    track: "all",
    minutes: "all",
  });
  const titles = {
    summary: t("Live-Überblick"),
    details: t("Wichtige Details"),
    impulses: t("Gesprächsimpulse"),
    relevant: t("Jetzt relevant"),
    changes: t("Änderungen im Gespräch"),
    transcript: t("Transkript"),
  };
  const icons = {
    summary: FileText,
    details: ListChecks,
    impulses: Lightbulb,
    relevant: Activity,
    changes: Activity,
    transcript: FileText,
  };
  let items = stableVisibleItems(
    workspace,
    panel === "impulses" ? "impulse" : null,
    view.query || "",
  );
  const relevant =
    panel === "summary"
      ? items.filter(
          (item) =>
            item.attention !== "normal" &&
            !["completed", "withdrawn", "discussed"].includes(item.status),
        )
      : [];
  if (panel === "summary") {
    const overview = (workspace?.overviewRefs || [])
      .map((id) => items.find((item) => item.id === id))
      .filter(Boolean);
    items = [
      ...relevant,
      ...overview.filter((item) => !relevant.some(({ id }) => id === item.id)),
    ];
  }
  if (panel === "details")
    items = items.filter((e) => !["agenda", "impulse"].includes(e.kind));
  if (panel === "relevant")
    items = items.filter(
      (e) =>
        e.attention !== "normal" &&
        !["completed", "withdrawn", "discussed"].includes(e.status),
    );
  const impulseTotal = panel === "impulses" ? items.length : 0;
  const impulseLimit = Math.min(
    impulseTotal || 1,
    Math.max(1, view.count || 5),
  );
  if (panel === "impulses") items = items.slice(0, impulseLimit);
  const needle = (view.query || "").trim().toLocaleLowerCase();
  const transcript = (meeting.parts || [])
    .map((part) => ({
      ...part,
      text: part.status === "complete" ? part.text : part.committedText,
    }))
    .filter((part) => part.text?.trim());
  const latestEnd = transcript.reduce(
    (end, part) =>
      Math.max(end, (part.startSample + (part.sampleCount || 0)) / 16000),
    0,
  );
  const custom = view.minutes === "custom";
  const fromSeconds = custom
    ? Number(view.fromMinute || 0) * 60
    : view.minutes && view.minutes !== "all"
      ? Math.max(0, latestEnd - Number(view.minutes) * 60)
      : 0;
  const toSeconds =
    custom && view.toMinute != null && view.toMinute !== ""
      ? Number(view.toMinute) * 60
      : Infinity;
  const invalidRange =
    !Number.isFinite(fromSeconds) ||
    fromSeconds < 0 ||
    Number.isNaN(toSeconds) ||
    toSeconds <= fromSeconds;
  const direction = view.ascending === false ? -1 : 1;
  const transcriptRows = transcript
    .filter(
      (part) =>
        !invalidRange &&
        (!needle || part.text.toLocaleLowerCase().includes(needle)) &&
        (view.track === "all" || !view.track || part.source === view.track) &&
        ((part.startSample + (part.sampleCount || 0)) / 16000 > fromSeconds ||
          (!part.sampleCount && part.startSample / 16000 === fromSeconds)) &&
        part.startSample / 16000 < toSeconds,
    )
    .sort((a, b) => direction * (a.startSample - b.startSample));
  const timeline = (workspace?.events || [])
    .filter(
      (event) =>
        (!view.eventStatus ||
          view.eventStatus === "all" ||
          event.status === view.eventStatus) &&
        (!needle ||
          `${event.text} ${t(statusName[event.status]) || event.status}`
            .toLocaleLowerCase()
            .includes(needle)),
    )
    .sort((a, b) => direction * (a.atMs - b.atMs));
  const anchor = useRef(null);
  useEffect(() => {
    if (panel === "transcript" && full && ctx.focusedFile) {
      anchor.current?.scrollIntoView?.({ block: "nearest" });
      anchor.current?.focus?.({ preventScroll: true });
    }
  }, [
    panel,
    full,
    ctx.focusedFile,
    transcriptRows.some((part) => part.file === ctx.focusedFile),
  ]);
  return (
    <Panel
      title={titles[panel]}
      icon={icons[panel]}
      onExpand={full ? undefined : onExpand}
      actions={
        panel === "impulses" && (
          <label className="count-select">
            {t("Anzahl")}
            <input
              type="number"
              min="1"
              max={impulseTotal || 1}
              aria-label={t("Anzahl Gesprächsimpulse")}
              disabled={!impulseTotal}
              value={view.countInput ?? impulseLimit}
              onChange={(e) => {
                const count = e.target.valueAsNumber;
                patch({
                  countInput: e.target.value,
                  ...(count > 0
                    ? { count: Math.min(impulseTotal, Math.floor(count)) }
                    : {}),
                });
              }}
              onBlur={() => patch({ countInput: undefined })}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
              }}
            />
            <span>
              {t("von")} {impulseTotal}
            </span>
          </label>
        )
      }
    >
      {["transcript", "changes", "details"].includes(panel) && (
        <div className="live-transcript-controls compact-filters">
          <label className="search-input compact-search">
            <Search size={14} />
            <input
              aria-label={`${titles[panel]} ${t("durchsuchen")}`}
              placeholder={t("Suchen …")}
              value={view.query || ""}
              onChange={(e) => patch({ query: e.target.value })}
            />
          </label>
          {panel === "transcript" && (
            <>
              {" "}
              <select
                aria-label={t("Transkriptspur")}
                value={view.track || "all"}
                onChange={(e) => patch({ track: e.target.value })}
              >
                <option value="all">{t("Beide Spuren")}</option>
                <option value="system">{t("Computerton")}</option>
                <option value="microphone">{t("Mikrofon")}</option>
              </select>
              <select
                aria-label={t("Transkriptzeitraum")}
                value={view.minutes || "all"}
                onChange={(e) => patch({ minutes: e.target.value })}
              >
                <option value="all">{t("Gesamt")}</option>
                {[5, 10, 15, 30, 60].map((value) => (
                  <option key={value} value={value}>
                    {t("Letzte")} {value} {t("Min.")}
                  </option>
                ))}
                <option value="custom">{t("Individueller Zeitraum")}</option>
              </select>
              <button
                type="button"
                className="subtle-button sort-toggle"
                aria-label={
                  view.ascending === false
                    ? t("Neueste zuerst")
                    : t("Älteste zuerst")
                }
                title={
                  view.ascending === false
                    ? t("Neueste zuerst · Reihenfolge umkehren")
                    : t("Älteste zuerst · Reihenfolge umkehren")
                }
                onClick={() => patch({ ascending: view.ascending === false })}
              >
                <ArrowDownUp size={13} />
              </button>
            </>
          )}
          {panel === "changes" && (
            <>
              {" "}
              <select
                aria-label={t("Verlauf nach Status filtern")}
                value={view.eventStatus || "all"}
                onChange={(e) => patch({ eventStatus: e.target.value })}
              >
                <option value="all">{t("Alle Status")}</option>
                {Object.entries(statusName).map(([id, label]) => (
                  <option key={id} value={id}>
                    {t(label)}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="subtle-button sort-toggle"
                aria-label={
                  view.ascending === false
                    ? t("Neueste zuerst")
                    : t("Älteste zuerst")
                }
                title={
                  view.ascending === false
                    ? t("Neueste zuerst · Reihenfolge umkehren")
                    : t("Älteste zuerst · Reihenfolge umkehren")
                }
                onClick={() => patch({ ascending: view.ascending === false })}
              >
                <ArrowDownUp size={13} />
              </button>
            </>
          )}
        </div>
      )}
      {panel === "transcript" && custom && (
        <div className="custom-time-range">
          <label>
            {t("Von (Min.)")}
            <input
              type="number"
              min="0"
              step="any"
              value={view.fromMinute ?? ""}
              onChange={(e) => patch({ fromMinute: e.target.value })}
            />
          </label>
          <label>
            {t("Bis (Min.)")}
            <input
              type="number"
              min="0"
              step="any"
              value={view.toMinute ?? ""}
              onChange={(e) => patch({ toMinute: e.target.value })}
            />
          </label>
          {invalidRange && (
            <span role="status">
              {t("Das Ende muss nach dem Anfang liegen.")}
            </span>
          )}
        </div>
      )}
      <div className="scroll-region facts-list live-native-list">
        {panel === "transcript"
          ? transcriptRows.map((p, index) => (
              <article
                className={`transcript-row${p.file && p.file === ctx.focusedFile ? " source-focused" : ""}`}
                key={`${p.file}-${index}`}
                ref={p.file && p.file === ctx.focusedFile ? anchor : undefined}
                tabIndex={p.file && p.file === ctx.focusedFile ? -1 : undefined}
              >
                <div className="transcript-meta">
                  {p.source === "microphone" ? t("Mikrofon") : t("Computerton")}{" "}
                  · {formatTime(p.startSample / 16000)}
                </div>
                <p>{p.text}</p>
              </article>
            ))
          : panel === "changes"
            ? timeline.map((e) => (
                <article className="change-row" key={e.id}>
                  <span>{formatTime(e.atMs / 1000)}</span>
                  <div>
                    <div className="result-title">
                      <b>{e.text}</b>
                      <Status
                        kind={
                          e.status === "completed" || e.status === "discussed"
                            ? "done"
                            : "open"
                        }
                      >
                        {t(statusName[e.status])}
                        {e.human ? t(" · manuell") : ""}
                      </Status>
                    </div>
                    {quote(e)}
                  </div>
                </article>
              ))
            : items.map((item, index) => (
                <Fragment key={item.id}>
                  {panel === "summary" &&
                    index === 0 &&
                    relevant.length > 0 && (
                      <h3 className="overview-section-title">
                        {t("Jetzt relevant")}
                      </h3>
                    )}
                  {panel === "summary" && index === relevant.length && (
                    <h3 className="overview-section-title">
                      {t("Bisheriger Stand")}
                    </h3>
                  )}
                  <article
                    className={`impulse-row ${item.status === "discussed" ? "impulse-done" : ""}`}
                    key={item.id}
                  >
                    <div className="result-title">
                      <b>{item.text}</b>
                      <Status
                        kind={
                          ["completed", "discussed"].includes(item.status)
                            ? "done"
                            : item.attention === "clarification"
                              ? "question"
                              : "open"
                        }
                      >
                        {t(statusName[item.status])}
                        {item.humanLocked ? t(" · manuell") : ""}
                      </Status>
                    </div>
                    {item.kind === "task" && (
                      <small>
                        {t("Zuständig:")} {item.owner || t("nicht benannt")}{" "}
                        {t("· Termin:")} {item.due || t("nicht benannt")}
                      </small>
                    )}
                    {quote(item)}
                    {panel === "impulses" && (
                      <div className="impulse-actions">
                        <button
                          className="subtle-button impulse-save"
                          aria-pressed={Boolean(
                            ctx.state.impulseStates[item.id]?.saved,
                          )}
                          onClick={() =>
                            ctx._mutate((v) => ({
                              ...v,
                              impulseStates: {
                                ...v.impulseStates,
                                [item.id]: {
                                  ...v.impulseStates[item.id],
                                  saved: !v.impulseStates[item.id]?.saved,
                                },
                              },
                            }))
                          }
                        >
                          <Bookmark size={14} aria-hidden="true" />
                          {ctx.state.impulseStates[item.id]?.saved
                            ? t("Gemerkt")
                            : t("Merken")}
                        </button>
                        <button
                          className={`subtle-button impulse-status ${item.status === "discussed" ? "is-reopen" : "is-complete"}`}
                          onClick={() =>
                            override(
                              item,
                              item.status === "discussed"
                                ? "open"
                                : "discussed",
                            )
                          }
                        >
                          {item.status === "discussed" ? (
                            <RotateCcw size={14} aria-hidden="true" />
                          ) : (
                            <Check size={14} aria-hidden="true" />
                          )}
                          {item.status === "discussed"
                            ? t("Wieder öffnen")
                            : t("Besprochen")}
                        </button>
                      </div>
                    )}
                  </article>
                </Fragment>
              ))}
        {panel === "transcript" && !transcriptRows.length && (
          <p className="empty-inline">
            {t("Für diese Auswahl kein bestätigter Transkriptabschnitt.")}
          </p>
        )}
        {panel !== "transcript" &&
          !(panel === "changes" ? timeline.length : items.length) && (
            <p className="empty-inline">
              {workspace
                ? t("Für diese Auswahl kein gespeicherter Eintrag.")
                : panel === "changes"
                  ? t(
                      "Hier erscheinen belegte Entwicklungen und Statusänderungen. Der Wortlaut steht im Transkript.",
                    )
                  : t("Noch keine geprüfte lokale Auswertung.")}
            </p>
          )}
      </div>
    </Panel>
  );
}
