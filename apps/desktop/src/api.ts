import { invoke, isTauri } from "@tauri-apps/api/core";
import type { DesktopApi } from "./types";
import { requestJobStatusRefresh } from "./jobPolling";
/**
 * Invokes a typed native command only inside Tauri; browser previews reject instead of
 * pretending a desktop operation succeeded.
 */
function desktopInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri())
    return Promise.reject(
      "Diese Ansicht benötigt die installierte Meeting for Handy-Desktop-App. / Open the installed Meeting for Handy desktop application.",
    );
  return invoke<T>(command, args);
}
/**
 * Submits a native job request and wakes status pollers immediately and on settlement. The
 * returned promise still carries the command's original success or failure.
 */
function jobInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const pending = desktopInvoke<T>(command, args);
  requestJobStatusRefresh();
  void pending.then(requestJobStatusRefresh, requestJobStatusRefresh);
  return pending;
}
export const desktopApi: DesktopApi = {
  previewMeetingErasure: (meetingId) =>
    jobInvoke("preview_meeting_erasure", { meetingId }),
  getMeetingErasurePreview: (jobId) =>
    desktopInvoke("get_meeting_erasure_preview", { jobId }),
  eraseMeetingNow: (previewId, meetingId, confirmation) =>
    jobInvoke("erase_meeting_now", { previewId, meetingId, confirmation }),
  listPendingErasures: () => desktopInvoke("list_pending_erasures"),

  listAudioCopies: (offset) => desktopInvoke("list_audio_copies", { offset }),
  transcribeAudioCopy: (runId, modelId, confirmed) =>
    jobInvoke("transcribe_audio_copy", { runId, modelId, confirmed }),
  compactArchive: (archiveId, confirmed) =>
    jobInvoke("compact_archive", { archiveId, confirmed }),
  listArchives: (offset, limit) =>
    desktopInvoke("list_archives", { offset, limit }),
  restoreArchive: (archiveId, confirmed) =>
    jobInvoke("restore_archive", { archiveId, confirmed }),
  listUnfinishedDeletions: () => desktopInvoke("list_unfinished_deletions"),
  resumePermanentDeletion: (logId, confirmation) =>
    jobInvoke("resume_permanent_deletion", { logId, confirmation }),
  applyRetention: (planId, candidateIndex, confirmation) =>
    jobInvoke("apply_retention", { planId, candidateIndex, confirmation }),
  getRetentionPolicy: () => desktopInvoke("get_retention_policy"),
  saveRetentionPolicy: (expectedRevision, policy) =>
    desktopInvoke("save_retention_policy", { expectedRevision, policy }),
  previewRetention: () => jobInvoke("preview_retention"),
  getRetentionPlan: (jobId) => desktopInvoke("get_retention_plan", { jobId }),
  getMaintenanceProgress: () => desktopInvoke("get_maintenance_progress"),
  cancelMaintenance: (jobId) => desktopInvoke("cancel_maintenance", { jobId }),
  startSpeakerExperiment: (meetingId, modelId, mode) =>
    jobInvoke("start_speaker_experiment", { meetingId, modelId, mode }),
  checkSpeakerModel: (modelId) => jobInvoke("check_speaker_model", { modelId }),
  archiveSpeakerModelChecks: () => jobInvoke("archive_speaker_model_checks"),
  getSpeakerProgress: () => desktopInvoke("get_speaker_progress"),
  cancelSpeakerExperiment: (jobId) =>
    desktopInvoke("cancel_speaker_experiment", { jobId }),
  getSpeakerSections: (meetingId) =>
    desktopInvoke("get_speaker_sections", { meetingId }),
  checkForUpdate: () => jobInvoke("check_for_update"),
  downloadUpdate: () => jobInvoke("download_update"),
  getUpdateProgress: () => desktopInvoke("get_update_progress"),
  cancelUpdate: () => desktopInvoke("cancel_update"),
  installUpdate: () => desktopInvoke("install_update"),
  openReleasePage: () => desktopInvoke("open_release_page"),
  getSpeakerLabels: (meetingId) =>
    desktopInvoke("get_speaker_labels", { meetingId }),
  saveSpeakerLabels: (meetingId, expectedRevision, enabled, assignments) =>
    desktopInvoke("save_speaker_labels", {
      meetingId,
      expectedRevision,
      enabled,
      assignments,
    }),
  editMeetingMetadata: (meetingId, expectedRevision, title, pinned) =>
    desktopInvoke("edit_meeting_metadata", {
      meetingId,
      expectedRevision,
      title,
      pinned,
    }),
  searchMeetings: (query, offset, limit) =>
    desktopInvoke("search_meetings", { query, offset, limit }),
  rebuildSearchIndex: () => jobInvoke("rebuild_search_index"),
  getLibraryProgress: () => desktopInvoke("get_library_progress"),
  cancelLibraryJob: (jobId) => desktopInvoke("cancel_library_job", { jobId }),
  getAppState: () => desktopInvoke("get_app_state"),
  getAudioLevels: () => desktopInvoke("get_audio_levels"),
  getSystemLoad: () => desktopInvoke("get_system_load"),
  reconnectMicrophone: (meetingId) =>
    desktopInvoke("reconnect_microphone", { meetingId }),
  setCapturePaused: (meetingId, paused) =>
    desktopInvoke("set_capture_paused", { meetingId, paused }),
  setMicrophoneMuted: (meetingId, muted) =>
    desktopInvoke("set_microphone_muted", { meetingId, muted }),
  chooseHandyExecutable: () => desktopInvoke("choose_handy_executable"),
  chooseFfmpegExecutable: () => desktopInvoke("choose_ffmpeg_executable"),
  chooseLegacyManifest: () => desktopInvoke("choose_legacy_manifest"),
  refreshHandy: (executable) => desktopInvoke("refresh_handy", { executable }),
  listDevices: () => desktopInvoke("list_devices"),
  startMeeting: (request) => desktopInvoke("start_meeting", { request }),
  stopMeeting: () => desktopInvoke("stop_meeting"),
  listMeetings: () => desktopInvoke("list_meetings"),
  getMeeting: (id) => desktopInvoke("get_meeting", { id }),
  resumeMeeting: (id) => desktopInvoke("resume_meeting", { id }),
  importLegacy: (manifest) => desktopInvoke("import_legacy", { manifest }),
  generateNotes: (request) => desktopInvoke("generate_notes", { request }),
  listAnalyses: (meetingId) => desktopInvoke("list_analyses", { meetingId }),
  listUnfinishedAnalyses: (meetingId) =>
    desktopInvoke("list_unfinished_analyses", { meetingId }),
  listTextModels: (endpoint) =>
    desktopInvoke("list_text_models", { endpoint, mode: "local" }),
  getNotesProgress: () => desktopInvoke("get_notes_progress"),
  cancelNotes: (jobId) => desktopInvoke("cancel_notes", { jobId }),
  startLiveNotes: (request) => jobInvoke("start_live_notes", { request }),
  stopLiveNotes: () => desktopInvoke("stop_live_notes"),
  refreshLiveNotes: () => jobInvoke("refresh_live_notes"),
  setLiveNotesInterval: (intervalMs) =>
    desktopInvoke("set_live_notes_interval", { intervalMs }),
  getLiveNotes: (meetingId) => desktopInvoke("get_live_notes", { meetingId }),
  localModelSetup: () => desktopInvoke("local_model_setup"),
  pullOllamaModel: (model) => jobInvoke("pull_ollama_model", { model }),
  getModelPullProgress: () => desktopInvoke("get_model_pull_progress"),
  cancelModelPull: () => desktopInvoke("cancel_model_pull"),
  notesMarkdown: (meetingId, analysisId) =>
    desktopInvoke("notes_markdown", { meetingId, analysisId }),
  copyText: (text) => desktopInvoke("copy_text", { text }),
  openMeetingFolder: (id) => desktopInvoke("open_meeting_folder", { id }),
  exportTranscript: (id, locale, format = "txt") =>
    desktopInvoke("export_transcript", { id, locale, format }),
  listTemplates: () => desktopInvoke("list_templates"),
  saveTemplates: (library) => desktopInvoke("save_templates", { library }),
  importTemplates: (templates) =>
    desktopInvoke("import_templates", { templates }),
  credentialStatus: (endpoint, mode) =>
    desktopInvoke("credential_status", { endpoint, mode }),
  saveCredential: (request) => desktopInvoke("save_credential", { request }),
  removeCredential: (endpoint, mode, confirm) =>
    desktopInvoke("remove_credential", { endpoint, mode, confirm }),
  agentDecideRecording: (requestId, approve, request) =>
    desktopInvoke("agent_decide_recording", { requestId, approve, request }),
  agentRevoke: () => desktopInvoke("agent_revoke"),
};
/**
 * Preserves a supplied Error message or string for the UI and uses a desktop-connection
 * instruction for unknown error shapes.
 */
export function errorMessage(error: unknown): string {
  return error instanceof Error
    ? error.message
    : typeof error === "string"
      ? error
      : "Die Desktop-Verbindung ist nicht verfügbar. Bitte die installierte Meeting for Handy-App öffnen. / Open the installed desktop application.";
}
