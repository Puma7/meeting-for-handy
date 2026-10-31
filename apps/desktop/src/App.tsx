import EraseMeetingDialog from "./views/EraseMeetingDialog";
import PendingErasures from "./views/PendingErasures";
import MeetingLabels from "./views/MeetingLabels";
import SpeakerLabels from "./views/SpeakerLabels";
import SpeakerExperiments from "./views/SpeakerExperiments";
import { SPEAKER_NAME_TEMPLATE } from "./speaker-proposals";
import HistorySearch from "./views/HistorySearch";
import LibraryStatus from "./views/LibraryStatus";
import UpdatePanel from "./views/UpdatePanel";
import SettingsView from "./views/SettingsView";
import HistoryList from "./views/HistoryList";
import MaintenanceStatus from "./views/MaintenanceStatus";
import NotesView from "./views/NotesView";
import TextModelSettings from "./views/TextModelSettings";
import ResumeAnalysisDialog from "./views/ResumeAnalysisDialog";
import Sidebar, { type Page } from "./views/Sidebar";
import RecordingStatus from "./views/RecordingStatus";
import MeetingView from "./views/MeetingView";
import LiveNotesPanel, {
  INTERVAL_CHOICES,
  type LivePreferences,
} from "./views/LiveNotesPanel";
import MeetingDetail from "./views/MeetingDetail";
import NotesProgressPanel from "./views/NotesProgressPanel";
import TemplateDialog, { type TemplateAction } from "./views/TemplateDialog";
import SourceDialog, { type SourceSelection } from "./views/SourceDialog";
import AgentPanel from "./views/AgentPanel";
import DeviceNotices from "./views/DeviceNotices";
import {
  lostSelections,
  useDeviceRefresh,
  type DeviceLoss,
  type DeviceSource,
} from "./deviceWatch";
import { useTemplateLibrary } from "./useTemplateLibrary";
import { useLocalTextModels } from "./useLocalTextModels";
import { useNotesJob } from "./useNotesJob";
import { hydrateLiveTranscript, mergeLiveTranscript } from "./live-transcript";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { RefreshCw, X } from "lucide-react";
import { desktopApi, errorMessage } from "./api";
import {
  defaultMeetingTitle,
  meetingLabel,
  defaultTemplate,
  renderTranscript,
  uid,
} from "./domain";
import { de, en, extraDe, extraEn } from "./i18n";
import type {
  AnalysisResult,
  AnalysisStep,
  AnalysisTemplate,
  AppState,
  AudioDevice,
  DesktopApi,
  Evidence,
  Locale,
  Meeting,
  MeetingSummary,
  NotesRequest,
  PendingAnalysis,
  SkippedAnalysis,
  Theme,
} from "./types";

const PREFS_KEY = "handymeeting.rust.beta.preferences.v1";
/**
 * Reads only locale/theme preferences. Malformed saved data returns safe defaults with
 * persistence disabled so the original value is retained.
 */
function loadPreferences(): {
  locale: Locale;
  theme: Theme;
  persistable: boolean;
} {
  try {
    const value = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}");
    if (!value || typeof value !== "object" || Array.isArray(value))
      throw new Error("Invalid preferences");
    return {
      locale: value.locale === "en" ? "en" : "de",
      theme: ["dark", "light", "system"].includes(value.theme)
        ? value.theme
        : "system",
      persistable: true,
    };
  } catch {
    return { locale: "de", theme: "system", persistable: false };
  }
}
const LIVE_PREFS_KEY = "handymeeting.rust.live-notes.v1";
/**
 * Live summary preferences and the last local text model choice. Cloud settings and keys are
 * never stored here. Unreadable data falls back to defaults.
 */
function loadLivePreferences(): LivePreferences & {
  endpoint: string | null;
  model: string | null;
} {
  const fallback = {
    auto: false,
    intervalMinutes: 3,
    endpoint: null,
    model: null,
  };
  try {
    const value = JSON.parse(localStorage.getItem(LIVE_PREFS_KEY) ?? "{}");
    if (!value || typeof value !== "object" || Array.isArray(value))
      return fallback;
    const text = (v: unknown, max: number) =>
      typeof v === "string" && v.trim() && v.length <= max ? v : null;
    return {
      auto: value.auto === true,
      intervalMinutes: INTERVAL_CHOICES.includes(value.intervalMinutes)
        ? value.intervalMinutes
        : 3,
      endpoint: text(value.endpoint, 4096),
      model: text(value.model, 300),
    };
  } catch {
    return fallback;
  }
}
/**
 * Coordinates live and selected meetings, native IPC, explicit analysis consent and job
 * feedback. Child actions are bound to meeting IDs rather than display titles.
 */
export default function App({ api = desktopApi }: { api?: DesktopApi }) {
  const [erasureTarget, setErasureTarget] = useState<MeetingSummary | null>(
    null,
  );
  const [erasureRevision, setErasureRevision] = useState(0);
  const [erasureNotice, setErasureNotice] = useState<string | null>(null);
  const erasureBlocked = useRef(new Set<string>()),
    erasureEpoch = useRef(0);
  const [preferences] = useState(loadPreferences);
  const [savedLive] = useState(loadLivePreferences);
  const [livePreferences, setLivePreferences] = useState<LivePreferences>({
    auto: savedLive.auto,
    intervalMinutes: savedLive.intervalMinutes,
  });
  const [liveFocus, setLiveFocus] = useState(false);
  const [locale, setLocale] = useState<Locale>(preferences.locale),
    [theme, setTheme] = useState<Theme>(preferences.theme);
  const t = locale === "de" ? de : en,
    x = locale === "de" ? extraDe : extraEn;
  const [page, setPage] = useState<Page>("meeting"),
    [app, setApp] = useState<AppState | null>(null),
    [connected, setConnected] = useState(false),
    [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null),
    [busy, setBusy] = useState(false),
    // A pending microphone reconnect blocks other controls but never Stop, which cancels it.
    [reconnecting, setReconnecting] = useState(false),
    [isStopping, setIsStopping] = useState(false),
    [refreshingHandy, setRefreshingHandy] = useState(false),
    [handyRefreshFailure, setHandyRefreshFailure] = useState<string | null>(
      null,
    ),
    [copied, setCopied] = useState<"transcript" | "notes" | null>(null);
  const [fileExport, setFileExport] = useState<{
    id: string;
    file: string;
  } | null>(null);
  const [devices, setDevices] = useState<AudioDevice[]>([]),
    [deviceLosses, setDeviceLosses] = useState<DeviceLoss[]>([]),
    [history, setHistory] = useState<MeetingSummary[]>([]),
    [meeting, setMeeting] = useState<Meeting | null>(null),
    [liveMeeting, setLiveMeeting] = useState<Meeting | null>(null),
    [notesMeetingId, setNotesMeetingId] = useState(""),
    [notesMeetingLoading, setNotesMeetingLoading] = useState(false);
  const [transcriptLoading, setTranscriptLoading] = useState(false);
  const transcriptRead = useRef<{
    id: string;
    promise: Promise<Meeting>;
  } | null>(null);
  const loadLiveTranscript = useCallback(async () => {
    const id = liveMeeting?.id;
    if (!id || erasureBlocked.current.has(id)) return;
    const epoch = erasureEpoch.current;
    let read = transcriptRead.current;
    if (!read || read.id !== id) {
      read = { id, promise: api.getMeeting(id) };
      transcriptRead.current = read;
    }
    setTranscriptLoading(true);
    try {
      const full = await read.promise;
      if (epoch !== erasureEpoch.current || erasureBlocked.current.has(id))
        return;
      if (full.id !== id)
        throw new Error(
          "Das geladene Transkript gehört zu einem anderen Meeting.",
        );
      setLiveMeeting((current) => hydrateLiveTranscript(current, full));
      setMeeting((current) => hydrateLiveTranscript(current, full));
    } catch (failure) {
      if (transcriptRead.current === read) setError(errorMessage(failure));
    } finally {
      if (transcriptRead.current === read) {
        transcriptRead.current = null;
        setTranscriptLoading(false);
      }
    }
  }, [api, liveMeeting?.id]);
  useEffect(() => {
    if (liveMeeting?.livePreviewLimited) void loadLiveTranscript();
  }, [liveMeeting?.id, liveMeeting?.livePreviewLimited, loadLiveTranscript]);
  useEffect(() => {
    if (
      page !== "meeting" ||
      !liveMeeting?.id ||
      !["recording", "streaming", "recorded", "catching-up"].includes(
        liveMeeting.status,
      )
    )
      return;
    // Full text is read outside the application lock at a slower cadence than status.
    // A backlog can update old sections that never enter the bounded recent preview.
    const timer = setInterval(() => void loadLiveTranscript(), 5000);
    return () => clearInterval(timer);
  }, [page, liveMeeting?.id, liveMeeting?.status, loadLiveTranscript]);
  const [speakerLabels, setSpeakerLabels] = useState<{
    meetingId: string;
    labels: Record<string, string>;
  } | null>(null);
  const [speakerOptionsOpen, setSpeakerOptionsOpen] = useState(false);
  const [speakerChecks, setSpeakerChecks] = useState<Record<string, boolean>>(
    {},
  );
  const [speakerModelId, setSpeakerModelId] = useState("");
  const [speakerChecksInFlight, setSpeakerChecksInFlight] = useState(0);
  const speakerChecking = speakerChecksInFlight > 0;
  const onSpeakerCheckingChange = useCallback((checking: boolean) => {
    setSpeakerChecksInFlight((count) =>
      Math.max(0, count + (checking ? 1 : -1)),
    );
  }, []);
  const onSpeakerModelChecked = useCallback(
    (id: string, supported: boolean) => {
      setSpeakerChecks((previous) => ({ ...previous, [id]: supported }));
    },
    [],
  );
  useEffect(
    () => setSpeakerOptionsOpen(false),
    [page, meeting?.id, liveMeeting?.id],
  );
  /**
   * Keeps caller-supplied speaker labels with their meeting ID so another selected meeting
   * cannot inherit the display mapping.
   */
  const onSpeakerLabels = useCallback(
    (meetingId: string, labels: Record<string, string>) =>
      setSpeakerLabels({ meetingId, labels }),
    [],
  );
  const [searchSource, setSearchSource] = useState<{
    meetingId: string;
    file: string | null;
  } | null>(null);
  const [title, setTitle] = useState(""),
    [modelId, setModelId] = useState(""),
    [systemId, setSystemId] = useState(""),
    [micId, setMicId] = useState(""),
    [micEnabled, setMicEnabled] = useState(false),
    [handyPath, setHandyPath] = useState("");
  const templateStore = useTemplateLibrary(api, connected, locale);
  const templates = useMemo(
    () =>
      templateStore.templates.some((t) => t.id === SPEAKER_NAME_TEMPLATE.id)
        ? templateStore.templates
        : [...templateStore.templates, SPEAKER_NAME_TEMPLATE],
    [templateStore.templates],
  );
  const [templateId, setTemplateId] = useState(""),
    [stepId, setStepId] = useState("");
  const [editing, setEditing] = useState<AnalysisStep | null>(null),
    [editingNew, setEditingNew] = useState(false),
    [inspectingStep, setInspectingStep] = useState(false),
    [templateAction, setTemplateAction] = useState<TemplateAction | null>(null),
    [templateActionError, setTemplateActionError] = useState<string | null>(
      null,
    );
  const [endpoint, setEndpoint] = useState(
      savedLive.endpoint ?? "http://127.0.0.1:1234/v1/",
    ),
    [textModel, setTextModel] = useState(savedLive.model ?? ""),
    [manualTextModel, setManualTextModel] = useState(false),
    [apiKey, setApiKey] = useState(""),
    [savedKeyScope, setSavedKeyScope] = useState<string | null>(null),
    [mode, setProcessingMode] = useState<"local" | "cloud">("local");
  const connectionDrafts = useRef({
    local: {
      endpoint: savedLive.endpoint ?? "http://127.0.0.1:1234/v1/",
      model: savedLive.model ?? "",
      manual: false,
    },
    cloud: {
      endpoint: "https://openrouter.ai/api/v1/",
      model: "",
      manual: false,
    },
  });
  const setMode = useCallback(
    (next: "local" | "cloud") => {
      if (mode === next) return;
      connectionDrafts.current[mode] = {
        endpoint,
        model: textModel,
        manual: manualTextModel,
      };
      const draft = connectionDrafts.current[next];
      setEndpoint(draft.endpoint);
      setTextModel(draft.model);
      setManualTextModel(draft.manual);
      setApiKey("");
      setSavedKeyScope(null);
      setProcessingMode(next);
    },
    [mode, endpoint, textModel, manualTextModel],
  );
  const credentialScope = JSON.stringify([mode, endpoint.trim()]);
  const useSavedKey = savedKeyScope === credentialScope;
  /**
   * Scopes saved-key use to the current endpoint and mode; the encrypted stored secret is
   * never read into this callback.
   */
  const setUseSavedKey = useCallback(
    (value: boolean) => setSavedKeyScope(value ? credentialScope : null),
    [credentialScope],
  );
  const [analyses, setAnalyses] = useState<AnalysisResult[]>([]),
    [resultId, setResultId] = useState(""),
    [cloudRequest, setCloudRequest] = useState<NotesRequest | null>(null),
    [source, setSource] = useState<SourceSelection | null>(null);
  const [pendingAnalyses, setPendingAnalyses] = useState<PendingAnalysis[]>([]);
  // Stored work states the backend could not verify; they stay untouched and are only reported.
  const [skippedResults, setSkippedResults] = useState<SkippedAnalysis[]>([]),
    [skippedPending, setSkippedPending] = useState<SkippedAnalysis[]>([]);
  const skippedRuns = [
    ...new Set([...skippedResults, ...skippedPending].map((item) => item.id)),
  ];
  const [resumeAnalysis, setResumeAnalysis] = useState<PendingAnalysis | null>(
    null,
  );
  const requestSequence = useRef(0),
    // Counts stop requests; a reconnect answer from before a stop is outdated.
    stopEpoch = useRef(0),
    mainRef = useRef<HTMLElement>(null),
    previousPage = useRef(page),
    selectedRef = useRef<string | null>(null),
    fullMeetingRef = useRef<string | null>(null),
    liveFullRef = useRef<string | null>(null),
    notesSelectionRef = useRef(""),
    notesLoadSequence = useRef(0),
    initialModelSelected = useRef(false),
    initialCatalogSeen = useRef(false),
    lastModelRefresh = useRef(Date.now()),
    modelRefreshInFlight = useRef<Promise<void> | null>(null),
    nativeChoiceSequence = useRef(0),
    pendingNativeChoice = useRef<{ id: number; path: string } | null>(null),
    catalogueFailure = useRef<string | null>(null),
    mounted = useRef(true),
    copyTimer = useRef<ReturnType<typeof setTimeout>>();
  const template =
    templates.find((item) => item.id === templateId) ?? templates[0];
  const templatePending = useRef(templateStore.pending);
  templatePending.current = templateStore.pending;
  const result = analyses.find((item) => item.id === resultId);
  const models = (
    handyRefreshFailure !== null ? [] : (app?.handy?.models ?? [])
  ).filter(
    (item) =>
      item.isDownloaded && item.supportsStreaming && !item.compatibilityError,
  );
  const canEraseMeeting = Boolean(
    api.previewMeetingErasure &&
      api.getMeetingErasurePreview &&
      api.eraseMeetingNow &&
      api.getMaintenanceProgress &&
      api.cancelMaintenance,
  );
  const recording = app?.recording ?? false;
  const catchingUp =
    !recording &&
    ["catching-up", "streaming"].includes(app?.activeMeeting?.status ?? "");
  const latestRuntime = useRef({ app, busy, isStopping, connected });
  latestRuntime.current = { app, busy, isStopping, connected };
  const devicesRef = useRef<AudioDevice[]>([]),
    deviceLossesRef = useRef<DeviceLoss[]>([]),
    deviceRequest = useRef(0),
    // The device IDs a pending start sent to the capture, until the recording is reported.
    startDevices = useRef<Record<DeviceSource, string | null> | null>(null);
  /** Keeps the rendered device notices and the reconciliation input identical. */
  const updateDeviceLosses = useCallback((next: DeviceLoss[]) => {
    deviceLossesRef.current = next;
    setDeviceLosses(next);
  }, []);
  const meetingId = app?.activeMeeting?.id ?? null;
  const selectionRef = useRef({
    systemId,
    micId,
    micEnabled,
    recording,
    meetingId,
  });
  selectionRef.current = { systemId, micId, micEnabled, recording, meetingId };
  /**
   * Merges a completed result only into its still-selected meeting and removes the
   * corresponding resumable placeholder.
   */
  const onNotesResult = useCallback((next: AnalysisResult) => {
    if (
      notesSelectionRef.current !== next.meetingId ||
      erasureBlocked.current.has(next.meetingId)
    )
      return;
    setAnalyses((items) => [
      next,
      ...items.filter((item) => item.id !== next.id),
    ]);
    setResultId(next.id);
    setStepId(next.sections[0]?.id ?? "");
    setPendingAnalyses((items) => items.filter((item) => item.id !== next.id));
  }, []);
  const notesJob = useNotesJob(
    api,
    connected,
    app?.notesRunning ?? false,
    onNotesResult,
  );
  const notesBusy = notesJob.busy;
  const localTextModels = useLocalTextModels(
    api,
    endpoint,
    mode,
    connected,
    page === "notes" || page === "settings",
  );
  const notesMeeting = meeting?.id === notesMeetingId ? meeting : null;
  const notesMeetingOptions =
    history.some((item) => item.id === meeting?.id) || !meeting
      ? history
      : [meeting, ...history];
  const notesModelReady =
    mode === "cloud" ||
    manualTextModel ||
    (localTextModels.loaded && localTextModels.models.includes(textModel));

  const canStartNotes =
    connected &&
    Boolean(notesMeeting) &&
    notesMeeting?.status === "complete" &&
    !recording &&
    !notesBusy &&
    !busy &&
    !notesMeetingLoading &&
    Boolean(textModel.trim()) &&
    notesModelReady &&
    Boolean(endpoint.trim()) &&
    templateStore.ready &&
    !templateStore.pending;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useLayoutEffect(() => {
    if (previousPage.current === page) return;
    previousPage.current = page;
    const main = mainRef.current;
    if (!main) return;
    // Reset before paint and before a search source's effect scrolls into view.
    // Background updates must not move the reader.
    main.scrollTop = 0;
    main.scrollLeft = 0;
    if (!erasureTarget)
      main.querySelector("h1")?.focus({ preventScroll: true });
  }, [page, erasureTarget]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.lang = locale;
    if (!preferences.persistable) return;
    try {
      const previous = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}");
      if (!previous || typeof previous !== "object" || Array.isArray(previous))
        throw new Error("Invalid preferences");
      localStorage.setItem(
        PREFS_KEY,
        JSON.stringify({ ...previous, locale, theme }),
      );
    } catch {
      setError(t.preferencesError);
    }
  }, [locale, theme]);
  useEffect(() => {
    try {
      const previous = JSON.parse(localStorage.getItem(LIVE_PREFS_KEY) ?? "{}");
      const base =
        previous && typeof previous === "object" && !Array.isArray(previous)
          ? previous
          : {};
      // Only a local route is remembered; cloud endpoints and keys never are.
      const local =
        mode === "local" && textModel.trim()
          ? { endpoint: endpoint.trim(), model: textModel.trim() }
          : {};
      localStorage.setItem(
        LIVE_PREFS_KEY,
        JSON.stringify({ ...base, ...livePreferences, ...local }),
      );
    } catch {
      // Preferences are a convenience; the live summary works without them.
    }
  }, [livePreferences, mode, endpoint, textModel]);
  useEffect(
    () => () => {
      if (copyTimer.current) clearTimeout(copyTimer.current);
    },
    [],
  );
  const modalOpen = Boolean(
    editing ||
      templateAction ||
      cloudRequest ||
      source ||
      resumeAnalysis ||
      erasureTarget,
  );
  useEffect(() => {
    if (!modalOpen) return;
    const previous = document.activeElement as HTMLElement | null;
    const dialog = document.querySelector<HTMLElement>(
      '[role="dialog"],[role="alertdialog"]',
    );
    const surfaces = document.querySelectorAll<HTMLElement>(
      ".sidebar, main, .notes-statusbar, .job-status-stack",
    );
    surfaces.forEach((element) => {
      element.inert = true;
    });
    /**
     * Finds eligible focus targets inside the current modal, excluding hidden elements and
     * controls inside closed details sections.
     */
    const focusable = () =>
      Array.from(
        dialog?.querySelectorAll<HTMLElement>(
          'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),summary,[tabindex="0"]',
        ) ?? [],
      ).filter((element) => {
        if (element.closest("[hidden]")) return false;
        let parent = element.parentElement;
        while (parent && parent !== dialog) {
          if (
            parent instanceof HTMLDetailsElement &&
            !parent.open &&
            parent.querySelector(":scope > summary") !== element
          )
            return false;
          parent = parent.parentElement;
        }
        return true;
      });
    const initialTargets = focusable();
    if (!erasureTarget)
      (
        initialTargets.find((element) => element.matches("input,textarea")) ??
        initialTargets[0]
      )?.focus();
    /**
     * Keeps keyboard focus inside the modal and handles Escape while retaining an
     * in-progress template save.
     */
    const keydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (event.key === "Escape") {
        if (erasureTarget) return; // The erasure dialog owns cancellation and terminal recovery.
        if (templatePending.current) {
          event.preventDefault();
          return;
        }
        setEditing(null);
        setTemplateAction(null);
        setCloudRequest(null);
        setSource(null);
        setResumeAnalysis(null);
        return;
      }
      if (event.key !== "Tab") return;
      const targets = focusable(),
        first = targets[0],
        last = targets.at(-1);
      if (
        event.shiftKey &&
        (document.activeElement === first ||
          !dialog?.contains(document.activeElement))
      ) {
        event.preventDefault();
        last?.focus();
      } else if (
        !event.shiftKey &&
        (document.activeElement === last ||
          !dialog?.contains(document.activeElement))
      ) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener("keydown", keydown);
    return () => {
      document.removeEventListener("keydown", keydown);
      surfaces.forEach((element) => {
        element.inert = false;
      });
      if (previous?.isConnected) previous.focus();
      else document.querySelector<HTMLElement>("main h1")?.focus();
    };
  }, [modalOpen]);
  useEffect(() => {
    if (!app?.handy) return;
    if (
      !initialCatalogSeen.current &&
      (app.handy.executable ||
        app.handy.version ||
        app.handy.error ||
        app.handy.models.length)
    ) {
      initialCatalogSeen.current = true;
      lastModelRefresh.current = Date.now();
    }
    if (modelId) {
      initialModelSelected.current = true;
      if (!models.some((item) => item.id === modelId)) {
        setModelId("");
        setError(handyRefreshFailure || app.handy.error || t.modelUnavailable);
      }
    } else if (!initialModelSelected.current && models.length) {
      initialModelSelected.current = true;
      setModelId(models[0].id);
    }
  }, [app?.handy, modelId, handyRefreshFailure, t.modelUnavailable]);
  /**
   * Retains available device selections; otherwise chooses the default or first device
   * separately for system audio and microphone. A running recording keeps its devices,
   * and every selected device that disappeared is reported.
   */
  const applyDevices = useCallback(
    (items: AudioDevice[]) => {
      const previous = devicesRef.current;
      const current = selectionRef.current;
      devicesRef.current = items;
      setDevices(items);
      const listed = (source: DeviceSource, id: string) =>
        items.some((d) => d.source === source && d.id === id);
      const fallback = (source: DeviceSource) =>
        items.find((d) => d.source === source && d.isDefault) ??
        items.find((d) => d.source === source);
      const lost = lostSelections(previous, items, {
        system: current.systemId,
        microphone: current.micEnabled ? current.micId : null,
      });
      const chosen = { system: current.systemId, microphone: current.micId };
      let losses = deviceLossesRef.current;
      // Devices that stand in automatically for a lost one; losing them too keeps the
      // original record, so the first device still returns.
      const standIns = new Set(
        losses
          .filter((loss) => loss.replacementId && !loss.userChose)
          .map((loss) => `${loss.source}:${loss.replacementId}`),
      );
      // The device selected in place of a lost one. A recording keeps its selection, so a
      // loss from a recording stands in for itself until an automatic choice replaces it.
      // After the user's own choice nothing stands in: the lost device no longer returns.
      const standing = (loss: DeviceLoss) =>
        loss.userChose
          ? null
          : (loss.replacementId ?? (loss.duringRecording ? loss.id : ""));
      if (!current.recording) {
        for (const source of ["system", "microphone"] as const) {
          if (listed(source, chosen[source])) continue;
          const automatic = fallback(source);
          // A later automatic choice is not the user's: the lost device still returns, and
          // its notice names the device now selected. Without any device the record stands
          // for the empty selection, so the next device that appears stands in too.
          losses = losses.map((loss) =>
            loss.source === source && standing(loss) === chosen[source]
              ? {
                  ...loss,
                  replacement: automatic?.name,
                  replacementId: automatic?.id ?? "",
                }
              : loss,
          );
          chosen[source] = automatic?.id ?? "";
        }
      }
      if (!current.recording) {
        // A device that is back takes over again unless another one was chosen meanwhile,
        // also when it was lost during a recording that has ended since.
        for (const loss of losses) {
          if (!listed(loss.source, loss.id)) continue;
          if (chosen[loss.source] === standing(loss))
            chosen[loss.source] = loss.id;
          losses = losses.filter((item) => item !== loss);
        }
      }
      for (const item of lost) {
        if (!current.recording && standIns.has(`${item.source}:${item.id}`))
          continue;
        // The native capture usually reports the stop first; a later list change must not
        // weaken that confirmation or reopen a closed notice.
        if (
          current.recording &&
          losses.some(
            (loss) =>
              loss.source === item.source &&
              loss.duringRecording &&
              loss.confirmed,
          )
        )
          continue;
        const replacement = current.recording
          ? undefined
          : items.find(
              (d) => d.source === item.source && d.id === chosen[item.source],
            );
        losses = [
          ...losses.filter((loss) => loss.source !== item.source),
          {
            ...item,
            duringRecording: current.recording,
            meetingId: current.recording ? current.meetingId : undefined,
            replacement: replacement?.name,
            replacementId: replacement?.id,
          },
        ];
      }
      if (
        losses.length !== deviceLossesRef.current.length ||
        losses.some((loss, index) => loss !== deviceLossesRef.current[index])
      )
        updateDeviceLosses(losses);
      setSystemId(chosen.system);
      setMicId(chosen.microphone);
    },
    [updateDeviceLosses],
  );
  /**
   * A device the user picks wins over a later returning one, even when it is the device
   * that was chosen automatically before. The notice then names the picked device.
   */
  const chooseDevice = useCallback(
    (source: DeviceSource, id: string) => {
      if (source === "system") setSystemId(id);
      else setMicId(id);
      const losses = deviceLossesRef.current;
      const name = devicesRef.current.find(
        (d) => d.source === source && d.id === id,
      )?.name;
      if (
        losses.some(
          (loss) =>
            loss.source === source &&
            (!loss.userChose || loss.replacementId !== id),
        )
      )
        updateDeviceLosses(
          losses.map((loss) =>
            loss.source === source
              ? {
                  ...loss,
                  userChose: true,
                  replacement: name,
                  replacementId: id,
                }
              : loss,
          ),
        );
    },
    [updateDeviceLosses],
  );
  /**
   * Reloads the backend meeting summaries without changing the currently selected meeting
   * or opening its source files.
   */
  const refreshHistory = useCallback(async () => {
    const epoch = erasureEpoch.current;
    try {
      const items = await api.listMeetings();
      if (epoch === erasureEpoch.current)
        setHistory(
          items.filter((item) => !erasureBlocked.current.has(item.id)),
        );
    } catch (failure) {
      if (epoch === erasureEpoch.current) throw failure;
    }
  }, [api]);
  useEffect(() => {
    /**
     * Refreshes history after the local index-update event and reports refresh failures
     * through the existing error display.
     */
    const updated = () => {
      void refreshHistory().catch((e) => setError(errorMessage(e)));
    };
    window.addEventListener("library-index-updated", updated);
    return () => window.removeEventListener("library-index-updated", updated);
  }, [refreshHistory]);
  /**
   * Reloads native device metadata and reconciles the two selected track IDs without
   * opening an audio stream.
   */
  const refreshDevices = useCallback(async () => {
    // Startup, manual and automatic refreshes can overlap; only the newest list applies,
    // so an older reply cannot bring back a device that was just removed.
    const request = ++deviceRequest.current;
    const items = await api.listDevices();
    if (request === deviceRequest.current) applyDevices(items);
  }, [api, applyDevices]);
  useDeviceRefresh(refreshDevices, connected);
  const micCapturing = useRef(false),
    wasRecording = useRef(false);
  useEffect(() => {
    // A new recording starts with its own devices; earlier notices no longer apply. A loss
    // of this recording may already be recorded before this effect runs, so it stays.
    if (recording && !wasRecording.current) {
      const id = app?.activeMeeting?.id ?? null,
        started = startDevices.current;
      startDevices.current = null;
      const kept = deviceLossesRef.current.flatMap((loss) =>
        loss.duringRecording && loss.meetingId === id
          ? [loss]
          : // A device lost while the start was pending still went to the capture: the
            // recording keeps it, like a loss during the recording.
            !loss.duringRecording && started?.[loss.source] === loss.id
            ? [
                {
                  ...loss,
                  duringRecording: true,
                  meetingId: id,
                  replacement: undefined,
                  replacementId: undefined,
                },
              ]
            : [],
      );
      updateDeviceLosses(kept);
      for (const loss of kept) {
        if (started?.[loss.source] !== loss.id) continue;
        if (loss.source === "system") setSystemId(loss.id);
        else setMicId(loss.id);
      }
    }
    wasRecording.current = recording;
    // The native microphone track ended while the recording continues: the device was
    // removed or failed, even before the next device list shows it.
    const capturing = Boolean(recording && app?.captureControl?.hasMicrophone);
    const ended =
      micCapturing.current &&
      !capturing &&
      recording &&
      !isStopping &&
      !app?.transitioning;
    micCapturing.current = capturing;
    if (!ended) return;
    // This confirms the stop, so it replaces an earlier list-based or closed notice.
    const id = selectionRef.current.micId;
    const earlier = deviceLossesRef.current.find(
      (loss) => loss.source === "microphone" && loss.duringRecording,
    );
    updateDeviceLosses([
      ...deviceLossesRef.current.filter((loss) => loss.source !== "microphone"),
      {
        source: "microphone",
        id,
        // After the device left the list, only the earlier record still knows its name.
        name:
          devicesRef.current.find(
            (d) => d.source === "microphone" && d.id === id,
          )?.name ??
          earlier?.name ??
          "",
        duringRecording: true,
        meetingId: selectionRef.current.meetingId,
        confirmed: true,
      },
    ]);
  }, [
    recording,
    app?.captureControl?.hasMicrophone,
    app?.transitioning,
    isStopping,
    updateDeviceLosses,
  ]);
  /**
   * Applies only the newest state request and loads a complete transcript when the active
   * session has finished. It observes capture rather than starting or stopping it.
   */
  const poll = useCallback(async () => {
    const seq = ++requestSequence.current;
    const epoch = erasureEpoch.current;
    try {
      const state = await api.getAppState();
      if (
        seq !== requestSequence.current ||
        epoch !== erasureEpoch.current ||
        (state.activeMeeting &&
          erasureBlocked.current.has(state.activeMeeting.id))
      )
        return;
      setApp(state);
      setConnected(true);
      setLoading(false);
      if (state.activeMeeting) {
        const active = state.activeMeeting;
        const live =
          state.recording ||
          ["recording", "catching-up", "streaming"].includes(active.status);
        if (live) {
          liveFullRef.current = null;
          setLiveMeeting((previous) => mergeLiveTranscript(previous, active));
          if (!selectedRef.current || selectedRef.current === active.id) {
            selectedRef.current = active.id;
            fullMeetingRef.current = null;
            setMeeting((previous) => mergeLiveTranscript(previous, active));
          }
        } else if (
          liveFullRef.current !== active.id ||
          ((!selectedRef.current || selectedRef.current === active.id) &&
            fullMeetingRef.current !== active.id)
        ) {
          const full = await api.getMeeting(active.id);
          if (
            seq !== requestSequence.current ||
            epoch !== erasureEpoch.current ||
            erasureBlocked.current.has(full.id)
          )
            return;
          liveFullRef.current = full.id;
          setLiveMeeting(full);
          if (!selectedRef.current || selectedRef.current === active.id) {
            selectedRef.current = full.id;
            fullMeetingRef.current = full.id;
            setMeeting(full);
          }
        }
      }
    } catch (failure) {
      if (seq !== requestSequence.current) return;
      setConnected(false);
      setLoading(false);
      setError(errorMessage(failure));
    }
  }, [api]);
  /**
   * Serializes native-confirmed program choices with model discovery. Only a successful
   * query for the pending choice may release its failure gate; automatic replies cannot
   * substitute an older origin.
   */
  const refreshModels = useCallback(
    async (automatic: boolean, selectedPath?: string) => {
      if (selectedPath !== undefined) {
        pendingNativeChoice.current = {
          id: ++nativeChoiceSequence.current,
          path: selectedPath,
        };
      }
      // A native confirmation remains an unverified choice across failed or
      // postponed queries. Manual Refresh must validate that exact choice.
      const choice = automatic ? null : pendingNativeChoice.current;
      const current = latestRuntime.current;
      /**
       * Identifies capture, transition and ASR catch-up states that must finish before
       * another program/model query is started.
       */
      const blocked = (state: AppState | null) =>
        Boolean(
          state?.recording ||
            state?.transitioning ||
            ["recording", "catching-up", "streaming"].includes(
              state?.activeMeeting?.status ?? "",
            ),
        );
      if (
        automatic &&
        (pendingNativeChoice.current !== null ||
          !initialCatalogSeen.current ||
          !current.connected ||
          current.busy ||
          current.isStopping ||
          blocked(current.app) ||
          Date.now() - lastModelRefresh.current < 30000)
      )
        return;
      const previous = modelRefreshInFlight.current;
      if (previous && choice === null) return previous;
      let queriedHandy = false;
      const pending = (async () => {
        // This new native choice owns the queue immediately. Drain the older
        // query, then verify the chosen path even if the older query failed.
        if (previous) {
          try {
            await previous;
          } catch {
            // The previous query owns its error and catalogue-failure gate.
          }
          if (!mounted.current) return;
        }
        if (choice && pendingNativeChoice.current?.id !== choice.id) return;
        // Recheck the backend before spawning Handy: the last rendered status can
        // predate a recording or ASR catch-up started by another command.
        const fresh = await api.getAppState();
        if (
          mounted.current &&
          choice !== null &&
          (blocked(fresh) || latestRuntime.current.isStopping)
        ) {
          const reason =
            locale === "de"
              ? "Die gewählte Programmdatei wurde noch nicht geprüft. Beende zuerst die laufende Aufnahme oder Verarbeitung und klicke dann auf „Aktualisieren“."
              : "The selected program has not been checked yet. Finish the current recording or processing, then click Refresh.";
          catalogueFailure.current = reason;
          setHandyRefreshFailure(reason);
          setModelId("");
          throw new Error(reason);
        }
        if (
          !mounted.current ||
          (automatic && !fresh.handy) ||
          blocked(fresh) ||
          latestRuntime.current.isStopping ||
          (automatic && latestRuntime.current.busy)
        )
          return;
        lastModelRefresh.current = Date.now();
        queriedHandy = true;
        if (!automatic) setSpeakerChecks({});
        setRefreshingHandy(true);
        // The backend retains discovery/native origin. A displayed cached path
        // must not turn an automatic refresh into an explicit file selection.
        const handy = await api
          .refreshHandy(
            automatic ? null : (choice?.path ?? (handyPath.trim() || null)),
          )
          .catch(async (failure) => {
            const reason = errorMessage(failure) || t.modelUnavailable;
            // This gate is independent of status-poll ordering. A newer poll can
            // supersede the reread below, but cannot restore an invalid model.
            catalogueFailure.current = reason;
            if (mounted.current) {
              setSpeakerChecks({});
              setHandyRefreshFailure(reason);
              setModelId("");
            }
            // A rejected refresh can invalidate the backend catalogue. Re-read
            // it before releasing the busy state so stale models cannot start.
            if (mounted.current) await poll();
            throw failure;
          });
        if (!mounted.current) return;
        if (automatic && JSON.stringify(fresh.handy) !== JSON.stringify(handy))
          setSpeakerChecks({});
        // An older automatic query, or an earlier same-path confirmation, has
        // no authority to release the latest still-pending native choice.
        if (
          pendingNativeChoice.current !== null &&
          pendingNativeChoice.current.id !== choice?.id
        )
          return;
        if (choice && handy.error == null) pendingNativeChoice.current = null;
        // Only a successful program/model query may clear a prior failure gate;
        // an older getAppState response has no authority to clear it.
        catalogueFailure.current =
          handy.error == null ? null : handy.error || t.modelUnavailable;
        setHandyRefreshFailure(catalogueFailure.current);
        if (catalogueFailure.current !== null) setModelId("");
        requestSequence.current++;
        setApp((previous) =>
          previous ? { ...previous, handy } : { ...fresh, handy },
        );
        if (catalogueFailure.current !== null)
          setError(catalogueFailure.current);
        else if (choice !== null) setError(null);
      })();
      modelRefreshInFlight.current = pending;
      try {
        await pending;
      } catch (failure) {
        // A confirmed choice that failed before querying Handy is unverified too.
        // Never release the queue back to the previous installation's catalogue.
        if (choice !== null && !queriedHandy) {
          const reason = errorMessage(failure) || t.modelUnavailable;
          catalogueFailure.current = reason;
          if (mounted.current) {
            setHandyRefreshFailure(reason);
            setModelId("");
          }
        }
        throw failure;
      } finally {
        if (modelRefreshInFlight.current === pending) {
          modelRefreshInFlight.current = null;
          if (mounted.current) setRefreshingHandy(false);
        }
        if (queriedHandy) lastModelRefresh.current = Date.now();
      }
    },
    [api, handyPath, poll, locale, t.modelUnavailable],
  );
  useEffect(() => {
    /**
     * Requests a throttled automatic catalogue refresh on window focus and keeps any
     * failure visible to the user.
     */
    const onFocus = () => {
      void refreshModels(true).catch((failure) => {
        if (mounted.current) setError(errorMessage(failure));
      });
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refreshModels]);
  useEffect(() => {
    let disposed = false;
    void poll().then(async () => {
      if (disposed) return;
      try {
        await Promise.all([refreshDevices(), refreshHistory()]);
      } catch (failure) {
        if (!disposed) setError(errorMessage(failure));
      }
    });
    const timer = setInterval(() => void poll(), 1500);
    return () => {
      disposed = true;
      requestSequence.current++;
      clearInterval(timer);
    };
  }, [poll, refreshDevices, refreshHistory]);
  /**
   * Wraps a requested foreground operation with visible busy/error state. Native services
   * remain responsible for conflicts and durable changes.
   */
  async function action(work: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }
  /** Own only the fields displayed by confirmation, never a full runtime Meeting. */
  function openMeetingErasure(target: MeetingSummary) {
    setErasureTarget({
      id: target.id,
      title: target.title,
      startedAt: target.startedAt,
      durationMs: target.durationMs,
      status: target.status,
      modelId: target.modelId,
    });
  }
  /** Invalidate pending reads and discard only the explicitly selected meeting's cached content. */
  function beginMeetingErasure(id: string) {
    notesJob.forget(id);
    erasureBlocked.current.add(id);
    ++erasureEpoch.current;
    ++requestSequence.current;
    ++notesLoadSequence.current;
    if (transcriptRead.current?.id === id) {
      transcriptRead.current = null;
      setTranscriptLoading(false);
    }
    if (selectedRef.current === id) {
      selectedRef.current = null;
      fullMeetingRef.current = null;
      setMeeting(null);
      setSource(null);
      setFileExport(null);
      setCopied(null);
    }
    if (liveFullRef.current === id) liveFullRef.current = null;
    setLiveMeeting((current) => (current?.id === id ? null : current));
    setApp((current) =>
      current?.activeMeeting?.id === id
        ? { ...current, activeMeeting: null, recording: false }
        : current,
    );
    if (notesSelectionRef.current === id) {
      notesSelectionRef.current = "";
      setNotesMeetingId("");
      setAnalyses([]);
      setResultId("");
      setPendingAnalyses([]);
      setSkippedResults([]);
      setSkippedPending([]);
      setResumeAnalysis(null);
    }
    setCloudRequest((current) => (current?.meetingId === id ? null : current));
    setSearchSource((current) => (current?.meetingId === id ? null : current));
    setError(null);
    setErasureNotice(null);
    setPage("history");
  }
  /** Keep stopped partial erasure visible for a new preview; stale replies remain invalidated. */
  function stopMeetingErasure(id: string) {
    ++erasureEpoch.current;
    ++requestSequence.current;
    erasureBlocked.current.delete(id);
    setErasureRevision((value) => value + 1);
    void refreshHistory().catch((failure) => setError(errorMessage(failure)));
    window.dispatchEvent(new Event("library-index-updated"));
  }
  /** File erasure is already confirmed; index rebuilding has its own subsequent outcome. */
  function completeMeetingErasure(id: string, preservedAtomicRemnants = 0) {
    notesJob.forget(id, true);
    ++erasureEpoch.current;
    ++requestSequence.current;
    erasureBlocked.current.add(id);
    setHistory((current) => current.filter((item) => item.id !== id));
    setErasureTarget(null);
    setErasureRevision((value) => value + 1);
    setErasureNotice(
      preservedAtomicRemnants > 0
        ? locale === "de"
          ? `Zugeordnete Meetinginhalte entfernt. ${preservedAtomicRemnants} unterbrochene Dateien sind nicht sicher zuordenbar und bleiben erhalten. Sie können weiterhin Meetingdaten enthalten; bitte die Ablage manuell prüfen.`
          : `Attributed meeting contents removed. ${preservedAtomicRemnants} interrupted files cannot be assigned safely and are preserved. They may still contain meeting data; inspect storage manually.`
        : locale === "de"
          ? "Meetinginhalte vollständig aus der HandyMeeting-Ablage entfernt."
          : "Meeting contents completely removed from HandyMeeting storage.",
    );
    void refreshHistory().catch((failure) => setError(errorMessage(failure)));
    void api
      .rebuildSearchIndex?.()
      .catch(() =>
        setError(
          locale === "de"
            ? "Meeting gelöscht. Der Suchindex muss unter Einstellungen neu aufgebaut werden."
            : "Meeting deleted. Rebuild the search index in Settings.",
        ),
      );
    window.dispatchEvent(new Event("library-index-updated"));
  }
  /**
   * Loads the exact requested meeting before switching its detail or analysis view; a
   * search-source highlight stays bound to that meeting ID.
   */
  async function openMeeting(
    id: string,
    nextPage: Page = "detail",
    source: string | null = null,
  ) {
    if (nextPage === "notes") {
      await openNotesFor(id);
      return;
    }
    await action(async () => {
      const epoch = erasureEpoch.current;
      const item = await api.getMeeting(id);
      if (epoch !== erasureEpoch.current || erasureBlocked.current.has(id))
        return;
      // A search hit applies only to the meeting it opened; other opens clear it.
      setSearchSource(source ? { meetingId: item.id, file: source } : null);
      selectedRef.current = item.id;
      fullMeetingRef.current = item.id;
      setMeeting(item);
      setAnalyses([]);
      setResultId("");
      setPage(nextPage);
    });
  }
  /**
   * Discovers finished and resumable analyses independently for one meeting. Generation
   * checks discard stale selection replies while preserving results from a successful
   * sibling lookup.
   */
  async function selectNotesMeeting(id: string) {
    const sequence = ++notesLoadSequence.current;
    /**
     * Checks mounted state, request generation and meeting ownership before a discovery
     * reply may update the analysis view.
     */
    const isCurrentSelection = () =>
      mounted.current &&
      sequence === notesLoadSequence.current &&
      notesSelectionRef.current === id &&
      !erasureBlocked.current.has(id);
    notesSelectionRef.current = id;
    setNotesMeetingId(id);
    setAnalyses([]);
    setResultId("");
    setPendingAnalyses([]);
    setSkippedResults([]);
    setSkippedPending([]);
    setResumeAnalysis(null);
    if (!id) {
      setNotesMeetingLoading(false);
      return;
    }
    setNotesMeetingLoading(true);
    setError(null);
    try {
      const current = await api.getMeeting(id);
      if (
        !mounted.current ||
        sequence !== notesLoadSequence.current ||
        notesSelectionRef.current !== id
      )
        return;
      if (current.id !== id) throw new Error(t.wrongMeeting);
      selectedRef.current = current.id;
      fullMeetingRef.current = current.id;
      setMeeting(current);
      const failures: string[] = [];
      /**
       * Combines distinct discovery failures only for the still-current selection, leaving
       * successful sibling results visible.
       */
      const reportDiscoveryFailure = (failure: unknown) => {
        if (!isCurrentSelection()) return;
        failures.push(errorMessage(failure));
        setError([...new Set(failures)].join("\n"));
      };
      await Promise.all([
        api
          .listAnalyses(id)
          .then(({ analyses: items, skipped }) => {
            if (!isCurrentSelection()) return;
            if (items.some((item) => item.meetingId !== id))
              throw new Error(t.wrongMeeting);
            setAnalyses(items);
            setSkippedResults(skipped);
            setResultId(items[0]?.id ?? "");
          })
          .catch(reportDiscoveryFailure),
        api
          .listUnfinishedAnalyses(id)
          .then(({ items: pending, skipped }) => {
            if (!isCurrentSelection()) return;
            if (pending.some((item) => item.meetingId !== id))
              throw new Error(t.wrongMeeting);
            setPendingAnalyses(pending);
            setSkippedPending(skipped);
          })
          .catch(reportDiscoveryFailure),
      ]);
    } catch (failure) {
      if (mounted.current && sequence === notesLoadSequence.current)
        setError(errorMessage(failure));
    } finally {
      if (mounted.current && sequence === notesLoadSequence.current)
        setNotesMeetingLoading(false);
    }
  }
  /**
   * Opens analysis for an explicit meeting ID and refreshes history without starting an
   * analysis job.
   */
  async function openNotesFor(id: string) {
    setPage("notes");
    void refreshHistory().catch((failure) => setError(errorMessage(failure)));
    await selectNotesMeeting(id);
  }
  /**
   * Changes the visible page and refreshes the corresponding history or meeting-bound
   * analysis selection when needed.
   */
  async function navigate(next: Page) {
    setPage(next);
    if (next === "history") void action(refreshHistory);
    if (next === "notes") {
      const id =
        (page === "detail"
          ? meeting?.id
          : page === "meeting"
            ? liveMeeting?.id
            : notesSelectionRef.current) ||
        meeting?.id ||
        "";
      void refreshHistory().catch((failure) => setError(errorMessage(failure)));
      void selectNotesMeeting(id);
    }
  }
  /**
   * Starts capture only after the synchronous catalogue, pending-choice and refresh gates
   * pass. It sends the selected model and separate track IDs to the native service.
   */
  async function start() {
    await action(async () => {
      if (catalogueFailure.current !== null)
        throw new Error(catalogueFailure.current);
      if (pendingNativeChoice.current !== null) throw new Error(t.busy);
      if (modelRefreshInFlight.current) throw new Error(t.busy);
      const selected = {
        system: systemId,
        microphone: micEnabled ? micId : null,
      };
      startDevices.current = selected;
      let item: Meeting;
      try {
        item = await api.startMeeting({
          title: title.trim() || defaultMeetingTitle(locale),
          modelId,
          systemDeviceId: selected.system,
          microphoneDeviceId: selected.microphone,
        });
      } catch (failure) {
        if (startDevices.current === selected) startDevices.current = null;
        throw failure;
      }
      selectedRef.current = item.id;
      fullMeetingRef.current = null;
      liveFullRef.current = null;
      setLiveMeeting(item);
      setMeeting(item);
      setAnalyses([]);
      setResultId("");
      await poll();
      await refreshHistory();
    });
  }
  /**
   * Requests native capture finalization, then refreshes the retained meeting and history
   * while keeping the stopping state visible.
   */
  async function stop() {
    const work = async () => {
      // A reconnect answer created before this stop must not restore the recording.
      stopEpoch.current++;
      setIsStopping(true);
      try {
        const item = await api.stopMeeting();
        selectedRef.current = item.id;
        fullMeetingRef.current = item.id;
        liveFullRef.current = item.id;
        setLiveMeeting(item);
        setMeeting(item);
        await poll();
        await refreshHistory();
      } finally {
        setIsStopping(false);
      }
    };
    // During a reconnect Stop must reach the native cancellation even when an unrelated
    // foreground action (such as a history refresh) holds the busy state.
    if (busy && reconnecting) {
      if (isStopping) return;
      setError(null);
      await work().catch((failure) => setError(errorMessage(failure)));
      return;
    }
    await action(work);
  }
  /**
   * Reloads the bound meeting before copying every original section, bypassing the bounded
   * live preview used for display.
   */
  async function copyFullTranscript(target: Meeting | null = meeting) {
    if (!target) return;
    const id = target.id;
    await action(async () => {
      const epoch = erasureEpoch.current;
      const full = await api.getMeeting(id);
      if (epoch !== erasureEpoch.current || erasureBlocked.current.has(id))
        return;
      await api.copyText(renderTranscript(full, locale));
      setCopied("transcript");
      if (copyTimer.current) clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(null), 2000);
    });
  }
  /**
   * Commits a replacement template through the revisioned library; editing the protected
   * speaker-name template requires duplication first.
   */
  async function updateTemplate(next: AnalysisTemplate) {
    if (next.id === SPEAKER_NAME_TEMPLATE.id) {
      setError(
        locale === "de"
          ? "Diese integrierte Vorlage ist geschützt. Bitte zuerst duplizieren."
          : "This built-in template is protected. Duplicate it first.",
      );
      return false;
    }
    return templateStore.commit(
      templateStore.templates.map((item) =>
        item.id === next.id ? next : item,
      ),
    );
  }
  /**
   * Saves a validated new or edited template step and closes the editor only after the
   * library accepts the change.
   */
  async function saveStep() {
    if (!editing) return;
    if (!editing.name.trim() || !editing.prompt.trim()) {
      setError(t.invalidTemplate);
      return;
    }
    const next = {
      ...editing,
      name: editing.name.trim(),
      prompt: editing.prompt.trim(),
    };
    if (
      !(await updateTemplate({
        ...template,
        steps: editingNew
          ? [...template.steps, next]
          : template.steps.map((item) => (item.id === next.id ? next : item)),
      }))
    )
      return;
    setStepId(next.id);
    setEditing(null);
  }
  /**
   * Swaps the selected template step with an in-range offset target and delegates
   * persistence to the template library.
   */
  function moveStep(delta: number) {
    const index = template.steps.findIndex((item) => item.id === stepId);
    if (
      index < 0 ||
      index + delta < 0 ||
      index + delta >= template.steps.length
    )
      return;
    const steps = [...template.steps];
    [steps[index], steps[index + delta]] = [steps[index + delta], steps[index]];
    void updateTemplate({ ...template, steps });
  }
  /**
   * Prepares a detached template-action draft so opening a rename, duplicate or delete
   * dialog cannot mutate the stored template.
   */
  function openTemplateAction(
    kind: TemplateAction["kind"],
    original = template,
  ) {
    setTemplateActionError(null);
    setTemplateAction({
      kind,
      original: structuredClone(original),
      name:
        kind === "new"
          ? ""
          : kind === "duplicate"
            ? original.name + " · " + t.duplicateName
            : original.name,
    });
  }
  /**
   * Validates the template action and commits one new library revision. Duplicates receive
   * fresh template and step IDs; protected/last-template deletions are refused.
   */
  async function saveTemplateAction() {
    if (!templateAction) return;
    const { kind, original } = templateAction;
    const name = templateAction.name.trim();
    if (
      kind !== "delete" &&
      templates.some(
        (item) =>
          item.name.trim().toLocaleLowerCase() === name.toLocaleLowerCase() &&
          !(kind === "rename" && item.id === original.id),
      )
    ) {
      setTemplateActionError(t.duplicateTemplateName);
      return;
    }
    let next: AnalysisTemplate | null = null;
    let updated: AnalysisTemplate[];
    if (kind === "delete") {
      if (
        templateStore.templates.length < 2 ||
        original.id === SPEAKER_NAME_TEMPLATE.id
      )
        return;
      updated = templateStore.templates.filter(
        (item) => item.id !== original.id,
      );
    } else if (kind === "rename") {
      const current = templateStore.templates.find(
        (item) => item.id === original.id,
      );
      if (!current) {
        setTemplateActionError(t.templateSaveFailed);
        return;
      }
      next = { ...current, name };
      updated = templateStore.templates.map((item) =>
        item.id === original.id ? next! : item,
      );
    } else {
      const base = kind === "new" ? defaultTemplate(locale) : original;
      next = {
        ...structuredClone(base),
        id: uid(),
        name,
        steps: base.steps.map((item) => ({ ...item, id: uid() })),
      };
      updated = [...templateStore.templates, next];
    }
    if (!(await templateStore.commit(updated))) {
      setTemplateActionError(t.templateSaveFailed);
      return;
    }
    const selected = next ?? updated[0];
    setTemplateId(selected.id);
    setStepId(selected.steps[0].id);
    setResultId("");
    setTemplateAction(null);
  }
  /**
   * Snapshots the current meeting/template/model choice. Local analysis starts directly;
   * cloud text remains pending explicit per-run consent.
   */
  function beginNotes() {
    if (!notesMeeting || !canStartNotes) return;
    const request: NotesRequest = {
      meetingId: notesMeeting.id,
      template: structuredClone(template),
      endpoint: endpoint.trim(),
      model: textModel.trim(),
      mode,
      cloudConsent: false,
      apiKey,
      useSavedKey,
    };
    if (mode === "cloud") setCloudRequest(request);
    else void runNotes(request);
  }
  /**
   * Runs the requested analysis and rediscovers resumable work only for its still-selected
   * meeting, retaining discovery errors for review.
   */
  async function runNotes(request: NotesRequest) {
    setError(null);
    setCloudRequest(null);
    await notesJob.run(request);
    const sequence = notesLoadSequence.current;
    try {
      const { items: pending, skipped } = await api.listUnfinishedAnalyses(
        request.meetingId,
      );
      if (
        mounted.current &&
        sequence === notesLoadSequence.current &&
        notesSelectionRef.current === request.meetingId
      ) {
        if (pending.some((item) => item.meetingId !== request.meetingId))
          throw new Error(t.wrongMeeting);
        setPendingAnalyses(pending);
        setSkippedPending(skipped);
      }
    } catch (failure) {
      if (mounted.current && notesSelectionRef.current === request.meetingId)
        setError(errorMessage(failure));
    }
  }
  /**
   * Retries the last in-memory run only for the same meeting and resets cloud consent
   * before any cloud retry.
   */
  function resumeNotes() {
    const previous = notesJob.lastRequest,
      runId = notesJob.progress?.runId;
    if (!previous || !runId || previous.meetingId !== notesMeetingId) return;
    const request = { ...previous, resumeRunId: runId, cloudConsent: false };
    if (request.mode === "cloud") setCloudRequest(request);
    else void runNotes(request);
  }
  /**
   * Copies backend-rendered Markdown only when the selected result belongs to the selected
   * meeting; it does not render model text as browser markup.
   */
  async function copyNotesMarkdown() {
    if (!result || !notesMeeting || result.meetingId !== notesMeeting.id) {
      setError(t.wrongMeeting);
      return;
    }
    await action(async () => {
      const markdown = await api.notesMarkdown(result.meetingId, result.id);
      await api.copyText(markdown);
      setCopied("notes");
      if (copyTimer.current) clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(null), 2000);
    });
  }
  /**
   * Opens a source only when its ID exists and its text contains the literal quote. This
   * structural check does not establish semantic correctness.
   */
  function showEvidence(evidence: Evidence) {
    const original = result?.sources.find(
      (item) => item.id === evidence.sourceId,
    );
    if (!original || !original.text.includes(evidence.quote)) {
      setError(t.sourceMissing);
      return;
    }
    setSource({
      sources: result!.sources,
      index: result!.sources.indexOf(original),
      quote: evidence.quote,
    });
  }
  /**
   * Maps known meeting states to localized labels and treats unrecognized states as
   * pending rather than complete.
   */
  const statusName = (value: string) =>
    value === "complete"
      ? t.complete
      : value === "recording"
        ? t.recording
        : value === "recorded"
          ? t.locallySaved
          : t.pending;
  const selectedStep =
    template.steps.find((item) => item.id === stepId) ?? template.steps[0];
  const resultSection =
    result?.sections.find((item) => item.id === stepId) ?? result?.sections[0];
  const shownMeeting = page === "meeting" ? liveMeeting : meeting;
  const orderedParts = shownMeeting
    ? [...shownMeeting.parts]
        .sort(
          (a, b) =>
            a.startSample - b.startSample || a.source.localeCompare(b.source),
        )
        .filter((item) => (item.text ?? "").trim())
    : [];
  const recentParts = orderedParts;
  const textModelSettings = (
    <TextModelSettings
      api={api}
      t={t}
      locale={locale}
      connected={connected}
      disabled={notesBusy || busy}
      mode={mode}
      setMode={setMode}
      endpoint={endpoint}
      setEndpoint={setEndpoint}
      textModel={textModel}
      setTextModel={setTextModel}
      apiKey={apiKey}
      setApiKey={setApiKey}
      useSavedKey={useSavedKey}
      setUseSavedKey={setUseSavedKey}
      localModels={localTextModels.models}
      modelsLoading={localTextModels.loading}
      modelsLoaded={localTextModels.loaded}
      modelsError={localTextModels.error}
      refreshModels={() => void localTextModels.refresh()}
      manualModel={manualTextModel}
      setManualModel={setManualTextModel}
    />
  );
  /**
   * Uses native saved-audio totals or the greatest confirmed 16 kHz sample boundary,
   * expressed in milliseconds independently of ASR progress.
   */
  const savedByTrack = (track: "system" | "microphone") =>
    shownMeeting?.savedAudioMs?.[track] ??
    Math.max(
      0,
      ...(shownMeeting?.parts ?? [])
        .filter((item) => item.source === track)
        .map(
          (item) =>
            (item.startSample +
              (item.confirmedSampleCount ?? item.sampleCount)) /
            16,
        ),
    );
  const backlog =
    shownMeeting?.pendingAudioMs ??
    (shownMeeting?.livePreviewLimited
      ? null
      : Math.max(
          0,
          ...["system", "microphone"].map(
            (track) =>
              (shownMeeting?.parts ?? [])
                .filter((item) => item.source === track)
                .reduce(
                  (sum, item) =>
                    sum +
                    (item.status === "complete"
                      ? 0
                      : Math.max(
                          0,
                          item.sampleCount - (item.processedSamples ?? 0),
                        )),
                  0,
                ) / 16,
          ),
        ));

  /**
   * Binds live notes to the recording, including headless auto-start on other pages.
   * Switching to a local model clears credentials and respects a running analysis lock.
   */
  const livePanel = (headless: boolean) =>
    liveMeeting && !liveMeeting.readOnly ? (
      <LiveNotesPanel
        api={api}
        locale={locale}
        connected={connected}
        meetingId={liveMeeting.id}
        meetingTitle={liveMeeting.title}
        meetingActive={
          app?.activeMeeting?.id === liveMeeting.id && (recording || catchingUp)
        }
        connection={{
          endpoint,
          model: textModel,
          mode,
          apiKey,
          useSavedKey,
        }}
        preferences={livePreferences}
        setPreferences={setLivePreferences}
        onUseLocalModel={(nextEndpoint, nextModel) => {
          if (notesBusy) return;
          // A key entered for another route must never reach the local server.
          setApiKey("");
          setUseSavedKey(false);
          setMode("local");
          setEndpoint(nextEndpoint);
          setTextModel(nextModel);
          setManualTextModel(false);
        }}
        onOpenTextModelSettings={() => {
          setPage("settings");
          requestAnimationFrame(() => {
            const section = document.getElementById("text-model-settings");
            section?.scrollIntoView?.({ block: "start" });
            section?.focus();
          });
        }}
        focus={liveFocus}
        setFocus={setLiveFocus}
        headless={headless}
        connectionLocked={notesBusy}
      />
    ) : undefined;

  return (
    <div className="app-shell">
      <Sidebar
        app={app}
        page={page}
        locale={locale}
        theme={theme}
        t={t}
        navigate={navigate}
        setTheme={setTheme}
        setLocale={setLocale}
        recordingStatus={
          recording && app?.activeMeeting ? (
            <RecordingStatus
              locale={locale}
              paused={Boolean(app.captureControl?.paused)}
              durationMs={
                liveMeeting?.id === app.activeMeeting.id
                  ? liveMeeting.durationMs
                  : app.activeMeeting.durationMs
              }
              parts={
                liveMeeting?.id === app.activeMeeting.id
                  ? liveMeeting.parts
                  : app.activeMeeting.parts
              }
              warningCount={new Set(app.activeMeeting.warnings).size}
              microphoneSilent={
                micEnabled &&
                app.captureControl?.hasMicrophone === false &&
                !app.captureControl.paused
              }
              microphoneMuted={Boolean(app.captureControl?.microphoneMuted)}
              backlogMs={app.activeMeeting.pendingAudioMs ?? null}
              getSystemLoad={api.getSystemLoad}
              onOpen={() => setPage("meeting")}
            />
          ) : undefined
        }
      />
      <main ref={mainRef}>
        {erasureNotice && (
          <p role="status" className="notice">
            {erasureNotice}
          </p>
        )}
        {page === "history" && canEraseMeeting && (
          <PendingErasures
            api={api}
            locale={locale}
            revision={erasureRevision}
            onResume={openMeetingErasure}
          />
        )}
        <header className="page-header">
          <div>
            <h1 tabIndex={-1}>
              {page === "detail" ? t.meetingDetail : t[page]}
            </h1>
            <p className={page === "meeting" ? "sr-only" : undefined}>
              {page === "meeting"
                ? t.preserve
                : page === "notes"
                  ? t.draft
                  : page === "history" || page === "detail"
                    ? t.historyHelp
                    : t.compatibility}
            </p>
          </div>
        </header>
        {(page === "meeting"
          ? liveMeeting
          : page === "detail"
            ? meeting
            : null) && (
          <MeetingLabels
            api={api}
            meeting={(page === "meeting" ? liveMeeting : meeting)!}
            locale={locale}
            onChanged={(meta) => {
              const target = (page === "meeting" ? liveMeeting : meeting)!;
              setMeeting((old) =>
                old?.id === target.id
                  ? {
                      ...old,
                      title: meta.title ?? old.title,
                      userMetadata: meta,
                    }
                  : old,
              );
              setLiveMeeting((old) =>
                old?.id === target.id
                  ? {
                      ...old,
                      title: meta.title ?? old.title,
                      userMetadata: meta,
                    }
                  : old,
              );
              void refreshHistory().catch((e) => setError(errorMessage(e)));
            }}
          />
        )}
        {((page === "detail" && meeting) ||
          (page === "meeting" && liveMeeting)) && (
          <details
            className="speaker-tools"
            open={speakerOptionsOpen}
            onToggle={(event) => {
              if (event.target === event.currentTarget)
                setSpeakerOptionsOpen(event.currentTarget.open);
            }}
          >
            <summary>
              {locale === "de"
                ? "Sprecher zuordnen · optional"
                : "Speaker tools · optional"}
            </summary>
            {page === "detail" && meeting && (
              <SpeakerLabels
                embedded
                key={"speaker-labels-" + meeting.id}
                api={api}
                meeting={meeting}
                locale={locale}
                proposeDisabled={notesBusy}
                onLabels={onSpeakerLabels}
                onPropose={() => {
                  if (notesBusy) return;
                  void action(async () => {
                    // Name suggestions run locally only; local settings stay as they are.
                    if (mode === "cloud") {
                      setMode("local");
                      setApiKey("");
                      setSavedKeyScope(null);
                    }
                    await openNotesFor(meeting.id);
                    if (notesSelectionRef.current === meeting.id) {
                      setTemplateId(SPEAKER_NAME_TEMPLATE.id);
                      setResultId("");
                    }
                  });
                }}
              />
            )}
            {speakerOptionsOpen &&
              (page === "detail"
                ? meeting
                : page === "meeting"
                  ? liveMeeting
                  : null) && (
                <SpeakerExperiments
                  embedded
                  checkedModels={speakerChecks}
                  selectedModelId={speakerModelId}
                  onSelectedModelChange={setSpeakerModelId}
                  checkingBlocked={
                    speakerChecking || notesBusy || busy || refreshingHandy
                  }
                  onModelChecked={onSpeakerModelChecked}
                  onCheckingChange={onSpeakerCheckingChange}
                  key={
                    "speaker-experiments-" +
                    (page === "detail" ? meeting : liveMeeting)!.id
                  }
                  api={api}
                  locale={locale}
                  meeting={page === "detail" ? meeting : liveMeeting}
                  models={app?.handy?.models ?? []}
                  recording={Boolean(
                    app?.recording &&
                      (page === "detail" ? meeting : liveMeeting)?.id ===
                        liveMeeting?.id,
                  )}
                />
              )}
          </details>
        )}
        {error && (
          <div role="alert" className="notice error">
            <span>{error}</span>
            <button
              className="icon-button"
              aria-label={t.close}
              onClick={() => setError(null)}
            >
              <X size={17} />
            </button>
          </div>
        )}
        <DeviceNotices
          losses={deviceLosses.filter((loss) => !loss.dismissed)}
          locale={locale}
          t={t}
          onDismiss={(loss) =>
            updateDeviceLosses(
              deviceLossesRef.current.map((item) =>
                item === loss ? { ...item, dismissed: true } : item,
              ),
            )
          }
        />
        {isStopping && (
          <div role="status" className="notice info">
            {t.stoppingDetail}
          </div>
        )}
        {!!app?.error && app.error !== error && (
          <div className="notice warning" role="status">
            {app.error}
          </div>
        )}
        {!connected && (
          <div className="notice warning">
            <div>
              <strong>{loading ? t.connecting : t.disconnected}</strong>
              <p>{t.connectHelp}</p>
            </div>
            <button className="secondary" onClick={() => void poll()}>
              <RefreshCw size={15} />
              {t.retry}
            </button>
          </div>
        )}
        <AgentPanel
          agent={app?.agent}
          blocked={
            recording || isStopping || catchingUp || Boolean(app?.transitioning)
          }
          devicesReady={Boolean(
            modelId &&
              models.some((item) => item.id === modelId) &&
              devices.some(
                (item) => item.id === systemId && item.source === "system",
              ),
          )}
          microphoneReady={devices.some(
            (item) => item.id === micId && item.source === "microphone",
          )}
          busy={busy}
          t={t}
          onDecide={(approve) => {
            const pending = app?.agent?.pending;
            if (!pending) return;
            void action(async () => {
              await api.agentDecideRecording(
                pending.requestId,
                approve,
                approve
                  ? {
                      title: pending.title,
                      modelId,
                      systemDeviceId: systemId,
                      microphoneDeviceId: pending.includeMicrophone
                        ? micId
                        : null,
                    }
                  : undefined,
              );
              await poll();
              await refreshHistory();
            });
          }}
          onRevoke={() =>
            void action(async () => {
              await api.agentRevoke();
              await poll();
            })
          }
        />
        {page === "meeting" && (
          <MeetingView
            t={t}
            x={x}
            app={app}
            meeting={liveMeeting}
            recording={recording}
            busy={
              busy ||
              reconnecting ||
              speakerChecking ||
              refreshingHandy ||
              catchingUp ||
              (app?.transitioning ?? false)
            }
            reconnecting={reconnecting}
            controlsAvailable={Boolean(
              api.setCapturePaused && api.setMicrophoneMuted,
            )}
            getAudioLevels={api.getAudioLevels}
            onReconnectMicrophone={
              api.reconnectMicrophone
                ? () => {
                    const id = app?.activeMeeting?.id;
                    if (!id || busy || reconnecting) return;
                    const epoch = stopEpoch.current;
                    setReconnecting(true);
                    setError(null);
                    void api.reconnectMicrophone!(id)
                      .then((state) => {
                        if (epoch !== stopEpoch.current) return;
                        requestSequence.current++;
                        setApp(state);
                      })
                      .catch((failure) => {
                        // Stopping cancels the reconnect; that is not a device error,
                        // also when the stop has already finished.
                        if (
                          epoch === stopEpoch.current &&
                          !latestRuntime.current.isStopping
                        )
                          setError(errorMessage(failure));
                      })
                      .finally(() => setReconnecting(false));
                  }
                : undefined
            }
            onPause={(paused) => {
              const id = app?.activeMeeting?.id;
              if (!id || !api.setCapturePaused) return;
              void action(async () => {
                const state = await api.setCapturePaused!(id, paused);
                requestSequence.current++;
                setApp(state);
              });
            }}
            onMute={(muted) => {
              const id = app?.activeMeeting?.id;
              if (!id || !api.setMicrophoneMuted) return;
              void action(async () => {
                const state = await api.setMicrophoneMuted!(id, muted);
                requestSequence.current++;
                setApp(state);
              });
            }}
            isStopping={isStopping}
            connected={connected}
            title={title}
            defaultTitle={defaultMeetingTitle(locale)}
            onAnalyse={() => {
              if (liveMeeting) void openNotesFor(liveMeeting.id);
            }}
            onOpenFolder={() =>
              void action(async () => {
                if (liveMeeting) await api.openMeetingFolder(liveMeeting.id);
              })
            }
            onExport={(format) =>
              void action(async () => {
                if (liveMeeting) {
                  const id = liveMeeting.id;
                  setFileExport(null);
                  const file =
                    format === "txt"
                      ? await api.exportTranscript(id, locale)
                      : await api.exportTranscript(id, locale, format);
                  setFileExport({ id, file });
                }
              })
            }
            savedFile={
              fileExport?.id === liveMeeting?.id
                ? (fileExport?.file ?? null)
                : null
            }
            modelId={modelId}
            systemId={systemId}
            micId={micId}
            micEnabled={micEnabled}
            models={models}
            devices={devices}
            copied={copied === "transcript"}
            backlog={backlog}
            recentParts={recentParts}
            transcriptLoading={transcriptLoading}
            loadFullTranscript={loadLiveTranscript}
            locale={locale}
            setTitle={setTitle}
            setModelId={setModelId}
            setSystemId={(id) => chooseDevice("system", id)}
            setMicId={(id) => chooseDevice("microphone", id)}
            setMicEnabled={setMicEnabled}
            start={start}
            stop={stop}
            savedByTrack={savedByTrack}
            copyFullTranscript={() => copyFullTranscript(liveMeeting)}
            liveFocus={liveFocus}
            onLiveFocus={setLiveFocus}
            liveNotes={livePanel(false)}
            refreshHandy={() => void action(() => refreshModels(false))}
            refreshDevices={() => void action(refreshDevices)}
            resume={() =>
              void action(async () => {
                if (!meeting) return;
                const current = await api.resumeMeeting(meeting.id);
                selectedRef.current = current.id;
                fullMeetingRef.current = null;
                setMeeting(current);
                await poll();
              })
            }
          />
        )}
        {page !== "meeting" && livePreferences.auto && livePanel(true)}
        {page === "detail" && meeting && (
          <MeetingDetail
            meeting={meeting}
            onErase={
              canEraseMeeting ? () => openMeetingErasure(meeting) : undefined
            }
            selectedFile={
              searchSource?.meetingId === meeting.id ? searchSource.file : null
            }
            locale={locale}
            t={t}
            busy={busy}
            copied={copied === "transcript"}
            onBack={() => void navigate("history")}
            onCopy={() => void copyFullTranscript(meeting)}
            onAnalyse={() => void openNotesFor(meeting.id)}
            onOpenFolder={() =>
              void action(() => api.openMeetingFolder(meeting.id))
            }
            onExport={(format) =>
              void action(async () => {
                const id = meeting.id;
                setFileExport(null);
                const file =
                  format === "txt"
                    ? await api.exportTranscript(id, locale)
                    : await api.exportTranscript(id, locale, format);
                setFileExport({ id, file });
              })
            }
            savedFile={fileExport?.id === meeting.id ? fileExport.file : null}
            speakers={
              speakerLabels?.meetingId === meeting.id
                ? speakerLabels.labels
                : undefined
            }
          />
        )}
        {page === "history" && api.searchMeetings && (
          <HistorySearch
            api={api}
            locale={locale}
            imported={history.filter((item) => item.id.startsWith("legacy-"))}
            nativeMeetings={history.filter(
              (item) => !item.id.startsWith("legacy-"),
            )}
            onRefresh={() => void action(refreshHistory)}
            onErase={canEraseMeeting ? openMeetingErasure : undefined}
            hiddenMeetingIds={erasureBlocked.current}
            onOpen={(id, file) => {
              void openMeeting(id, "detail", file);
            }}
          />
        )}
        {page === "history" && !api.searchMeetings && (
          <HistoryList
            history={history.filter(
              (item) => !erasureBlocked.current.has(item.id),
            )}
            onErase={canEraseMeeting ? openMeetingErasure : undefined}
            locale={locale}
            t={t}
            busy={busy}
            connected={connected}
            onRefresh={() => void action(refreshHistory)}
            onOpen={(id) => void openMeeting(id)}
            statusName={statusName}
          />
        )}
        {page === "settings" && (
          <SettingsView
            textModelSettings={textModelSettings}
            speakerModelSettings={
              api.checkSpeakerModel
                ? (statusContainer) => (
                    <SpeakerExperiments
                      setupOnly
                      embedded
                      statusContainer={statusContainer}
                      api={api}
                      locale={locale}
                      models={app?.handy?.models ?? []}
                      recording={
                        recording || catchingUp || Boolean(app?.transitioning)
                      }
                      checkedModels={speakerChecks}
                      selectedModelId={speakerModelId}
                      onSelectedModelChange={setSpeakerModelId}
                      onModelChecked={onSpeakerModelChecked}
                      onCheckingChange={onSpeakerCheckingChange}
                      checkingBlocked={
                        speakerChecking || notesBusy || busy || refreshingHandy
                      }
                    />
                  )
                : undefined
            }
            api={api}
            app={app}
            locale={locale}
            theme={theme}
            t={t}
            busy={busy}
            connected={connected}
            refreshingHandy={refreshingHandy}
            blocked={
              recording ||
              isStopping ||
              catchingUp ||
              Boolean(app?.transitioning)
            }
            preferencesPersistable={preferences.persistable}
            handyPath={handyPath}
            onLocale={setLocale}
            onTheme={setTheme}
            onRefreshHandy={() => void action(() => refreshModels(false))}
            onRefreshDevices={() => void action(refreshDevices)}
            onChooseHandy={() =>
              void action(async () => {
                const selected = await api.chooseHandyExecutable?.();
                if (!selected) return;
                setHandyPath(selected);
                await refreshModels(false, selected);
              })
            }
            onRebuildIndex={() =>
              void api
                .rebuildSearchIndex?.()
                .catch((e) => setError(errorMessage(e)))
            }
            onImportLegacy={() =>
              void action(async () => {
                const selected = await api.chooseLegacyManifest?.();
                if (!selected) return;
                const imported = await api.importLegacy(selected);
                selectedRef.current = imported.id;
                fullMeetingRef.current = imported.id;
                setMeeting(imported);
                setAnalyses([]);
                setResultId("");
                setPage("detail");
                await refreshHistory();
              })
            }
          />
        )}
        {page === "notes" && skippedRuns.length > 0 && (
          <div className="notice warning" role="status">
            <p>{t.skippedAnalyses}</p>
            <p className="muted">{skippedRuns.join(", ")}</p>
          </div>
        )}
        {page === "notes" && pendingAnalyses.length > 0 && (
          <section
            className="pending-analyses"
            aria-label={t.unfinishedAnalyses}
          >
            <h2>{t.unfinishedAnalyses}</h2>
            <p>{t.resumeHelp}</p>
            <div className="pending-analyses-list">
              {pendingAnalyses.map((run) => (
                <div className="pending-analysis-row" key={run.id}>
                  <div>
                    <strong>{run.template.name}</strong> ·{" "}
                    {new Date(run.createdAt).toLocaleString(
                      locale === "de" ? "de-DE" : "en-GB",
                    )}
                    <p>
                      {run.model} · {run.confirmedTasks}/{run.totalTasks}{" "}
                      {t.confirmedTasks}
                    </p>
                    {!run.resumable && (
                      <p className="muted">
                        {t.earlierPrompt}: {run.promptVersion} ·{" "}
                        {t.earlierPromptPending}
                      </p>
                    )}
                  </div>
                  {run.resumable && (
                    <button
                      className="secondary"
                      disabled={
                        !connected ||
                        notesBusy ||
                        recording ||
                        busy ||
                        notesMeetingLoading
                      }
                      onClick={() => setResumeAnalysis(run)}
                    >
                      {t.resumeAnalysis}
                    </button>
                  )}
                </div>
              ))}
            </div>
          </section>
        )}
        {page === "notes" && (
          <NotesView
            connectionSettings={textModelSettings}
            t={t}
            meeting={notesMeeting}
            meetingId={notesMeetingId}
            meetings={notesMeetingOptions}
            meetingLoading={notesMeetingLoading}
            selectMeeting={(id) => void selectNotesMeeting(id)}
            modelReady={notesModelReady}
            modelDiscoveryError={localTextModels.error}
            libraryInvalid={!templateStore.ready}
            libraryPending={templateStore.pending}
            libraryError={templateStore.error ?? templateStore.migrationError}
            browserInvalid={templateStore.browserInvalid}
            refreshTemplates={() => void templateStore.refresh()}
            importBrowser={
              templateStore.hasBrowserTemplates
                ? () => void templateStore.importBrowser()
                : null
            }
            templates={templates}
            notesBusy={notesBusy}
            textModel={textModel}
            endpoint={endpoint}
            template={template}
            resultId={resultId}
            analyses={analyses}
            setTemplateId={setTemplateId}
            setStepId={setStepId}
            setResultId={setResultId}
            openTemplateAction={openTemplateAction}
            connected={connected}
            recording={recording}
            busy={busy || notesMeetingLoading}
            beginNotes={beginNotes}
            showStart={Boolean(
              (notesJob.progress || notesJob.feedback) && !notesBusy,
            )}
            result={result}
            resultSection={resultSection}
            selectedStep={selectedStep}
            setEditing={(value) => {
              setInspectingStep(false);
              setEditing(value);
            }}
            inspectStep={(value) => {
              setInspectingStep(true);
              setEditing(value);
            }}
            setEditingNew={setEditingNew}
            moveStep={moveStep}
            updateTemplate={updateTemplate}
            copied={copied === "notes"}
            copyResults={copyNotesMarkdown}
            showEvidence={showEvidence}
            locale={locale}
          />
        )}
      </main>
      <div className="job-status-stack">
        {speakerChecking && (
          <div role="status" className="notice info">
            {locale === "de"
              ? "Sprecherunterstützung wird lokal geprüft. Bitte vor dem Start einer Aufnahme abschließen lassen."
              : "Checking speaker support locally. Please let this finish before starting a recording."}
          </div>
        )}
        {/* Settings already shows retention progress and the full update panel. */}
        <MaintenanceStatus
          api={api}
          locale={locale}
          hidden={page === "settings" && Boolean(api.getRetentionPolicy)}
        />
        <SpeakerExperiments api={api} locale={locale} footer />
        <UpdatePanel
          api={api}
          locale={locale}
          footer
          hidden={page === "settings"}
        />
        <LibraryStatus api={api} connected={connected} locale={locale} />
        <NotesProgressPanel
          progress={notesJob.progress}
          elapsedMs={notesJob.elapsedMs}
          cancelPending={notesJob.cancelPending}
          feedback={notesJob.feedback}
          pollError={notesJob.pollError}
          awaitingProgress={Boolean(app?.notesRunning) && !notesJob.progress}
          meetingTitle={(() => {
            const id =
              notesJob.progress?.meetingId ?? notesJob.feedback?.meetingId;
            const value = [meeting, liveMeeting, ...history].find(
              (item) => item?.id === id,
            );
            return value
              ? meetingLabel(value, locale)
              : [t.notesTask, id].filter(Boolean).join(" · ");
          })()}
          setup={
            page === "notes"
              ? {
                  loading: notesMeetingLoading,
                  message: notesMeetingLoading
                    ? t.busy
                    : !notesMeeting
                      ? t.noMeeting
                      : notesMeeting.status !== "complete" || recording
                        ? t.pendingNotes
                        : !notesModelReady || !textModel.trim()
                          ? t.analysisChooseModel
                          : !connected
                            ? t.disconnected
                            : !templateStore.ready || templateStore.pending
                              ? t.templateLoadFailed
                              : t.analysisReady,
                  startDisabled: !canStartNotes,
                  onStart: beginNotes,
                }
              : undefined
          }
          t={t}
          onCancel={() => void notesJob.cancel()}
          onOpen={() => {
            const id =
              notesJob.progress?.meetingId ?? notesJob.feedback?.meetingId;
            if (id) void openNotesFor(id);
          }}
          onDismiss={notesJob.dismiss}
          onResume={
            notesJob.progress?.runId &&
            ["failed", "cancelled"].includes(notesJob.progress.status) &&
            notesJob.lastRequest?.meetingId === notesMeetingId
              ? resumeNotes
              : null
          }
        />
      </div>
      {resumeAnalysis && (
        <ResumeAnalysisDialog
          key={resumeAnalysis.id}
          run={resumeAnalysis}
          api={api}
          t={t}
          blocked={
            !connected || notesBusy || recording || busy || notesMeetingLoading
          }
          onClose={() => setResumeAnalysis(null)}
          onResume={(request) => {
            setResumeAnalysis(null);
            void runNotes(request);
          }}
        />
      )}
      {editing && (
        <div
          className="modal-backdrop"
          onClick={() => {
            if (!templateStore.pending) setEditing(null);
          }}
        >
          <section
            role="dialog"
            aria-modal="true"
            aria-labelledby="step-title"
            className="dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="section-heading">
              <h2 id="step-title">
                {inspectingStep ? t.snapshotInstructions : t.editTab}
              </h2>
              <button
                className="icon-button"
                disabled={templateStore.pending}
                aria-label={t.close}
                onClick={() => setEditing(null)}
              >
                <X size={19} />
              </button>
            </div>
            <p>{t.instructionHelp}</p>
            <p className="muted">{t.templateLimit}</p>
            <label>
              {t.tabName}
              <input
                maxLength={100}
                value={editing.name}
                readOnly={inspectingStep || templateStore.pending}
                onChange={(event) =>
                  setEditing({ ...editing, name: event.target.value })
                }
              />
            </label>
            <label>
              {t.kind}
              <select
                value={editing.kind}
                disabled={inspectingStep || templateStore.pending}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    kind: event.target.value as AnalysisStep["kind"],
                  })
                }
              >
                {(["summary", "decisions", "tasks", "analysis"] as const).map(
                  (kind) => (
                    <option key={kind} value={kind}>
                      {t[kind]}
                    </option>
                  ),
                )}
              </select>
            </label>
            <p className="muted">{t.stepKindHelp}</p>
            <label>
              {t.instruction}
              <textarea
                rows={8}
                maxLength={8000}
                value={editing.prompt}
                readOnly={inspectingStep || templateStore.pending}
                onChange={(event) =>
                  setEditing({ ...editing, prompt: event.target.value })
                }
              />
            </label>
            <div className="dialog-actions">
              {templateStore.error && (
                <div>
                  <p className="inline-error" role="alert">
                    {t.templateSaveFailed}
                  </p>
                  <button
                    className="secondary"
                    disabled={templateStore.pending}
                    onClick={() => void templateStore.refresh()}
                  >
                    {t.refreshTemplates}
                  </button>
                </div>
              )}
              <button
                className="secondary"
                disabled={templateStore.pending}
                onClick={() => setEditing(null)}
              >
                {inspectingStep ? t.close : t.cancel}
              </button>
              {!inspectingStep && (
                <button
                  className="primary"
                  onClick={() => void saveStep()}
                  disabled={
                    templateStore.pending ||
                    !editing.name.trim() ||
                    !editing.prompt.trim()
                  }
                >
                  {templateStore.pending ? t.busy : t.save}
                </button>
              )}
            </div>
          </section>
        </div>
      )}
      {templateAction && (
        <TemplateDialog
          action={templateAction}
          pending={templateStore.pending}
          error={templateActionError}
          t={t}
          onChange={(name) => setTemplateAction({ ...templateAction, name })}
          onSave={() => void saveTemplateAction()}
          onClose={() => setTemplateAction(null)}
          onRefresh={() => void templateStore.refresh()}
        />
      )}
      {cloudRequest && (
        <div className="modal-backdrop">
          <section
            className="dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="cloud-title"
          >
            <h2 id="cloud-title">{t.cloudTitle}</h2>
            <p>{t.cloudBody}</p>
            <code className="storage-path">{cloudRequest.endpoint}</code>
            <p>
              {cloudRequest.model} · {meeting?.title}
            </p>
            <div className="dialog-actions">
              <button
                className="secondary"
                onClick={() => setCloudRequest(null)}
              >
                {t.cancel}
              </button>
              <button
                className="primary"
                onClick={() =>
                  void runNotes({ ...cloudRequest, cloudConsent: true })
                }
              >
                {t.approve}
              </button>
            </div>
          </section>
        </div>
      )}
      {erasureTarget && canEraseMeeting && (
        <EraseMeetingDialog
          key={erasureTarget.id}
          api={api}
          meeting={erasureTarget}
          locale={locale}
          onStart={() => beginMeetingErasure(erasureTarget.id)}
          onDeleted={(remnants) =>
            completeMeetingErasure(erasureTarget.id, remnants)
          }
          onStopped={() => stopMeetingErasure(erasureTarget.id)}
          onClose={() => setErasureTarget(null)}
        />
      )}
      {source && (
        <SourceDialog
          selection={source}
          locale={locale}
          t={t}
          onClose={() => setSource(null)}
        />
      )}
    </div>
  );
}
