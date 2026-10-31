import { beforeEach, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { de } from "./i18n";
import { testServices } from "./test-services";
import { useNotesJob } from "./useNotesJob";
import { defaultTemplate } from "./domain";
import type { DesktopApi, Meeting, NotesProgress } from "./types";

// Synthetic IPC: no real recordings, providers or stored credentials are accessed.
const meeting: Meeting = {
  id: "statusbar-meeting",
  title: "Statusbar meeting",
  startedAt: "2026-10-27T06:00:00Z",
  durationMs: 30000,
  status: "complete",
  modelId: "synthetic",
  warnings: [],
  parts: [
    {
      file: "system.wav",
      source: "system",
      startSample: 0,
      sampleCount: 480000,
      status: "complete",
      text: "An original retained transcript.",
    },
  ],
};
function fixture() {
  return {
    ...testServices(),
    getAppState: vi.fn().mockResolvedValue({
      version: "synthetic",
      handy: null,
      recording: false,
      activeMeeting: meeting,
    }),
    listDevices: vi.fn().mockResolvedValue([]),
    listMeetings: vi.fn().mockResolvedValue([meeting]),
    getMeeting: vi.fn().mockResolvedValue(meeting),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    listTextModels: vi.fn().mockResolvedValue(["synthetic-text"]),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    generateNotes: vi.fn(),
  } as unknown as DesktopApi;
}
beforeEach(() => localStorage.clear());

it("keeps discovery failures visible beside a retained analysis result", async () => {
  const api = fixture();
  const template = defaultTemplate("de");
  vi.mocked(api.listAnalyses).mockResolvedValue({
    analyses: [
      {
        id: "retained-analysis",
        meetingId: meeting.id,
        sourceHash: "synthetic",
        template,
        createdAt: "2026-10-30T09:00:00Z",
        sources: [],
        sections: [
          {
            ...template.steps[0],
            items: [
              {
                text: "Retained synthetic result",
                owner: null,
                due: null,
                evidence: [],
              },
            ],
          },
        ],
      },
    ],
    skipped: [],
  });
  vi.mocked(api.listTextModels).mockRejectedValue(
    new Error("Synthetic retained-result discovery failed"),
  );
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: de.analyzeMeeting }),
  );
  await screen.findByText("Retained synthetic result");
  const messages = await screen.findAllByText(
    /Synthetic retained-result discovery failed/,
  );
  expect(messages.some((message) => !message.closest("details"))).toBe(true);
  expect(screen.getByText(de.analysisSetup).closest("details")!.open).toBe(
    false,
  );
  expect(api.generateNotes).not.toHaveBeenCalled();
});

it("keeps idle model discovery failures outside collapsed analysis setup", async () => {
  const api = fixture();
  vi.mocked(api.listTextModels).mockRejectedValue(
    new Error("Synthetic model discovery failed"),
  );
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: de.analyzeMeeting }),
  );
  await waitFor(() => expect(api.listTextModels).toHaveBeenCalled());
  const messages = await screen.findAllByText(
    /Synthetic model discovery failed/,
  );
  expect(messages.some((message) => !message.closest("details"))).toBe(true);
  expect(screen.getByText(de.analysisSetup).closest("details")!.open).toBe(
    false,
  );
  expect(api.generateNotes).not.toHaveBeenCalled();
});

it("opens analysis setup only from the visible discovery-error remedy", async () => {
  const api = fixture();
  vi.mocked(api.listTextModels).mockRejectedValue(
    new Error("Synthetic discovery unavailable"),
  );
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: de.analyzeMeeting }),
  );
  const message = (
    await screen.findAllByText("Synthetic discovery unavailable")
  ).find((element) => !element.closest("details"))!;
  expect(message).toBeDefined();
  const details = screen.getByText(de.analysisSetup).closest("details")!;
  expect(details.open).toBe(false);
  const remedy = screen.getByRole("button", { name: "Einrichtung öffnen" });
  expect(remedy.closest("details")).toBeNull();
  expect(
    screen
      .getByText(
        "Starte deinen lokalen Modelldienst und aktualisiere die Liste.",
      )
      .closest("details"),
  ).toBeNull();
  await userEvent.click(remedy);
  expect(details.open).toBe(true);
  expect(api.generateNotes).not.toHaveBeenCalled();
});

it("does not claim a backend job is unstarted when its progress cannot be read", async () => {
  const api = fixture();
  vi.mocked(api.getAppState).mockResolvedValue({
    version: "synthetic",
    handy: null,
    recording: false,
    activeMeeting: meeting,
    notesRunning: true,
  });
  vi.mocked(api.getNotesProgress).mockRejectedValue(
    new Error("Synthetic progress read failure"),
  );
  render(<App api={api} />);
  const status = await screen.findByRole("status", { name: de.notesTask });
  expect(within(status).getByText(de.notesCheckingStatus)).toBeTruthy();
  await within(status).findByRole("alert");
  expect(within(status).queryByText(de.notesNotStarted)).toBeNull();
  expect(
    (
      within(status).getByRole("progressbar") as HTMLProgressElement
    ).hasAttribute("value"),
  ).toBe(false);
  expect(api.generateNotes).not.toHaveBeenCalled();
});

it("keeps the new footer inert while fresh cloud consent is pending", async () => {
  const api = fixture();
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: de.analyzeMeeting }),
  );
  await screen.findByRole("option", { name: "synthetic-text" });
  await userEvent.selectOptions(
    screen.getByLabelText(de.textModel),
    "synthetic-text",
  );
  await userEvent.selectOptions(screen.getByLabelText(de.processing), "cloud");
  await userEvent.clear(screen.getByLabelText(de.endpoint));
  await userEvent.type(
    screen.getByLabelText(de.endpoint),
    "https://example.invalid/v1",
  );
  await userEvent.type(screen.getByLabelText(de.textModel), "synthetic-cloud");
  const status = screen.getByRole("status", { name: de.notesTask });
  await userEvent.click(
    within(status).getByRole("button", { name: de.startAnalysis }),
  );
  await screen.findByRole("dialog", { name: de.cloudTitle });
  expect((status as HTMLElement).inert).toBe(true);
  expect(api.generateNotes).not.toHaveBeenCalled();
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: de.cancel }),
  );
  expect((status as HTMLElement).inert).toBe(false);
  expect(api.generateNotes).not.toHaveBeenCalled();
});

it("makes the meeting button's setup-only action explicit before any model request", async () => {
  const api = fixture();
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: de.analyzeMeeting }),
  );
  const status = await screen.findByRole("status", { name: de.notesTask });
  expect(
    within(status).getByText("Auswertung noch nicht gestartet."),
  ).toBeTruthy();
  expect(document.querySelector("main")!.contains(status)).toBe(false);
  expect(
    (
      within(status).getByRole("button", {
        name: "Auswertung starten",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(within(status).queryByRole("progressbar")).toBeNull();
  expect(api.generateNotes).not.toHaveBeenCalled();
});

it("starts from the bottom bar and retains a failed job's message, partial progress and stopped activity on another page", async () => {
  const api = fixture();
  let reject!: (error: unknown) => void;
  vi.mocked(api.generateNotes).mockImplementation(
    () =>
      new Promise((_, fail) => {
        reject = fail;
      }),
  );
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: de.analyzeMeeting }),
  );
  await screen.findByRole("option", { name: "synthetic-text" });
  await userEvent.selectOptions(
    screen.getByLabelText(de.textModel),
    "synthetic-text",
  );
  const status = screen.getByRole("status", { name: de.notesTask });
  const start = within(status).getByRole("button", {
    name: "Auswertung starten",
  });
  await waitFor(() =>
    expect((start as HTMLButtonElement).disabled).toBe(false),
  );
  await userEvent.click(start);
  await waitFor(() => expect(api.generateNotes).toHaveBeenCalledTimes(1));
  expect(within(status).getByRole("progressbar")).toBeTruthy();
  expect(status.querySelector(".spinning")).toBeTruthy();
  await userEvent.click(screen.getByRole("button", { name: de.settings }));
  expect(document.querySelector("main")!.contains(status)).toBe(false);
  await act(async () =>
    reject(
      new Error("Synthetic local service timeout. Check the model service."),
    ),
  );
  await within(status).findByText(
    "Synthetic local service timeout. Check the model service.",
  );
  expect(within(status).getByText(de.notesFailed)).toBeTruthy();
  expect(status.querySelector(".spinning")).toBeNull();
  expect(
    within(status).queryByRole("button", { name: de.cancelAnalysis }),
  ).toBeNull();
  expect(within(status).getByLabelText(de.elapsed).textContent).toMatch(
    /^\d+(?::\d{2}){1,2}$/,
  );
});

it("freezes elapsed time after a confirmed failure even while app polling still reports a busy backend", async () => {
  vi.useFakeTimers();
  try {
    const api = fixture();
    const progress: NotesProgress = {
      jobId: "owned",
      runId: "saved-run",
      meetingId: meeting.id,
      status: "running",
      stage: "requesting",
      stepName: "Summary",
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
    function Harness() {
      const job = useNotesJob(api, true, true, vi.fn());
      return (
        <output data-status={job.progress?.status}>{job.elapsedMs}</output>
      );
    }
    const { container } = render(<Harness />);
    await act(async () => {});
    vi.mocked(api.getNotesProgress).mockResolvedValue({
      ...progress,
      status: "failed",
      stage: "finished",
      elapsedMs: 2000,
      error: "Synthetic failure",
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(container.querySelector("output")!.dataset.status).toBe("failed");
    const stopped = container.textContent;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(container.textContent).toBe(stopped);
  } finally {
    vi.useRealTimers();
  }
});
