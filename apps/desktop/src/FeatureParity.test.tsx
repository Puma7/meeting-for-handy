import { beforeEach, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { defaultTemplate } from "./domain";
import { testServices } from "./test-services";
import { TEMPLATE_KEY } from "./useTemplateLibrary";
import type {
  AnalysisResult,
  AppState,
  DesktopApi,
  Meeting,
  TemplateLibrary,
} from "./types";
// Full-App cases render the whole UI with its status polling. On slow CI runners
// (macos-15-intel) they exceeded the 5 s default; focused component files keep it.
vi.setConfig({ testTimeout: 15_000 });

// Synthetic IPC fixtures: no audio devices, native files, keychains or provider requests.
const meeting: Meeting = {
  id: "fixture-meeting",
  title: "Fixture",
  startedAt: "2026-10-25T10:00:00Z",
  durationMs: 60000,
  status: "complete",
  modelId: "streaming",
  warnings: [],
  parts: [
    {
      source: "system",
      startSample: 0,
      sampleCount: 480000,
      status: "complete",
      text: "Anna schreibt den Bericht bis Freitag.",
    },
  ],
};
function fixture() {
  let state: AppState = {
    version: "fixture",
    recording: false,
    activeMeeting: meeting,
    handy: {
      executable: "fixture",
      version: "0.9.8",
      models: [
        {
          id: "streaming",
          name: "Streaming",
          isDownloaded: true,
          supportsStreaming: true,
        },
      ],
    },
  };
  const api: DesktopApi = {
    ...testServices(),
    getAppState: vi.fn(async () => state),
    refreshHandy: vi.fn(async () => state.handy!),
    listDevices: vi.fn().mockResolvedValue([
      { id: "output", name: "Output", source: "system" },
      { id: "input", name: "Input", source: "microphone" },
    ]),
    startMeeting: vi.fn(),
    stopMeeting: vi.fn(),
    listMeetings: vi.fn().mockResolvedValue([meeting]),
    getMeeting: vi.fn().mockResolvedValue(meeting),
    resumeMeeting: vi.fn(),
    importLegacy: vi.fn(),
    generateNotes: vi.fn(),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    listTextModels: vi.fn().mockResolvedValue(["fixture-text"]),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    cancelNotes: vi.fn(),
    notesMarkdown: vi.fn(),
    copyText: vi.fn(),
  };
  return {
    api,
    setState: (next: AppState) => {
      state = next;
    },
    getState: () => state,
  };
}
async function notes() {
  await screen.findByRole("button", { name: "Dieses Meeting auswerten" });
  await userEvent.click(
    screen.getByRole("button", { name: "Nachbearbeitung" }),
  );
  await screen.findByRole("option", { name: "fixture-text" });
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: "Neue Vorlage",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
}
async function saveName(name: string) {
  const dialog = screen.getByRole("dialog");
  await userEvent.clear(within(dialog).getByLabelText("Name der Vorlage"));
  await userEvent.type(within(dialog).getByLabelText("Name der Vorlage"), name);
  await userEvent.click(
    within(dialog).getByRole("button", { name: "Speichern" }),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
}
function result(template = defaultTemplate()): AnalysisResult {
  return {
    id: "saved-result",
    meetingId: meeting.id,
    sourceHash: "fixture",
    createdAt: "2026-10-25T10:01:00Z",
    template,
    sections: template.steps.map((step, index) => ({
      ...step,
      items: index
        ? []
        : [
            {
              text: "Original claim",
              owner: null,
              due: null,
              evidence: [{ sourceId: "S00001", quote: "Anna schreibt" }],
            },
          ],
    })),
    sources: [
      {
        id: "S00001",
        source: "system",
        startMs: 0,
        endMs: 30000,
        text: "Anna schreibt den Bericht bis Freitag.",
      },
      {
        id: "S00002",
        source: "microphone",
        startMs: 30000,
        endMs: 60000,
        text: "<img src=x onerror=alert(1)> Dieser Abschnitt stammt aus dem gesicherten Stand.",
      },
    ],
  };
}
beforeEach(() => localStorage.clear());

it("migrates browser templates once and creates an independent native preset", async () => {
  const custom = defaultTemplate();
  custom.name = "Browser original";
  custom.steps[0].prompt = "Original custom prompt";
  const raw = JSON.stringify([custom]);
  localStorage.setItem(TEMPLATE_KEY, raw);
  const { api } = fixture();
  const view = render(<App api={api} />);
  await notes();
  expect(vi.mocked(api.saveTemplates).mock.calls[0][0].templates).toEqual([
    custom,
  ]);
  await userEvent.click(screen.getByRole("button", { name: "Neue Vorlage" }));
  await saveName("Fresh preset");
  const saved = vi.mocked(api.saveTemplates).mock.calls.at(-1)![0].templates;
  const fresh = saved.find((item) => item.name === "Fresh preset")!;
  expect(fresh.steps[0].prompt).not.toBe(custom.steps[0].prompt);
  expect(saved.find((item) => item.id === custom.id)).toEqual(custom);
  const writes = vi.mocked(api.saveTemplates).mock.calls.length;
  view.unmount();
  render(<App api={api} />);
  await notes();
  expect(api.saveTemplates).toHaveBeenCalledTimes(writes);
  expect(localStorage.getItem(TEMPLATE_KEY)).toBe(raw);
});

it("duplicates and renames a native preset without sharing step identities or changing the original", async () => {
  const fresh = defaultTemplate();
  fresh.name = "Native original";
  const { api } = fixture();
  await api.saveTemplates({ revision: 0, templates: [fresh] });
  vi.mocked(api.saveTemplates).mockClear();
  render(<App api={api} />);
  await notes();
  await userEvent.click(
    screen.getByRole("button", { name: "Vorlage duplizieren" }),
  );
  await saveName("Independent copy");
  let saved = vi.mocked(api.saveTemplates).mock.calls.at(-1)![0].templates;
  const copied = saved.find((item) => item.name === "Independent copy")!;
  expect(copied.steps.map((item) => item.prompt)).toEqual(
    fresh.steps.map((item) => item.prompt),
  );
  expect(
    copied.steps.every(
      (item) => !fresh.steps.some((old) => old.id === item.id),
    ),
  ).toBe(true);
  await userEvent.click(
    screen.getByRole("button", { name: "Vorlage umbenennen" }),
  );
  await saveName("Renamed copy");
  saved = vi.mocked(api.saveTemplates).mock.calls.at(-1)![0].templates;
  expect(saved.find((item) => item.id === fresh.id)).toEqual(fresh);
  expect(saved.find((item) => item.id === copied.id)).toEqual({
    ...copied,
    name: "Renamed copy",
  });
});

it("requires confirmation before removing a native preset and preserves other presets", async () => {
  const original = defaultTemplate();
  original.name = "Retained original";
  const removable = defaultTemplate();
  removable.name = "Remove this preset";
  const { api } = fixture();
  await api.saveTemplates({ revision: 0, templates: [original, removable] });
  vi.mocked(api.saveTemplates).mockClear();
  render(<App api={api} />);
  await notes();
  await userEvent.selectOptions(
    screen.getByRole("combobox", { name: "Vorlage" }),
    removable.id,
  );
  const before = vi.mocked(api.saveTemplates).mock.calls.length;
  await userEvent.click(
    screen.getByRole("button", { name: "Vorlage entfernen" }),
  );
  expect(
    within(screen.getByRole("dialog")).getByText(
      /gespeicherte Auswertungen und ihre ursprünglichen Arbeitsaufträge bleiben erhalten/,
    ),
  ).toBeTruthy();
  expect(api.saveTemplates).toHaveBeenCalledTimes(before);
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "Vorlage entfernen",
    }),
  );
  await waitFor(() =>
    expect(
      screen.queryByRole("option", { name: "Remove this preset" }),
    ).toBeNull(),
  );
  expect((await api.listTemplates()).templates).toEqual([original]);
});

it("keeps native templates authoritative and imports previous browser templates only on explicit request", async () => {
  const browser = defaultTemplate();
  browser.name = "Old browser";
  const native = defaultTemplate();
  native.name = "Native authoritative";
  localStorage.setItem(TEMPLATE_KEY, JSON.stringify([browser]));
  const { api } = fixture();
  vi.mocked(api.listTemplates).mockResolvedValue({
    revision: 7,
    templates: [native],
  });
  vi.mocked(api.importTemplates).mockResolvedValue({
    revision: 8,
    templates: [native, browser],
  });
  render(<App api={api} />);
  await notes();
  expect(
    screen.getByRole("option", { name: "Native authoritative" }),
  ).toBeTruthy();
  expect(screen.queryByRole("option", { name: "Old browser" })).toBeNull();
  expect(api.importTemplates).not.toHaveBeenCalled();
  expect(api.saveTemplates).not.toHaveBeenCalled();
  fireEvent.click(
    screen.getByText("Frühere Browser-Vorlagen übernehmen", {
      selector: "summary",
    }),
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Frühere Browser-Vorlagen übernehmen" }),
  );
  expect(
    await screen.findByRole("option", { name: "Old browser" }),
  ).toBeTruthy();
  expect(api.importTemplates).toHaveBeenCalledWith([browser]);
});

it("keeps rename input after a revision conflict and preserves concurrently changed instructions after reload", async () => {
  const preset = defaultTemplate();
  preset.name = "Original";
  let current: TemplateLibrary = { revision: 1, templates: [preset] };
  const { api } = fixture();
  vi.mocked(api.listTemplates).mockImplementation(async () =>
    structuredClone(current),
  );
  vi.mocked(api.saveTemplates)
    .mockRejectedValueOnce("Synthetic revision conflict")
    .mockImplementation(async (next) => ({
      ...next,
      revision: next.revision + 1,
    }));
  render(<App api={api} />);
  await notes();
  await userEvent.click(
    screen.getByRole("button", { name: "Vorlage umbenennen" }),
  );
  await userEvent.clear(screen.getByLabelText("Name der Vorlage"));
  await userEvent.type(screen.getByLabelText("Name der Vorlage"), "My rename");
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "Speichern",
    }),
  );
  await within(screen.getByRole("dialog")).findByRole("alert");
  expect(
    (screen.getByLabelText("Name der Vorlage") as HTMLInputElement).value,
  ).toBe("My rename");
  current = {
    revision: 2,
    templates: [
      {
        ...preset,
        steps: [
          { ...preset.steps[0], prompt: "Concurrent preserved prompt" },
          ...preset.steps.slice(1),
        ],
      },
    ],
  };
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "Vorlagen neu laden",
    }),
  );
  await waitFor(() =>
    expect(
      (
        within(screen.getByRole("dialog")).getByRole("button", {
          name: "Speichern",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "Speichern",
    }),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  const saved = vi.mocked(api.saveTemplates).mock.calls.at(-1)![0];
  expect(saved.revision).toBe(2);
  expect(saved.templates[0].name).toBe("My rename");
  expect(saved.templates[0].steps[0].prompt).toBe(
    "Concurrent preserved prompt",
  );
});

it("keeps saved instructions read only, duplicates their snapshot and navigates original source sections safely", async () => {
  const saved = result();
  saved.template.name = "Historical";
  saved.template.steps[0].prompt = "Historical immutable prompt";
  const before = JSON.stringify(saved);
  const { api } = fixture();
  vi.mocked(api.listAnalyses).mockResolvedValue({
    analyses: [saved],
    skipped: [],
  });
  render(<App api={api} />);
  await notes();
  await screen.findByText("Original claim");
  const initialCalls = vi.mocked(api.saveTemplates).mock.calls.length;
  await userEvent.click(
    screen.getByRole("button", { name: "Gesicherte Arbeitsaufträge ansehen" }),
  );
  expect(
    (screen.getByLabelText("Arbeitsauftrag") as HTMLTextAreaElement).readOnly,
  ).toBe(true);
  expect(screen.queryByRole("button", { name: "Speichern" })).toBeNull();
  await userEvent.keyboard("{Escape}");
  expect(api.saveTemplates).toHaveBeenCalledTimes(initialCalls);
  await userEvent.click(screen.getByRole("button", { name: /S00001/ }));
  expect(
    within(screen.getByRole("dialog")).getByText("Anna schreibt").tagName,
  ).toBe("MARK");
  await userEvent.click(
    screen.getByRole("button", { name: "Nächster Originalabschnitt" }),
  );
  expect(
    within(screen.getByRole("dialog")).getByText(saved.sources[1].text),
  ).toBeTruthy();
  expect(document.querySelector(".source-dialog img")).toBeNull();
  await userEvent.click(
    screen.getByRole("button", { name: "Vorheriger Originalabschnitt" }),
  );
  expect(
    within(screen.getByRole("dialog")).getByText("Anna schreibt").tagName,
  ).toBe("MARK");
  await userEvent.keyboard("{Escape}");
  await userEvent.click(
    screen.getByRole("button", { name: "Vorlage duplizieren" }),
  );
  await saveName("From saved snapshot");
  expect(
    vi.mocked(api.saveTemplates).mock.calls.at(-1)![0].templates.at(-1)!
      .steps[0].prompt,
  ).toBe("Historical immutable prompt");
  expect(JSON.stringify(saved)).toBe(before);
});

it("supports roving keyboard tabs and opens the selected instruction with F2", async () => {
  const { api } = fixture();
  render(<App api={api} />);
  await notes();
  await userEvent.click(screen.getByRole("tab", { name: "Zusammenfassung" }));
  await userEvent.keyboard("{ArrowRight}");
  const decision = screen.getByRole("tab", { name: "Entscheidungen" });
  expect(document.activeElement).toBe(decision);
  expect(decision.getAttribute("aria-selected")).toBe("true");
  expect(screen.getByRole("tabpanel").getAttribute("aria-labelledby")).toBe(
    decision.id,
  );
  await userEvent.keyboard("{End}");
  expect(document.activeElement).toBe(
    screen.getByRole("tab", { name: "To-dos" }),
  );
  await userEvent.keyboard("{Home}{F2}");
  expect(screen.getByRole("dialog")).toBeTruthy();
  expect(
    (screen.getByLabelText("Name des Tabs") as HTMLInputElement).value,
  ).toBe("Zusammenfassung");
});

it("creates English instructions for a new English preset without rewriting existing custom prompts", async () => {
  const { api } = fixture();
  render(<App api={api} />);
  await notes();
  const initial = vi.mocked(api.saveTemplates).mock.calls[0][0].templates[0];
  await userEvent.selectOptions(screen.getByLabelText("Sprache"), "en");
  await userEvent.click(screen.getByRole("button", { name: "New template" }));
  await userEvent.type(
    screen.getByLabelText("Template name"),
    "English preset",
  );
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "Save" }),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  const templates = vi.mocked(api.saveTemplates).mock.calls.at(-1)![0]
    .templates;
  expect(templates[0]).toEqual(initial);
  expect(templates[1].steps[0].name).toBe("Summary");
  expect(templates[1].steps[0].prompt).toContain("in English");
});

it("passes only the approved agent scope and current UI devices without starting a manual capture", async () => {
  const data = fixture();
  data.setState({
    ...data.getState(),
    activeMeeting: null,
    agent: {
      connected: true,
      label: "Fixture agent",
      ownerId: "fixture-owner",
      revoked: false,
      pending: {
        requestId: "fixture-request",
        title: "Explicit request title",
        includeMicrophone: false,
        maxDurationSeconds: 600,
        status: "pending",
      },
      ownedSessionId: null,
      expiresAtUnixMs: null,
    },
  });
  render(<App api={data.api} />);
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: "Diese Aufnahme erlauben",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
  await userEvent.click(screen.getByLabelText("Mikrofon zusätzlich aufnehmen"));
  await userEvent.click(
    screen.getByRole("button", { name: "Diese Aufnahme erlauben" }),
  );
  await waitFor(() =>
    expect(data.api.agentDecideRecording).toHaveBeenCalledWith(
      "fixture-request",
      true,
      {
        title: "Explicit request title",
        modelId: "streaming",
        systemDeviceId: "output",
        microphoneDeviceId: null,
      },
    ),
  );
  expect(data.api.startMeeting).not.toHaveBeenCalled();
});

it("blocks agent approval for batch-only models while allowing manual section capture", async () => {
  const data = fixture();
  const current = data.getState();
  data.setState({
    ...current,
    activeMeeting: null,
    handy: {
      ...current.handy!,
      models: [
        ...current.handy!.models,
        {
          id: "parakeet-ultra",
          name: "Parakeet Ultra",
          isDownloaded: true,
          supportsStreaming: false,
          supportsBatch: true,
        },
      ],
    },
    agent: {
      connected: true,
      label: "Synthetic agent",
      ownerId: "fixture-owner",
      revoked: false,
      pending: {
        requestId: "fixture-batch-request",
        title: "Explicit request",
        includeMicrophone: false,
        maxDurationSeconds: 600,
        status: "pending",
      },
      ownedSessionId: null,
      expiresAtUnixMs: null,
    },
  });
  render(<App api={data.api} />);
  const approve = await screen.findByRole("button", {
    name: "Diese Aufnahme erlauben",
  });
  await waitFor(() =>
    expect((approve as HTMLButtonElement).disabled).toBe(false),
  );
  await userEvent.selectOptions(
    screen.getByLabelText("Erkennungsmodell aus Handy"),
    "parakeet-ultra",
  );
  expect(
    (
      screen.getByRole("button", {
        name: "Aufnahme starten",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(false);
  expect((approve as HTMLButtonElement).disabled).toBe(true);
  await userEvent.click(approve);
  expect(data.api.agentDecideRecording).not.toHaveBeenCalled();
  await userEvent.selectOptions(
    screen.getByLabelText("Erkennungsmodell aus Handy"),
    "streaming",
  );
  expect((approve as HTMLButtonElement).disabled).toBe(false);
});
