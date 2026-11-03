export const duration = 43.902;
export const transcript = [
  {
    id: "T01",
    start: 2,
    end: 6.004,
    speaker: "Mira",
    text: "Der Pilot startet am 12. November.",
  },
  {
    id: "T02",
    start: 9.004,
    end: 12.913,
    speaker: "Jonas",
    text: "Vorher brauchen wir die Freigabe für den Entwurf.",
  },
  {
    id: "T03",
    start: 15.913,
    end: 18.967,
    speaker: "Mira",
    text: "Wer gibt den Entwurf frei?",
  },
  {
    id: "T04",
    start: 21.967,
    end: 25.855,
    speaker: "Jana",
    text: "Danke für eure Unterstützung bei meiner Einarbeitung.",
  },
  {
    id: "T05",
    start: 28.855,
    end: 31.944,
    speaker: "Jonas",
    text: "Die Rückmeldung vom Kunden fehlt noch.",
  },
  {
    id: "T06",
    start: 34.944,
    end: 37.788,
    speaker: "Mira",
    text: "Ich gebe den Entwurf morgen frei.",
  },
  {
    id: "T07",
    start: 40.788,
    end: 43.902,
    speaker: "Jonas",
    text: "Zum Budget habe ich noch keine Zusage.",
  },
];

export const impulses = [
  {
    id: "reply",
    text: "Wann kommt die Kundenrückmeldung?",
    source: "T05",
    category: "Fachlich",
  },
  {
    id: "budget",
    text: "Welcher Budgetrahmen ist möglich?",
    source: "T07",
    category: "Fachlich",
  },
  {
    id: "approval",
    text: "Wie bestätigen wir die Freigabe?",
    source: "T06",
    category: "Fachlich",
  },
  {
    id: "pilot",
    text: "Was brauchen wir noch für den Pilotstart?",
    source: "T01",
    category: "Fachlich",
  },
  {
    id: "jana",
    text: "Jana, wie läuft die Einarbeitung inzwischen?",
    source: "T04",
    category: "Persönlich",
  },
  {
    id: "reply-owner",
    text: "Wer fragt beim Kunden nach?",
    source: "T05",
    category: "Fachlich",
  },
  {
    id: "reply-channel",
    text: "Wie holen wir die Rückmeldung ein?",
    source: "T05",
    category: "Fachlich",
  },
  {
    id: "reply-next",
    text: "Was können wir bis zur Rückmeldung vorbereiten?",
    source: "T05",
    category: "Fachlich",
  },
  {
    id: "budget-owner",
    text: "Wer kann den Budgetrahmen klären?",
    source: "T07",
    category: "Fachlich",
  },
  {
    id: "budget-timing",
    text: "Wann lässt sich das Budget besprechen?",
    source: "T07",
    category: "Fachlich",
  },
  {
    id: "budget-options",
    text: "Welche Optionen möchten wir zum Budget sammeln?",
    source: "T07",
    category: "Fachlich",
  },
  {
    id: "approval-channel",
    text: "Wo halten wir die Freigabe fest?",
    source: "T06",
    category: "Fachlich",
  },
  {
    id: "approval-followup",
    text: "Wann fragen wir zur zugesagten Freigabe nach?",
    source: "T06",
    category: "Fachlich",
  },
  {
    id: "approval-scope",
    text: "Was umfasst die Freigabe des Entwurfs?",
    source: "T02",
    category: "Fachlich",
  },
  {
    id: "pilot-ready",
    text: "Woran erkennen wir die Bereitschaft für den Pilotstart?",
    source: "T01",
    category: "Fachlich",
  },
  {
    id: "pilot-plan",
    text: "Welche Vorbereitung fehlt für den 12. November?",
    source: "T01",
    category: "Fachlich",
  },
  {
    id: "pilot-contact",
    text: "Wer begleitet den Pilotstart?",
    source: "T01",
    category: "Fachlich",
  },
  {
    id: "draft-review",
    text: "Wer sollte den Entwurf vor der Freigabe prüfen?",
    source: "T02",
    category: "Fachlich",
  },
  {
    id: "draft-questions",
    text: "Welche Fragen zum Entwurf sind noch offen?",
    source: "T03",
    category: "Fachlich",
  },
  {
    id: "jana-support",
    text: "Welche Unterstützung wäre für die Einarbeitung hilfreich?",
    source: "T04",
    category: "Persönlich",
  },
];

export function initialState() {
  return {
    version: 1,
    notes: { private: "", protocol: "" },
    preparation: { goal: "", agenda: "" },
    topics: [
      {
        id: "approval",
        text: "Freigabe klären",
        matchKey: "approval",
        shared: true,
        discussed: true,
        method: "demo",
        source: "T06",
        reason:
          "Mira nennt eine Zusage und einen Zeitpunkt. Die Freigabe selbst steht noch aus.",
      },
      {
        id: "reply",
        text: "Kundenrückmeldung erfragen",
        shared: true,
        discussed: false,
      },
      {
        id: "budget",
        text: "Budgetspielraum klären",
        shared: true,
        discussed: false,
      },
    ],
    attachments: [{ id: "agenda", name: "Pilot-Agenda.pdf", example: true }],
    impulseStates: {},
    taskDone: false,
    chat: [],
    history: [],
  };
}

export function formatTime(seconds) {
  const value = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  return `${String(Math.floor(value / 60)).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
}

export function snapshotAt(time) {
  const available = transcript.filter((row) => row.end <= time + 0.001);
  const has = (id) => available.some((row) => row.id === id);
  const facts = [];
  if (has("T01"))
    facts.push({
      key: "pilot",
      text: "Pilotstart am 12. November",
      source: "T01",
      kind: "info",
    });
  if (has("T06"))
    facts.push({
      key: "approval",
      text: "Freigabe zugesagt · noch offen",
      source: "T06",
      kind: "open",
    });
  else if (has("T02"))
    facts.push({
      key: "approval",
      text: "Freigabe wird benötigt",
      source: "T02",
      kind: "open",
    });
  if (has("T05"))
    facts.push({
      key: "reply",
      text: "Kundenrückmeldung steht aus",
      source: "T05",
      kind: "open",
    });
  if (has("T07"))
    facts.push({
      key: "budget",
      text: "Budget braucht eine Rückfrage",
      source: "T07",
      kind: "question",
    });
  return { available, facts, has };
}

export function filterTranscript(
  rows,
  {
    query = "",
    speaker = "Alle",
    period = "all",
    time = duration,
    ascending = true,
    from = 0,
    to = duration,
  } = {},
) {
  const cutoff =
    period === "30"
      ? time - 30
      : period === "5"
        ? time - 300
        : period === "10"
          ? time - 600
          : 0;
  const needle = query.trim().toLocaleLowerCase("de");
  return rows
    .filter(
      (row) =>
        row.end >= cutoff &&
        row.end <= time + 0.001 &&
        row.end >= from &&
        row.start <= to &&
        (speaker === "Alle" || row.speaker === speaker) &&
        (!needle ||
          `${row.speaker} ${row.id} ${row.text}`
            .toLocaleLowerCase("de")
            .includes(needle)),
    )
    .sort((a, b) => (ascending ? a.start - b.start : b.start - a.start));
}

export function decodeSaved(raw) {
  if (!raw) return { state: initialState(), blocked: false };
  try {
    const parsed = JSON.parse(raw);
    if (
      parsed.version !== 1 ||
      !Array.isArray(parsed.topics) ||
      !Array.isArray(parsed.history) ||
      !Array.isArray(parsed.attachments) ||
      (parsed.chat !== undefined && !Array.isArray(parsed.chat)) ||
      typeof parsed.notes?.private !== "string" ||
      typeof parsed.notes?.protocol !== "string" ||
      (parsed.preparation !== undefined &&
        (typeof parsed.preparation?.goal !== "string" ||
          typeof parsed.preparation?.agenda !== "string")) ||
      !parsed.topics.every(
        (t) => t && typeof t.text === "string" && typeof t.id === "string",
      )
    )
      throw new Error("format");
    return { state: { ...initialState(), ...parsed }, blocked: false };
  } catch {
    // Preserve unreadable browser bytes; never autosave over them.
    return { state: initialState(), blocked: true };
  }
}

export function simulateMatch(topics, time = duration) {
  const evidence = transcript.find(
    (row) => row.id === "T06" && row.end <= time + 0.001,
  );
  return topics.map((topic) =>
    topic.shared &&
    topic.matchKey === "approval" &&
    topic.text === "Freigabe klären" &&
    evidence
      ? {
          ...topic,
          discussed: true,
          method: "demo",
          source: evidence.id,
          reason:
            "Mira nennt eine Zusage und einen Zeitpunkt. Die Freigabe selbst steht noch aus.",
        }
      : topic,
  );
}

export function topicsAt(topics, time) {
  return topics.map((topic) =>
    topic.source && !snapshotAt(time).has(topic.source)
      ? {
          ...topic,
          discussed: false,
          source: undefined,
          method: undefined,
          reason: undefined,
        }
      : topic,
  );
}

export function demoAnswer(question, time = duration) {
  const text = question.toLocaleLowerCase("de");
  if (/privat|notiz/.test(text))
    return {
      text: "Private Notizen sind nicht Teil dieses Beispielchats.",
      sources: [],
    };
  const snap = snapshotAt(time);
  let ids = [];
  if (/freig|entwurf/.test(text)) ids = [snap.has("T06") ? "T06" : "T02"];
  else if (/kund|rückmeld/.test(text)) ids = ["T05"];
  else if (/budget|geld/.test(text)) ids = ["T07"];
  else if (/pilot|start|termin/.test(text)) ids = ["T01"];
  else if (/offen|stand|überblick/.test(text))
    ids = snap.facts
      .filter((fact) => fact.kind !== "info")
      .map((fact) => fact.source);
  const sources = transcript.filter(
    (row) => ids.includes(row.id) && snap.has(row.id),
  );
  return {
    text: sources.length
      ? "Diese Aussagen sind im gewählten Gesprächsstand belegt:"
      : "Dazu finde ich in den verfügbaren Beispielaussagen keinen Beleg.",
    sources,
  };
}
