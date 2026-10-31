export type Locale = "de" | "en";
export type Theme = "light" | "dark" | "system";
export type Track = "system" | "microphone";
export interface HandyModel {
  id: string;
  name: string;
  isDownloaded: boolean;
  supportsStreaming: boolean;
  compatibilityError?: string;
}
export interface HandyStatus {
  executable: string | null;
  version: string | null;
  models: HandyModel[];
  error?: string | null;
}
export interface AudioDevice {
  id: string;
  name: string;
  source: Track;
  isDefault?: boolean;
}
/** Read-only levels of the effective capture signal; no audio or file access is exposed. */
export interface AudioLevel {
  status: "active" | "waiting" | "stale" | "paused" | "muted" | "unavailable";
  peakDbfs: number | null;
  rmsDbfs: number | null;
  clipped: boolean;
  ageMs: number | null;
}
/** Load of the whole computer, not only of this app. */
export interface SystemLoad {
  cpuPercent: number | null;
  memoryUsedBytes: number | null;
  memoryTotalBytes: number | null;
}
export interface AudioLevels {
  meetingId: string;
  system: AudioLevel;
  microphone: AudioLevel;
}
export interface AudioPart {
  source: Track;
  startSample: number;
  sampleCount: number;
  confirmedSampleCount?: number;
  text: string | null;
  status: string;
  committedText?: string;
  tentativeText?: string;
  file?: string;
  processedSamples?: number;
  transcriptSequence?: number;
}
export interface Meeting {
  id: string;
  title: string;
  startedAt: string;
  durationMs: number;
  status: string;
  modelId: string;
  parts: AudioPart[];
  warnings: string[];
  livePreviewLimited?: boolean;
  snapshotSequence?: number;
  readOnly?: boolean;
  pendingAudioMs?: number;
  savedAudioMs?: { system: number; microphone: number };
  userMetadata?: MeetingMetadata;
  metadataError?: string | null;
}
export interface MeetingMetadata {
  revision: number;
  title: string | null;
  pinned: boolean;
  audioRemoved: boolean;
}
export interface SearchHit {
  id: string;
  title: string;
  startedAt: string;
  durationMs: number;
  status: string;
  modelId: string;
  pinned: boolean;
  audioRemoved: boolean;
  metadataRevision: number;
  sourceId: string | null;
  startMs: number | null;
  endMs: number | null;
  excerpt: string;
}
export interface SearchPage {
  items: SearchHit[];
  total: number;
  offset: number;
  limit: number;
}
export interface LibraryProgress {
  jobId: string;
  kind: string;
  status: "running" | "complete" | "cancelled" | "failed";
  confirmedTasks: number;
  totalTasks: number;
  elapsedMs: number;
  skipped: number;
  error: string | null;
  cancelRequested: boolean;
}
export type MeetingSummary = Omit<Meeting, "parts" | "warnings">;
export interface AppState {
  microphoneReconnectAvailable?: boolean;
  version: string;
  handy: HandyStatus | null;
  activeMeeting: Meeting | null;
  recording: boolean;
  captureControl?: CaptureControl | null;
  transitioning?: boolean;
  error?: string | null;
  dataDirectory?: string;
  notesRunning?: boolean;
  agent?: AgentState | null;
}
/** Acknowledged native track controls; these describe this app's capture only. */
export interface CaptureControl {
  paused: boolean;
  microphoneMuted: boolean;
  hasMicrophone: boolean;
}
/** Complete original-transcript export formats accepted by the native saver. */
export type TranscriptFormat = "txt" | "md" | "json";
export interface AgentState {
  connected: boolean;
  label: string;
  ownerId: string;
  revoked: boolean;
  pending: {
    requestId: string;
    title: string;
    includeMicrophone: boolean;
    maxDurationSeconds: number;
    status: "pending" | "starting";
  } | null;
  ownedSessionId: string | null;
  expiresAtUnixMs: number | null;
  /** Actual native session ownership, independent of pending or expired consent. */
  activeCapture?: boolean;
  lastOutcome?: string | null;
}
export type AnalysisKind = "summary" | "decisions" | "tasks" | "analysis";
export interface AnalysisStep {
  id: string;
  name: string;
  kind: AnalysisKind;
  prompt: string;
}
export interface AnalysisTemplate {
  id: string;
  name: string;
  steps: AnalysisStep[];
}
export interface TemplateLibrary {
  revision: number;
  templates: AnalysisTemplate[];
}
export interface Evidence {
  sourceId: string;
  quote: string;
}
export interface NoteFact {
  text: string;
  owner: string | null;
  due: string | null;
  evidence: Evidence[];
}
export interface TranscriptSource {
  id: string;
  source: Track;
  startMs: number;
  endMs: number;
  text: string;
}
/** Why the backend discarded a model statement; discarded text is never stored. */
export type RejectionReason =
  | "invalidText"
  | "evidenceCount"
  | "quoteNotLiteral"
  | "ownerOrDueOutsideTasks"
  | "unverifiedOwnerOrDue"
  | "invalidNameCandidate"
  | "numberNotQuoted"
  | "nonbindingTask"
  | "itemLimit";
export interface AnalysisResult {
  id: string;
  meetingId: string;
  sourceHash: string;
  template: AnalysisTemplate;
  sections: {
    id: string;
    name: string;
    kind: AnalysisKind;
    items: NoteFact[];
    rejected?: Partial<Record<RejectionReason, number>>;
  }[];
  sources: TranscriptSource[];
  createdAt: string;
  promptVersion?: string;
  resultSchema?: number;
  /** False for a result of an earlier prompt version: shown read-only, never continued. */
  currentPrompt?: boolean;
}
/** A stored work state the backend left untouched and did not show. */
export interface SkippedAnalysis {
  id: string;
  reason: "damaged" | "newerFormat" | "otherMeeting";
}
export interface AnalysisListing {
  analyses: AnalysisResult[];
  skipped: SkippedAnalysis[];
}
export interface PendingListing {
  items: PendingAnalysis[];
  skipped: SkippedAnalysis[];
}
export interface StartRequest {
  title: string;
  modelId: string;
  systemDeviceId: string;
  microphoneDeviceId: string | null;
}
export interface PendingAnalysis {
  id: string;
  meetingId: string;
  sourceHash: string;
  template: AnalysisTemplate;
  createdAt: string;
  model: string;
  mode: "local" | "cloud";
  endpoint: string;
  confirmedTasks: number;
  totalTasks: number;
  promptVersion: string;
  /** Only runs of the current prompt version may be continued. */
  resumable: boolean;
}
export interface NotesRequest {
  jobId?: string;
  resumeRunId?: string;
  meetingId: string;
  template: AnalysisTemplate;
  endpoint: string;
  model: string;
  mode: "local" | "cloud";
  cloudConsent: boolean;
  apiKey: string;
  useSavedKey?: boolean;
}
export interface CredentialStatus {
  state: "missing" | "stored" | "unavailable";
  provider: string;
  message: string | null;
}
export type LiveTrigger =
  | "manual"
  | "final"
  | "volume"
  | "longContribution"
  | "interval";
export interface LiveEvidence {
  sourceId: string;
  quote: string;
  track: string;
  startMs: number;
  endMs: number;
}
export interface LiveItem {
  text: string;
  owner: string | null;
  due: string | null;
  window: number;
  tracks: string[];
  evidence: LiveEvidence[];
}
export interface LiveView {
  lastRequest?: {
    operation: "window" | "overview";
    window: number;
    request: {
      result:
        | "transport"
        | "http"
        | "envelope"
        | "incomplete"
        | "validation"
        | "persistence"
        | "stored"
        | "cancelled";
      httpStatus: number | null;
      finish: "stop" | "length" | "filtered" | "other" | null;
      elapsedMs: number;
      outputLimit: number;
    };
  } | null;
  /** Independent coverage and timestamp of the rolling overview; older backends omit them. */
  overviewPending?: boolean;
  overviewUpdatedAt?: string | null;
  runId: string | null;
  model: string;
  mode: string;
  endpoint: string;
  language: string;
  promptVersion: string;
  createdAt: string | null;
  updatedAt: string | null;
  windows: number;
  summarizedSections: number;
  overview: LiveItem[];
  overviewWindow: number;
  points: LiveItem[];
  decisions: LiveItem[];
  tasks: LiveItem[];
  questions: LiveItem[];
  rejected: number;
  damagedRuns: number;
}
export interface LiveNotesStatus {
  waitingForTranscript?: boolean;
  meetingId: string;
  status:
    | "off"
    | "starting"
    | "waiting"
    | "updating"
    | "finished"
    | "partial"
    | "stopped"
    | "failed";
  running: boolean;
  error: string | null;
  pendingChars: number;
  intervalMs: number;
  nextUpdateInMs: number | null;
  lastTrigger: LiveTrigger | null;
  view: LiveView | null;
}
export interface LiveNotesRequest {
  meetingId: string;
  endpoint: string;
  model: string;
  mode: "local" | "cloud";
  cloudConsent: boolean;
  apiKey: string;
  useSavedKey: boolean;
  intervalMs: number;
  language: Locale;
}
export interface LocalModelProvider {
  id: "ollama" | "lmstudio";
  name: string;
  endpoint: string;
  reachable: boolean;
  models: string[];
  recommendedInstalled: boolean;
}
export interface LocalModelSetupInfo {
  totalMemoryBytes: number | null;
  recommendation: {
    tier: "none" | "small" | "standard" | "large" | "unknown";
    model: string | null;
    lmStudioSearch: string | null;
  };
  providers: LocalModelProvider[];
}
export interface ModelPullProgress {
  model: string;
  status: "running" | "complete" | "failed" | "cancelled";
  detail: string;
  completed: number;
  total: number;
  error: string | null;
}
export interface NotesProgress {
  jobId: string;
  runId: string | null;
  meetingId: string;
  status: "running" | "complete" | "cancelled" | "failed";
  stage: "preparing" | "requesting" | "validating" | "saving" | "finished";
  stepName: string | null;
  stepIndex: number;
  totalSteps: number;
  batchIndex: number;
  totalBatches: number;
  confirmedTasks: number;
  totalTasks: number;
  elapsedMs: number;
  error: string | null;
  cancelRequested: boolean;
}
/**
 * Typed renderer/native boundary. Optional methods describe available capabilities;
 * native commands still validate IDs, file grants and explicit confirmations.
 */
export interface ErasurePreview {
  meetingId: string;
  fingerprint: string;
  files: number;
  bytes: number;
  rebuildSearch: boolean;
  /** Unassignable interrupted metadata stays intact and is disclosed before confirmation. */
  preservedAtomicRemnants?: number;
}
export interface PendingErasure {
  meetingId: string;
  confirmedAt: string;
}
export interface DesktopApi {
  previewMeetingErasure?(meetingId: string): Promise<string>;
  getMeetingErasurePreview?(jobId: string): Promise<ErasurePreview>;
  eraseMeetingNow?(
    previewId: string,
    meetingId: string,
    confirmation: string,
  ): Promise<string>;
  listPendingErasures?(): Promise<PendingErasure[]>;

  listAudioCopies?(offset: number): Promise<AudioCopyPage>;
  transcribeAudioCopy?(
    runId: string,
    modelId: string,
    confirmed: boolean,
  ): Promise<string>;
  compactArchive?(archiveId: string, confirmed: boolean): Promise<string>;
  listArchives?(offset: number, limit: number): Promise<ArchivePage>;
  restoreArchive?(archiveId: string, confirmed: boolean): Promise<string>;
  listUnfinishedDeletions?(): Promise<UnfinishedDeletion[]>;
  resumePermanentDeletion?(
    logId: string,
    confirmation: string,
  ): Promise<string>;
  applyRetention?(
    planId: string,
    candidateIndex: number,
    confirmation: string,
  ): Promise<string>;
  getRetentionPolicy?(): Promise<RetentionPolicy>;
  saveRetentionPolicy?(
    expectedRevision: number,
    policy: RetentionPolicy,
  ): Promise<RetentionPolicy>;
  previewRetention?(): Promise<string>;
  getRetentionPlan?(jobId: string): Promise<RetentionPlan>;
  getMaintenanceProgress?(): Promise<LibraryProgress | null>;
  cancelMaintenance?(jobId: string): Promise<boolean>;
  startSpeakerExperiment?(
    meetingId: string,
    modelId: string,
    mode: "live" | "post-meeting",
  ): Promise<string>;
  getSpeakerProgress?(): Promise<SpeakerProgress | null>;
  cancelSpeakerExperiment?(jobId: string): Promise<boolean>;
  getSpeakerSections?(meetingId: string): Promise<SavedSpeakerSections | null>;
  checkForUpdate?(): Promise<UpdateProgress>;
  downloadUpdate?(): Promise<UpdateProgress>;
  getUpdateProgress?(): Promise<UpdateProgress>;
  cancelUpdate?(): Promise<void>;
  installUpdate?(): Promise<void>;
  openReleasePage?(): Promise<void>;
  getSpeakerLabels?(
    meetingId: string,
  ): Promise<SpeakerDocument & { sources: Record<string, string> }>;
  saveSpeakerLabels?(
    meetingId: string,
    expectedRevision: number,
    enabled: boolean,
    assignments: SpeakerAssignment[],
  ): Promise<SpeakerDocument>;
  editMeetingMetadata?(
    meetingId: string,
    expectedRevision: number,
    title: string | null,
    pinned: boolean,
  ): Promise<MeetingMetadata>;
  searchMeetings?(
    query: string,
    offset: number,
    limit: number,
  ): Promise<SearchPage>;
  rebuildSearchIndex?(): Promise<string>;
  getLibraryProgress?(): Promise<LibraryProgress | null>;
  cancelLibraryJob?(jobId: string): Promise<boolean>;
  getAppState(): Promise<AppState>;
  /** Lightweight, read-only capture snapshot; null when no capture is active. */
  getAudioLevels?(): Promise<AudioLevels | null>;
  /** Whole-computer processor and memory load; values that cannot be read are null. */
  getSystemLoad?(): Promise<SystemLoad>;
  /** Binds a pause request to the current meeting; resolves with native acknowledgement. */
  setCapturePaused?(meetingId: string, paused: boolean): Promise<AppState>;
  /** Mutes only this app's microphone track for the bound meeting, retaining system audio. */
  setMicrophoneMuted?(meetingId: string, muted: boolean): Promise<AppState>;
  reconnectMicrophone?(meetingId: string): Promise<AppState>;
  /** Opens native selection/confirmation; cancellation returns null and grants no new path. */
  chooseHandyExecutable?(): Promise<string | null>;
  /** Returns an approved canonical FFmpeg path; its native grant lasts for this app session. */
  chooseFfmpegExecutable?(): Promise<string | null>;
  /** Selects an original session manifest through the native dialog for read-only import. */
  chooseLegacyManifest?(): Promise<string | null>;
  /** Null delegates cached-origin resolution to the backend; an explicit path needs native approval. */
  refreshHandy(executable: string | null): Promise<HandyStatus>;
  listDevices(): Promise<AudioDevice[]>;
  startMeeting(request: StartRequest): Promise<Meeting>;
  stopMeeting(): Promise<Meeting>;
  listMeetings(): Promise<MeetingSummary[]>;
  getMeeting(id: string): Promise<Meeting>;
  resumeMeeting(id: string): Promise<Meeting>;
  importLegacy(manifest: string): Promise<Meeting>;
  generateNotes(request: NotesRequest): Promise<AnalysisResult>;
  listAnalyses(meetingId: string): Promise<AnalysisListing>;
  listUnfinishedAnalyses(meetingId: string): Promise<PendingListing>;
  listTextModels(endpoint: string): Promise<string[]>;
  getNotesProgress(): Promise<NotesProgress | null>;
  cancelNotes(jobId: string): Promise<boolean>;
  startLiveNotes?(request: LiveNotesRequest): Promise<LiveNotesStatus>;
  checkSpeakerModel?(modelId: string): Promise<boolean>;
  /** Explicitly moves generated check proofs into a retained archive, without deletion. */
  archiveSpeakerModelChecks?(): Promise<boolean>;
  stopLiveNotes?(): Promise<boolean>;
  refreshLiveNotes?(): Promise<boolean>;
  setLiveNotesInterval?(intervalMs: number): Promise<boolean>;
  getLiveNotes?(meetingId: string): Promise<LiveNotesStatus>;
  localModelSetup?(): Promise<LocalModelSetupInfo>;
  pullOllamaModel?(model: string): Promise<ModelPullProgress>;
  getModelPullProgress?(): Promise<ModelPullProgress | null>;
  cancelModelPull?(): Promise<boolean>;
  notesMarkdown(meetingId: string, analysisId: string): Promise<string>;
  copyText(text: string): Promise<void>;
  openMeetingFolder(id: string): Promise<void>;
  /**
   * Saves every original section for the bound meeting and returns the saved path.
   * Omitting the format preserves the native TXT default.
   */
  exportTranscript(
    id: string,
    locale: Locale,
    format?: TranscriptFormat,
  ): Promise<string>;
  listTemplates(): Promise<TemplateLibrary>;
  saveTemplates(library: TemplateLibrary): Promise<TemplateLibrary>;
  importTemplates(templates: AnalysisTemplate[]): Promise<TemplateLibrary>;
  credentialStatus(
    endpoint: string,
    mode: "local" | "cloud",
  ): Promise<CredentialStatus>;
  saveCredential(request: {
    endpoint: string;
    mode: "local" | "cloud";
    apiKey: string;
    replaceExisting: boolean;
  }): Promise<CredentialStatus>;
  removeCredential(
    endpoint: string,
    mode: "local" | "cloud",
    confirm: boolean,
  ): Promise<CredentialStatus>;
  agentDecideRecording(
    requestId: string,
    approve: boolean,
    request?: StartRequest,
  ): Promise<void>;
  agentRevoke(): Promise<void>;
}
export interface SpeakerAssignment {
  sourceId: string;
  textSha256: string;
  name: string;
  evidence: string;
}
export interface RetentionPolicy {
  revision: number;
  untouchedDays: number;
  audioArchiveAfterDays: number | null;
  archiveAfterDays: number | null;
  convertAfterDays: number | null;
  maxVisibleMeetings: number | null;
  deleteArchivesAfterDays: number | null;
  codec: "flac" | "opus";
  ffmpegExecutable: string | null;
}
export interface RetentionCandidate {
  meetingId: string;
  title: string;
  action:
    | "archive-audio"
    | "archive-meeting"
    | "convert-audio"
    | "delete-archived-meeting";
  archiveId: string | null;
  metadataRevision: number;
  originalManifestSha256: string;
  ageDays: number;
  reason: string;
}
export interface RetentionPlan {
  id: string;
  policyRevision: number;
  createdAt: string;
  scanned: number;
  protected: number;
  unreadable: number;
  candidates: RetentionCandidate[];
  limited: boolean;
}
export interface SpeakerTurn {
  startMs: number;
  endMs: number;
  speakerId: number;
  confidence: number | null;
}
export interface SpeakerSection {
  sourceId: string;
  startMs: number;
  durationMs: number;
  audioSha256: string;
  turns: SpeakerTurn[];
}
export interface SpeakerProgress {
  jobId: string;
  meetingId: string;
  mode: string;
  status: string;
  stage: string;
  modelId: string;
  confirmedTasks: number;
  totalTasks: number;
  elapsedMs: number;
  error: string | null;
  cancelRequested: boolean;
  lastSection: SpeakerSection | null;
}
export interface SavedSpeakerSections {
  runId: string;
  mode: string;
  modelId: string;
  sections: SpeakerSection[];
}
export interface UpdateProgress {
  status: string;
  version: string | null;
  releaseUrl: string | null;
  receivedBytes: number;
  totalBytes: number;
  elapsedMs: number;
  error: string | null;
  attempt?: number;
  /** Operating system of this build: "windows", "linux", "macos", ... */
  platform?: string;
  /** Official release package for this platform, when the release lists one. */
  assetName?: string | null;
  /** Only the verified Windows setup can be downloaded inside the app. */
  canDownload?: boolean;
}
export interface SpeakerDocument {
  revision: number;
  enabled: boolean;
  assignments: SpeakerAssignment[];
}

export interface ArchiveReceipt {
  archiveId: string;
  meetingId: string;
  kind: string;
  archiveBytes: number;
  originalBytes: number;
  createdAt: string;
}
export interface ArchiveEntry {
  archiveId: string;
  /** Null when the receipt could not be read; its files are still preserved. */
  receipt: ArchiveReceipt | null;
  title: string;
  sessionPresent: boolean;
  sourceCopyRetained: boolean;
  /** Which record could not be read. Such entries offer no restore or compaction. */
  unreadable: "receipt" | "labels" | null;
}
export interface UnfinishedDeletion {
  logId: string;
  meetingId: string;
  deletedFiles: number;
  totalFiles: number;
  confirmedAt: string;
}
export interface ArchivePage {
  items: ArchiveEntry[];
  more: boolean;
}

export interface AudioCopy {
  runId: string;
  meetingId: string;
  title: string;
  codec: "flac" | "opus";
  createdAt: string;
  sections: number;
}
export interface AudioCopyPage {
  items: AudioCopy[];
  more: boolean;
}
