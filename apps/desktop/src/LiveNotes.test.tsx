import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { StrictMode } from "react";
import App from "./App";
import { testServices } from "./test-services";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import LiveNotesPanel, {
  endpointAllowed,
  liveNotesText,
  type LiveConnection,
} from "./views/LiveNotesPanel";
import type {
  AppState,
  DesktopApi,
  Meeting,
  LiveItem,
  LiveNotesStatus,
  LiveView,
  LocalModelSetupInfo,
} from "./types";

vi.setConfig({ testTimeout: 15_000 });
// Synthetic IPC only: no text model, network, microphone or application storage is used.

function item(text: string, extra: Partial<LiveItem> = {}): LiveItem {
  return {
    text,
    owner: null,
    due: null,
    window: 1,
    tracks: ["system"],
    evidence: [
      {
        sourceId: "C60",
        quote: `Zitat zu ${text}`,
        track: "system",
        startMs: 60_000,
        endMs: 90_000,
      },
    ],
    ...extra,
  };
}

const view: LiveView = {
  runId: "a".repeat(32),
  model: "gemma4:e4b",
  mode: "local",
  endpoint: "http://127.0.0.1:11434/v1/",
  language: "de",
  promptVersion: "handymeeting-live-notes-1",
  createdAt: "2026-10-28T10:00:00Z",
  updatedAt: "2026-10-28T10:30:00Z",
  windows: 2,
  summarizedSections: 6,
  overview: [item("Budget Q3 besprochen")],
  overviewWindow: 2,
  points: [item("Erster Punkt"), item("Zweiter Punkt", { window: 2 })],
  decisions: [item("Günstigerer Anbieter gewählt")],
  tasks: [
    item("Angebot verschicken", {
      owner: "Anna",
      due: "Freitag",
      tracks: ["microphone"],
    }),
  ],
  questions: [],
  rejected: 2,
  damagedRuns: 0,
};

function status(next: Partial<LiveNotesStatus> = {}): LiveNotesStatus {
  return {
    meetingId: "m".repeat(32),
    status: "off",
    running: false,
    error: null,
    pendingChars: 0,
    intervalMs: 180_000,
    nextUpdateInMs: null,
    lastTrigger: null,
    view: null,
    ...next,
  };
}

const local: LiveConnection = {
  endpoint: "http://127.0.0.1:11434/v1/",
  model: "gemma4:e4b",
  mode: "local",
  apiKey: "",
  useSavedKey: false,
};

it("does not call a finished run complete while its overview still covers only the first window", async () => {
  const start = vi
    .fn()
    .mockResolvedValue(status({ status: "updating", running: true }));
  setup({
    getLiveNotes: vi.fn().mockResolvedValue(
      status({
        status: "finished",
        view: { ...view, windows: 10, overviewWindow: 1 },
      }),
    ),
    startLiveNotes: start,
  });
  await waitFor(() =>
    expect(
      screen.getAllByText(/Kurzfassung ist noch nicht vollständig/).length,
    ).toBeGreaterThan(0),
  );
  expect(
    screen.queryByText("Abgeschlossen: das ganze Meeting ist zusammengefasst."),
  ).toBeNull();
  expect(screen.getByText("Kurzfassung: 1 von 10 Textfenstern")).toBeTruthy();
  await userEvent.click(
    screen.getByRole("button", { name: "Auswertung erneuern" }),
  );
  await waitFor(() => expect(start).toHaveBeenCalledTimes(1));
});

it("exports the incomplete overview coverage instead of presenting a newer window as its date", () => {
  const text = liveNotesText(
    { ...view, windows: 10, overviewWindow: 1 },
    "Synthetic",
    "de",
  );
  expect(text).toContain("Kurzfassung: 1 von 10 Textfenstern");
  expect(text).toContain("Zweiter Punkt");
});

function setup(
  api: Partial<DesktopApi>,
  props: Partial<Parameters<typeof LiveNotesPanel>[0]> = {},
) {
  const setPreferences = vi.fn();
  const onUseLocalModel = vi.fn();
  render(
    <LiveNotesPanel
      api={
        { copyText: vi.fn().mockResolvedValue(undefined), ...api } as DesktopApi
      }
      locale="de"
      connected
      meetingId={"m".repeat(32)}
      meetingTitle="Synthetisches Meeting"
      meetingActive
      connection={local}
      preferences={{ auto: false, intervalMinutes: 3 }}
      setPreferences={setPreferences}
      onUseLocalModel={onUseLocalModel}
      onOpenTextModelSettings={vi.fn()}
      focus={false}
      setFocus={vi.fn()}
      {...props}
    />,
  );
  return { setPreferences, onUseLocalModel };
}

it("keeps the local memory warning visible before starting with options closed", async () => {
  setup({
    getLiveNotes: vi.fn().mockResolvedValue(status()),
    startLiveNotes: vi.fn().mockResolvedValue(status()),
  });
  const start = await screen.findByRole("button", {
    name: "Live-Zusammenfassung starten",
  });
  const warning = screen
    .getAllByText(
      /Lokales Textmodell benötigt zusätzlichen Arbeitsspeicher|Das lokale Textmodell läuft neben der Aufnahme/,
    )
    .find((node) => node.closest("details") === null);
  expect(warning).toBeDefined();
  expect(
    warning!.compareDocumentPosition(start) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(
    screen.getByText("Optionen und Hinweise").closest("details")?.open,
  ).toBe(false);
  expect(start.hasAttribute("disabled")).toBe(false);
});

it("starts a local live summary with the chosen model, interval and language", async () => {
  const start = vi
    .fn()
    .mockResolvedValue(status({ status: "waiting", running: true }));
  setup({
    getLiveNotes: vi.fn().mockResolvedValue(status()),
    startLiveNotes: start,
  });
  await screen.findByRole("button", { name: "Live-Zusammenfassung starten" });
  // A local model shares the computer's memory with the recording; the panel says so.
  expect(
    screen.getByText(/belegt auch in kleinen Größen \(etwa 4B\) mehrere GB/),
  ).toBeTruthy();
  await userEvent.click(
    screen.getByRole("button", { name: "Live-Zusammenfassung starten" }),
  );
  expect(start).toHaveBeenCalledWith({
    meetingId: "m".repeat(32),
    endpoint: "http://127.0.0.1:11434/v1/",
    model: "gemma4:e4b",
    mode: "local",
    cloudConsent: false,
    apiKey: "",
    useSavedKey: false,
    intervalMs: 180_000,
    language: "de",
  });
});

it("never sends cloud text without the per-start release and never auto-starts cloud", async () => {
  // The provider refuses, so the panel stays on the start screen afterwards.
  const start = vi.fn().mockRejectedValue("HTTP 401");
  setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(status()),
      startLiveNotes: start,
    },
    {
      connection: {
        ...local,
        mode: "cloud",
        endpoint: "https://openrouter.ai/api/v1/",
        model: "cloud-model",
      },
      preferences: { auto: true, intervalMinutes: 3 },
    },
  );
  const button = await screen.findByRole("button", {
    name: "Live-Zusammenfassung starten",
  });
  await waitFor(() =>
    expect((button as HTMLButtonElement).disabled).toBe(true),
  );
  expect(start).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole("checkbox", { name: /an https:\/\/openrouter.ai/ }),
  );
  await userEvent.click(button);
  expect(start).toHaveBeenCalledWith(
    expect.objectContaining({ mode: "cloud", cloudConsent: true }),
  );
  expect(await screen.findByRole("alert")).toBeTruthy();
  // The release covered that start only.
  expect(
    (
      screen.getByRole("checkbox", {
        name: /an https:\/\/openrouter.ai/,
      }) as HTMLInputElement
    ).checked,
  ).toBe(false);
});

it("keeps the cloud-settings remedy visible while an invalid endpoint disables start", async () => {
  const onOpenTextModelSettings = vi.fn();
  const start = vi.fn();
  setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(status()),
      startLiveNotes: start,
    },
    {
      connection: {
        ...local,
        mode: "cloud",
        endpoint: "http://invalid.example/v1/",
        model: "synthetic-cloud",
      },
      onOpenTextModelSettings,
    },
  );
  const button = (await screen.findByRole("button", {
    name: "Live-Zusammenfassung starten",
  })) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
  const remedy = screen.getByRole("button", {
    name: "Alle Textmodell-Einstellungen",
  });
  expect(remedy.closest("details")).toBeNull();
  expect(
    screen.getByText("Optionen und Hinweise").closest("details")!.open,
  ).toBe(false);
  await userEvent.click(remedy);
  expect(onOpenTextModelSettings).toHaveBeenCalledOnce();
  expect(start).not.toHaveBeenCalled();
});

it.each([
  [
    "de",
    "Alle Textmodell-Einstellungen",
    "Optionen und Hinweise",
    "Live-Zusammenfassung starten",
  ],
  [
    "en",
    "All text model settings",
    "Options and guidance",
    "Start live summary",
  ],
] as const)(
  "offers visible cloud settings for an empty model in %s",
  async (locale, action, options, startLabel) => {
    const onOpenTextModelSettings = vi.fn();
    const start = vi.fn();
    setup(
      {
        getLiveNotes: vi.fn().mockResolvedValue(status()),
        startLiveNotes: start,
      },
      {
        locale,
        connection: {
          ...local,
          mode: "cloud",
          endpoint: "https://openrouter.ai/api/v1/",
          model: "",
        },
        preferences: { auto: true, intervalMinutes: 3 },
        onOpenTextModelSettings,
      },
    );
    const startButton = await screen.findByRole("button", { name: startLabel });
    expect((startButton as HTMLButtonElement).disabled).toBe(true);
    const disclosure = screen.getByText(options).closest("details")!;
    expect(disclosure.open).toBe(false);
    const remedy = screen.getByRole("button", { name: action });
    expect(remedy.closest("details")).toBeNull();
    await userEvent.click(remedy);
    expect(onOpenTextModelSettings).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByText(options));
    expect(disclosure.open).toBe(true);
    expect(screen.getAllByRole("button", { name: action })).toHaveLength(1);
    await userEvent.click(startButton);
    expect(start).not.toHaveBeenCalled();
    expect(
      screen.queryByRole("checkbox", {
        name: /Send this meeting|Transkripttext dieses/,
      }),
    ).toBeNull();
  },
);

it("auto-starts once for a local model while the meeting is recording", async () => {
  const start = vi
    .fn()
    .mockResolvedValue(status({ status: "waiting", running: true }));
  setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(status()),
      startLiveNotes: start,
    },
    { preferences: { auto: true, intervalMinutes: 5 } },
  );
  await waitFor(() => expect(start).toHaveBeenCalledOnce());
  expect(start.mock.calls[0][0].intervalMs).toBe(300_000);
});

it("retries an automatic start while the previous meeting's summary is still finishing", async () => {
  // Codex review 5686078293: a refused first attempt used up the automatic start for good.
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    const start = vi
      .fn()
      .mockRejectedValueOnce(
        "Eine Live-Zusammenfassung läuft bereits. Bitte sie zuerst beenden.",
      )
      .mockResolvedValue(status({ status: "waiting", running: true }));
    setup(
      {
        getLiveNotes: vi.fn().mockResolvedValue(status()),
        startLiveNotes: start,
      },
      { preferences: { auto: true, intervalMinutes: 3 } },
    );
    await waitFor(() => expect(start).toHaveBeenCalledTimes(1));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15_000);
    });
    await waitFor(() => expect(start).toHaveBeenCalledTimes(2));
  } finally {
    vi.useRealTimers();
  }
});

it("does not show a stopped summary as waiting for recognition", async () => {
  setup({
    startLiveNotes: vi.fn(),
    getLiveNotes: vi.fn().mockResolvedValue(
      status({
        status: "stopped",
        running: false,
        waitingForTranscript: true,
      }),
    ),
  });
  await screen.findByText("Angehalten");
  expect(screen.queryByText(/Wartet auf Spracherkennung/)).toBeNull();
});

it("does not auto-start after an explicit stop or without a model", async () => {
  const start = vi.fn();
  setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(status({ status: "stopped" })),
      startLiveNotes: start,
    },
    { preferences: { auto: true, intervalMinutes: 3 } },
  );
  await screen.findByText("Angehalten");
  expect(start).not.toHaveBeenCalled();
});

it("shows summary, decisions, tasks with owner and track, evidence and the discarded count", async () => {
  setup({
    getLiveNotes: vi.fn().mockResolvedValue(
      status({
        status: "waiting",
        running: true,
        view,
        pendingChars: 400,
        nextUpdateInMs: 90_000,
      }),
    ),
    startLiveNotes: vi.fn(),
    refreshLiveNotes: vi.fn().mockResolvedValue(true),
  });
  expect(await screen.findByText("Budget Q3 besprochen")).toBeTruthy();
  expect(screen.getByText("Günstigerer Anbieter gewählt")).toBeTruthy();
  expect(screen.getByText("Anna · Freitag")).toBeTruthy();
  expect(screen.getByText(/01:00 · Mikrofon/)).toBeTruthy();
  // Codex review 5683390412: the on-screen evidence names its source like the copy text.
  expect(
    screen.getAllByText(/\[C60\] 01:00 · Computerton/).length,
  ).toBeGreaterThan(0);
  // Codex review 5686078293: a later answer is not reconciled, so the list must not claim
  // that its questions are still open.
  expect(screen.getByText("Keine Fragen aufgeworfen.")).toBeTruthy();
  expect(screen.queryByText(/Offene Fragen/)).toBeNull();
  expect(screen.getByText(/nächste Aktualisierung in ca. 2 Min./)).toBeTruthy();
  expect(
    screen.getByText(/2 KI-Aussage\(n\) ohne gültigen Beleg wurden verworfen/),
  ).toBeTruthy();
  expect(screen.getAllByText("Zitat zu Budget Q3 besprochen")[0]).toBeTruthy();
  expect(
    (
      screen.getByRole("button", {
        name: "Jetzt aktualisieren",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(false);
});

it("copies a plain text summary for reading out", async () => {
  const copy = vi.fn().mockResolvedValue(undefined);
  setup({
    getLiveNotes: vi.fn().mockResolvedValue(status({ view })),
    startLiveNotes: vi.fn(),
    copyText: copy,
  });
  await userEvent.click(
    await screen.findByRole("button", { name: "Zusammenfassung kopieren" }),
  );
  const text = copy.mock.calls[0][0] as string;
  expect(text).toContain("Bisheriger Verlauf\n- Budget Q3 besprochen");
  expect(text).toContain("Aufgaben\n- Angebot verschicken (Anna, Freitag)");
  expect(text).not.toContain("Aufgeworfene Fragen");
  // Codex review 5687252688 (P2): accepted points are copied even without an overview.
  expect(text).toContain(
    "Wichtige Punkte und Erkenntnisse\n- Erster Punkt\n  [C60 · 01:00 · Computerton] „Zitat zu Erster Punkt“",
  );
  const withoutOverview = liveNotesText(
    { ...view, overview: [], decisions: [], tasks: [] },
    "T",
    "de",
  );
  expect(withoutOverview).toContain("- Zweiter Punkt");
  expect(text).toContain("KI-Entwurf");
  expect(liveNotesText(view, "T", "en")).toContain("Summary so far");
});

it("recommends a model from memory, applies a running Ollama model and downloads a missing one", async () => {
  const setupInfo: LocalModelSetupInfo = {
    totalMemoryBytes: 16 * 1024 ** 3,
    recommendation: {
      tier: "standard",
      model: "gemma4:e4b",
      lmStudioSearch: "Gemma 4 E4B",
    },
    providers: [
      {
        id: "ollama",
        name: "Ollama",
        endpoint: "http://127.0.0.1:11434/v1/",
        reachable: true,
        models: ["llama3.2:latest"],
        recommendedInstalled: false,
      },
      {
        id: "lmstudio",
        name: "LM Studio",
        endpoint: "http://127.0.0.1:1234/v1/",
        reachable: false,
        models: [],
        recommendedInstalled: false,
      },
    ],
  };
  const pull = vi.fn().mockResolvedValue({
    model: "gemma4:e4b",
    status: "running",
    detail: "pulling manifest",
    completed: 0,
    total: 0,
    error: null,
  });
  const { onUseLocalModel } = setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(status()),
      startLiveNotes: vi.fn(),
      localModelSetup: vi.fn().mockResolvedValue(setupInfo),
      pullOllamaModel: pull,
      getModelPullProgress: vi.fn().mockResolvedValue(null),
    },
    { connection: { ...local, model: "" } },
  );
  expect(
    (
      (await screen.findByRole("button", {
        name: "Live-Zusammenfassung starten",
      })) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  await userEvent.click(
    screen.getByRole("button", { name: "Lokales Modell einrichten" }),
  );
  expect(
    await screen.findByText(/Arbeitsspeicher: 16 GB · Empfehlung: gemma4:e4b/),
  ).toBeTruthy();
  expect(
    screen.getByText(
      /„Gemma 4 E4B“ herunterladen und unter „Developer“ den lokalen Server starten/,
    ),
  ).toBeTruthy();
  await userEvent.click(
    screen.getByRole("button", { name: "Verwenden: llama3.2:latest" }),
  );
  expect(onUseLocalModel).toHaveBeenCalledWith(
    "http://127.0.0.1:11434/v1/",
    "llama3.2:latest",
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Lokales Modell einrichten" }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "gemma4:e4b in Ollama laden" }),
  );
  expect(pull).toHaveBeenCalledWith("gemma4:e4b");
  expect(await screen.findByText(/Lädt gemma4:e4b/)).toBeTruthy();
});

it("warns instead of suggesting a local model on too little memory", async () => {
  setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(status()),
      startLiveNotes: vi.fn(),
      localModelSetup: vi.fn().mockResolvedValue({
        totalMemoryBytes: 4 * 1024 ** 3,
        recommendation: { tier: "none", model: null, lmStudioSearch: null },
        providers: [],
      }),
    },
    { connection: { ...local, model: "" } },
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Lokales Modell einrichten" }),
  );
  expect(
    await screen.findByText(/Für eine lokale Live-Zusammenfassung zu wenig/),
  ).toBeTruthy();
});

const ollamaReady: LocalModelSetupInfo = {
  totalMemoryBytes: 16 * 1024 ** 3,
  recommendation: {
    tier: "standard",
    model: "gemma4:e4b",
    lmStudioSearch: "Gemma 4 E4B",
  },
  providers: [
    {
      id: "ollama",
      name: "Ollama",
      endpoint: "http://127.0.0.1:11434/v1/",
      reachable: true,
      models: ["gemma4:e4b"],
      recommendedInstalled: true,
    },
  ],
};

it("keeps polling, starting and model discovery working under StrictMode", async () => {
  const start = vi
    .fn()
    .mockResolvedValue(status({ status: "waiting", running: true }));
  render(
    <StrictMode>
      <LiveNotesPanel
        api={
          {
            copyText: vi.fn(),
            getLiveNotes: vi.fn().mockResolvedValue(status()),
            startLiveNotes: start,
            localModelSetup: vi.fn().mockResolvedValue(ollamaReady),
          } as unknown as DesktopApi
        }
        locale="de"
        connected
        meetingId={"m".repeat(32)}
        meetingTitle="Synthetisches Meeting"
        meetingActive
        connection={local}
        preferences={{ auto: true, intervalMinutes: 3 }}
        setPreferences={vi.fn()}
        onUseLocalModel={vi.fn()}
        onOpenTextModelSettings={vi.fn()}
        focus={false}
        setFocus={vi.fn()}
      />
    </StrictMode>,
  );
  // The automatic start needs the polled "off" state, which only arrives while mounted.
  await waitFor(() => expect(start).toHaveBeenCalled());
  expect(await screen.findByText(/Läuft/)).toBeTruthy();
  await userEvent.click(
    screen.getByRole("button", { name: "Lokales Modell einrichten" }),
  );
  expect(
    await screen.findByRole("button", { name: "Verwenden: gemma4:e4b" }),
  ).toBeTruthy();
});

it("copies every statement with its source identifier, time, track and verbatim quote", () => {
  const text = liveNotesText(view, "T", "de");
  expect(text).toContain(
    "- Budget Q3 besprochen\n  [C60 · 01:00 · Computerton] „Zitat zu Budget Q3 besprochen“",
  );
  expect(text).toContain(
    "- Angebot verschicken (Anna, Freitag)\n  [C60 · 01:00 · Computerton] „Zitat zu Angebot verschicken“",
  );
});

it("shows the running route, not settings that were changed for the next start", async () => {
  setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(
        status({
          status: "waiting",
          running: true,
          view: {
            ...view,
            mode: "cloud",
            model: "cloud-model",
            endpoint: "https://openrouter.ai/api/v1/",
          },
        }),
      ),
      startLiveNotes: vi.fn(),
    },
    // The settings now point at a local model, but the cloud run is still active.
    { connection: local },
  );
  expect(await screen.findByText("Cloud")).toBeTruthy();
  expect(screen.getByText(/cloud-model/)).toBeTruthy();
  expect(
    screen.getByText(/Geänderte Einstellungen gelten ab dem nächsten Start/),
  ).toBeTruthy();
  expect(screen.queryByText("Lokal")).toBeNull();
});

it("never sends a key typed for another route to a local model chosen from the presets", async () => {
  const meeting: Meeting = {
    id: "c".repeat(32),
    title: "Synthetic live",
    startedAt: "2026-10-28T10:00:00Z",
    durationMs: 5000,
    status: "recording",
    modelId: "synthetic",
    warnings: [],
    parts: [],
  };
  const state: AppState = {
    version: "synthetic",
    handy: null,
    recording: true,
    activeMeeting: meeting,
    captureControl: {
      paused: false,
      microphoneMuted: false,
      hasMicrophone: true,
    },
  };
  const start = vi
    .fn()
    .mockResolvedValue(status({ status: "waiting", running: true }));
  const api = {
    ...testServices(),
    getAppState: vi.fn(async () => structuredClone(state)),
    listDevices: vi.fn().mockResolvedValue([]),
    listMeetings: vi.fn().mockResolvedValue([]),
    getMeeting: vi.fn().mockResolvedValue(meeting),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    listTextModels: vi.fn().mockResolvedValue([]),
    refreshHandy: vi.fn().mockResolvedValue({
      executable: "native-choice",
      version: "synthetic",
      models: [],
    }),
    getLiveNotes: vi.fn().mockResolvedValue(status({ meetingId: meeting.id })),
    startLiveNotes: start,
    localModelSetup: vi.fn().mockResolvedValue(ollamaReady),
  } as unknown as DesktopApi;
  render(<App api={api} />);
  await screen.findByRole("button", { name: "Live-Zusammenfassung starten" });
  // A cloud key is typed on the analysis page.
  await userEvent.click(
    screen.getByRole("button", { name: "Nachbearbeitung" }),
  );
  await userEvent.selectOptions(
    await screen.findByLabelText("Verarbeitung"),
    "cloud",
  );
  await userEvent.type(
    screen.getByLabelText("Schlüssel nur für diese App-Sitzung"),
    "synthetic-cloud-key",
  );
  // Back on the recording page, a local model is chosen from the presets.
  await userEvent.click(screen.getByRole("button", { name: "Meeting" }));
  await userEvent.click(
    await screen.findByRole("button", { name: "Lokales Modell einrichten" }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Verwenden: gemma4:e4b" }),
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Live-Zusammenfassung starten" }),
  );
  await waitFor(() => expect(start).toHaveBeenCalled());
  expect(start.mock.calls[0][0]).toMatchObject({
    mode: "local",
    endpoint: "http://127.0.0.1:11434/v1/",
    model: "gemma4:e4b",
    apiKey: "",
    useSavedKey: false,
  });
});

it("offers explicit local server setup when the configured endpoint has no model", async () => {
  // Earlier tests remember a local endpoint; this case starts from the default port.
  localStorage.clear();
  // Keep setup available without expanding installation guidance automatically.
  const meeting: Meeting = {
    id: "d".repeat(32),
    title: "Synthetic analysis",
    startedAt: "2026-10-28T10:00:00Z",
    durationMs: 5000,
    status: "complete",
    modelId: "synthetic",
    warnings: [],
    parts: [],
  };
  const api = {
    ...testServices(),
    getAppState: vi.fn(async () => ({
      version: "synthetic",
      handy: null,
      recording: false,
      activeMeeting: null,
    })),
    listDevices: vi.fn().mockResolvedValue([]),
    listMeetings: vi.fn().mockResolvedValue([]),
    getMeeting: vi.fn().mockResolvedValue(meeting),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    listTextModels: vi.fn(async (endpoint: string) => {
      if (endpoint.includes(":11434")) return ["gemma4:e4b"];
      throw "Synthetic: no local server at 127.0.0.1:1234";
    }),
    refreshHandy: vi.fn().mockResolvedValue({
      executable: "native-choice",
      version: "synthetic",
      models: [],
    }),
    localModelSetup: vi.fn().mockResolvedValue(ollamaReady),
  } as unknown as DesktopApi;
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Nachbearbeitung" }),
  );
  fireEvent.click(screen.getByText("Auswertung einrichten"));
  expect(api.localModelSetup).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole("button", { name: "Ollama / LM Studio erkennen" }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Verwenden: gemma4:e4b" }),
  );
  expect(api.localModelSetup).toHaveBeenCalled();
  // CodeRabbit review 5683206688: the chosen server's models load without a manual refresh.
  await waitFor(() =>
    expect(api.listTextModels).toHaveBeenCalledWith(
      "http://127.0.0.1:11434/v1/",
    ),
  );
  const select = screen.getByRole("combobox", { name: "Textmodell" });
  await waitFor(() =>
    expect((select as HTMLSelectElement).value).toBe("gemma4:e4b"),
  );
  expect(
    within(select).queryByRole("option", { name: /Noch nicht erkannt/ }),
  ).toBeNull();
});

it("offers the start only for endpoints the backend transport accepts", () => {
  // CodeRabbit review 5682803580: a remote address in local mode passed the start gate and
  // failed only in the backend.
  expect(endpointAllowed("local", "http://127.0.0.1:11434/v1/")).toBe(true);
  expect(endpointAllowed("local", "http://localhost:1234/v1/")).toBe(true);
  expect(endpointAllowed("local", "http://[::1]:1234/v1/")).toBe(true);
  expect(endpointAllowed("local", "https://ai.example.com/v1/")).toBe(false);
  expect(endpointAllowed("local", "http://192.168.1.5:11434/v1/")).toBe(false);
  expect(endpointAllowed("local", "http://user:pw@127.0.0.1:1234/v1/")).toBe(
    false,
  );
  expect(endpointAllowed("local", "ftp://127.0.0.1/")).toBe(false);
  expect(endpointAllowed("local", "not a url")).toBe(false);
  expect(endpointAllowed("cloud", "https://openrouter.ai/api/v1/")).toBe(true);
  expect(endpointAllowed("cloud", "http://openrouter.ai/api/v1/")).toBe(false);
  expect(endpointAllowed("cloud", "https://openrouter.ai/api/v1/?key=x")).toBe(
    false,
  );
});

it("reconnects to a model download that kept running while the setup was closed", async () => {
  // Codex review 5683390412: reopening offered a second download instead of the progress.
  const running = {
    model: "gemma4:e4b",
    status: "running",
    detail: "pulling",
    completed: 5,
    total: 10,
    error: null,
  };
  const pull = vi.fn();
  setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(status()),
      startLiveNotes: vi.fn(),
      localModelSetup: vi.fn().mockResolvedValue({
        ...ollamaReady,
        providers: [
          {
            ...ollamaReady.providers[0],
            models: [],
            recommendedInstalled: false,
          },
        ],
      }),
      pullOllamaModel: pull,
      getModelPullProgress: vi.fn().mockResolvedValue(running),
      cancelModelPull: vi.fn(),
    },
    { connection: { ...local, model: "" } },
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Lokales Modell einrichten" }),
  );
  expect(await screen.findByText(/Lädt gemma4:e4b · 50 %/)).toBeTruthy();
  expect(screen.getByRole("button", { name: "Abbrechen" })).toBeTruthy();
  expect(
    screen.queryByRole("button", { name: "gemma4:e4b in Ollama laden" }),
  ).toBeNull();
  expect(pull).not.toHaveBeenCalled();
});

it("reloads the model list after Use even when the address stays the same", async () => {
  // Codex review 5683390412: a model loaded later at the configured server stayed unavailable.
  localStorage.clear();
  const meeting: Meeting = {
    id: "e".repeat(32),
    title: "Synthetic analysis",
    startedAt: "2026-10-28T10:00:00Z",
    durationMs: 5000,
    status: "complete",
    modelId: "synthetic",
    warnings: [],
    parts: [],
  };
  let loaded: string[] = [];
  const api = {
    ...testServices(),
    getAppState: vi.fn(async () => ({
      version: "synthetic",
      handy: null,
      recording: false,
      activeMeeting: null,
    })),
    listDevices: vi.fn().mockResolvedValue([]),
    listMeetings: vi.fn().mockResolvedValue([]),
    getMeeting: vi.fn().mockResolvedValue(meeting),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    listTextModels: vi.fn(async () => loaded),
    refreshHandy: vi.fn().mockResolvedValue({
      executable: "native-choice",
      version: "synthetic",
      models: [],
    }),
    localModelSetup: vi.fn(async () => ({
      ...ollamaReady,
      providers: [
        {
          id: "lmstudio",
          name: "LM Studio",
          endpoint: "http://127.0.0.1:1234/v1/",
          reachable: true,
          models: loaded,
          recommendedInstalled: false,
        },
      ],
    })),
  } as unknown as DesktopApi;
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Nachbearbeitung" }),
  );
  fireEvent.click(screen.getByText("Auswertung einrichten"));
  expect(api.localModelSetup).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole("button", { name: "Ollama / LM Studio erkennen" }),
  );
  // The user requests setup, then loads a model in LM Studio.
  await screen.findByText("LM Studio");
  loaded = ["gemma-4-e4b"];
  await userEvent.click(screen.getByRole("button", { name: "Erneut prüfen" }));
  await userEvent.click(
    await screen.findByRole("button", { name: "Verwenden: gemma-4-e4b" }),
  );
  const select = screen.getByRole("combobox", { name: "Textmodell" });
  await waitFor(() =>
    expect(
      within(select).getByRole("option", { name: "gemma-4-e4b" }),
    ).toBeTruthy(),
  );
  expect((select as HTMLSelectElement).value).toBe("gemma-4-e4b");
});

it("reports a failed download-state lookup instead of leaving an unhandled rejection", async () => {
  // CodeRabbit review on cc67da1: the reconnect lookup had no error path.
  setup(
    {
      getLiveNotes: vi.fn().mockResolvedValue(status()),
      startLiveNotes: vi.fn(),
      localModelSetup: vi.fn().mockResolvedValue(ollamaReady),
      getModelPullProgress: vi
        .fn()
        .mockRejectedValue("Synthetic: download state unavailable"),
    },
    { connection: { ...local, model: "" } },
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Lokales Modell einrichten" }),
  );
  expect(
    await screen.findByText("Synthetic: download state unavailable"),
  ).toBeTruthy();
});

it("ignores start answers and errors that arrive after the meeting was switched", async () => {
  // Review 6474935681 (P2): only the mount was checked, so a late answer for meeting A
  // replaced meeting B's state, error and busy flag, and Copy used B's title for A's notes.
  const a = "a".repeat(32),
    b = "b".repeat(32);
  const answers: {
    resolve: (next: LiveNotesStatus) => void;
    reject: (error: unknown) => void;
  }[] = [];
  const start = vi.fn(
    () =>
      new Promise<LiveNotesStatus>((resolve, reject) =>
        answers.push({ resolve, reject }),
      ),
  );
  const copyText = vi.fn().mockResolvedValue(undefined);
  const api = {
    copyText,
    getLiveNotes: vi.fn((id: string) =>
      Promise.resolve(status({ meetingId: id })),
    ),
    startLiveNotes: start,
  } as unknown as DesktopApi;
  const panel = (meetingId: string, title: string) => (
    <LiveNotesPanel
      api={api}
      locale="de"
      connected
      meetingId={meetingId}
      meetingTitle={title}
      meetingActive
      connection={local}
      preferences={{ auto: false, intervalMinutes: 3 }}
      setPreferences={vi.fn()}
      onUseLocalModel={vi.fn()}
      onOpenTextModelSettings={vi.fn()}
      focus={false}
      setFocus={vi.fn()}
    />
  );
  const { rerender } = render(panel(a, "Meeting A"));
  const startButton = () =>
    screen.getByRole("button", {
      name: "Live-Zusammenfassung starten",
    }) as HTMLButtonElement;
  await userEvent.click(
    await screen.findByRole("button", { name: "Live-Zusammenfassung starten" }),
  );
  expect(start).toHaveBeenCalledWith(expect.objectContaining({ meetingId: a }));
  rerender(panel(b, "Meeting B"));
  await waitFor(() => expect(startButton().disabled).toBe(false));
  await act(async () =>
    answers[0].resolve(
      status({ meetingId: a, status: "waiting", running: true, view }),
    ),
  );
  expect(screen.queryByText("Erster Punkt")).toBeNull();
  expect(startButton().disabled).toBe(false);
  expect(
    (
      screen.getByRole("button", {
        name: "Zusammenfassung kopieren",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);

  // The same for a refusal: meeting B's start fails only after A is shown again.
  await userEvent.click(startButton());
  expect(start).toHaveBeenLastCalledWith(
    expect.objectContaining({ meetingId: b }),
  );
  rerender(panel(a, "Meeting A"));
  await act(async () => answers[1].reject("Synthetischer Fehler für B"));
  expect(screen.queryByText(/Synthetischer Fehler für B/)).toBeNull();
  expect(startButton().disabled).toBe(false);
  expect(copyText).not.toHaveBeenCalled();
});

it("auto-starts the live summary for a recording that begins while another page is open", async () => {
  // Codex review 5687252688 (P2): the automatic start lived only in the panel on the
  // recording page, so a recording approved for an agent while Settings was open got no
  // live summary until the user opened that page.
  localStorage.clear();
  localStorage.setItem(
    "handymeeting.rust.live-notes.v1",
    JSON.stringify({
      auto: true,
      intervalMinutes: 3,
      endpoint: "http://127.0.0.1:11434/v1/",
      model: "gemma4:e4b",
    }),
  );
  const meeting: Meeting = {
    id: "d".repeat(32),
    title: "Synthetic agent recording",
    startedAt: "2026-10-28T10:00:00Z",
    durationMs: 5000,
    status: "recording",
    modelId: "synthetic",
    warnings: [],
    parts: [],
  };
  let state: AppState = {
    version: "synthetic",
    handy: null,
    recording: false,
    activeMeeting: null,
    captureControl: {
      paused: false,
      microphoneMuted: false,
      hasMicrophone: true,
    },
  };
  const start = vi
    .fn()
    .mockResolvedValue(status({ status: "waiting", running: true }));
  const api = {
    ...testServices(),
    getAppState: vi.fn(async () => structuredClone(state)),
    listDevices: vi.fn().mockResolvedValue([]),
    listMeetings: vi.fn().mockResolvedValue([]),
    getMeeting: vi.fn().mockResolvedValue(meeting),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    listTextModels: vi.fn().mockResolvedValue([]),
    refreshHandy: vi.fn().mockResolvedValue({
      executable: "native-choice",
      version: "synthetic",
      models: [],
    }),
    getLiveNotes: vi.fn((id: string) =>
      Promise.resolve(status({ meetingId: id })),
    ),
    startLiveNotes: start,
    localModelSetup: vi.fn().mockResolvedValue(ollamaReady),
  } as unknown as DesktopApi;
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Einstellungen" }),
  );
  // The recording starts without the recording page (e.g. an approved agent request).
  state = { ...state, recording: true, activeMeeting: meeting };
  await waitFor(() => expect(start).toHaveBeenCalledOnce(), {
    timeout: 8000,
  });
  expect(start.mock.calls[0][0]).toMatchObject({
    meetingId: meeting.id,
    mode: "local",
    model: "gemma4:e4b",
  });
  expect(
    screen.queryByRole("button", { name: "Live-Zusammenfassung starten" }),
  ).toBeNull();
  localStorage.clear();
});

it("never shows another meeting's live summary, even for a moment", async () => {
  // CodeRabbit review 5687473668: between the meeting change and the reset effect, an
  // answer for the previous meeting could be displayed under the new one. Synthetic IPC
  // that always answers for meeting A while meeting B is shown.
  const a = "a".repeat(32),
    b = "b".repeat(32);
  setup(
    {
      getLiveNotes: vi
        .fn()
        .mockResolvedValue(
          status({ meetingId: a, status: "waiting", running: true, view }),
        ),
      startLiveNotes: vi.fn(),
    },
    { meetingId: b, meetingTitle: "Meeting B" },
  );
  await screen.findByRole("button", { name: "Live-Zusammenfassung starten" });
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 50));
  });
  expect(screen.queryByText("Erster Punkt")).toBeNull();
  expect(screen.queryByText(/Läuft/)).toBeNull();
});

it("does not remember a cloud model as the local live summary model after switching back", async () => {
  // CodeRabbit review 5687473668 (Major): switching from cloud to local kept the cloud model
  // name, which was then saved as the local choice for automatic live summaries.
  localStorage.clear();
  const api = {
    ...testServices(),
    getAppState: vi.fn().mockResolvedValue({
      version: "synthetic",
      handy: null,
      recording: false,
      activeMeeting: null,
      captureControl: {
        paused: false,
        microphoneMuted: false,
        hasMicrophone: true,
      },
    } satisfies AppState),
    listDevices: vi.fn().mockResolvedValue([]),
    listMeetings: vi.fn().mockResolvedValue([]),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    listTextModels: vi.fn().mockResolvedValue([]),
    refreshHandy: vi.fn().mockResolvedValue({
      executable: "native-choice",
      version: "synthetic",
      models: [],
    }),
    localModelSetup: vi.fn().mockResolvedValue(ollamaReady),
  } as unknown as DesktopApi;
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Nachbearbeitung" }),
  );
  await userEvent.selectOptions(
    await screen.findByLabelText("Verarbeitung"),
    "cloud",
  );
  await userEvent.type(
    screen.getByLabelText("Textmodell"),
    "synthetic/cloud-model",
  );
  await userEvent.selectOptions(screen.getByLabelText("Verarbeitung"), "local");
  await waitFor(() =>
    expect(localStorage.getItem("handymeeting.rust.live-notes.v1")).not.toBe(
      null,
    ),
  );
  expect(localStorage.getItem("handymeeting.rust.live-notes.v1")).not.toContain(
    "synthetic/cloud-model",
  );
  localStorage.clear();
});
