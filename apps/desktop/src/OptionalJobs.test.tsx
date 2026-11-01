import { expect, it, vi } from "vitest";
import { useState } from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import UpdatePanel from "./views/UpdatePanel";
import SpeakerExperiments from "./views/SpeakerExperiments";
import MaintenanceStatus from "./views/MaintenanceStatus";
import LibraryStatus from "./views/LibraryStatus";
import {
  ACTIVE_POLL_MS,
  IDLE_POLL_MS,
  requestJobStatusRefresh,
} from "./jobPolling";
import { nameCandidates, SPEAKER_NAME_TEMPLATE } from "./speaker-proposals";
import type {
  DesktopApi,
  Meeting,
  AnalysisResult,
  UpdateProgress,
} from "./types";

it.each([
  ["de", "Update prüfen", "Update wird geprüft…"],
  ["en", "Check for updates", "Checking for updates…"],
] as const)(
  "announces the sidebar check politely in %s while its request is pending",
  async (locale, label, checking) => {
    let finish!: (value: UpdateProgress) => void;
    const idle: UpdateProgress = {
      status: "idle",
      version: null,
      releaseUrl: null,
      receivedBytes: 0,
      totalBytes: 0,
      elapsedMs: 0,
      error: null,
    };
    const api = {
      checkForUpdate: vi.fn(
        () =>
          new Promise<UpdateProgress>((resolve) => {
            finish = resolve;
          }),
      ),
      getUpdateProgress: vi.fn().mockResolvedValue(idle),
    } as unknown as DesktopApi;
    render(<UpdatePanel api={api} locale={locale} shortcut />);
    await userEvent.click(screen.getByRole("button", { name: label }));
    const live = screen.getByRole("status");
    expect(live.getAttribute("aria-live")).toBe("polite");
    expect(live.textContent).toBe(checking);
    expect(
      (screen.getByRole("button", { name: checking }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
    await act(async () => finish(idle));
    expect(live.isConnected).toBe(true);
    expect(live.textContent).toBe("");
  },
);

it.each([
  ["de", "current", "App ist aktuell.", "Update prüfen"],
  ["de", "available", "Update verfügbar.", "Update prüfen"],
  ["en", "current", "App is up to date.", "Check for updates"],
  ["en", "available", "Update available.", "Check for updates"],
] as const)(
  "announces a user-started check result in the original polite region (%s/%s)",
  async (locale, result, text, label) => {
    let finish!: () => void;
    let progress: UpdateProgress["status"] = "idle";
    const api = {
      checkForUpdate: vi.fn(
        () =>
          new Promise<void>((resolve) => {
            finish = () => {
              progress = result;
              resolve();
            };
          }),
      ),
      getUpdateProgress: vi.fn(async () => ({
        status: progress,
        version: "synthetic",
        receivedBytes: 0,
        totalBytes: 0,
        elapsedMs: 0,
        error: null,
      })),
    } as unknown as DesktopApi;
    const { container } = render(
      <UpdatePanel api={api} locale={locale} shortcut />,
    );
    const live = container.querySelector('[aria-live="polite"]')!;
    await userEvent.click(screen.getByRole("button", { name: label }));
    await act(async () => finish());
    await waitFor(() => expect(live.textContent).toBe(text));
    expect(container.querySelector('[aria-live="polite"]')).toBe(live);
    expect(screen.getAllByRole("status")).toHaveLength(1);
    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
  },
);

it.each([true, false])(
  "requires an explicit successful speaker capability check (%s) before a job can start",
  async (supported) => {
    const start = vi.fn().mockResolvedValue("owned-job");
    const check = vi.fn().mockResolvedValue(supported);
    const api = {
      startSpeakerExperiment: start,
      checkSpeakerModel: check,
    } as unknown as DesktopApi;
    function Harness() {
      const [checks, setChecks] = useState<Record<string, boolean>>({});
      return (
        <SpeakerExperiments
          api={api}
          locale="en"
          meeting={meeting}
          models={[
            {
              id: "installed",
              name: "Installed",
              isDownloaded: true,
              supportsStreaming: false,
            },
          ]}
          checkedModels={checks}
          onModelChecked={(id, value) => setChecks({ [id]: value })}
        />
      );
    }
    render(<Harness />);
    await userEvent.selectOptions(
      screen.getByLabelText("Installed model for speaker capability check"),
      "installed",
    );
    const run = screen.getByRole("button", {
      name: "Run local speaker separation",
    }) as HTMLButtonElement;
    expect(run.disabled).toBe(true);
    expect(check).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: "Check speaker support" }),
    );
    await waitFor(() => expect(check).toHaveBeenCalledWith("installed"));
    await waitFor(() => expect(run.disabled).toBe(!supported));
    if (supported) {
      await userEvent.click(run);
      expect(start).toHaveBeenCalledWith(
        meeting.id,
        "installed",
        "post-meeting",
      );
    } else {
      expect((await screen.findByRole("alert")).textContent).toContain(
        "does not support local speaker separation",
      );
      expect(start).not.toHaveBeenCalled();
    }
  },
);
const meeting: Meeting = {
  id: "0123456789abcdef0123456789abcdef",
  title: "Synthetic fixture",
  startedAt: "2026-10-27T00:00:00Z",
  durationMs: 30000,
  status: "complete",
  modelId: "asr",
  warnings: [],
  parts: [
    {
      file: "system-000000000000.wav",
      source: "system",
      startSample: 0,
      sampleCount: 480000,
      status: "complete",
      text: "Anna, please read the report.",
    },
  ],
};

it("recovers a full speaker-check archive only through an explicit preserving action", async () => {
  const check = vi
    .fn()
    .mockRejectedValueOnce(
      "[SPEAKER_CHECK_ARCHIVE_REQUIRED] Proof archive is full.",
    )
    .mockResolvedValue(true);
  const archive = vi.fn().mockResolvedValue(true);
  const checked = vi.fn();
  const api = {
    startSpeakerExperiment: vi.fn(),
    checkSpeakerModel: check,
    archiveSpeakerModelChecks: archive,
  } as unknown as DesktopApi;
  render(
    <SpeakerExperiments
      api={api}
      locale="en"
      setupOnly
      selectedModelId="installed"
      models={[
        {
          id: "installed",
          name: "Installed",
          isDownloaded: true,
          supportsStreaming: false,
        },
      ]}
      onModelChecked={checked}
    />,
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Check speaker support" }),
  );
  const action = await screen.findByRole("button", {
    name: "Archive check records",
  });
  expect(archive).not.toHaveBeenCalled();
  expect(screen.getByRole("alert").textContent).not.toContain(
    "SPEAKER_CHECK_ARCHIVE_REQUIRED",
  );
  await userEvent.click(action);
  await screen.findByText(
    "Check records archived and retained. Run the model check again.",
  );
  expect(archive).toHaveBeenCalledTimes(1);
  await userEvent.click(
    screen.getByRole("button", { name: "Check speaker support" }),
  );
  await waitFor(() => expect(checked).toHaveBeenCalledWith("installed", true));
  expect(check).toHaveBeenCalledTimes(2);
});

it.each([
  ["de", "Windows-Installer wird geladen"],
  ["en", "Downloading the Windows installer"],
] as const)(
  "announces a sidebar download distinctly from a check in %s",
  async (locale, downloading) => {
    const api = {
      checkForUpdate: vi.fn(),
      getUpdateProgress: vi.fn().mockResolvedValue({
        status: "downloading",
        version: "9.9.9",
        releaseUrl: null,
        receivedBytes: 25,
        totalBytes: 100,
        elapsedMs: 1000,
        error: null,
      }),
    } as unknown as DesktopApi;
    render(<UpdatePanel api={api} locale={locale} shortcut />);
    const live = screen.getByRole("status");
    await waitFor(() => expect(live.textContent).toBe(downloading));
    expect(live.getAttribute("aria-live")).toBe("polite");
    expect(
      (screen.getByRole("button", { name: downloading }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(api.checkForUpdate).not.toHaveBeenCalled();
    expect(screen.queryByText("Update wird geprüft…")).toBeNull();
    expect(screen.queryByText("Checking for updates…")).toBeNull();
  },
);

it.each(["recording", "checkingBlocked"] as const)(
  "blocks check-proof archival during %s",
  async (gate) => {
    const archive = vi.fn().mockResolvedValue(true);
    const api = {
      startSpeakerExperiment: vi.fn(),
      checkSpeakerModel: vi
        .fn()
        .mockRejectedValue(
          "[SPEAKER_CHECK_ARCHIVE_REQUIRED] Retained records need review.",
        ),
      archiveSpeakerModelChecks: archive,
    } as unknown as DesktopApi;
    const props = {
      api,
      locale: "en" as const,
      setupOnly: true,
      selectedModelId: "installed",
      models: [
        {
          id: "installed",
          name: "Installed",
          isDownloaded: true,
          supportsStreaming: false,
        },
      ],
    };
    const view = render(<SpeakerExperiments {...props} />);
    await userEvent.click(
      screen.getByRole("button", { name: "Check speaker support" }),
    );
    await screen.findByRole("button", { name: "Archive check records" });
    view.rerender(<SpeakerExperiments {...props} {...{ [gate]: true }} />);
    const action = screen.getByRole("button", {
      name: "Archive check records",
    }) as HTMLButtonElement;
    expect(action.disabled).toBe(true);
    await userEvent.click(action);
    expect(archive).not.toHaveBeenCalled();
  },
);

it("keeps the archive retry available after another instance temporarily owns the proof lock", async () => {
  const archive = vi
    .fn()
    .mockRejectedValueOnce("Another model check is running; please wait.")
    .mockResolvedValue(true);
  const api = {
    startSpeakerExperiment: vi.fn(),
    checkSpeakerModel: vi
      .fn()
      .mockRejectedValue(
        "[SPEAKER_CHECK_ARCHIVE_REQUIRED] Records need review.",
      ),
    archiveSpeakerModelChecks: archive,
  } as unknown as DesktopApi;
  render(
    <SpeakerExperiments
      api={api}
      locale="en"
      setupOnly
      selectedModelId="installed"
      models={[
        {
          id: "installed",
          name: "Installed",
          isDownloaded: true,
          supportsStreaming: false,
        },
      ]}
    />,
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Check speaker support" }),
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Archive check records" }),
  );
  await screen.findByText("Another model check is running; please wait.");
  await userEvent.click(
    screen.getByRole("button", { name: "Archive check records" }),
  );
  await screen.findByText(
    "Check records archived and retained. Run the model check again.",
  );
  expect(archive).toHaveBeenCalledTimes(2);
});
it("never checks or installs updates merely by opening settings and blocks installation during active work", async () => {
  const check = vi.fn(),
    download = vi.fn(),
    install = vi.fn();
  const api = {
    checkForUpdate: check,
    downloadUpdate: download,
    installUpdate: install,
    getUpdateProgress: vi.fn().mockResolvedValue({
      status: "ready",
      version: "1.0.3",
      releaseUrl:
        "https://github.com/Puma7/meeting-for-handy/releases/tag/v1.0.3",
      receivedBytes: 10,
      totalBytes: 10,
      elapsedMs: 20,
      error: null,
    }),
  } as unknown as DesktopApi;
  render(<UpdatePanel api={api} locale="en" blocked />);
  const button = await screen.findByRole("button", {
    name: "Start installer and close app",
  });
  expect((button as HTMLButtonElement).disabled).toBe(true);
  expect(check).not.toHaveBeenCalled();
  expect(download).not.toHaveBeenCalled();
  expect(install).not.toHaveBeenCalled();
});
it("shows download progress only from confirmed bytes", async () => {
  const api = {
    checkForUpdate: vi.fn(),
    getUpdateProgress: vi.fn().mockResolvedValue({
      status: "downloading",
      version: "1.0.3",
      releaseUrl: null,
      receivedBytes: 25,
      totalBytes: 100,
      elapsedMs: 7000,
      error: null,
    }),
    cancelUpdate: vi.fn().mockResolvedValue(undefined),
  } as unknown as DesktopApi;
  render(<UpdatePanel api={api} locale="en" footer />);
  const bar = await screen.findByRole("progressbar");
  expect(bar.getAttribute("value")).toBe("25");
  expect(bar.getAttribute("max")).toBe("100");
  expect(screen.getByText(/25%/)).toBeTruthy();
  await userEvent.click(
    screen.getByRole("button", { name: "Cancel download" }),
  );
  expect(api.cancelUpdate).toHaveBeenCalledOnce();
});
it("keeps live and post-meeting speaker actions separate and never starts them automatically", async () => {
  const start = vi.fn().mockResolvedValue("speaker-job");
  const api = {
    startSpeakerExperiment: start,
    getSpeakerProgress: vi.fn().mockResolvedValue(null),
    getSpeakerSections: vi.fn().mockResolvedValue(null),
  } as unknown as DesktopApi;
  render(
    <SpeakerExperiments
      api={api}
      locale="en"
      meeting={meeting}
      checkedModels={{ "optional-model": true }}
      models={[
        {
          id: "optional-model",
          name: "Installed optional model",
          isDownloaded: true,
          supportsStreaming: false,
        },
      ]}
      recording
    />,
  );
  await userEvent.selectOptions(
    screen.getByLabelText("Installed model for speaker capability check"),
    "optional-model",
  );
  expect(start).not.toHaveBeenCalled();
  expect(
    (
      screen.getByRole("button", {
        name: "Run local speaker separation",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  await userEvent.click(
    screen.getByRole("button", { name: "Check live voice sections" }),
  );
  await waitFor(() =>
    expect(start).toHaveBeenCalledWith(meeting.id, "optional-model", "live"),
  );
});
it("targets cancellation only at the owned speaker job and displays unsupported-model failure", async () => {
  const cancel = vi.fn().mockResolvedValue(true);
  const get = vi.fn().mockResolvedValue({
    jobId: "owned-speaker-job",
    meetingId: meeting.id,
    mode: "live",
    status: "running",
    stage: "waiting",
    modelId: "optional",
    confirmedTasks: 2,
    totalTasks: 0,
    elapsedMs: 4000,
    error: null,
    cancelRequested: false,
    lastSection: null,
  });
  const api = {
    startSpeakerExperiment: vi.fn(),
    getSpeakerProgress: get,
    cancelSpeakerExperiment: cancel,
    stopMeeting: vi.fn(),
  } as unknown as DesktopApi;
  render(<SpeakerExperiments api={api} locale="en" footer />);
  const button = await screen.findByRole("button", {
    name: "Cancel only the speaker experiment",
  });
  expect(
    (screen.getByRole("progressbar") as HTMLProgressElement).hasAttribute(
      "value",
    ),
  ).toBe(false);
  await userEvent.click(button);
  expect(cancel).toHaveBeenCalledWith("owned-speaker-job");
  expect(api.stopMeeting).not.toHaveBeenCalled();
});
it("filters forged, stale and wrong-meeting name candidates before they can prefill a manual label", () => {
  const analysis = {
    id: "result",
    meetingId: meeting.id,
    sourceHash: "synthetic",
    template: SPEAKER_NAME_TEMPLATE,
    createdAt: "synthetic",
    sources: [
      {
        id: "original",
        source: "system",
        startMs: 0,
        endMs: 30000,
        text: meeting.parts[0].text!,
      },
    ],
    sections: [
      {
        id: "names",
        name: "Names",
        kind: "summary",
        items: [
          {
            text: "Anna",
            owner: null,
            due: null,
            evidence: [{ sourceId: "original", quote: "Anna, please read" }],
          },
        ],
      },
    ],
  } as AnalysisResult;
  expect(nameCandidates([analysis], meeting).map((c) => c.name)).toEqual([
    "Anna",
  ]);
  expect(
    nameCandidates([{ ...analysis, meetingId: "other" }], meeting),
  ).toEqual([]);
  const forged = structuredClone(analysis);
  forged.sections[0].items[0].text = "Peter";
  expect(nameCandidates([forged], meeting)).toEqual([]);
  const stale = {
    ...meeting,
    parts: [{ ...meeting.parts[0], text: "Changed original text" }],
  };
  expect(nameCandidates([analysis], stale)).toEqual([]);
  const owner = structuredClone(analysis);
  owner.sections[0].items[0].owner = "Anna";
  expect(nameCandidates([owner], meeting)).toEqual([]);
  // An earlier prompt version stays readable but never feeds new speaker labels.
  expect(
    nameCandidates(
      [
        {
          ...analysis,
          promptVersion: "handymeeting-rust-notes-1",
          currentPrompt: false,
        },
      ],
      meeting,
    ),
  ).toEqual([]);
});
it("keeps cancel available while the download request is still running", async () => {
  let status = "available";
  const api = {
    checkForUpdate: vi.fn(),
    downloadUpdate: vi.fn(() => {
      status = "downloading";
      return new Promise(() => {});
    }),
    cancelUpdate: vi.fn().mockResolvedValue(undefined),
    getUpdateProgress: vi.fn(async () => ({
      status,
      version: "1.0.3",
      releaseUrl: null,
      receivedBytes: 1,
      totalBytes: 10,
      elapsedMs: 5,
      error: null,
    })),
  } as unknown as DesktopApi;
  render(<UpdatePanel api={api} locale="en" />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Download Windows installer" }),
  );
  const cancel = await screen.findByRole("button", { name: "Cancel download" });
  expect((cancel as HTMLButtonElement).disabled).toBe(false);
  await userEvent.click(cancel);
  expect(api.cancelUpdate).toHaveBeenCalledOnce();
});
it("blocks installation while archive maintenance is running", async () => {
  const api = {
    checkForUpdate: vi.fn(),
    installUpdate: vi.fn(),
    getMaintenanceProgress: vi.fn().mockResolvedValue({ status: "running" }),
    getLibraryProgress: vi.fn().mockResolvedValue(null),
    getUpdateProgress: vi.fn().mockResolvedValue({
      status: "ready",
      version: "1.0.3",
      releaseUrl: null,
      receivedBytes: 10,
      totalBytes: 10,
      elapsedMs: 20,
      error: null,
    }),
  } as unknown as DesktopApi;
  render(<UpdatePanel api={api} locale="en" />);
  await screen.findByText(/archive maintenance and indexing first/);
  const button = screen.getByRole("button", {
    name: "Start installer and close app",
  });
  expect((button as HTMLButtonElement).disabled).toBe(true);
});
it.each([
  [
    "de",
    "Windows-Installer wird geladen",
    "Installer vollständig geladen und geprüft",
    "Installer bereit.",
    "Update verfügbar.",
  ],
  [
    "en",
    "Downloading the Windows installer",
    "Installer downloaded and verified",
    "Installer ready.",
    "Update available.",
  ],
] as const)(
  "announces confirmed sidebar download completion once without losing the ready action (%s)",
  async (locale, downloading, ready, visibleReady, available) => {
    let status: UpdateProgress["status"] = "available";
    const api = {
      checkForUpdate: vi.fn(),
      downloadUpdate: vi.fn(),
      installUpdate: vi.fn(),
      getUpdateProgress: vi.fn(async () => ({
        status,
        version: "synthetic",
        receivedBytes: status === "ready" ? 10 : 1,
        totalBytes: 10,
        elapsedMs: 0,
        error: null,
      })),
    } as unknown as DesktopApi;
    const openSettings = vi.fn();
    const { container } = render(
      <UpdatePanel
        api={api}
        locale={locale}
        shortcut
        onOpenSettings={openSettings}
      />,
    );
    await screen.findByText(available, { selector: "p" });
    const live = container.querySelector('[aria-live="polite"]')!;
    status = "downloading";
    act(() => requestJobStatusRefresh());
    await waitFor(() => expect(live.textContent).toBe(downloading));
    status = "ready";
    act(() => requestJobStatusRefresh());
    await waitFor(() => expect(live.textContent).toBe(ready));
    expect(container.querySelector('[aria-live="polite"]')).toBe(live);
    expect(screen.getByText(visibleReady, { exact: false })).toBeTruthy();
    expect(
      screen.queryByText(available, { exact: false, selector: "p" }),
    ).toBeNull();
    expect(screen.getAllByRole("status")).toHaveLength(1);
    await userEvent.click(
      screen.getByRole("button", {
        name: locale === "de" ? "Update öffnen" : "Open update",
      }),
    );
    expect(openSettings).toHaveBeenCalledTimes(1);
    expect(api.checkForUpdate).not.toHaveBeenCalled();
    expect(api.downloadUpdate).not.toHaveBeenCalled();
    expect(api.installUpdate).not.toHaveBeenCalled();
  },
);

it.each(["de", "en"] as const)(
  "announces a native update failure once and keeps it dismissed across refresh (%s)",
  async (locale) => {
    let attempt = 1;
    const api = {
      checkForUpdate: vi.fn(),
      getUpdateProgress: vi.fn(async () => ({
        status: "failed",
        version: "synthetic",
        receivedBytes: 0,
        totalBytes: 0,
        elapsedMs: 0,
        error: "Synthetic native update failure",
        attempt,
      })),
    } as unknown as DesktopApi;
    render(
      <>
        <UpdatePanel api={api} locale={locale} shortcut />
        <UpdatePanel api={api} locale={locale} footer />
      </>,
    );
    await screen.findByRole("button", {
      name: locale === "de" ? "Ausblenden" : "Dismiss",
    });
    expect(
      screen.getAllByRole("alert").map((item) => item.textContent),
    ).toEqual(["Synthetic native update failure"]);
    await userEvent.click(
      screen.getByRole("button", {
        name: locale === "de" ? "Ausblenden" : "Dismiss",
      }),
    );
    expect(screen.queryAllByRole("alert")).toHaveLength(0);
    await act(async () => requestJobStatusRefresh());
    expect(screen.queryAllByRole("alert")).toHaveLength(0);
    attempt = 2;
    act(() => requestJobStatusRefresh());
    await screen.findByRole("button", {
      name: locale === "de" ? "Ausblenden" : "Dismiss",
    });
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  },
);

it("retains a shortcut command failure when no native job reports it", async () => {
  const api = {
    checkForUpdate: vi
      .fn()
      .mockRejectedValue(new Error("Synthetic shortcut command failed")),
    getUpdateProgress: vi
      .fn()
      .mockResolvedValueOnce({
        status: "idle",
        receivedBytes: 0,
        totalBytes: 0,
        elapsedMs: 0,
        error: null,
      })
      .mockImplementation(() => new Promise(() => {})),
  } as unknown as DesktopApi;
  render(<UpdatePanel api={api} locale="en" shortcut />);
  await waitFor(() => expect(api.getUpdateProgress).toHaveBeenCalledOnce());
  await userEvent.click(
    screen.getByRole("button", { name: "Check for updates" }),
  );
  expect(await screen.findByRole("alert")).toHaveProperty(
    "textContent",
    "Synthetic shortcut command failed",
  );
});

it.each(["de", "en"] as const)(
  "preserves an already verified installer when the compact update action is used (%s)",
  async (locale) => {
    let verified = true;
    const openSettings = vi.fn();
    const api = {
      checkForUpdate: vi.fn(async () => {
        verified = false;
      }),
      getUpdateProgress: vi.fn(async () => ({
        status: verified ? "ready" : "available",
        version: "synthetic",
        receivedBytes: 10,
        totalBytes: 10,
        elapsedMs: 0,
        error: null,
      })),
      downloadUpdate: vi.fn(),
      installUpdate: vi.fn(),
    } as unknown as DesktopApi;
    const { container } = render(
      <UpdatePanel
        api={api}
        locale={locale}
        shortcut
        onOpenSettings={openSettings}
      />,
    );
    await screen.findByRole("button", {
      name: locale === "de" ? "Update öffnen" : "Open update",
    });
    await userEvent.click(
      container.querySelector(".sidebar-update button") as HTMLButtonElement,
    );
    expect(verified).toBe(true);
    expect(api.checkForUpdate).not.toHaveBeenCalled();
    expect(openSettings).toHaveBeenCalledOnce();
    expect(api.downloadUpdate).not.toHaveBeenCalled();
    expect(api.installUpdate).not.toHaveBeenCalled();
  },
);

it("does not keep a cancelled download in the footer and lets a failure be dismissed", async () => {
  let status = "cancelled";
  const api = {
    checkForUpdate: vi.fn(),
    getUpdateProgress: vi.fn(async () => ({
      status,
      version: "1.0.3",
      releaseUrl: null,
      receivedBytes: 0,
      totalBytes: 0,
      elapsedMs: 0,
      error: status === "failed" ? "offline" : null,
    })),
  } as unknown as DesktopApi;
  const { container } = render(<UpdatePanel api={api} locale="en" footer />);
  await new Promise((r) => setTimeout(r, 30));
  expect(container.textContent).toBe("");
  status = "failed";
  // Idle status bars wait for their slow interval unless a new job is announced.
  act(() => requestJobStatusRefresh());
  await userEvent.click(await screen.findByRole("button", { name: "Dismiss" }));
  expect(container.textContent).toBe("");
});
it("shows a repeated failure again after a new attempt", async () => {
  let attempt = 1;
  const api = {
    checkForUpdate: vi.fn(),
    getUpdateProgress: vi.fn(async () => ({
      status: "failed",
      version: "1.0.3",
      releaseUrl: null,
      receivedBytes: 0,
      totalBytes: 0,
      elapsedMs: 0,
      error: "offline",
      attempt,
    })),
  } as unknown as DesktopApi;
  const { container } = render(<UpdatePanel api={api} locale="en" footer />);
  await userEvent.click(await screen.findByRole("button", { name: "Dismiss" }));
  expect(container.textContent).toBe("");
  attempt = 2;
  act(() => requestJobStatusRefresh());
  expect(
    await screen.findByRole("button", { name: "Dismiss" }, { timeout: 2000 }),
  ).toBeTruthy();
});
it("polls status bars slowly while idle and quickly only while a job runs", async () => {
  vi.useFakeTimers();
  try {
    let status = "complete";
    const job = () => ({
      jobId: "synthetic-job",
      kind: "cleanup-preview",
      status,
      confirmedTasks: 1,
      totalTasks: 1,
      elapsedMs: 10,
      skipped: 0,
      error: null,
      cancelRequested: false,
    });
    const maintenance = vi.fn(async () => job());
    const library = vi.fn(async () => job());
    const speaker = vi.fn(async () => null);
    const update = vi.fn(async () => ({
      status: "current",
      version: null,
      releaseUrl: null,
      receivedBytes: 0,
      totalBytes: 0,
      elapsedMs: 0,
      error: null,
    }));
    const api = {
      checkForUpdate: vi.fn(),
      startSpeakerExperiment: vi.fn(),
      getMaintenanceProgress: maintenance,
      getLibraryProgress: library,
      getSpeakerProgress: speaker,
      getUpdateProgress: update,
    } as unknown as DesktopApi;
    render(
      <>
        <MaintenanceStatus api={api} locale="en" />
        <LibraryStatus api={api} connected locale="en" />
        <SpeakerExperiments api={api} locale="en" footer />
        <UpdatePanel api={api} locale="en" footer />
      </>,
    );
    await act(async () => {});
    const pollers = [maintenance, library, speaker, update];
    for (const poll of pollers) expect(poll).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(IDLE_POLL_MS - 100);
    });
    for (const poll of pollers) expect(poll).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    for (const poll of pollers) expect(poll).toHaveBeenCalledTimes(2);
    status = "running";
    await act(async () => {
      await vi.advanceTimersByTimeAsync(IDLE_POLL_MS);
    });
    expect(maintenance).toHaveBeenCalledTimes(3);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ACTIVE_POLL_MS * 2);
    });
    expect(maintenance).toHaveBeenCalledTimes(5);
    expect(library).toHaveBeenCalledTimes(5);
    expect(speaker).toHaveBeenCalledTimes(3);
  } finally {
    vi.useRealTimers();
  }
});
it("keeps footer duplicates of settings panels hidden without polling", async () => {
  const maintenance = vi.fn().mockResolvedValue({
    jobId: "synthetic-job",
    kind: "cleanup-preview",
    status: "running",
    confirmedTasks: 0,
    totalTasks: 1,
    elapsedMs: 10,
    skipped: 0,
    error: null,
    cancelRequested: false,
  });
  const update = vi.fn().mockResolvedValue({
    status: "downloading",
    version: "1.0.3",
    releaseUrl: null,
    receivedBytes: 1,
    totalBytes: 10,
    elapsedMs: 5,
    error: null,
  });
  const api = {
    checkForUpdate: vi.fn(),
    getMaintenanceProgress: maintenance,
    getUpdateProgress: update,
  } as unknown as DesktopApi;
  const { container, rerender } = render(
    <>
      <MaintenanceStatus api={api} locale="en" hidden />
      <UpdatePanel api={api} locale="en" footer hidden />
    </>,
  );
  await new Promise((r) => setTimeout(r, 30));
  expect(container.textContent).toBe("");
  expect(maintenance).not.toHaveBeenCalled();
  expect(update).not.toHaveBeenCalled();
  rerender(
    <>
      <MaintenanceStatus api={api} locale="en" />
      <UpdatePanel api={api} locale="en" footer />
    </>,
  );
  expect(
    await screen.findByRole("status", { name: "Retention and archive" }),
  ).toBeTruthy();
  expect(
    await screen.findByRole("status", { name: "Update app" }),
  ).toBeTruthy();
});
it("sends Linux and macOS users to the official release page instead of a Windows download", async () => {
  const open = vi.fn().mockResolvedValue(undefined);
  const download = vi.fn();
  const api = {
    checkForUpdate: vi.fn(),
    downloadUpdate: download,
    openReleasePage: open,
    getUpdateProgress: vi.fn().mockResolvedValue({
      status: "available",
      version: "1.0.3",
      releaseUrl:
        "https://github.com/Puma7/meeting-for-handy/releases/tag/v1.0.3",
      receivedBytes: 0,
      totalBytes: 0,
      elapsedMs: 5,
      error: null,
      platform: "linux",
      assetName: "HandyMeeting-1.0.3-linux-amd64.deb",
      canDownload: false,
    }),
  } as unknown as DesktopApi;
  const { container } = render(<UpdatePanel api={api} locale="en" />);
  expect(
    await screen.findByText(/HandyMeeting-1\.0\.3-linux-amd64\.deb/),
  ).toBeTruthy();
  expect(
    screen.queryByRole("button", { name: "Download Windows installer" }),
  ).toBeNull();
  expect(container.textContent).not.toMatch(/Windows installer is verified/);
  // No WebView link that needs opener permissions; the backend opens the official page.
  expect(container.querySelector("a[target]")).toBeNull();
  await userEvent.click(
    screen.getByRole("button", { name: "Open official release page" }),
  );
  expect(open).toHaveBeenCalledOnce();
  expect(download).not.toHaveBeenCalled();
});
it("explains a missing platform package and still offers the release page", async () => {
  const api = {
    checkForUpdate: vi.fn(),
    openReleasePage: vi.fn().mockRejectedValue("Browser unavailable"),
    getUpdateProgress: vi.fn().mockResolvedValue({
      status: "available",
      version: "1.0.3",
      releaseUrl: null,
      receivedBytes: 0,
      totalBytes: 0,
      elapsedMs: 5,
      error: null,
      platform: "macos",
      assetName: null,
      canDownload: false,
    }),
  } as unknown as DesktopApi;
  render(<UpdatePanel api={api} locale="de" />);
  expect(
    await screen.findByText(/listet noch kein Paket für dieses System/),
  ).toBeTruthy();
  expect(
    screen.queryByRole("button", { name: "Windows-Installer laden" }),
  ).toBeNull();
  await userEvent.click(
    screen.getByRole("button", { name: "Offizielle Release-Seite öffnen" }),
  );
  expect((await screen.findByRole("alert")).textContent).toBe(
    "Browser unavailable",
  );
});
it("hides a finished search-index update but keeps running and failed ones visible", async () => {
  // 1.0.5 kept "Local search index · updated 0 of 0" in the footer permanently.
  const progress = (status: string, error: string | null = null) => ({
    jobId: "synthetic-index-" + status,
    kind: "search-index",
    status,
    confirmedTasks: 0,
    totalTasks: 0,
    elapsedMs: 10,
    skipped: 0,
    error,
    cancelRequested: false,
  });
  const library = vi.fn(async () => progress("complete"));
  const api = { getLibraryProgress: library } as unknown as DesktopApi;
  const view = render(<LibraryStatus api={api} connected locale="en" />);
  await waitFor(() => expect(library).toHaveBeenCalled());
  await act(async () => {});
  expect(
    screen.queryByRole("status", { name: "Local search index" }),
  ).toBeNull();
  library.mockResolvedValue(progress("failed", "Synthetic index failure"));
  view.unmount();
  render(<LibraryStatus api={api} connected locale="en" />);
  expect(
    await screen.findByRole("status", { name: "Local search index" }),
  ).toBeTruthy();
});
