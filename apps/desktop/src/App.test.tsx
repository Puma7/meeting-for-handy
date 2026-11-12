import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import * as MeetingView from "./views/MeetingView";
import { testServices } from "./test-services";
import { defaultTemplate, renderTranscript, validateLibrary } from "./domain";
import { de, en } from "./i18n";
import type { AnalysisResult, AppState, DesktopApi, Meeting } from "./types";
// Full-App cases render the whole UI with its status polling. On slow CI runners
// (macos-15-intel) they exceeded the 5 s default; focused component files keep it.
vi.setConfig({ testTimeout: 15_000 });

// Synthetic IPC fixtures only. These tests never open an audio device or a model server.
const meeting: Meeting = {
  id: "synthetic-session",
  title: "Synthetic meeting",
  startedAt: "2026-10-25T12:00:00+01:00",
  durationMs: 60000,
  status: "complete",
  modelId: "installed-stream",
  warnings: [],
  parts: [
    {
      file: "system-000000000000.wav",
      source: "system",
      startSample: 0,
      sampleCount: 480000,
      text: "Anna übernimmt den Bericht bis Freitag.",
      status: "complete",
    },
  ],
};
/**
 * Builds a synthetic ready catalogue snapshot for UI tests; it contains no installed model
 * or device evidence.
 */
function state(active: Meeting | null = null): AppState {
  return {
    version: "1.0.0-beta.1",
    recording: false,
    activeMeeting: active,
    handy: {
      executable: "synthetic-handy",
      version: "0.9.8",
      models: [
        {
          id: "installed-stream",
          name: "Installed streaming model",
          isDownloaded: true,
          supportsStreaming: true,
        },
      ],
    },
  };
}
it.each(["de", "en"] as const)(
  "offers compact, activatable result and resume guidance (%s)",
  async (locale) => {
    localStorage.setItem(
      "handymeeting.rust.beta.preferences.v1",
      JSON.stringify({ locale }),
    );
    const t = locale === "de" ? de : en;
    const api = fakeApi(meeting);
    api.listUnfinishedAnalyses = vi.fn().mockResolvedValue({
      items: [
        {
          id: "a".repeat(32),
          meetingId: meeting.id,
          sourceHash: "synthetic",
          template: defaultTemplate(locale),
          createdAt: meeting.startedAt,
          model: "synthetic",
          mode: "local",
          endpoint: "http://127.0.0.1:9/v1/",
          confirmedTasks: 1,
          totalTasks: 2,
          promptVersion: "synthetic",
          resumable: true,
        },
      ],
      skipped: [],
    });
    render(<App api={api} />);
    await waitFor(() =>
      expect(
        (screen.getByRole("button", { name: t.start }) as HTMLButtonElement)
          .disabled,
      ).toBe(false),
    );
    await userEvent.click(screen.getByRole("button", { name: t.notes }));
    await screen.findByRole("heading", { name: t.unfinishedAnalyses });
    const draftHelp = screen.getByText(
      locale === "de" ? "Hinweise zu KI-Ergebnissen" : "About AI results",
    );
    const resumeHelp = screen.getByText(
      locale === "de" ? "Wie geht es weiter?" : "How do I resume?",
    );
    for (const [summary, text] of [
      [draftHelp, t.draft],
      [resumeHelp, t.resumeHelp],
    ] as const) {
      const disclosure = summary.closest("details")!;
      expect(disclosure.open).toBe(false);
      await userEvent.click(summary);
      expect(disclosure.open).toBe(true);
      expect(within(disclosure).getByText(text)).toBeTruthy();
    }
    expect(
      screen
        .getByRole("heading", { level: 1, name: t.notes })
        .hasAttribute("title"),
    ).toBe(false);
  },
);

it("checks for updates from the sidebar without leaving the current page", async () => {
  const api = fakeApi();
  api.checkForUpdate = vi.fn().mockResolvedValue(undefined);
  render(<App api={api} />);
  const check = await screen.findByRole("button", { name: "Update prüfen" });
  expect(check.closest("aside")).toBeTruthy();
  await userEvent.click(check);
  await waitFor(() => expect(api.checkForUpdate).toHaveBeenCalledTimes(1));
  expect(
    screen.getByRole("heading", { level: 1, name: "Meeting" }),
  ).toBeTruthy();
});

it.each([false, true])(
  "opens and focuses the exact update panel, including already-open Settings (%s)",
  async (alreadyOpen) => {
    const api = fakeApi();
    api.checkForUpdate = vi.fn();
    api.getUpdateProgress = vi.fn().mockResolvedValue({
      status: "available",
      version: "9.9.9",
      releaseUrl: null,
      receivedBytes: 0,
      totalBytes: 0,
      elapsedMs: 10,
      error: null,
    });
    const previousScroll = Element.prototype.scrollIntoView;
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    try {
      const user = userEvent.setup();
      render(<App api={api} />);
      await ready();
      if (alreadyOpen)
        await user.click(screen.getByRole("button", { name: "Einstellungen" }));
      await user.click(
        await screen.findByRole("button", { name: "Update öffnen" }),
      );
      const target = await screen.findByRole("region", {
        name: "App aktualisieren",
      });
      await waitFor(() => expect(document.activeElement).toBe(target));
      expect(scroll).toHaveBeenCalledOnce();
      expect(scroll.mock.instances[0]).toBe(target);
      expect(scroll).toHaveBeenCalledWith({ block: "start" });
      // Ordinary navigation later retains the existing page-heading focus contract.
      await user.click(screen.getByRole("button", { name: "Meeting" }));
      await user.click(screen.getByRole("button", { name: "Einstellungen" }));
      expect(document.activeElement).toBe(
        screen.getByRole("heading", { level: 1, name: "Einstellungen" }),
      );
      expect(scroll).toHaveBeenCalledOnce();
      expect(api.checkForUpdate).not.toHaveBeenCalled();
    } finally {
      Element.prototype.scrollIntoView = previousScroll;
    }
  },
);

it.each(["de", "en"] as const)(
  "acknowledges a slow history open in a persistent polite region without submitting twice (%s)",
  async (locale) => {
    localStorage.setItem(
      "handymeeting.rust.beta.preferences.v1",
      JSON.stringify({ locale }),
    );
    const t = locale === "de" ? de : en;
    const api = fakeApi();
    let finish!: (item: Meeting) => void;
    api.getMeeting = vi.fn(
      () =>
        new Promise<Meeting>((resolve) => {
          finish = resolve;
        }),
    );
    const { container } = render(<App api={api} />);
    await screen.findByRole("button", { name: t.history });
    await userEvent.click(screen.getByRole("button", { name: t.history }));
    const persistent = container.querySelector('main > p[role="status"]');
    expect(persistent).not.toBeNull();
    expect(persistent!.textContent).toBe("");
    expect(persistent!.getAttribute("aria-live")).toBe("polite");
    const open = await screen.findByRole("button", {
      name: locale === "de" ? "Öffnen" : "Open",
    });
    fireEvent.click(open);
    fireEvent.click(open);
    expect(api.getMeeting).toHaveBeenCalledTimes(1);
    expect(container.querySelector('main > p[role="status"]')).toBe(persistent);
    expect(persistent!.textContent).toBe(
      locale === "de" ? "Meeting wird geöffnet…" : "Opening meeting…",
    );
    await act(async () => finish(meeting));
    expect(api.getMeeting).toHaveBeenCalledTimes(1);
    expect(container.querySelector('main > p[role="status"]')).toBe(persistent);
    expect(persistent!.textContent).toBe("");
  },
);

it.each([false, true])(
  "keeps only the latest queued history selection after a pending request (failure=%s)",
  async (failure) => {
    const api = fakeApi();
    const items = ["A", "B", "C"].map((name) => ({
      ...meeting,
      id: `synthetic-${name}`,
      title: `Synthetic ${name}`,
    }));
    api.listMeetings = vi.fn().mockResolvedValue(items);
    api.searchMeetings = vi.fn();
    let finish!: (item: Meeting) => void;
    let reject!: (reason: unknown) => void;
    let finishLast!: (item: Meeting) => void;
    api.getMeeting = vi.fn().mockImplementation((id: string) => {
      if (id === items[0].id)
        return new Promise<Meeting>((resolve, fail) => {
          finish = resolve;
          reject = fail;
        });
      if (id === items[2].id)
        return new Promise<Meeting>((resolve) => {
          finishLast = resolve;
        });
      return Promise.resolve(items[1]);
    });
    render(<App api={api} />);
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "Historie" }));
    const buttons = await screen.findAllByRole("button", { name: "Öffnen" });
    const opener = (title: string) =>
      buttons.find((button) =>
        button.closest("article")?.textContent?.includes(title),
      )!;
    fireEvent.click(opener("Synthetic A"));
    fireEvent.click(opener("Synthetic B"));
    fireEvent.click(opener("Synthetic C"));
    fireEvent.click(opener("Synthetic C"));
    expect(api.getMeeting).toHaveBeenCalledExactlyOnceWith(items[0].id);
    await act(async () => {
      if (failure) reject(new Error("Synthetic first request failed"));
      else finish(items[0]);
    });
    await waitFor(() => expect(api.getMeeting).toHaveBeenCalledTimes(2));
    expect(api.getMeeting).toHaveBeenLastCalledWith(items[2].id);
    expect(
      screen.getByRole("heading", { level: 1, name: "Historie" }),
    ).toBeTruthy();
    expect(screen.getByText("Meeting wird geöffnet…")).toBeTruthy();
    await act(async () => finishLast(items[2]));
    await waitFor(() =>
      expect(screen.queryByText("Meeting wird geöffnet…")).toBeNull(),
    );
    expect(screen.getByRole("heading", { name: "Synthetic C" })).toBeTruthy();
    expect(api.getMeeting).toHaveBeenCalledTimes(2);
  },
);

it.each([false, true])(
  "keeps a newer Notes selection after an older history request settles (failure=%s)",
  async (failure) => {
    const api = fakeApi();
    const other = {
      ...meeting,
      id: "synthetic-other",
      title: "Other selected meeting",
    };
    api.listMeetings = vi.fn().mockResolvedValue([meeting, other]);
    api.searchMeetings = vi.fn();
    let finish!: (value: Meeting) => void;
    let reject!: (reason: unknown) => void;
    api.getMeeting = vi.fn().mockImplementation((id: string) =>
      id === meeting.id
        ? new Promise<Meeting>((resolve, fail) => {
            finish = resolve;
            reject = fail;
          })
        : Promise.resolve(other),
    );
    render(<App api={api} />);
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "Historie" }));
    const row = (await screen.findByText(meeting.title)).closest("article")!;
    fireEvent.click(within(row).getByRole("button", { name: "Öffnen" }));
    await userEvent.click(
      screen.getByRole("button", { name: "Nachbearbeitung" }),
    );
    await userEvent.selectOptions(
      screen.getByLabelText("Meeting für die Auswertung"),
      other.id,
    );
    await waitFor(() =>
      expect(api.listAnalyses).toHaveBeenCalledWith(other.id),
    );
    await act(async () => {
      if (failure) reject(new Error("Outdated history failure"));
      else finish(meeting);
    });
    expect(
      screen.getByRole("heading", { level: 1, name: "Nachbearbeitung" }),
    ).toBeTruthy();
    expect(
      (screen.getByLabelText("Meeting für die Auswertung") as HTMLSelectElement)
        .value,
    ).toBe(other.id);
    expect(screen.queryByText("Outdated history failure")).toBeNull();
    expect(screen.queryByText("Meeting wird geöffnet…")).toBeNull();
  },
);

it("does not let an older Notes read replace a newer history detail", async () => {
  const api = fakeApi();
  const other = {
    ...meeting,
    id: "synthetic-other",
    title: "Other selected meeting",
  };
  api.listMeetings = vi.fn().mockResolvedValue([meeting, other]);
  api.searchMeetings = vi.fn();
  let finish!: (value: Meeting) => void;
  api.getMeeting = vi.fn().mockImplementation((id: string) =>
    id === other.id
      ? new Promise<Meeting>((resolve) => {
          finish = resolve;
        })
      : Promise.resolve(meeting),
  );
  render(<App api={api} />);
  await ready();
  await userEvent.click(
    screen.getByRole("button", { name: "Nachbearbeitung" }),
  );
  await userEvent.selectOptions(
    screen.getByLabelText("Meeting für die Auswertung"),
    other.id,
  );
  await userEvent.click(screen.getByRole("button", { name: "Historie" }));
  const row = (await screen.findByText(meeting.title)).closest("article")!;
  await userEvent.click(within(row).getByRole("button", { name: "Öffnen" }));
  await screen.findByRole("heading", { name: meeting.title });
  await act(async () => finish(other));
  expect(screen.getByRole("heading", { name: meeting.title })).toBeTruthy();
  expect(screen.queryByRole("heading", { name: other.title })).toBeNull();
});

it.each(["Einstellungen", "Meeting"])(
  "does not replace newer %s navigation or fetch a stale queued history selection",
  async (destination) => {
    const api = fakeApi();
    const other = {
      ...meeting,
      id: "synthetic-other",
      title: "Other selected meeting",
    };
    api.listMeetings = vi.fn().mockResolvedValue([meeting, other]);
    api.searchMeetings = vi.fn();
    let finish!: (value: Meeting) => void;
    api.getMeeting = vi.fn().mockImplementation(
      () =>
        new Promise<Meeting>((resolve) => {
          finish = resolve;
        }),
    );
    render(<App api={api} />);
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "Historie" }));
    for (const title of [meeting.title, other.title]) {
      const row = (await screen.findByText(title)).closest("article")!;
      fireEvent.click(within(row).getByRole("button", { name: "Öffnen" }));
    }
    await userEvent.click(screen.getByRole("button", { name: destination }));
    await act(async () => finish(meeting));
    expect(
      screen.getByRole("heading", { level: 1, name: destination }),
    ).toBeTruthy();
    expect(api.getMeeting).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Meeting wird geöffnet…")).toBeNull();
  },
);

/**
 * Creates mocked desktop services with capture rejected by default. Tests override only
 * the IPC behavior under examination; no device, model or native dialog is opened.
 */
function fakeApi(active: Meeting | null = null): DesktopApi {
  return {
    ...testServices(),
    getAppState: vi.fn().mockResolvedValue(state(active)),
    refreshHandy: vi.fn().mockResolvedValue(state().handy),
    listDevices: vi.fn().mockResolvedValue([
      {
        id: "out",
        name: "Synthetic output",
        source: "system",
        isDefault: true,
      },
      { id: "mic", name: "Synthetic microphone", source: "microphone" },
    ]),
    startMeeting: vi.fn().mockRejectedValue("Synthetic capture unavailable"),
    stopMeeting: vi.fn().mockResolvedValue(meeting),
    listMeetings: vi.fn().mockResolvedValue([meeting]),
    getMeeting: vi.fn().mockResolvedValue(active ?? meeting),
    resumeMeeting: vi.fn().mockResolvedValue(active ?? meeting),
    importLegacy: vi.fn().mockResolvedValue({ ...meeting, readOnly: true }),
    generateNotes: vi.fn(),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    listTextModels: vi.fn().mockResolvedValue(["synthetic-text-model"]),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    cancelNotes: vi.fn().mockResolvedValue(false),
    notesMarkdown: vi
      .fn()
      .mockResolvedValue("# Synthetic analysis\n\nEscaped source text.\n"),
    copyText: vi.fn().mockResolvedValue(undefined),
  };
}
/**
 * Waits for the rendered start control to become enabled, allowing startup
 * device/catalogue reconciliation to finish.
 */
async function ready() {
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: "Aufnahme starten",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
}
beforeEach(() => localStorage.clear());

it.each(["session", "saved"] as const)(
  "changes the setup probe identity for the %s credential choice without exposing the key",
  async (choice) => {
    const api = fakeApi();
    api.credentialStatus = vi.fn().mockResolvedValue({
      state: "stored",
      provider: "synthetic",
      message: null,
    });
    api.checkMeetingSetup = vi.fn();
    const view = vi.spyOn(MeetingView, "default");
    const probeKey = () => view.mock.lastCall?.[0].textModelCheck?.probeKey;
    const syntheticKey = "synthetic-session-secret";
    try {
      render(<App api={api} />);
      await screen.findByRole("button", { name: de.settings });
      const first = probeKey();
      expect(first).toBeTruthy();
      await userEvent.click(screen.getByRole("button", { name: de.settings }));
      await userEvent.click(screen.getByText(de.apiKey));
      const saved = await screen.findByRole("checkbox", {
        name: de.useSavedKey,
      });
      if (choice === "saved") await userEvent.click(saved);
      else {
        await userEvent.type(
          screen.getByLabelText(de.sessionKey),
          syntheticKey,
        );
      }
      await userEvent.click(screen.getByRole("button", { name: de.meeting }));
      const latest = probeKey();
      expect(latest).toBeTruthy();
      expect(latest).not.toBe(first);
      expect(latest).not.toContain(syntheticKey);
    } finally {
      view.mockRestore();
    }
  },
);

it.each(
  (["de", "en"] as const).flatMap((locale) =>
    (["manual", "agent-other", "agent-owned", "disconnected"] as const).map(
      (scenario) => ({ locale, scenario }),
    ),
  ),
)(
  "offers workspace capture controls only for connected manual ownership ($locale/$scenario)",
  async ({ locale, scenario }) => {
    await import("./workspace/WorkspaceBridge");
    localStorage.setItem(
      "handymeeting.rust.beta.preferences.v1",
      JSON.stringify({ locale }),
    );
    localStorage.setItem(
      "handymeeting.meeting-view.full-window.v1",
      "workspace",
    );
    const t = locale === "de" ? de : en;
    const active = { ...meeting, status: "recording" };
    let current: AppState = state();
    let disconnected = false;
    const api = fakeApi(active);
    api.getAppState = vi.fn(async () => {
      if (disconnected) throw "Synthetic disconnected core";
      return current;
    });
    api.getLiveNotes = vi
      .fn()
      .mockResolvedValue({ status: "off", running: false });
    api.startLiveNotes = vi.fn();
    api.setWorkspaceContext = vi.fn();
    api.stopLiveNotes = vi.fn();
    api.refreshLiveNotes = vi.fn();
    api.setLiveNotesInterval = vi.fn();
    api.localModelSetup = vi.fn();
    api.setCapturePaused = vi.fn(async (_id: string, paused: boolean) => {
      current = {
        ...current,
        captureControl: { ...current.captureControl!, paused },
      };
      return current;
    });
    api.setMicrophoneMuted = vi.fn(
      async (_id: string, microphoneMuted: boolean) => {
        current = {
          ...current,
          captureControl: { ...current.captureControl!, microphoneMuted },
        };
        return current;
      },
    );
    const restoreObserver = installWorkspaceObserver();
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const mounted = render(<App api={api} />);
    try {
      // The default microphone is selected before the synthetic recording starts.
      await waitFor(() =>
        expect(
          screen.getByRole("checkbox", { name: t.microphone }),
        ).toHaveProperty("checked", true),
      );
      current = {
        ...state(active),
        recording: true,
        captureControl: {
          paused: false,
          microphoneMuted: false,
          hasMicrophone: true,
        },
        agent: scenario.startsWith("agent")
          ? {
              connected: true,
              label: "Synthetic agent",
              ownerId: "synthetic-owner",
              revoked: false,
              pending: null,
              expiresAtUnixMs: null,
              activeCapture: true,
              ownedSessionId:
                scenario === "agent-owned" ? active.id : "another-session",
            }
          : null,
      };
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1500);
      });
      const root = () =>
        mounted.container.querySelector(".native-workspace-host")?.shadowRoot;
      await waitFor(() =>
        expect(root()?.querySelector('[role="tablist"]')).toBeTruthy(),
      );
      if (scenario === "disconnected") {
        disconnected = true;
        await act(async () => {
          await vi.advanceTimersByTimeAsync(1500);
        });
        await waitFor(() =>
          expect(root()?.textContent).toContain("Synthetic disconnected core"),
        );
      }
      const shadow = within(root()!.querySelector(".prototype") as HTMLElement);
      const pause = locale === "de" ? "Aufnahme pausieren" : "Pause recording";
      const mute =
        locale === "de" ? "Mikrofon stummschalten" : "Mute microphone";
      if (scenario === "agent-owned" || scenario === "disconnected") {
        expect.soft(shadow.queryByRole("button", { name: pause })).toBeNull();
        expect.soft(shadow.queryByRole("button", { name: mute })).toBeNull();
        expect(api.setCapturePaused).not.toHaveBeenCalled();
        expect(api.setMicrophoneMuted).not.toHaveBeenCalled();
      } else {
        fireEvent.click(shadow.getByRole("button", { name: pause }));
        await waitFor(() =>
          expect(api.setCapturePaused).toHaveBeenCalledWith(active.id, true),
        );
        await waitFor(() =>
          expect(
            shadow.getByRole("button", {
              name:
                locale === "de" ? "Aufnahme fortsetzen" : "Resume recording",
            }),
          ).toBeTruthy(),
        );
        fireEvent.click(shadow.getByRole("button", { name: mute }));
        await waitFor(() =>
          expect(api.setMicrophoneMuted).toHaveBeenCalledWith(active.id, true),
        );
        await waitFor(() =>
          expect(
            shadow.getByRole("button", {
              name:
                locale === "de" ? "Mikrofon aktivieren" : "Enable microphone",
            }),
          ).toBeTruthy(),
        );
      }
      expect(api.startMeeting).not.toHaveBeenCalled();
      expect(api.stopMeeting).not.toHaveBeenCalled();
      expect(api.startLiveNotes).not.toHaveBeenCalled();
    } finally {
      mounted.unmount();
      vi.useRealTimers();
      restoreObserver();
    }
  },
);

/** Scope the observer fixture without undoing the test environment's storage globals. */
function installWorkspaceObserver() {
  const previous = Object.getOwnPropertyDescriptor(
    globalThis,
    "ResizeObserver",
  );
  Object.defineProperty(globalThis, "ResizeObserver", {
    configurable: true,
    writable: true,
    value: class {
      observe() {}
      disconnect() {}
    },
  });
  return () => {
    if (previous) Object.defineProperty(globalThis, "ResizeObserver", previous);
    else Reflect.deleteProperty(globalThis, "ResizeObserver");
  };
}

it("keeps a late speaker model error visible after its settings disclosure closes", async () => {
  const api = fakeApi();
  api.startSpeakerExperiment = vi.fn();
  let reject!: (reason: unknown) => void;
  api.checkSpeakerModel = vi.fn().mockImplementation(
    () =>
      new Promise((_, fail) => {
        reject = fail;
      }),
  );
  render(<App api={api} />);
  await ready();
  await userEvent.click(screen.getByRole("button", { name: "Einstellungen" }));
  const summary = screen.getByText("Sprecherfunktionen · optional");
  const disclosure = summary.closest("details")!;
  await userEvent.click(summary);
  await userEvent.selectOptions(
    screen.getByLabelText("Installiertes Modell für die Sprecherprüfung"),
    "installed-stream",
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Sprecherunterstützung prüfen" }),
  );
  expect(api.checkSpeakerModel).toHaveBeenCalledExactlyOnceWith(
    "installed-stream",
  );
  await userEvent.click(summary);
  await act(async () =>
    reject(new Error("Synthetic late speaker check failed")),
  );
  const alert = await screen.findByText("Synthetic late speaker check failed");
  expect(alert.closest("details")).toBeNull();
  expect(disclosure.open).toBe(false);
});

it("keeps the speaker check archive remedy and its completion outside closed settings", async () => {
  const api = fakeApi();
  api.startSpeakerExperiment = vi.fn();
  api.checkSpeakerModel = vi
    .fn()
    .mockRejectedValue(
      new Error(
        "[SPEAKER_CHECK_ARCHIVE_REQUIRED] Synthetic check archive full",
      ),
    );
  api.archiveSpeakerModelChecks = vi.fn().mockResolvedValue(1);
  render(<App api={api} />);
  await ready();
  await userEvent.click(screen.getByRole("button", { name: "Einstellungen" }));
  const summary = screen.getByText("Sprecherfunktionen · optional");
  await userEvent.click(summary);
  await userEvent.selectOptions(
    screen.getByLabelText("Installiertes Modell für die Sprecherprüfung"),
    "installed-stream",
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Sprecherunterstützung prüfen" }),
  );
  await screen.findByText("Synthetic check archive full");
  await userEvent.click(summary);
  const archive = screen.getByRole("button", {
    name: "Prüfnachweise archivieren",
  });
  expect(archive.closest("details")).toBeNull();
  expect(api.archiveSpeakerModelChecks).not.toHaveBeenCalled();
  await userEvent.click(archive);
  expect(api.archiveSpeakerModelChecks).toHaveBeenCalledOnce();
  const notice = await screen.findByText(
    "Prüfnachweise archiviert und erhalten. Bitte die Modellprüfung erneut starten.",
  );
  expect(notice.closest("details")).toBeNull();
  expect(summary.closest("details")!.open).toBe(false);
});

describe("migration frontend boundaries (synthetic IPC)", () => {
  it("selects the default microphone and passes both audio tracks", async () => {
    const api = fakeApi();
    render(<App api={api} />);
    await ready();
    expect(
      screen.getByLabelText("Mikrofon zusätzlich aufnehmen"),
    ).toHaveProperty("checked", true);
    await userEvent.click(
      screen.getByRole("button", { name: "Aufnahme starten" }),
    );
    await waitFor(() =>
      expect(api.startMeeting).toHaveBeenCalledWith({
        title: expect.stringMatching(
          /^Meeting \d{1,2}\.\d{1,2}\.\d{4} \d{2}:\d{2}$/,
        ),
        modelId: "installed-stream",
        systemDeviceId: "out",
        microphoneDeviceId: "mic",
      }),
    );
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      expect.stringContaining("Synthetic capture unavailable"),
    );
  });
  it("remembers when the microphone is turned off", async () => {
    const api = fakeApi();
    const { unmount } = render(<App api={api} />);
    await ready();
    const microphone = screen.getByLabelText("Mikrofon zusätzlich aufnehmen");
    expect(microphone).toHaveProperty("checked", true);
    await userEvent.click(microphone);
    unmount();

    render(<App api={api} />);
    await ready();
    expect(
      screen.getByLabelText("Mikrofon zusätzlich aufnehmen"),
    ).toHaveProperty("checked", false);
    await userEvent.click(
      screen.getByRole("button", { name: "Aufnahme starten" }),
    );
    expect(api.startMeeting).toHaveBeenCalledWith(
      expect.objectContaining({
        systemDeviceId: "out",
        microphoneDeviceId: null,
      }),
    );
  });
  it.each([{}, { microphone: true }])(
    "starts with computer audio alone when no microphone is available (%j)",
    async (preferences) => {
      const preferencesKey = "handymeeting.rust.beta.preferences.v1";
      localStorage.setItem(preferencesKey, JSON.stringify(preferences));
      const api = fakeApi();
      vi.mocked(api.listDevices).mockResolvedValue([
        { id: "out", name: "Output", source: "system", isDefault: true },
      ]);
      render(<App api={api} />);
      await ready();
      const microphone = screen.getByLabelText("Mikrofon zusätzlich aufnehmen");
      expect(microphone).toHaveProperty("checked", false);
      expect(microphone).toHaveProperty("disabled", true);
      await userEvent.click(
        screen.getByRole("button", { name: "Aufnahme starten" }),
      );
      expect(api.startMeeting).toHaveBeenCalledWith(
        expect.objectContaining({
          systemDeviceId: "out",
          microphoneDeviceId: null,
        }),
      );
      expect(JSON.parse(localStorage.getItem(preferencesKey)!)).toEqual(
        expect.objectContaining(preferences),
      );
      vi.mocked(api.listDevices).mockResolvedValue([
        { id: "out", name: "Output", source: "system", isDefault: true },
        {
          id: "mic",
          name: "Microphone",
          source: "microphone",
          isDefault: true,
        },
      ]);
      fireEvent.focus(window);
      await waitFor(() => {
        expect(microphone).toHaveProperty("checked", true);
        expect(microphone).toHaveProperty("disabled", false);
      });
    },
  );
  it("uses refreshed Handy models and excludes unsupported/uninstalled entries", async () => {
    const api = fakeApi();
    const refreshed = {
      executable: "new-handy",
      version: "1.0.0",
      models: [
        {
          id: "new",
          name: "New upstream model",
          isDownloaded: true,
          supportsStreaming: true,
        },
        {
          id: "parakeet-ultra",
          name: "Parakeet Ultra",
          isDownloaded: true,
          supportsStreaming: false,
          supportsBatch: true,
        },
        {
          id: "batch",
          name: "Batch only",
          isDownloaded: true,
          supportsStreaming: false,
        },
        {
          id: "missing",
          name: "Not installed",
          isDownloaded: false,
          supportsStreaming: true,
        },
      ],
    };
    // Like the backend, later status polls report the refreshed Handy models. A poll
    // with the old list could otherwise replace them on a slow runner.
    vi.mocked(api.refreshHandy).mockImplementation(async () => {
      vi.mocked(api.getAppState).mockResolvedValue({
        ...state(),
        handy: refreshed,
      });
      return refreshed;
    });
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Aktualisieren" }),
    );
    expect(
      await screen.findByRole("option", { name: "New upstream model" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("option", {
        name: "Parakeet Ultra · abschnittsweise",
      }),
    ).toBeTruthy();
    expect(screen.queryByRole("option", { name: "Batch only" })).toBeNull();
    expect(screen.queryByRole("option", { name: "Not installed" })).toBeNull();
    await waitFor(() =>
      expect(
        (
          screen.getByLabelText(
            "Erkennungsmodell aus Handy",
          ) as HTMLSelectElement
        ).value,
      ).toBe(""),
    );
    expect(
      await screen.findByText(
        /zuvor gewählte Modell ist nicht mehr installiert oder kompatibel/,
      ),
    ).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Aufnahme starten",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    await userEvent.selectOptions(
      screen.getByLabelText("Erkennungsmodell aus Handy"),
      "new",
    );
    expect(
      (screen.getByLabelText("Erkennungsmodell aus Handy") as HTMLSelectElement)
        .value,
    ).toBe("new");
  });
  it("refreshes new Handy models on focus with startup throttling and coalesced in-flight requests", async () => {
    let now = 100000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    let current = state();
    const api = fakeApi();
    vi.mocked(api.getAppState).mockImplementation(async () => current);
    let finishRefresh!: (value: NonNullable<AppState["handy"]>) => void;
    vi.mocked(api.refreshHandy).mockImplementation(
      () =>
        new Promise((resolve) => {
          finishRefresh = resolve;
        }),
    );
    render(<App api={api} />);
    await ready();
    fireEvent.focus(window);
    expect(api.refreshHandy).not.toHaveBeenCalled();
    now += 31000;
    fireEvent.focus(window);
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(1));
    now += 31000;
    for (let index = 0; index < 5; index++) fireEvent.focus(window);
    expect(api.refreshHandy).toHaveBeenCalledTimes(1);
    current = {
      ...current,
      handy: {
        ...current.handy!,
        models: [
          ...current.handy!.models,
          {
            id: "new-from-handy",
            name: "New model after Handy update",
            isDownloaded: true,
            supportsStreaming: true,
          },
        ],
      },
    };
    await act(async () => {
      finishRefresh(current.handy!);
    });
    expect(
      await screen.findByRole("option", {
        name: "New model after Handy update",
      }),
    ).toBeTruthy();
    expect(
      (screen.getByLabelText("Erkennungsmodell aus Handy") as HTMLSelectElement)
        .value,
    ).toBe("installed-stream");
    now += 1000;
    fireEvent.focus(window);
    expect(api.refreshHandy).toHaveBeenCalledTimes(1);
  });
  it.each(["success", "failure"] as const)(
    "checks a confirmed native selection after an overlapping automatic refresh ends with %s",
    async (automaticOutcome) => {
      let now = 100000;
      vi.spyOn(Date, "now").mockImplementation(() => now);
      const scheduleInterval = globalThis.setInterval;
      vi.spyOn(globalThis, "setInterval").mockImplementation(
        (handler, delay, ...args) =>
          delay === 1500
            ? (0 as unknown as ReturnType<typeof setInterval>)
            : scheduleInterval(handler, delay, ...args),
      );
      const nativePath = "/confirmed-other-install/Handy.exe";
      let current = state();
      let finishAutomatic!: (handy: NonNullable<AppState["handy"]>) => void;
      let failAutomatic!: (error: Error) => void;
      let finishChoice!: (path: string) => void;
      let finishNative!: (handy: NonNullable<AppState["handy"]>) => void;
      const automatic = new Promise<NonNullable<AppState["handy"]>>(
        (resolve, reject) => {
          finishAutomatic = resolve;
          failAutomatic = reject;
        },
      );
      const choice = new Promise<string>((resolve) => {
        finishChoice = resolve;
      });
      const nativeVerification = new Promise<NonNullable<AppState["handy"]>>(
        (resolve) => {
          finishNative = resolve;
        },
      );
      const selectedCatalogue = {
        ...state().handy!,
        executable: nativePath,
        models: [
          {
            id: "native-model",
            name: "Model from confirmed installation",
            isDownloaded: true,
            supportsStreaming: true,
          },
        ],
      };
      const api = fakeApi();
      api.chooseHandyExecutable = vi.fn(() => choice);
      vi.mocked(api.getAppState).mockImplementation(async () => current);
      vi.mocked(api.refreshHandy)
        .mockImplementationOnce(() => automatic)
        .mockImplementation(async (path) => {
          if (path !== nativePath)
            throw new Error("Synthetic wrong origin after native selection");
          return nativeVerification;
        });
      render(<App api={api} />);
      await ready();
      await userEvent.click(
        screen.getByRole("button", { name: "Einstellungen" }),
      );
      const choose = screen.getByRole("button", {
        name: "Handy-Programm auswählen …",
      });
      now += 31000;
      // Both actions begin before the pending automatic status read commits its
      // busy render. The native dialog itself stays pending independently.
      await act(async () => {
        fireEvent.focus(window);
        fireEvent.click(choose);
        await Promise.resolve();
      });
      expect(api.chooseHandyExecutable).toHaveBeenCalledOnce();
      await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(1));
      expect(api.refreshHandy).toHaveBeenNthCalledWith(1, null);
      await act(async () => finishChoice(nativePath));
      expect(api.refreshHandy).toHaveBeenCalledTimes(1);
      expect(
        (
          screen.getByLabelText(
            "Handy-Programmdatei (optional)",
          ) as HTMLInputElement
        ).value,
      ).toBe(nativePath);
      await userEvent.click(screen.getByRole("button", { name: "Meeting" }));
      const queuedStart = screen.getByRole("button", {
        name: "Bitte warten …",
      });
      expect((queuedStart as HTMLButtonElement).disabled).toBe(true);
      await userEvent.click(queuedStart);
      expect(api.startMeeting).not.toHaveBeenCalled();
      await act(async () => {
        if (automaticOutcome === "failure")
          failAutomatic(new Error("Synthetic older automatic query failed"));
        else finishAutomatic(current.handy!);
      });
      await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(2));
      expect(api.refreshHandy).toHaveBeenNthCalledWith(2, nativePath);
      const checkingStart = screen.getByRole("button", {
        name: "Bitte warten …",
      });
      expect((checkingStart as HTMLButtonElement).disabled).toBe(true);
      now += 31000;
      fireEvent.focus(window);
      expect(api.refreshHandy).toHaveBeenCalledTimes(2);
      await userEvent.click(
        screen.getByRole("button", { name: "Einstellungen" }),
      );
      expect(
        (
          screen.getByLabelText(
            "Handy-Programmdatei (optional)",
          ) as HTMLInputElement
        ).value,
      ).toBe(nativePath);
      await act(async () => {
        current = { ...current, handy: selectedCatalogue };
        finishNative(selectedCatalogue);
      });
      await userEvent.click(screen.getByRole("button", { name: "Meeting" }));
      await screen.findByRole("option", {
        name: "Model from confirmed installation",
      });
      expect(
        screen.queryByRole("option", { name: "Installed streaming model" }),
      ).toBeNull();
      expect(
        screen.queryByText("Synthetic older automatic query failed"),
      ).toBeNull();
      await userEvent.selectOptions(
        screen.getByLabelText("Erkennungsmodell aus Handy"),
        "native-model",
      );
      await ready();
      expect(current.handy?.executable).toBe(nativePath);
      expect(api.startMeeting).not.toHaveBeenCalled();
    },
  );
  it("retains active capture and blocks fallback when recording postpones a queued native choice", async () => {
    let now = 100000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    let periodicPoll!: () => void;
    const scheduleInterval = globalThis.setInterval;
    vi.spyOn(globalThis, "setInterval").mockImplementation(
      (handler, delay, ...args) => {
        if (delay === 1500 && typeof handler === "function") {
          periodicPoll = () => handler(...args);
          return 0 as unknown as ReturnType<typeof setInterval>;
        }
        return scheduleInterval(handler, delay, ...args);
      },
    );
    const nativePath = "/confirmed-later/Handy.exe";
    let current = state();
    let finishAutomatic!: (value: NonNullable<AppState["handy"]>) => void;
    let finishChoice!: (path: string) => void;
    const automatic = new Promise<NonNullable<AppState["handy"]>>((resolve) => {
      finishAutomatic = resolve;
    });
    const choice = new Promise<string>((resolve) => {
      finishChoice = resolve;
    });
    const api = fakeApi();
    api.chooseHandyExecutable = vi.fn(() => choice);
    vi.mocked(api.getAppState).mockImplementation(async () => current);
    vi.mocked(api.refreshHandy)
      .mockImplementationOnce(() => automatic)
      .mockImplementation(async (path) => {
        if (path === null) return state().handy!;
        if (path !== nativePath)
          throw new Error("Synthetic old-origin fallback");
        current = {
          ...current,
          handy: { ...state().handy!, executable: nativePath },
        };
        return current.handy!;
      });
    vi.mocked(api.stopMeeting).mockImplementation(async () => {
      current = { ...current, activeMeeting: meeting, recording: false };
      return meeting;
    });
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Einstellungen" }),
    );
    now += 31000;
    await act(async () => {
      fireEvent.focus(window);
      fireEvent.click(
        screen.getByRole("button", { name: "Handy-Programm auswählen …" }),
      );
      await Promise.resolve();
    });
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(1));
    await act(async () => finishChoice(nativePath));
    current = {
      ...current,
      recording: true,
      activeMeeting: { ...meeting, status: "recording" },
    };
    await act(async () => finishAutomatic(current.handy!));
    await act(async () => periodicPoll());
    await userEvent.click(screen.getByRole("button", { name: "Meeting" }));
    const stop = await screen.findByRole("button", { name: "Meeting beenden" });
    expect((stop as HTMLButtonElement).disabled).toBe(false);
    expect(api.stopMeeting).not.toHaveBeenCalled();
    expect(api.refreshHandy).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      expect.stringContaining(
        "gewählte Programmdatei wurde noch nicht geprüft",
      ),
    );
    await userEvent.click(stop);
    const start = await screen.findByRole("button", {
      name: "Aufnahme starten",
    });
    await userEvent.click(start);
    expect((start as HTMLButtonElement).disabled).toBe(true);
    expect(api.startMeeting).not.toHaveBeenCalled();
    now += 31000;
    await act(async () => {
      fireEvent.focus(window);
      await Promise.resolve();
    });
    expect(api.refreshHandy).toHaveBeenCalledTimes(1);
    expect((start as HTMLButtonElement).disabled).toBe(true);
    await userEvent.click(start);
    expect(api.startMeeting).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: "Aktualisieren" }),
    );
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(2));
    expect(api.refreshHandy).toHaveBeenNthCalledWith(2, nativePath);
    await userEvent.selectOptions(
      screen.getByLabelText("Erkennungsmodell aus Handy"),
      "installed-stream",
    );
    await ready();
    expect(api.startMeeting).not.toHaveBeenCalled();
  });
  it("blocks the old catalogue when a confirmed native choice cannot read fresh state", async () => {
    let now = 100000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    const api = fakeApi();
    const path = "/confirmed-state-unavailable/Handy.exe";
    // Arm the fault only after this native choice is actually confirmed. An idle
    // poll while userEvent prepares the click must not consume this failure.
    api.chooseHandyExecutable = vi.fn().mockImplementation(async () => {
      vi.mocked(api.getAppState).mockRejectedValueOnce(
        new Error("Synthetic state unavailable before explicit verification"),
      );
      return path;
    });
    vi.mocked(api.refreshHandy).mockImplementation(async (explicit) => ({
      ...state().handy!,
      executable: explicit === path ? path : state().handy!.executable,
    }));
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Einstellungen" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Handy-Programm auswählen …" }),
    );
    await screen.findByText(
      "Synthetic state unavailable before explicit verification",
    );
    expect(api.chooseHandyExecutable).toHaveBeenCalledOnce();
    expect(api.refreshHandy).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Meeting" }));
    const start = screen.getByRole("button", { name: "Aufnahme starten" });
    await userEvent.click(start);
    expect(api.startMeeting).not.toHaveBeenCalled();
    expect((start as HTMLButtonElement).disabled).toBe(true);
    now += 31000;
    await act(async () => {
      fireEvent.focus(window);
      await Promise.resolve();
    });
    expect(api.refreshHandy).not.toHaveBeenCalled();
    expect((start as HTMLButtonElement).disabled).toBe(true);
    await userEvent.click(start);
    expect(api.startMeeting).not.toHaveBeenCalled();
    expect(
      screen.queryByRole("option", { name: "Installed streaming model" }),
    ).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: "Aktualisieren" }),
    );
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledWith(path));
    await userEvent.selectOptions(
      screen.getByLabelText("Erkennungsmodell aus Handy"),
      "installed-stream",
    );
    await ready();
    expect(api.startMeeting).not.toHaveBeenCalled();
  });
  it("keeps automatically discovered Handy origin through a focus refresh and a later recording start", async () => {
    let now = 100000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    let current: AppState = {
      ...state(),
      handy: { ...state().handy!, executable: "/auto-discovered/Handy.exe" },
    };
    let discoveredOrigin = true;
    const api = fakeApi();
    vi.mocked(api.getAppState).mockImplementation(async () => current);
    // Synthetic contract of the native origin boundary: displaying a discovered
    // path does not grant explicit-file permission. A rejected explicit request
    // clears its discovery origin, so a later start must not silently fall back.
    vi.mocked(api.refreshHandy).mockImplementation(async (explicit) => {
      if (explicit !== null || !discoveredOrigin) {
        discoveredOrigin = false;
        current = {
          ...current,
          handy: {
            ...current.handy!,
            models: [],
            error: "Synthetic native selection required",
          },
        };
        throw new Error("Synthetic native selection required");
      }
      return current.handy!;
    });
    vi.mocked(api.startMeeting).mockImplementation(async () => {
      if (!discoveredOrigin)
        throw new Error("Synthetic native selection required before capture");
      const active = { ...meeting, status: "recording" };
      current = { ...current, activeMeeting: active, recording: true };
      return active;
    });
    render(<App api={api} />);
    await ready();
    now += 31000;
    fireEvent.focus(window);
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(1));
    expect(api.refreshHandy).toHaveBeenCalledWith(null);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Aufnahme starten" }),
    );
    expect(
      await screen.findByRole("button", { name: "Meeting beenden" }),
    ).toBeTruthy();
    expect(api.startMeeting).toHaveBeenCalledOnce();
    expect(discoveredOrigin).toBe(true);
    expect(
      screen.queryByText("Synthetic native selection required"),
    ).toBeNull();
  });
  it("keeps a manual native choice explicit and shows changed-file errors without automatic fallback", async () => {
    // Hold the regular status interval: the rejected refresh must invalidate the
    // visible start state immediately, without waiting for that later poll.
    const scheduleInterval = globalThis.setInterval;
    vi.spyOn(globalThis, "setInterval").mockImplementation(
      (handler, delay, ...args) =>
        delay === 1500
          ? (0 as unknown as ReturnType<typeof setInterval>)
          : scheduleInterval(handler, delay, ...args),
    );
    let now = 100000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    const nativePath = "/native-selected/Handy.exe";
    let unchanged = true;
    let manualChoice = false;
    let invalidCatalogueRead = false;
    let current = state();
    const api = fakeApi();
    api.chooseHandyExecutable = vi.fn().mockResolvedValue(nativePath);
    vi.mocked(api.getAppState).mockImplementation(async () => {
      if (current.handy?.error) invalidCatalogueRead = true;
      return current;
    });
    vi.mocked(api.refreshHandy).mockImplementation(async (explicit) => {
      if (explicit === nativePath) manualChoice = true;
      if (!manualChoice || !unchanged) {
        current = {
          ...current,
          handy: {
            executable: nativePath,
            version: null,
            models: [],
            error: "Synthetic selected program changed; choose it again",
          },
        };
        throw new Error("Synthetic selected program changed; choose it again");
      }
      current = {
        ...current,
        handy: { ...state().handy!, executable: nativePath },
      };
      return current.handy!;
    });
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Einstellungen" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Handy-Programm auswählen …" }),
    );
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(1));
    expect(api.refreshHandy).toHaveBeenNthCalledWith(1, nativePath);
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Aktualisieren",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    unchanged = false;
    await userEvent.click(
      screen.getByRole("button", { name: "Aktualisieren" }),
    );
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(2));
    expect(api.refreshHandy).toHaveBeenNthCalledWith(2, nativePath);
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      expect.stringContaining(
        "Synthetic selected program changed; choose it again",
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Meeting" }));
    const start = screen.getByRole("button", {
      name: "Aufnahme starten",
    }) as HTMLButtonElement;
    await userEvent.click(start);
    expect(api.startMeeting).not.toHaveBeenCalled();
    expect(invalidCatalogueRead).toBe(true);
    expect(start.disabled).toBe(true);
    expect(screen.getByRole("alert")).toHaveProperty(
      "textContent",
      expect.stringContaining(
        "Synthetic selected program changed; choose it again",
      ),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Einstellungen" }),
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Aktualisieren",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    now += 31000;
    fireEvent.focus(window);
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(3));
    expect(api.refreshHandy).toHaveBeenNthCalledWith(3, null);
    expect(current.handy?.executable).toBe(nativePath);
    expect(current.handy?.models).toEqual([]);
    expect(screen.getByRole("alert")).toHaveProperty(
      "textContent",
      expect.stringContaining(
        "Synthetic selected program changed; choose it again",
      ),
    );
    expect(api.startMeeting).not.toHaveBeenCalled();
  });
  it.each(["stale catalogue", "active recording"] as const)(
    "keeps a failed catalogue blocked when its reread loses to a pending periodic poll returning $0",
    async (lateState) => {
      let periodicPoll!: () => void;
      const scheduleInterval = globalThis.setInterval;
      vi.spyOn(globalThis, "setInterval").mockImplementation(
        (handler, delay, ...args) => {
          if (delay === 1500 && typeof handler === "function") {
            periodicPoll = () => handler(...args);
            return 0 as unknown as ReturnType<typeof setInterval>;
          }
          return scheduleInterval(handler, delay, ...args);
        },
      );
      const healthy = state();
      const failed: AppState = {
        ...healthy,
        handy: {
          ...healthy.handy!,
          models: [],
          error: "Synthetic confirmed program changed; select it again",
        },
      };
      let current = healthy;
      let resolveReread!: (value: AppState) => void;
      let resolvePeriodic!: (value: AppState) => void;
      const reread = new Promise<AppState>((resolve) => {
        resolveReread = resolve;
      });
      const laterPoll = new Promise<AppState>((resolve) => {
        resolvePeriodic = resolve;
      });
      const queued: { name: string; value: Promise<AppState> }[] = [];
      const requested: string[] = [];
      const api = fakeApi();
      vi.mocked(api.getAppState).mockImplementation(async () => {
        const next = queued.shift();
        if (next) {
          requested.push(next.name);
          return next.value;
        }
        return current;
      });
      vi.mocked(api.refreshHandy).mockImplementationOnce(async () => {
        current = failed;
        queued.push({ name: "failed-reread", value: reread });
        throw new Error(failed.handy!.error!);
      });
      render(<App api={api} />);
      await ready();
      await userEvent.click(
        screen.getByRole("button", { name: "Aktualisieren" }),
      );
      await waitFor(() => expect(requested).toContain("failed-reread"));
      queued.push({ name: "periodic", value: laterPoll });
      act(() => periodicPoll());
      await waitFor(() => expect(requested).toContain("periodic"));
      await act(async () => resolveReread(failed));
      const start = await screen.findByRole("button", {
        name: "Aufnahme starten",
      });
      await userEvent.click(start);
      expect(api.startMeeting).not.toHaveBeenCalled();
      expect((start as HTMLButtonElement).disabled).toBe(true);
      const late =
        lateState === "active recording"
          ? {
              ...healthy,
              recording: true,
              activeMeeting: { ...meeting, status: "recording" },
            }
          : healthy;
      await act(async () => resolvePeriodic(late));
      expect(screen.getByRole("alert")).toHaveProperty(
        "textContent",
        expect.stringContaining(failed.handy!.error!),
      );
      if (lateState === "active recording") {
        const stop = await screen.findByRole("button", {
          name: "Meeting beenden",
        });
        expect((stop as HTMLButtonElement).disabled).toBe(false);
        expect(api.stopMeeting).not.toHaveBeenCalled();
      } else {
        await userEvent.click(
          screen.getByRole("button", { name: "Aufnahme starten" }),
        );
        expect(api.startMeeting).not.toHaveBeenCalled();
        expect(
          (
            screen.getByRole("button", {
              name: "Aufnahme starten",
            }) as HTMLButtonElement
          ).disabled,
        ).toBe(true);
        expect(
          screen.queryByRole("option", { name: "Installed streaming model" }),
        ).toBeNull();
        current = healthy;
        vi.mocked(api.refreshHandy).mockResolvedValue(healthy.handy!);
        await userEvent.click(
          screen.getByRole("button", { name: "Aktualisieren" }),
        );
        await screen.findByRole("option", {
          name: "Installed streaming model",
        });
        await userEvent.selectOptions(
          screen.getByLabelText("Erkennungsmodell aus Handy"),
          "installed-stream",
        );
        await ready();
        expect(api.startMeeting).not.toHaveBeenCalled();
      }
    },
  );
  it("rechecks fresh backend state before a focus refresh and retries after catch-up ends", async () => {
    let now = 100000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    let current = state();
    const api = fakeApi();
    vi.mocked(api.getAppState).mockImplementation(async () => current);
    render(<App api={api} />);
    await ready();
    current = {
      ...current,
      activeMeeting: { ...meeting, status: "catching-up" },
      recording: false,
    };
    now += 31000;
    const previous = vi.mocked(api.getAppState).mock.calls.length;
    fireEvent.focus(window);
    await waitFor(() =>
      expect(vi.mocked(api.getAppState).mock.calls.length).toBeGreaterThan(
        previous,
      ),
    );
    expect(api.refreshHandy).not.toHaveBeenCalled();
    current = state();
    now += 1;
    fireEvent.focus(window);
    await waitFor(() => expect(api.refreshHandy).toHaveBeenCalledTimes(1));
  });
  it("does not duplicate the backend startup model query before its first catalogue arrives", async () => {
    let now = 100000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    const api = fakeApi();
    vi.mocked(api.getAppState).mockResolvedValue({ ...state(), handy: null });
    render(<App api={api} />);
    await waitFor(() => expect(api.listDevices).toHaveBeenCalled());
    now += 45000;
    fireEvent.focus(window);
    await act(async () => {
      await Promise.resolve();
    });
    expect(api.refreshHandy).not.toHaveBeenCalled();
  });
  it.each([
    {
      name: "recording",
      recording: true,
      transitioning: false,
      status: "recording",
    },
    {
      name: "ending",
      recording: false,
      transitioning: true,
      status: "recorded",
    },
    {
      name: "ASR catch-up",
      recording: false,
      transitioning: false,
      status: "catching-up",
    },
  ])(
    "does not query Handy on focus while $name is active",
    async ({ recording, transitioning, status }) => {
      let now = 100000;
      vi.spyOn(Date, "now").mockImplementation(() => now);
      const api = fakeApi();
      vi.mocked(api.getAppState).mockResolvedValue({
        ...state({ ...meeting, status }),
        recording,
        transitioning,
      });
      render(<App api={api} />);
      await waitFor(() => expect(api.listDevices).toHaveBeenCalled());
      now += 31000;
      fireEvent.focus(window);
      await act(async () => {
        await Promise.resolve();
      });
      expect(api.refreshHandy).not.toHaveBeenCalled();
    },
  );
  it("does not pretend recording works when the desktop core is missing", async () => {
    const api = fakeApi();
    vi.mocked(api.getAppState).mockRejectedValue("Synthetic disconnected core");
    render(<App api={api} />);
    expect(
      await screen.findByText("Desktop-Verbindung nicht verfügbar"),
    ).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Aufnahme starten",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(api.startMeeting).not.toHaveBeenCalled();
  });
  it("displays and copies all transcript sections beyond the bounded poll preview", async () => {
    const long: Meeting = {
      ...meeting,
      status: "recording",
      parts: Array.from({ length: 70 }, (_, index) => ({
        ...meeting.parts[0],
        file: "system-" + index + ".wav",
        startSample: index * 480000,
        text: "Synthetic section " + index,
      })),
    };
    const api = fakeApi({
      ...long,
      parts: long.parts.slice(-16),
      livePreviewLimited: true,
    });
    vi.mocked(api.getMeeting).mockResolvedValue(long);
    vi.mocked(api.getAppState).mockResolvedValue({
      ...state({
        ...long,
        parts: long.parts.slice(-16),
        livePreviewLimited: true,
      }),
      recording: true,
    });
    render(<App api={api} />);
    await screen.findByRole("button", { name: "Meeting beenden" });
    await screen.findByText("Synthetic section 0");
    await userEvent.click(
      screen.getByRole("button", { name: "Transkript kopieren" }),
    );
    await waitFor(() => expect(api.copyText).toHaveBeenCalled());
    expect(api.getMeeting).toHaveBeenCalledWith(long.id);
    const text = vi.mocked(api.copyText).mock.calls[0][0];
    expect(text).toContain("Synthetic section 0");
    expect(text).toContain("Synthetic section 69");
    expect(text).toBe(renderTranscript(long, "de"));
  });
  it("handles pending null text and reports only globally confirmed counters", async () => {
    const pending: Meeting = {
      ...meeting,
      status: "recorded",
      livePreviewLimited: true,
      pendingAudioMs: 210000,
      savedAudioMs: { system: 1000, microphone: 0 },
      parts: [
        {
          ...meeting.parts[0],
          text: null,
          status: "pending",
          confirmedSampleCount: 16000,
        },
      ],
    };
    const api = fakeApi(pending);
    render(<App api={api} />);
    await ready();
    expect(screen.getByText("03:30")).toBeTruthy();
    expect(screen.getByText("00:01")).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Transkript kopieren",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    await userEvent.click(
      screen.getByRole("button", { name: "Offene Abschnitte nachholen" }),
    );
    await waitFor(() =>
      expect(api.resumeMeeting).toHaveBeenCalledWith(pending.id),
    );
  });
  it("keeps a fully opened finished history transcript when bounded status polling continues", async () => {
    const finished: Meeting = {
      ...meeting,
      parts: Array.from({ length: 25 }, (_, index) => ({
        ...meeting.parts[0],
        file: `system-${index}.wav`,
        startSample: index * 480000,
        text: `Finished section ${index}`,
      })),
    };
    const api = fakeApi();
    vi.mocked(api.getMeeting).mockResolvedValue(finished);
    vi.mocked(api.listMeetings).mockResolvedValue([finished]);
    render(<App api={api} />);
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "Historie" }));
    await userEvent.click(
      await screen.findByRole("button", { name: "Öffnen" }),
    );
    expect(await screen.findByText("Finished section 0")).toBeTruthy();
    expect(document.querySelectorAll(".transcript-part")).toHaveLength(25);
    const previousCalls = vi.mocked(api.getAppState).mock.calls.length;
    vi.mocked(api.getAppState).mockResolvedValue(
      state({
        ...finished,
        parts: finished.parts.slice(-16),
        livePreviewLimited: true,
      }),
    );
    await waitFor(
      () =>
        expect(vi.mocked(api.getAppState).mock.calls.length).toBeGreaterThan(
          previousCalls,
        ),
      { timeout: 2500 },
    );
    expect(screen.getByText("Finished section 0")).toBeTruthy();
    expect(screen.getByText("Finished section 24")).toBeTruthy();
    expect(document.querySelectorAll(".transcript-part")).toHaveLength(25);
  });
  it("keeps the explicit ending state while stop waits and status polling reports no active recorder", async () => {
    const active: Meeting = { ...meeting, status: "recording" };
    const api = fakeApi(active);
    vi.mocked(api.getAppState).mockResolvedValue({
      ...state(active),
      recording: true,
    });
    let finishStop!: (value: Meeting) => void;
    vi.mocked(api.stopMeeting).mockImplementation(
      () =>
        new Promise<Meeting>((resolve) => {
          finishStop = resolve;
        }),
    );
    render(<App api={api} />);
    await screen.findByRole("button", { name: "Meeting beenden" });
    await userEvent.click(
      screen.getByRole("button", { name: "Meeting beenden" }),
    );
    expect(
      await screen.findByText(
        /Meeting wird beendet\. Gespeichertes Audio bleibt erhalten/,
      ),
    ).toBeTruthy();
    const before = vi.mocked(api.getAppState).mock.calls.length;
    vi.mocked(api.getAppState).mockResolvedValue({
      ...state({ ...active, status: "catching-up", livePreviewLimited: true }),
      recording: false,
      transitioning: true,
    });
    await waitFor(
      () =>
        expect(vi.mocked(api.getAppState).mock.calls.length).toBeGreaterThan(
          before,
        ),
      { timeout: 2500 },
    );
    expect(screen.queryByText("Bereit zur Aufnahme")).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Aufnahme starten" }),
    ).toBeNull();
    expect(
      (
        screen.getByRole("button", {
          name: "Aufnahme wird beendet …",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    vi.mocked(api.getAppState).mockResolvedValue({
      ...state(meeting),
      recording: false,
      transitioning: false,
    });
    await act(async () => {
      finishStop(meeting);
    });
    await ready();
    expect(
      screen.queryByText(
        /Meeting wird beendet\. Gespeichertes Audio bleibt erhalten/,
      ),
    ).toBeNull();
  });
  it("disables capture readiness during a backend transition discovered at bootstrap", async () => {
    const api = fakeApi();
    vi.mocked(api.getAppState).mockResolvedValue({
      ...state(),
      transitioning: true,
    });
    render(<App api={api} />);
    await waitFor(() => expect(api.listDevices).toHaveBeenCalled());
    expect(screen.queryByText("Bereit zur Aufnahme")).toBeNull();
    const button = screen.getByRole("button", {
      name: "Bitte warten …",
    }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(api.startMeeting).not.toHaveBeenCalled();
  });
  it("does not offer in-place recognition of read-only legacy sessions", async () => {
    const api = fakeApi({ ...meeting, status: "recorded", readOnly: true });
    render(<App api={api} />);
    await ready();
    expect(
      screen.queryByRole("button", { name: "Offene Abschnitte nachholen" }),
    ).toBeNull();
  });
  it("preserves unreadable saved templates instead of silently replacing them", async () => {
    const broken = '{"privateOriginal":"keep me"}';
    localStorage.setItem("handymeeting.rust.beta.templates.v1", broken);
    const api = fakeApi(meeting);
    vi.mocked(api.listTemplates).mockRejectedValue(
      "Synthetic corrupt native library",
    );
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Nachbearbeitung" }),
    );
    expect(
      await screen.findByText(/gespeicherten Vorlagen sind nicht lesbar/),
    ).toBeTruthy();
    expect(localStorage.getItem("handymeeting.rust.beta.templates.v1")).toBe(
      broken,
    );
    expect(
      (
        screen.getByRole("button", {
          name: "Analyseschritt hinzufügen",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });
  it("requires fresh explicit text consent for cloud generation and keeps keys out of storage", async () => {
    const api = fakeApi(meeting);
    vi.mocked(api.generateNotes).mockRejectedValue(
      "Synthetic provider failure",
    );
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Nachbearbeitung" }),
    );
    await userEvent.selectOptions(
      screen.getByLabelText("Verarbeitung"),
      "cloud",
    );
    await userEvent.type(
      screen.getByLabelText("Textmodell"),
      "synthetic-text-model",
    );
    fireEvent.change(
      screen.getByLabelText("Schlüssel nur für diese App-Sitzung"),
      { target: { value: "synthetic-secret-do-not-save" } },
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Auswertung starten" }),
    );
    expect(await screen.findByRole("dialog")).toBeTruthy();
    expect(api.generateNotes).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Abbrechen" }));
    expect(api.generateNotes).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: "Auswertung starten" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Text senden und auswerten" }),
    );
    await waitFor(() =>
      expect(api.generateNotes).toHaveBeenCalledWith(
        expect.objectContaining({
          mode: "cloud",
          cloudConsent: true,
          apiKey: "synthetic-secret-do-not-save",
          meetingId: meeting.id,
        }),
      ),
    );
    expect(
      Array.from({ length: localStorage.length }, (_, index) =>
        localStorage.getItem(localStorage.key(index)!),
      ).join("\n"),
    ).not.toContain("synthetic-secret");
    await screen.findByText("Synthetic provider failure");
    await userEvent.click(
      screen.getByRole("button", { name: "Auswertung starten" }),
    );
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(api.generateNotes).toHaveBeenCalledTimes(1);
  });
  it("renders model text literally and opens only matching verbatim evidence", async () => {
    const template = defaultTemplate();
    const result: AnalysisResult = {
      id: "result",
      meetingId: meeting.id,
      createdAt: "2026-10-25T12:01:00Z",
      template,
      sourceHash: "synthetic-hash",
      sections: [
        {
          id: "summary",
          name: "Summary",
          kind: "summary",
          items: [
            {
              text: '<img src=x onerror="alert(1)">',
              owner: null,
              due: null,
              evidence: [{ sourceId: "S00001", quote: "Anna übernimmt" }],
            },
          ],
        },
      ],
      sources: [
        {
          id: "S00001",
          source: "system",
          startMs: 0,
          endMs: 30000,
          text: meeting.parts[0].text!,
        },
      ],
    };
    const api = fakeApi(meeting);
    vi.mocked(api.listAnalyses).mockResolvedValue({
      analyses: [result],
      skipped: [],
    });
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Nachbearbeitung" }),
    );
    expect(
      await screen.findByText('<img src=x onerror="alert(1)">'),
    ).toBeTruthy();
    expect(document.querySelector(".analysis-body img")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: /S00001/ }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Anna übernimmt").tagName).toBe("MARK");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  it("adds and edits a custom analysis without modifying saved-result instructions", async () => {
    const api = fakeApi(meeting);
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Nachbearbeitung" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Analyseschritt hinzufügen" }),
    );
    await userEvent.type(screen.getByLabelText("Name des Tabs"), "Risiken");
    await userEvent.type(
      screen.getByLabelText("Arbeitsauftrag"),
      "Extrahiere ausdrücklich genannte Risiken mit Originalzitaten.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Speichern" }));
    await waitFor(() =>
      expect(
        screen
          .getByRole("tab", { name: "Risiken" })
          .getAttribute("aria-selected"),
      ).toBe("true"),
    );
    const saved = vi.mocked(api.saveTemplates).mock.calls.at(-1)![0].templates;
    expect(saved[0].steps).toHaveLength(4);
    expect(saved[0].steps[3].prompt).toContain("Originalzitaten");
  });
  it("restores focus and traps keyboard navigation in the instruction editor", async () => {
    render(<App api={fakeApi(meeting)} />);
    await ready();
    await userEvent.click(
      screen.getByRole("button", { name: "Nachbearbeitung" }),
    );
    const opener = screen.getByRole("button", {
      name: "Analyseschritt hinzufügen",
    });
    await userEvent.click(opener);
    expect(document.activeElement).toBe(screen.getByLabelText("Name des Tabs"));
    await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
    expect(document.activeElement).toBe(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "Schließen",
      }),
    );
    await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
    expect(document.activeElement).toBe(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "Abbrechen",
      }),
    );
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});
describe("template structure", () => {
  it("rejects duplicate IDs and incomplete instructions", () => {
    const template = defaultTemplate();
    expect(validateLibrary([template])).toBe(true);
    expect(validateLibrary([template, template])).toBe(false);
    expect(
      validateLibrary([
        { ...template, steps: [{ ...template.steps[0], prompt: "" }] },
      ]),
    ).toBe(false);
  });
});

describe("workspace navigation (synthetic IPC)", () => {
  async function openMeetingBackup(title: string) {
    await userEvent.click(screen.getByRole("button", { name: "Historie" }));
    const row = (await screen.findByText(title)).closest(
      ".session-row",
    ) as HTMLElement;
    await userEvent.click(within(row).getByRole("button", { name: "Öffnen" }));
    await userEvent.click(
      await screen.findByText("Verschlüsselte Kopie erstellen", {
        selector: "summary",
      }),
    );
  }

  async function submitMeetingBackup() {
    fireEvent.change(screen.getByLabelText("Passphrase"), {
      target: { value: "synthetic passphrase" },
    });
    fireEvent.change(screen.getByLabelText("Passphrase wiederholen"), {
      target: { value: "synthetic passphrase" },
    });
    await userEvent.click(
      screen.getByRole("button", { name: "Verschlüsselte Kopie erstellen" }),
    );
  }

  it("preserves a delayed backup error only for its meeting until a valid retry", async () => {
    const other = {
      ...meeting,
      id: "other-synthetic-session",
      title: "Another meeting",
    };
    const api = fakeApi();
    api.listMeetings = vi.fn().mockResolvedValue([meeting, other]);
    api.getMeeting = vi.fn(async (id: string) =>
      id === meeting.id ? meeting : other,
    );
    let rejectBackup!: (message: string) => void;
    const pending = new Promise<never>((_, reject) => {
      rejectBackup = reject;
    });
    let cancelRetry!: () => void;
    const retry = new Promise<null>((resolve) => {
      cancelRetry = () => resolve(null);
    });
    api.createEncryptedBackup = vi
      .fn()
      .mockReturnValueOnce(pending)
      .mockReturnValueOnce(retry);
    const path = "D:/synthetic/uncertain.hmvault";
    const message = `Could not synchronize the selected folder.\nThe app could not confirm the backup's durability at ${path}. If the file is present, verify it with the passphrase before relying on it. A recovery key was not delivered.`;
    render(<App api={api} />);
    await ready();
    await openMeetingBackup(meeting.title);
    await submitMeetingBackup();
    await openMeetingBackup(other.title);
    await act(async () => rejectBackup(message));
    expect(screen.queryByText(/uncertain\.hmvault/)).toBeNull();
    await openMeetingBackup(meeting.title);
    const alert = await screen.findByText(/uncertain\.hmvault/);
    expect(alert.getAttribute("role")).toBe("alert");
    expect(alert.textContent).toBe(message);
    await submitMeetingBackup();
    expect(screen.queryByText(/uncertain\.hmvault/)).toBeNull();
    await act(async () => cancelRetry());
    await openMeetingBackup(other.title);
    await openMeetingBackup(meeting.title);
    expect(screen.queryByText(/uncertain\.hmvault/)).toBeNull();
    expect(api.createEncryptedBackup).toHaveBeenCalledTimes(2);
  });

  it.each([false, true])(
    "prioritizes final backup failure when cancellation rejects last: %s",
    async (cancelLast) => {
      const api = fakeApi();
      let failBackup!: (message: string) => void;
      let failCancellation!: (message: string) => void;
      api.createEncryptedBackup = vi.fn(
        () =>
          new Promise<never>((_, reject) => {
            failBackup = reject;
          }),
      );
      api.cancelEncryptedBackup = vi.fn(
        () =>
          new Promise<never>((_, reject) => {
            failCancellation = reject;
          }),
      );
      const cancellationError = "Synthetic cancellation IPC failure";
      const backupError =
        "The app could not confirm the backup's durability at D:/synthetic/uncertain.hmvault. A recovery key was not delivered.";
      render(<App api={api} />);
      await ready();
      await openMeetingBackup(meeting.title);
      await submitMeetingBackup();
      await userEvent.click(screen.getByRole("button", { name: "Abbrechen" }));
      expect(api.cancelEncryptedBackup).toHaveBeenCalledOnce();
      if (cancelLast) {
        await act(async () => failBackup(backupError));
        expect(await screen.findByText(backupError)).toBeTruthy();
        await act(async () => failCancellation(cancellationError));
      } else {
        await act(async () => failCancellation(cancellationError));
        expect(await screen.findByText(cancellationError)).toBeTruthy();
        await act(async () => failBackup(backupError));
      }
      expect(screen.getByRole("alert").textContent).toBe(backupError);
      expect(screen.queryByText(cancellationError)).toBeNull();
    },
  );

  it("preserves a completed backup receipt only for its meeting after navigating away", async () => {
    const other = {
      ...meeting,
      id: "other-synthetic-session",
      title: "Another meeting",
    };
    const api = fakeApi();
    api.listMeetings = vi.fn().mockResolvedValue([meeting, other]);
    api.getMeeting = vi.fn(async (id: string) =>
      id === meeting.id ? meeting : other,
    );
    type Receipt = NonNullable<
      Awaited<ReturnType<NonNullable<DesktopApi["createEncryptedBackup"]>>>
    >;
    let finish!: (receipt: Receipt) => void;
    const pending = new Promise<Receipt>((resolve) => {
      finish = resolve;
    });
    api.createEncryptedBackup = vi.fn(() => pending);
    let finishCancel!: (accepted: boolean) => void;
    const cancelling = new Promise<boolean>((resolve) => {
      finishCancel = resolve;
    });
    api.cancelEncryptedBackup = vi.fn(() => cancelling);
    const key = "af".repeat(32);
    const file = "synthetic-a.hmvault";
    render(<App api={api} />);
    await ready();
    await openMeetingBackup(meeting.title);
    await submitMeetingBackup();
    expect(api.createEncryptedBackup).toHaveBeenCalledExactlyOnceWith(
      meeting.id,
      "synthetic passphrase",
    );
    await openMeetingBackup(other.title);
    expect(screen.queryByText("Vorgang läuft …")).toBeNull();
    await openMeetingBackup(meeting.title);
    expect(screen.getByText("Vorgang läuft …")).toBeTruthy();
    expect(
      screen
        .getByRole("button", { name: "Verschlüsselte Kopie erstellen" })
        .matches(":disabled"),
    ).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: "Abbrechen" }));
    expect(api.cancelEncryptedBackup).toHaveBeenCalledOnce();
    await openMeetingBackup(other.title);
    await openMeetingBackup(meeting.title);
    expect(screen.getByText("Abbruch angefordert …")).toBeTruthy();
    await act(async () => finishCancel(false));
    expect(screen.getByRole("button", { name: "Abbrechen" })).toHaveProperty(
      "disabled",
      false,
    );
    await openMeetingBackup(other.title);
    await act(async () =>
      finish({
        file,
        recoveryKey: key,
        summary: { meetingId: meeting.id, files: 1, bytes: 1 },
      }),
    );
    expect(screen.queryByDisplayValue(key)).toBeNull();
    expect(screen.queryByText(file)).toBeNull();
    await openMeetingBackup(meeting.title);
    expect(screen.getByDisplayValue(key)).toBeTruthy();
    expect(screen.getByText(file)).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", { name: "Schlüssel ausblenden" }),
    );
    await openMeetingBackup(other.title);
    await openMeetingBackup(meeting.title);
    expect(screen.queryByDisplayValue(key)).toBeNull();
    expect(screen.getByText(file)).toBeTruthy();
    expect(Object.values(localStorage).join(" ")).not.toContain(key);
  });

  it.each([false, true])(
    "discards a VIP-only answer after unpinning (return before save: %s)",
    async (returnBeforeSave) => {
      localStorage.setItem(
        "handymeeting.rust.live-notes.v1",
        JSON.stringify({
          endpoint: "http://127.0.0.1:1235/v1/",
          model: "synthetic-text-model",
        }),
      );
      let labelled = {
        ...meeting,
        userMetadata: {
          revision: 0,
          title: null,
          pinned: true,
          audioRemoved: false,
        },
      };
      const api = fakeApi();
      api.listMeetings = vi.fn(async () => [labelled]);
      api.getMeeting = vi.fn(async () => labelled);
      api.searchMeetings = vi.fn();
      let finishUnpin = () => {};
      const saveReady = returnBeforeSave
        ? new Promise<void>((resolve) => {
            finishUnpin = resolve;
          })
        : Promise.resolve();
      api.editMeetingMetadata = vi.fn(async (_id, revision, _title, pinned) => {
        await saveReady;
        labelled = {
          ...labelled,
          userMetadata: {
            ...labelled.userMetadata,
            revision: revision + 1,
            pinned,
          },
        };
        return labelled.userMetadata;
      });
      api.askMeetingHistory = vi
        .fn()
        .mockResolvedValueOnce({
          searchedPassages: 1,
          citations: [
            {
              meetingId: meeting.id,
              meetingTitle: meeting.title,
              sourceId: meeting.parts[0].file,
              startMs: 0,
              quote: meeting.parts[0].text,
            },
          ],
        })
        .mockResolvedValue({ searchedPassages: 0, citations: [] });
      render(<App api={api} />);
      await ready();
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: "Historie" }));
      await user.click(
        screen.getByRole("checkbox", { name: "Nur VIP-Meetings" }),
      );
      await user.click(screen.getByText("Meetings befragen"));
      await user.click(
        screen.getByText("Alle Meetings im gewählten Zeitraum und VIP-Filter"),
      );
      await user.click(
        screen.getByRole("checkbox", { name: /Synthetic meeting/ }),
      );
      await user.type(
        screen.getByLabelText("Frage an deine Meetings"),
        "Was wurde entschieden?",
      );
      await user.click(
        screen.getByRole("button", { name: "Lokal beantworten" }),
      );
      await user.click(
        await screen.findByRole("button", { name: "Originalstelle öffnen" }),
      );
      await user.click(
        await screen.findByRole("button", {
          name: "VIP · vor Aufräumen geschützt",
        }),
      );
      await waitFor(() =>
        expect(api.editMeetingMetadata).toHaveBeenCalledWith(
          meeting.id,
          0,
          null,
          false,
        ),
      );
      await user.click(screen.getByRole("button", { name: "Historie" }));
      expect(
        screen.getByRole("checkbox", { name: "Nur VIP-Meetings" }),
      ).toHaveProperty("checked", true);
      if (returnBeforeSave) {
        expect(
          await screen.findByRole("button", { name: "Originalstelle öffnen" }),
        ).toBeTruthy();
        await act(async () => finishUnpin());
        await waitFor(() =>
          expect(
            screen.queryByRole("button", { name: "Originalstelle öffnen" }),
          ).toBeNull(),
        );
        await user.click(
          screen.getByRole("button", { name: "Lokal beantworten" }),
        );
        expect(api.askMeetingHistory).toHaveBeenCalledTimes(2);
        expect(
          vi.mocked(api.askMeetingHistory!).mock.lastCall?.[0],
        ).not.toHaveProperty("meetingIds");
        return;
      }
      expect(
        screen.queryByRole("button", { name: "Originalstelle öffnen" }),
      ).toBeNull();
      await user.click(screen.getByText("Meetings befragen"));
      expect(screen.getByLabelText("Frage an deine Meetings")).toHaveProperty(
        "value",
        "",
      );
    },
  );

  it("keeps calendar preparation discoverable beside the closed capture setup", async () => {
    render(<App api={fakeApi(meeting)} />);
    await ready();
    const setup = document.querySelector<HTMLDetailsElement>(
      "details.setup-details",
    )!;
    if (setup.open) await userEvent.click(setup.querySelector("summary")!);
    expect(setup.open).toBe(false);
    const calendar = await screen.findByText(
      "Meeting aus Kalenderdatei vorbereiten",
      { selector: "summary" },
    );
    expect(calendar.closest(".setup-details")).toBeNull();
    await userEvent.click(calendar);
    expect(screen.getByLabelText("Kalenderdatei (.ics)")).toBeTruthy();
    expect(setup.open).toBe(false);
  });

  it("opens the verified original and keeps the answer when returning to check other citations", async () => {
    localStorage.setItem(
      "handymeeting.rust.live-notes.v1",
      JSON.stringify({
        endpoint: "http://127.0.0.1:1235/v1/",
        model: "synthetic-text-model",
      }),
    );
    const api = fakeApi(meeting);
    api.searchMeetings = vi.fn();
    api.askMeetingHistory = vi.fn().mockResolvedValue({
      searchedPassages: 1,
      citations: [
        {
          meetingId: meeting.id,
          meetingTitle: meeting.title,
          sourceId: meeting.parts[0].file,
          startMs: 0,
          quote: meeting.parts[0].text!,
        },
        {
          meetingId: meeting.id,
          meetingTitle: meeting.title,
          sourceId: meeting.parts[0].file,
          startMs: 0,
          quote: "Ein alter Wortlaut aus einem früheren Transkriptstand.",
        },
      ],
    });
    render(<App api={api} />);
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "Historie" }));
    await userEvent.click(screen.getByText("Meetings befragen"));
    await userEvent.type(
      screen.getByLabelText("Frage an deine Meetings"),
      "Was wurde entschieden?",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Lokal beantworten" }),
    );
    const links = await screen.findAllByRole("button", {
      name: "Originalstelle öffnen",
    });
    await userEvent.click(links[0]);
    await screen.findByRole("heading", { name: "Sitzungsdetails" });
    expect(document.querySelector("[data-highlighted]")?.textContent).toContain(
      meeting.parts[0].text,
    );
    await userEvent.click(screen.getByRole("button", { name: "Historie" }));
    const retained = screen.getAllByRole("button", {
      name: "Originalstelle öffnen",
    });
    await userEvent.click(retained[1]);
    expect(api.askMeetingHistory).toHaveBeenCalledTimes(1);
    await screen.findByText(
      "Die Originalstelle hat sich geändert. Stelle die Frage erneut.",
    );
    expect(
      screen.queryByRole("heading", { name: "Sitzungsdetails" }),
    ).toBeNull();
  });

  it("keeps an explicit search source visible after resetting the previous page", async () => {
    const consoleError = vi.spyOn(console, "error");
    const api = fakeApi(meeting);
    api.searchMeetings = vi.fn().mockResolvedValue({
      items: [
        {
          ...meeting,
          pinned: false,
          audioRemoved: false,
          sourceId: meeting.parts[0].file,
          startMs: 0,
          endMs: 30000,
          excerpt: meeting.parts[0].text,
        },
      ],
      total: 1,
      offset: 0,
      limit: 25,
    });
    const previousScroll = Element.prototype.scrollIntoView;
    const scroll = vi.fn(function (this: HTMLElement) {
      // Simulate the browser scrolling the workspace to this source, after the
      // new detail page mounts. A later page reset would hide it again.
      this.closest("main")!.scrollTop = 300;
    });
    Element.prototype.scrollIntoView = scroll;
    try {
      render(<App api={api} />);
      await ready();
      await userEvent.click(screen.getByRole("button", { name: "Historie" }));
      // Explicit source highlighting belongs to transcript search. Unfiltered
      // history now uses complete native summaries with no selected source.
      await userEvent.type(screen.getByRole("searchbox"), "Quelle");
      await screen.findByText(meeting.parts[0].text!);
      expect(api.searchMeetings).toHaveBeenCalledWith("Quelle", 0, 25);
      await userEvent.click(
        screen.getByRole("checkbox", { name: "Nur Treffer im Transkript" }),
      );
      await waitFor(() =>
        expect(api.searchMeetings).toHaveBeenLastCalledWith(
          "Quelle",
          0,
          25,
          expect.objectContaining({ pinnedOnly: false, transcriptOnly: true }),
        ),
      );
      const main = screen.getByRole("main");
      main.scrollTop = 747;
      await userEvent.click(
        await screen.findByRole("button", { name: "Öffnen" }),
      );
      await screen.findByRole("heading", { level: 1, name: "Sitzungsdetails" });
      expect(scroll).toHaveBeenCalledOnce();
      expect(main.querySelector("[data-highlighted]")?.textContent).toContain(
        meeting.parts[0].text,
      );
      expect(main.scrollTop).toBe(300);
      await userEvent.click(screen.getByRole("button", { name: "Historie" }));
      expect((screen.getByRole("searchbox") as HTMLInputElement).value).toBe(
        "Quelle",
      );
      expect(
        (
          screen.getByRole("checkbox", {
            name: "Nur Treffer im Transkript",
          }) as HTMLInputElement
        ).checked,
      ).toBe(true);
      expect(consoleError).not.toHaveBeenCalled();
    } finally {
      Element.prototype.scrollIntoView = previousScroll;
    }
  });

  it("opens each primary page at the top of its scroll container and moves keyboard focus into the new page", async () => {
    const user = userEvent.setup();
    render(<App api={fakeApi(meeting)} />);
    await ready();
    const main = screen.getByRole("main");
    const navigation = within(screen.getByRole("navigation"));
    for (const name of [
      "Einstellungen",
      "Nachbearbeitung",
      "Historie",
      "Meeting",
    ]) {
      // JSDOM has no layout engine; set the real workspace container's retained
      // offset directly, rather than mocking window scrolling.
      main.scrollTop = 747;
      main.scrollLeft = 30;
      const link = navigation.getByRole("button", { name });
      link.focus();
      await user.keyboard("{Enter}");
      const heading = within(main).getByRole("heading", { level: 1, name });
      expect(screen.getByRole("main")).toBe(main);
      expect(main.scrollTop).toBe(0);
      expect(main.scrollLeft).toBe(0);
      expect(document.activeElement).toBe(heading);
      expect(link.getAttribute("aria-current")).toBe("page");
      await user.tab();
      expect(main.contains(document.activeElement)).toBe(true);
    }
  });

  it("retains the reader position and input focus during background job updates", async () => {
    const api = fakeApi(meeting);
    vi.mocked(api.getAppState).mockResolvedValue({
      ...state(meeting),
      notesRunning: true,
    });
    const progress = {
      jobId: "synthetic-navigation-job",
      runId: "synthetic-retained-run",
      meetingId: meeting.id,
      status: "running" as const,
      stage: "requesting" as const,
      stepName: "Synthetic summary",
      stepIndex: 1,
      totalSteps: 3,
      batchIndex: 1,
      totalBatches: 1,
      confirmedTasks: 1,
      totalTasks: 3,
      elapsedMs: 1000,
      error: null,
      cancelRequested: false,
    };
    vi.mocked(api.getNotesProgress).mockResolvedValue(progress);
    render(<App api={api} />);
    await screen.findByRole("progressbar");
    await userEvent.click(
      screen.getByRole("button", { name: "Einstellungen" }),
    );
    const main = screen.getByRole("main");
    const theme = within(main).getByLabelText("Darstellung");
    theme.focus();
    main.scrollTop = 747;
    vi.mocked(api.getNotesProgress).mockResolvedValue({
      ...progress,
      confirmedTasks: 2,
      elapsedMs: 2000,
    });
    await waitFor(() =>
      expect(
        (screen.getByRole("progressbar") as HTMLProgressElement).value,
      ).toBe(2),
    );
    expect(main.scrollTop).toBe(747);
    expect(document.activeElement).toBe(theme);
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
      "Einstellungen",
    );
    expect(api.generateNotes).not.toHaveBeenCalled();
    expect(api.cancelNotes).not.toHaveBeenCalled();
  });
});

it("opens the full-window workspace on demand and switches views without stopping capture", async () => {
  // Keep Vite module transformation outside the rendering assertion.
  await import("./workspace/WorkspaceBridge");
  const active = {
    ...meeting,
    status: "recording",
    modelId: "handy-computer/parakeet-ultra-gguf/parakeet-ultra-Q8_0.gguf",
    pendingAudioMs: 15000,
  };
  const api = fakeApi(active);
  const ready = state(active);
  ready.handy!.models.push({
    id: active.modelId,
    name: "Parakeet Ultra",
    isDownloaded: true,
    supportsStreaming: false,
    supportsBatch: true,
  });
  api.getAppState = vi.fn().mockResolvedValue({
    ...ready,
    recording: true,
    error: "Mikrofon getrennt. Aufnahme beenden und Gerät neu verbinden.",
  });
  api.getLiveNotes = vi
    .fn()
    .mockResolvedValue({ status: "off", running: false });
  api.startLiveNotes = vi.fn();
  api.setWorkspaceContext = vi.fn();
  api.stopLiveNotes = vi.fn();
  api.refreshLiveNotes = vi.fn();
  api.setLiveNotesInterval = vi.fn();
  api.localModelSetup = vi.fn();
  const restoreObserver = installWorkspaceObserver();
  try {
    const { container, unmount } = render(<App api={api} />);
    const getShadow = () =>
      container.querySelector(".native-workspace-host")?.shadowRoot;
    const collapse = await screen.findByRole("button", {
      name: "Seitenleiste einklappen",
    });
    fireEvent.click(collapse);
    const expand = screen.getByRole("button", {
      name: "Seitenleiste ausklappen",
    });
    expect(expand.getAttribute("aria-expanded")).toBe("false");
    expect(
      screen.getByRole("button", { name: de.history }).getAttribute("title"),
    ).toBe(de.history);
    fireEvent.click(expand);
    expect(collapse.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Erweiterte Meetingansicht öffnen",
      }),
    );
    await waitFor(() =>
      expect(
        getShadow()?.querySelector<HTMLSelectElement>(
          '[aria-label="Überblick-Layout"]',
        )?.value,
      ).toBe("verlauf"),
    );
    expect(screen.queryByRole("navigation")).toBeNull();
    await waitFor(() =>
      expect(api.getLiveNotes).toHaveBeenCalledWith(active.id, true),
    );
    const shadow = within(getShadow() as unknown as HTMLElement);
    expect(
      shadow.getByText("Abschnittstranskription · offenes Audio 00:15"),
    ).toBeTruthy();
    expect(shadow.getByRole("alert").textContent).toContain(
      "Mikrofon getrennt",
    );
    const recordingActions = shadow.getByRole("group", { name: "Aufnahme" });
    expect(
      within(recordingActions)
        .getByRole("button", {
          name: "Aufnahme beenden",
        })
        .getAttribute("title"),
    ).toBe("Aufnahme beenden und sichern");
    api.getMeeting = vi.fn().mockResolvedValue({
      ...active,
      parts: [
        ...active.parts,
        {
          ...active.parts[0],
          startSample: 480000,
          text: "Vollständig nachgeladen.",
        },
      ],
    });
    fireEvent.click(
      shadow.getByRole("button", { name: "Transkript kopieren" }),
    );
    await waitFor(() =>
      expect(api.copyText).toHaveBeenCalledWith(
        expect.stringContaining("Vollständig nachgeladen."),
      ),
    );
    expect(api.getMeeting).toHaveBeenLastCalledWith(active.id);
    fireEvent.click(
      shadow.getByRole("button", { name: "Zur Standardansicht" }),
    );
    expect(getShadow()).toBeUndefined();
    expect(screen.getByRole("button", { name: de.stop })).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Erweiterte Meetingansicht öffnen" }),
    );
    await waitFor(() => expect(getShadow()).toBeTruthy());
    expect(api.stopMeeting).not.toHaveBeenCalled();
    expect(api.startLiveNotes).not.toHaveBeenCalled();
    unmount();
  } finally {
    restoreObserver();
  }
});

it.each(["de", "en"] as const)(
  "keeps new agent requests actionable inside the workspace (%s)",
  async (locale) => {
    await import("./workspace/WorkspaceBridge");
    localStorage.setItem(
      "handymeeting.rust.beta.preferences.v1",
      JSON.stringify({ locale }),
    );
    localStorage.setItem(
      "handymeeting.meeting-view.full-window.v1",
      "workspace",
    );
    const t = locale === "de" ? de : en;
    const active = { ...meeting, status: "recording" };
    let current: AppState = { ...state(active), recording: true };
    const api = fakeApi(active);
    api.getAppState = vi.fn(async () => current);
    api.getLiveNotes = vi
      .fn()
      .mockResolvedValue({ status: "off", running: false });
    api.startLiveNotes = vi.fn();
    api.setWorkspaceContext = vi.fn();
    api.stopLiveNotes = vi.fn();
    api.refreshLiveNotes = vi.fn();
    api.setLiveNotesInterval = vi.fn();
    api.localModelSetup = vi.fn();
    const restoreObserver = installWorkspaceObserver();
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const mounted = render(<App api={api} />);
    try {
      const host = () =>
        mounted.container.querySelector(".native-workspace-host")?.shadowRoot;
      await waitFor(() =>
        expect(host()?.querySelector('[role="tablist"]')).toBeTruthy(),
      );
      current = {
        ...current,
        agent: {
          connected: true,
          label: "Synthetic agent",
          ownerId: "synthetic-owner",
          revoked: false,
          ownedSessionId: null,
          expiresAtUnixMs: null,
          pending: {
            requestId: "synthetic-request",
            title: "Synthetic requested capture",
            includeMicrophone: false,
            maxDurationSeconds: 60,
            status: "pending",
          },
        },
      };
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1500);
      });
      const shadow = within(host()!.querySelector(".prototype") as HTMLElement);
      const review = shadow.getByRole("button", {
        name: locale === "de" ? "Anfrage prüfen" : "Review request",
      });
      expect(review.closest('[role="alert"]')).toBeTruthy();
      expect(api.agentDecideRecording).not.toHaveBeenCalled();
      fireEvent.click(review);
      expect(
        mounted.container.querySelector(".native-workspace-host"),
      ).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: t.agentDeny }));
      await waitFor(() =>
        expect(api.agentDecideRecording).toHaveBeenCalledWith(
          "synthetic-request",
          false,
          undefined,
        ),
      );
      expect(api.startMeeting).not.toHaveBeenCalled();
      expect(api.stopMeeting).not.toHaveBeenCalled();
      expect(api.startLiveNotes).not.toHaveBeenCalled();
      expect(api.stopLiveNotes).not.toHaveBeenCalled();
    } finally {
      mounted.unmount();
      vi.useRealTimers();
      restoreObserver();
    }
  },
);

it.each(["de", "en"] as const)(
  "keeps device loss notices dismissible inside the workspace (%s)",
  async (locale) => {
    await import("./workspace/WorkspaceBridge");
    localStorage.setItem(
      "handymeeting.rust.beta.preferences.v1",
      JSON.stringify({ locale }),
    );
    localStorage.setItem(
      "handymeeting.meeting-view.full-window.v1",
      "workspace",
    );
    const t = locale === "de" ? de : en;
    const active = { ...meeting, status: "recording" };
    let current = state(meeting);
    const api = fakeApi(meeting);
    api.getAppState = vi.fn(async () => current);
    api.getLiveNotes = vi
      .fn()
      .mockResolvedValue({ status: "off", running: false });
    api.startLiveNotes = vi.fn();
    api.setWorkspaceContext = vi.fn();
    api.stopLiveNotes = vi.fn();
    api.refreshLiveNotes = vi.fn();
    api.setLiveNotesInterval = vi.fn();
    api.localModelSetup = vi.fn();
    const restoreObserver = installWorkspaceObserver();
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const mounted = render(<App api={api} />);
    try {
      const host = () =>
        mounted.container.querySelector(".native-workspace-host")?.shadowRoot;
      await waitFor(() =>
        expect(host()?.querySelector('[role="tablist"]')).toBeTruthy(),
      );
      await waitFor(() => expect(api.listDevices).toHaveBeenCalled());
      current = { ...state(active), recording: true };
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1500);
      });
      vi.mocked(api.listDevices).mockResolvedValue([
        {
          id: "out",
          name: "Synthetic output",
          source: "system",
          isDefault: true,
        },
      ]);
      fireEvent.focus(window);
      const shadow = within(host()!.querySelector(".prototype") as HTMLElement);
      const notice = await shadow.findByText(
        locale === "de"
          ? /Synthetic microphone.*aus der Geräteliste verschwunden/
          : /Synthetic microphone.*left the device list/,
      );
      expect(notice.closest('[role="alert"]')).toBeTruthy();
      expect(notice.textContent).toContain(
        locale === "de"
          ? "bereits gespeicherter Ton bleibt erhalten"
          : "audio saved so far is kept",
      );
      fireEvent.click(
        within(notice.closest('[role="alert"]') as HTMLElement).getByRole(
          "button",
          { name: t.dismiss },
        ),
      );
      await waitFor(() =>
        expect(
          shadow.queryByText(
            locale === "de"
              ? /Synthetic microphone.*aus der Geräteliste verschwunden/
              : /Synthetic microphone.*left the device list/,
          ),
        ).toBeNull(),
      );
      expect(api.startMeeting).not.toHaveBeenCalled();
      expect(api.stopMeeting).not.toHaveBeenCalled();
      expect(api.startLiveNotes).not.toHaveBeenCalled();
      expect(api.stopLiveNotes).not.toHaveBeenCalled();
    } finally {
      mounted.unmount();
      vi.useRealTimers();
      restoreObserver();
    }
  },
);

it("keeps the advanced output language and customized prompt across restarts", async () => {
  const openPromptSettings = async () => {
    await userEvent.click(
      await screen.findByRole("button", { name: "Einstellungen" }),
    );
    await userEvent.click(
      await screen.findByText("Erweiterte Auswertung: Sprache und Prompt"),
    );
    return {
      language: screen.getByLabelText<HTMLSelectElement>("Ausgabesprache"),
      prompt: screen.getByLabelText<HTMLTextAreaElement>(
        "Anweisungen für Stil und Schwerpunkte",
      ),
    };
  };
  const first = render(<App api={fakeApi(meeting)} />);
  const initial = await openPromptSettings();
  expect(initial.language.value).toBe("auto");
  expect(initial.prompt.value).toContain("Use concise, specific titles.");
  await userEvent.click(screen.getByText("Standardprompt anzeigen (Englisch)"));
  expect(screen.getByText(/RECONCILE phase/)).toBeTruthy();
  await userEvent.selectOptions(initial.language, "ru");
  fireEvent.change(initial.prompt, {
    target: { value: "Focus on release blockers." },
  });
  first.unmount();
  render(<App api={fakeApi(meeting)} />);
  const restored = await openPromptSettings();
  expect(restored.language.value).toBe("ru");
  expect(restored.prompt.value).toBe("Focus on release blockers.");
  await userEvent.click(
    screen.getByRole("button", { name: "Standard wiederherstellen" }),
  );
  expect(restored.prompt.value).toContain("Use concise, specific titles.");
  expect(restored.language.value).toBe("ru");
});
