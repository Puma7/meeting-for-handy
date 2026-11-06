// Explicit renderer/native DTOs. Private notes and preparation never enter this object.
import { formatMeetingDates, formatMeetingDue } from "./meetingDisplay.mjs";
/** Create an empty renderer draft; private fields remain separate from shared provider context. */
export function initialLiveState() {
  return {
    version: 1,
    topics: [],
    notes: { private: "", protocol: "" },
    preparation: { goal: "", agenda: "" },
    attachments: [],
    history: [],
    impulseStates: {},
    protocolRevision: 1,
  };
}
/** Validate cached drafts and block overwrites when an existing cache is unreadable. */
export function loadLiveState(raw) {
  if (!raw) return { state: initialLiveState(), blocked: false };
  try {
    const state = JSON.parse(raw);
    if (
      state.protocolShare !== undefined &&
      (typeof state.protocolShare?.text !== "string" ||
        !Number.isSafeInteger(state.protocolShare?.revision) ||
        state.protocolShare.revision < 1 ||
        typeof state.protocolShare?.pending !== "boolean")
    )
      throw Error("protocol share format");
    if (
      state.version !== 1 ||
      !Array.isArray(state.topics) ||
      !Array.isArray(state.history) ||
      !Array.isArray(state.attachments) ||
      !state.notes ||
      typeof state.notes.private !== "string" ||
      typeof state.notes.protocol !== "string" ||
      typeof state.preparation?.goal !== "string" ||
      typeof state.preparation?.agenda !== "string" ||
      !state.topics.every(
        (t) =>
          typeof t.id === "string" &&
          typeof t.text === "string" &&
          (t.pending === undefined || typeof t.pending === "boolean"),
      )
    )
      throw Error("format");
    return { state: { ...initialLiveState(), ...state }, blocked: false };
  } catch {
    return { state: initialLiveState(), blocked: true };
  }
}
/** Project only explicitly shared topics, protocol notes and manual overrides into native DTOs. */
export function sharedWorkspaceContext(state, revision = 0, overrides = []) {
  const protocol = Array.from(
    state.protocolShare?.text ?? state.notes.protocol,
  );
  const sharedNotes = [];
  for (let i = 0; i < protocol.length; i += 1600) {
    const text = protocol.slice(i, i + 1600).join("");
    if (text.trim())
      sharedNotes.push({
        id: `h_protocol_${i / 1600}`,
        revision: state.protocolShare?.revision ?? state.protocolRevision ?? 1,
        text,
      });
  }
  return {
    revision,
    agenda: state.topics
      .filter((t) => t.shared)
      .map((t) => ({ id: t.id, revision: t.revision || 1, text: t.text })),
    sharedNotes,
    overrides,
  };
}
export const SHARED_CONTEXT_LIMIT = 8000;
export const CONTEXT_BUDGET_MESSAGE =
  "Freigegebene Vorbereitung und Notizen sind zu groß. Bitte Text kürzen oder weniger Themen freigeben; private Entwürfe bleiben erhalten.";
/** Mirror native tuple serialization and Unicode-scalar accounting, including IDs, revisions and JSON escaping. */
export function sharedContextSize(context) {
  return Array.from(JSON.stringify([context.agenda, context.sharedNotes]))
    .length;
}
/** Describe current inference progress in the chosen UI language without implying recording failure. */
export function workspaceStatusText(status, language = "de") {
  if (language === "en") {
    if (!status) return "Local analysis is off";
    if (status.status === "updating") {
      const seconds = Math.floor((status.updateElapsedMs || 0) / 1000);
      return (
        "Analysis running · " +
        Math.floor(seconds / 60) +
        ":" +
        String(seconds % 60).padStart(2, "0") +
        (status.updateElapsedMs >= status.intervalMs
          ? " · new text follows afterwards"
          : "")
      );
    }
    if (status.waitingForTranscript)
      return "Waiting for transcription capacity";
    if (status.refreshRequested) return "Refresh queued";
    if (status.running) return "Collecting new text";
    return (
      {
        finished: "Analysis saved",
        partial: "Partial analysis saved",
        failed: "Check analysis",
        stopped: "Analysis stopped",
      }[status.status] || "Local analysis is off"
    );
  }
  if (!status) return "Lokale Auswertung ist aus";
  if (status.status === "updating") {
    const seconds = Math.floor((status.updateElapsedMs || 0) / 1000);
    return `Auswertung läuft · ${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}${status.updateElapsedMs >= status.intervalMs ? " · weiterer Text folgt danach" : ""}`;
  }
  if (status.waitingForTranscript)
    return "Wartet auf freie Kapazität für die Erkennung";
  if (status.refreshRequested) return "Aktualisierung vorgemerkt";
  if (status.running) return "Neue Texte werden gesammelt";
  return (
    {
      finished: "Auswertung gespeichert",
      partial: "Teilstand gespeichert",
      failed: "Auswertung prüfen",
      stopped: "Auswertung beendet",
    }[status.status] || "Lokale Auswertung ist aus"
  );
}
/** Filter active entities by kind and search while preserving native stable identities. */
export function stableVisibleItems(
  workspace,
  kind,
  query = "",
  language = "de",
) {
  const needle = query.trim().toLocaleLowerCase();
  return (workspace?.entities || [])
    .filter((e) => !(workspace?.contextExcludedIds || []).includes(e.id))
    .filter(
      (e) =>
        (!kind || e.kind === kind) &&
        (!needle ||
          `${e.text} ${formatMeetingDates(e.text, language)} ${e.owner || ""} ${e.due || ""} ${formatMeetingDue(e.due, language) || ""} ${e.status}`
            .toLocaleLowerCase()
            .includes(needle)),
    );
}

// Native shared inputs are authoritative across restarts. Preserve any divergent
// local draft as a note-history snapshot instead of silently erasing either copy.
/** Restore authoritative native shared inputs while retaining a divergent local protocol in history. */
export function restoreSharedContext(local, native, runId) {
  if (
    !native ||
    (local.nativeContextRunId === runId &&
      local.nativeContextRevision === native.revision)
  )
    return local;
  const protocol = native.sharedNotes.map((n) => n.text).join("");
  const pending = local.protocolShare?.pending === true;
  const editing =
    local.protocolShare && local.notes.protocol !== local.protocolShare.text;
  const keepLocal = pending || editing;
  const divergent =
    !keepLocal && local.notes.protocol && local.notes.protocol !== protocol;
  const nativeRevision = Math.max(
    1,
    ...native.sharedNotes.map((n) => n.revision),
  );
  return {
    ...local,
    nativeContextRevision: native.revision,
    nativeContextRunId: runId,
    topics: [
      ...native.agenda.map((a) => {
        const pending = local.topics.find(
          (topic) => topic.id === a.id && topic.pending,
        );
        return pending ?? { ...a, shared: true };
      }),
      ...local.topics
        .filter(
          (topic) => !native.agenda.some((entry) => entry.id === topic.id),
        )
        .map((topic) => (topic.pending ? topic : { ...topic, shared: false })),
    ],
    notes: {
      ...local.notes,
      protocol: keepLocal ? local.notes.protocol : protocol,
    },
    protocolRevision: keepLocal ? local.protocolRevision : nativeRevision,
    protocolShare: pending
      ? local.protocolShare
      : { text: protocol, revision: nativeRevision, pending: false },
    history: divergent
      ? [
          ...local.history,
          {
            id: `restored-${Date.now()}`,
            at: new Date().toISOString(),
            label: "Lokaler Entwurf vor Wiederaufnahme",
            noteKind: "protocol",
            content: local.notes.protocol,
          },
        ]
      : local.history,
  };
}

/** Clear only topic intentions actually represented by the accepted native agenda; preserve edits made while the request was in flight. */
export function acknowledgeSharedTopics(state, agenda) {
  return {
    ...state,
    topics: state.topics.map((topic) => {
      const accepted = agenda.find((entry) => entry.id === topic.id);
      const matches = topic.shared
        ? accepted &&
          accepted.text === topic.text &&
          accepted.revision === (topic.revision || 1)
        : !accepted;
      return topic.pending && matches ? { ...topic, pending: false } : topic;
    }),
  };
}

/** Compare released context fields so unchanged polling does not create redundant revisions. */
export function sameSharedContext(a, b) {
  const canonical = (value) =>
    Array.isArray(value)
      ? value.map(canonical)
      : value && typeof value === "object"
        ? Object.fromEntries(
            Object.keys(value)
              .sort()
              .map((key) => [key, canonical(value[key])]),
          )
        : value;
  return JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
}
