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

export function LiveWorkspace({
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
  language = "de",
}) {
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
    { id: "verlauf", label: catalog.names.verlauf || "Verlauf-Layout" },
    { id: "regie", label: catalog.names.regie || "Regie-Layout" },
    ...catalog.custom.map(({ id }) => ({ id, label: catalog.names[id] })),
  ];
  const layoutPrefix = "handymeeting.workspace.user-layout.v1.";
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
  function manageLayout(action, name, layout) {
    if (
      action !== "default" &&
      layoutChoices.some(
        ({ id, label }) =>
          (action === "copy" || id !== layoutId) &&
          label.toLocaleLowerCase() === name.toLocaleLowerCase(),
      )
    )
      return "Dieser Layoutname ist bereits vergeben.";
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
      return "Layout konnte nicht gespeichert werden.";
    }
    setCatalog(next);
    if (action === "copy") chooseLayout(id);
    return null;
  }
  const storageKey = `handymeeting.workspace.v1.${meeting.id}`;
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
  const [viewportHeight, setViewportHeight] = useState(window.innerHeight),
    [panelViews, setPanelViews] = useState({});
  const current = useRef(state),
    context = useRef(null),
    contextQueue = useRef(Promise.resolve()),
    polling = useRef(false),
    alive = useRef(true);
  current.current = state;
  const workspace = status?.view?.workspace;
  const entities = workspace?.entities || [],
    originals = workspace?.originals || [];
  async function poll() {
    if (polling.current) return;
    polling.current = true;
    try {
      const next = await adapter.status(meeting.id);
      if (!alive.current) return;
      if (next.running && !next.workspaceContext) context.current = null;
      if (
        next.workspaceContext &&
        context.current &&
        next.workspaceContext.revision < context.current.revision
      )
        return;
      setStatus(next);
      if (
        !hydrated.current ||
        (next.workspaceContext &&
          context.current &&
          next.workspaceContext.revision > context.current.revision)
      ) {
        const restored = restoreSharedContext(
          current.current,
          next.workspaceContext,
        );
        current.current = restored;
        setState(restored);
        hydrated.current = true;
        setStorageReady(true);
      }
      if (
        next.workspaceContext &&
        (!context.current ||
          next.workspaceContext.revision > context.current.revision)
      )
        context.current = next.workspaceContext;
      if (next.workspaceContext) await syncContext();
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      polling.current = false;
    }
  }
  function syncContext(extra) {
    const send = async () => {
      if (
        !context.current ||
        !alive.current ||
        blocked.current ||
        !hydrated.current
      )
        return;
      const next = sharedWorkspaceContext(
        current.current,
        context.current.revision,
        extra
          ? [...(context.current.overrides || []), extra]
          : context.current.overrides || [],
      );
      if (sameSharedContext(next, context.current)) return;
      await adapter.context(next);
      context.current = { ...next, revision: next.revision + 1 };
      const saved = {
        ...current.current,
        nativeContextRevision: context.current.revision,
      };
      current.current = saved;
      setState(saved);
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
  async function action(fn) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await fn();
      await poll();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function override(item, value) {
    if (!context.current) {
      setError(
        "Für diese Änderung zuerst den gespeicherten Meetingstand öffnen.",
      );
      return;
    }
    try {
      await syncContext({
        id: item.id,
        expectedRevision: item.revision,
        status: value,
        saved: Boolean(state.impulseStates[item.id]?.saved),
      });
      await poll();
    } catch (e) {
      setError(String(e));
    }
  }
  useEffect(() => {
    if (selectedFile) openTranscript(selectedFile);
  }, [selectedFile]);

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
        protocolRevision:
          (v.protocolRevision || 1) + (kind === "protocol" ? 1 : 0),
      })),
    commitNote: (kind) =>
      mutate((v) =>
        v.history.at(-1)?.content === v.notes[kind] &&
        v.history.at(-1)?.noteKind === kind
          ? v
          : {
              ...v,
              history: [
                ...v.history,
                {
                  id: `note-${Date.now()}`,
                  at: new Date().toISOString(),
                  label: "Notizstand gesichert",
                  noteKind: kind,
                  content: v.notes[kind],
                },
              ],
            },
      ),
  };
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
            setModal({ type: "panel", panel, title: "Vorbereitung" })
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
              summary: "Live-Überblick",
              relevant: "Jetzt relevant",
              changes: "Änderungen im Gespräch",
              impulses: "Gesprächsimpulse",
              details: "Details",
              transcript: "Transkript",
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
          Zur Standardansicht
        </button>
        <div className="live-update-bar">
          {(onStopRecording ||
            ["recording", "streaming", "catching-up"].includes(
              meeting.status,
            )) && (
            <span className="workspace-capture-state">
              {onStopRecording ||
              ["recording", "streaming"].includes(meeting.status)
                ? paused
                  ? "Aufnahme pausiert"
                  : "Aufnahme läuft"
                : "Erkennung holt nach"}
            </span>
          )}
          {onStopRecording && (
            <button
              type="button"
              className="subtle-button"
              disabled={stopping}
              onClick={onStopRecording}
            >
              Aufnahme beenden
            </button>
          )}
          {onTogglePause && (
            <button
              type="button"
              className="subtle-button"
              disabled={stopping}
              onClick={onTogglePause}
            >
              {paused ? "Aufnahme fortsetzen" : "Aufnahme pausieren"}
            </button>
          )}
          {onToggleMute && (
            <button
              type="button"
              className="subtle-button"
              disabled={stopping}
              onClick={onToggleMute}
            >
              {muted ? "Mikrofon aktivieren" : "Mikrofon stummschalten"}
            </button>
          )}
          {(meeting.captureIncomplete || meeting.warnings?.length > 0) && (
            <button type="button" className="subtle-button" onClick={onBack}>
              Aufnahme prüfen
            </button>
          )}
          {onModelSettings && (
            <IconButton
              label="Textmodell einrichten"
              title="Öffnet die Einstellungen für Anbieter und Modell der Meeting-Auswertung."
              onClick={onModelSettings}
            >
              <Settings2 size={16} />
            </IconButton>
          )}
          <span role="status">
            {classicRunning
              ? "Auswertung läuft in der bisherigen Ansicht."
              : storageReady
                ? mode === "cloud" && !status?.running
                  ? "Cloud-Modell in den Einstellungen ausgewählt"
                  : workspaceStatusText(status)
                : "Gespeicherte Auswertung wird geladen …"}
          </span>
          <label>
            Intervall{" "}
            <select
              aria-label="Auswertungsintervall"
              title="Legt fest, wie oft die KI den Überblick aktualisiert."
              disabled={!storageReady || classicRunning}
              value={minutes}
              onChange={(e) => {
                const value = Number(e.target.value);
                setMinutes(value);
                if (status?.running)
                  void action(() => adapter.interval(value * 60000));
              }}
            >
              <option value={2}>2 Min.</option>
              <option value={3}>3 Min.</option>
            </select>
          </label>
          {classicRunning ? null : status?.running ? (
            <>
              <button
                disabled={busy || status.refreshRequested}
                className="subtle-button"
                onClick={() => action(() => adapter.refresh())}
              >
                <RefreshCw size={13} />
                {status.refreshRequested ? "Vorgemerkt" : "Aktualisieren"}
              </button>
              <IconButton
                label="Nur Auswertung beenden"
                title="Stoppt die KI-Auswertung. Eine laufende Aufnahme geht weiter."
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
                  const next = await adapter.start({
                    meetingId: meeting.id,
                    endpoint,
                    model,
                    mode: "local",
                    cloudConsent: false,
                    apiKey: "",
                    useSavedKey: false,
                    intervalMs: minutes * 60000,
                    language,
                    workspace: sharedWorkspaceContext(state),
                  });
                  context.current =
                    next.workspaceContext || sharedWorkspaceContext(state);
                })
              }
            >
              {mode === "cloud"
                ? "Erweiterte Auswertung nur lokal verfügbar"
                : "Lokale Auswertung starten"}
            </button>
          )}
        </div>
        <IconButton
          label={
            dark ? "Helles Design aktivieren" : "Dunkles Design aktivieren"
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
              {(captureError || error || status?.error) && (
                <div className="state-banner warning" role="status">
                  {captureError || error || status?.error}
                </div>
              )}
              {storageBlocked && (
                <div className="state-banner warning">
                  Der lokale Notizstand ist nicht lesbar oder beschreibbar.
                  Vorhandene Daten bleiben erhalten; neue Änderungen nur in
                  diesem Fenster.
                </div>
              )}
              <nav className="tabs" role="tablist" aria-label="Meetingbereiche">
                {tabs.map(([id, name]) => (
                  <button
                    role="tab"
                    key={id}
                    aria-selected={tab === id}
                    onClick={() => setTab(id)}
                  >
                    {name}
                  </button>
                ))}
              </nav>
              <main
                role="tabpanel"
                aria-label={tabs.find(([id]) => id === tab)?.[1]}
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
                        aria-label="Überblick-Layout"
                        title="Zwischen deinen gespeicherten Kachelanordnungen wechseln"
                        value={layoutId}
                        onChange={(event) => chooseLayout(event.target.value)}
                      >
                        {layoutChoices.map(({ id, label }) => (
                          <option key={id} value={id}>
                            {label}
                            {catalog.defaultId === id ? " · Standard" : ""}
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
                    <Panel title="Meine Vorbereitung">
                      <label>
                        Mein Ziel
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
                        Private Vorbereitung
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
                        Nur freigegebene Gesprächspunkte und Protokollnotizen
                        gehen an das Modell.
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
                {originals.length} Originalstände gesichert
                {workspace?.contextExcludedIds?.length
                  ? ` · ${workspace.contextExcludedIds.length} frühere Einträge nur im Verlauf`
                  : ""}{" "}
                · {status?.view?.model || "kein Textmodell aktiv"} · KI-Entwürfe
                mit Originalbelegen
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
          title="Originalbeleg"
          onClose={() => setModal(modal.back || null)}
        >
          {source ? (
            <>
              <div className="source-meta">
                {source.source === "microphone" ? "Mikrofon" : "Computerton"} ·{" "}
                {workspace?.supersededSources?.includes(source.id) &&
                  "Früherer Transkriptstand · "}
                {formatTime(source.startMs / 1000)}–
                {formatTime(source.endMs / 1000)} · Abschnittszeiten
              </div>
              <blockquote className="source-quote">{source.text}</blockquote>
              {sourceParts.length === 1 && (
                <div className="source-actions">
                  <button onClick={() => openTranscript(sourceParts[0].file)}>
                    Im Transkript ansehen
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
            <p>Der Originalabschnitt ist in diesem Stand nicht verfügbar.</p>
          )}
        </Modal>
      )}
    </div>
  );
}

function LiveTopics({ ctx, entities, quote, override, full, onExpand }) {
  const [view, patch] = usePanelSettings(ctx, "topics", {
    draft: "",
    filter: "all",
  });
  const topics = ctx.state.topics.map((t) => ({
    ...t,
    native: entities.find((e) => e.id === t.id),
  }));
  const done = topics.filter((t) => t.native?.status === "discussed").length;
  function change(fn) {
    ctx._mutate?.(fn);
  }
  return (
    <Panel
      title="Vorbereitung"
      subtitle={`${topics.length - done} offen`}
      icon={ListChecks}
      onExpand={full ? undefined : onExpand}
    >
      <div className="topic-list scroll-region">
        {!topics.length && (
          <p className="empty-inline">
            Eigene Gesprächspunkte vor dem Meeting ergänzen.
          </p>
        )}
        {topics.map((t) => (
          <article className="topic-row" key={t.id}>
            <div className="topic-line">
              <div className="result-title">
                {" "}
                <button
                  className="topic-name"
                  aria-expanded={view.expanded === t.id}
                  onClick={() =>
                    patch({ expanded: view.expanded === t.id ? null : t.id })
                  }
                >
                  {t.text}
                </button>
                <Status
                  kind={t.native?.status === "discussed" ? "done" : "open"}
                >
                  {t.native
                    ? `${statusName[t.native.status]}${t.native.humanLocked ? " · manuell" : ""}`
                    : "Noch nicht abgeglichen"}
                </Status>{" "}
              </div>
              <label>
                <input
                  type="checkbox"
                  checked={Boolean(t.shared)}
                  onChange={(e) =>
                    change((v) => ({
                      ...v,
                      topics: v.topics.map((x) =>
                        x.id === t.id ? { ...x, shared: e.target.checked } : x,
                      ),
                    }))
                  }
                />
                Freigeben
              </label>
            </div>
            {view.expanded === t.id && t.native && (
              <>
                {quote(t.native)}
                <button
                  className="text-button"
                  onClick={() =>
                    override(
                      t.native,
                      t.native.status === "discussed" ? "open" : "discussed",
                    )
                  }
                >
                  {t.native.status === "discussed"
                    ? "Wieder öffnen"
                    : "Als besprochen markieren"}
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
            aria-label="Neuer Gesprächspunkt"
            maxLength={180}
            value={view.draft || ""}
            onChange={(e) => patch({ draft: e.target.value })}
            placeholder="Eigenen Gesprächspunkt ergänzen …"
          />
          <IconButton type="submit" label="Gesprächspunkt hinzufügen">
            <Plus size={16} />
          </IconButton>
        </form>
      )}
      <div className="panel-foot">
        Nur freigegebene Gesprächspunkte werden automatisch abgeglichen.
      </div>
    </Panel>
  );
}

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
  const [view, patch] = usePanelSettings(ctx, panel, {
    query: "",
    ascending: true,
    count: 5,
    track: "all",
    minutes: "all",
  });
  const titles = {
    summary: "Live-Überblick",
    details: "Wichtige Details",
    impulses: "Gesprächsimpulse",
    relevant: "Jetzt relevant",
    changes: "Änderungen im Gespräch",
    transcript: "Transkript",
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
          `${event.text} ${statusName[event.status] || event.status}`
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
            Anzahl
            <input
              type="number"
              min="1"
              max={impulseTotal || 1}
              aria-label="Anzahl Gesprächsimpulse"
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
            <span>von {impulseTotal}</span>
          </label>
        )
      }
    >
      {["transcript", "changes", "details"].includes(panel) && (
        <div className="live-transcript-controls compact-filters">
          <label className="search-input compact-search">
            <Search size={14} />
            <input
              aria-label={`${titles[panel]} durchsuchen`}
              placeholder="Suchen …"
              value={view.query || ""}
              onChange={(e) => patch({ query: e.target.value })}
            />
          </label>
          {panel === "transcript" && (
            <>
              {" "}
              <select
                aria-label="Transkriptspur"
                value={view.track || "all"}
                onChange={(e) => patch({ track: e.target.value })}
              >
                <option value="all">Beide Spuren</option>
                <option value="system">Computerton</option>
                <option value="microphone">Mikrofon</option>
              </select>
              <select
                aria-label="Transkriptzeitraum"
                value={view.minutes || "all"}
                onChange={(e) => patch({ minutes: e.target.value })}
              >
                <option value="all">Gesamt</option>
                {[5, 10, 15, 30, 60].map((value) => (
                  <option key={value} value={value}>
                    Letzte {value} Min.
                  </option>
                ))}
                <option value="custom">Individueller Zeitraum</option>
              </select>
              <button
                type="button"
                className="subtle-button sort-toggle"
                aria-label={
                  view.ascending === false ? "Neueste zuerst" : "Älteste zuerst"
                }
                title={
                  view.ascending === false
                    ? "Neueste zuerst · Reihenfolge umkehren"
                    : "Älteste zuerst · Reihenfolge umkehren"
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
                aria-label="Verlauf nach Status filtern"
                value={view.eventStatus || "all"}
                onChange={(e) => patch({ eventStatus: e.target.value })}
              >
                <option value="all">Alle Status</option>
                {Object.entries(statusName).map(([id, label]) => (
                  <option key={id} value={id}>
                    {label}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="subtle-button sort-toggle"
                aria-label={
                  view.ascending === false ? "Neueste zuerst" : "Älteste zuerst"
                }
                title={
                  view.ascending === false
                    ? "Neueste zuerst · Reihenfolge umkehren"
                    : "Älteste zuerst · Reihenfolge umkehren"
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
            Von (Min.)
            <input
              type="number"
              min="0"
              step="any"
              value={view.fromMinute ?? ""}
              onChange={(e) => patch({ fromMinute: e.target.value })}
            />
          </label>
          <label>
            Bis (Min.)
            <input
              type="number"
              min="0"
              step="any"
              value={view.toMinute ?? ""}
              onChange={(e) => patch({ toMinute: e.target.value })}
            />
          </label>
          {invalidRange && (
            <span role="status">Das Ende muss nach dem Anfang liegen.</span>
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
                  {p.source === "microphone" ? "Mikrofon" : "Computerton"} ·{" "}
                  {formatTime(p.startSample / 16000)}
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
                        {statusName[e.status]}
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
                      <h3 className="overview-section-title">Jetzt relevant</h3>
                    )}
                  {panel === "summary" && index === relevant.length && (
                    <h3 className="overview-section-title">Bisheriger Stand</h3>
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
                        {statusName[item.status]}
                        {item.humanLocked ? " · manuell" : ""}
                      </Status>
                    </div>
                    {item.kind === "task" && (
                      <small>
                        Zuständig: {item.owner || "nicht benannt"} · Termin:{" "}
                        {item.due || "nicht benannt"}
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
                            ? "Gemerkt"
                            : "Merken"}
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
                            ? "Wieder öffnen"
                            : "Besprochen"}
                        </button>
                      </div>
                    )}
                  </article>
                </Fragment>
              ))}
        {panel === "transcript" && !transcriptRows.length && (
          <p className="empty-inline">
            Für diese Auswahl kein bestätigter Transkriptabschnitt.
          </p>
        )}
        {panel !== "transcript" &&
          !(panel === "changes" ? timeline.length : items.length) && (
            <p className="empty-inline">
              {workspace
                ? "Für diese Auswahl kein gespeicherter Eintrag."
                : panel === "changes"
                  ? "Hier erscheinen belegte Entwicklungen und Statusänderungen. Der Wortlaut steht im Transkript."
                  : "Noch keine geprüfte lokale Auswertung."}
            </p>
          )}
      </div>
    </Panel>
  );
}
