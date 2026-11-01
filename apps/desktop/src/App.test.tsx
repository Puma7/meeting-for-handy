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
import { testServices } from "./test-services";
import { defaultTemplate, renderTranscript, validateLibrary } from "./domain";
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

it("acknowledges a slow history open and does not submit it twice", async () => {
  const api = fakeApi();
  let finish!: (item: Meeting) => void;
  api.getMeeting = vi.fn(
    () =>
      new Promise<Meeting>((resolve) => {
        finish = resolve;
      }),
  );
  render(<App api={api} />);
  await screen.findByRole("button", { name: "Historie" });
  await userEvent.click(screen.getByRole("button", { name: "Historie" }));
  const open = await screen.findByRole("button", { name: "Öffnen" });
  fireEvent.click(open);
  fireEvent.click(open);
  expect(api.getMeeting).toHaveBeenCalledTimes(1);
  expect(screen.getByText("Meeting wird geöffnet…")).toBeTruthy();
  await act(async () => finish(meeting));
  expect(api.getMeeting).toHaveBeenCalledTimes(1);
});

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
  it("requires actual desktop readiness and passes separate microphone selection", async () => {
    const api = fakeApi();
    render(<App api={api} />);
    await ready();
    await userEvent.click(
      screen.getByLabelText("Mikrofon zusätzlich aufnehmen"),
    );
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
    expect(screen.queryByRole("option", { name: "Batch only" })).toBeNull();
    expect(screen.queryByRole("option", { name: "Not installed" })).toBeNull();
    await waitFor(() =>
      expect(
        (
          screen.getByLabelText(
            "Streaming-Modell aus Handy",
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
      screen.getByLabelText("Streaming-Modell aus Handy"),
      "new",
    );
    expect(
      (screen.getByLabelText("Streaming-Modell aus Handy") as HTMLSelectElement)
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
      (screen.getByLabelText("Streaming-Modell aus Handy") as HTMLSelectElement)
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
        screen.getByLabelText("Streaming-Modell aus Handy"),
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
      screen.getByLabelText("Streaming-Modell aus Handy"),
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
      screen.getByLabelText("Streaming-Modell aus Handy"),
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
          screen.getByLabelText("Streaming-Modell aus Handy"),
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
