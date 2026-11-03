// Explicit renderer/native DTOs. Private notes and preparation never enter this object.
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
export function loadLiveState(raw) {
  if (!raw) return { state: initialLiveState(), blocked: false };
  try {
    const state = JSON.parse(raw);
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
        (t) => typeof t.id === "string" && typeof t.text === "string",
      )
    )
      throw Error("format");
    return { state: { ...initialLiveState(), ...state }, blocked: false };
  } catch {
    return { state: initialLiveState(), blocked: true };
  }
}
export function sharedWorkspaceContext(state, revision = 0, overrides = []) {
  const protocol = Array.from(state.notes.protocol);
  const sharedNotes = [];
  for (let i = 0; i < protocol.length; i += 1600) {
    const text = protocol.slice(i, i + 1600).join("");
    if (text.trim())
      sharedNotes.push({
        id: `h_protocol_${i / 1600}`,
        revision: state.protocolRevision || 1,
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
export function workspaceStatusText(status) {
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
export function stableVisibleItems(workspace, kind, query = "") {
  const needle = query.trim().toLocaleLowerCase();
  return (workspace?.entities || [])
    .filter((e) => !(workspace?.contextExcludedIds || []).includes(e.id))
    .filter(
      (e) =>
        (!kind || e.kind === kind) &&
        (!needle ||
          `${e.text} ${e.status}`.toLocaleLowerCase().includes(needle)),
    );
}

// Native shared inputs are authoritative across restarts. Preserve any divergent
// local draft as a note-history snapshot instead of silently erasing either copy.
export function restoreSharedContext(local, native, runId) {
  if (
    !native ||
    (local.nativeContextRunId === runId &&
      local.nativeContextRevision === native.revision)
  )
    return local;
  const protocol = native.sharedNotes.map((n) => n.text).join("");
  const divergent = local.notes.protocol && local.notes.protocol !== protocol;
  return {
    ...local,
    nativeContextRevision: native.revision,
    nativeContextRunId: runId,
    topics: [
      ...native.agenda.map((a) => ({ ...a, shared: true })),
      ...local.topics.filter(
        (t) => !t.shared && !native.agenda.some((a) => a.id === t.id),
      ),
    ],
    notes: { ...local.notes, protocol },
    protocolRevision: Math.max(1, ...native.sharedNotes.map((n) => n.revision)),
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
