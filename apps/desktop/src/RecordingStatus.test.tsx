import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  configure,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import RecordingStatus, { recentText } from "./views/RecordingStatus";
import { testServices } from "./test-services";
import SessionWarnings from "./views/SessionWarnings";
import { de } from "./i18n";
import type { AppState, AudioPart, DesktopApi, Meeting } from "./types";
vi.setConfig({ testTimeout: 15_000 });
configure({ asyncUtilTimeout: 5000 });

// Synthetic IPC fixtures only: no audio device, model server or real system reading.
const part = (start: number, text: string): AudioPart => ({
  source: "microphone",
  startSample: start * 16000,
  sampleCount: 30 * 16000,
  status: "complete",
  text,
  file: `microphone-${start}.wav`,
});
const live: Meeting = {
  id: "synthetic-live",
  title: "Synthetic live meeting",
  startedAt: "2026-10-29T16:33:40+01:00",
  durationMs: 486_000,
  status: "recording",
  modelId: "installed-stream",
  warnings: [
    "system: audio driver reported a discontinuity at 03:12 of the saved track. Missing audio remains missing; trustworthy microphone clock gaps are represented by explicit silence, not recovered speech. Check the affected range.",
  ],
  parts: [part(0, "Erster Abschnitt"), part(30, "Zuletzt gesagter Satz")],
  pendingAudioMs: 2_000,
};
const state = (recording: boolean): AppState => ({
  version: "1.0.8",
  recording,
  activeMeeting: recording ? live : null,
  captureControl: recording
    ? { paused: false, microphoneMuted: false, hasMicrophone: true }
    : null,
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
});
const load = {
  cpuPercent: 94.6,
  memoryUsedBytes: 15.4 * 1024 ** 3,
  memoryTotalBytes: 16 * 1024 ** 3,
};

beforeEach(() => localStorage.clear());

it("reopens capture warnings only when a new warning arrives", async () => {
  const warnings = [live.warnings[0]];
  const view = render(
    <SessionWarnings warnings={warnings} locale="de" t={de} />,
  );
  const detail = screen.getByText(de.audioQuality).closest("details")!;
  await userEvent.click(detail.querySelector("summary")!);
  expect(detail.open).toBe(false);
  view.rerender(
    <SessionWarnings warnings={[...warnings]} locale="de" t={de} />,
  );
  expect(detail.open).toBe(false);
  const next = [...warnings, "Synthetic newly reported capture warning"];
  view.rerender(<SessionWarnings warnings={next} locale="de" t={de} />);
  expect(detail.open).toBe(true);
  await userEvent.click(detail.querySelector("summary")!);
  view.rerender(<SessionWarnings warnings={[...next]} locale="de" t={de} />);
  expect(detail.open).toBe(false);
});

it("announces only new live capture warnings without reannouncing unchanged polls or history", () => {
  const warnings = ["Synthetic initial warning"];
  const view = render(
    <SessionWarnings warnings={warnings} locale="de" t={de} announceNew />,
  );
  const region = screen.getByRole("status");
  expect(region.textContent).toBe("");
  const next = [...warnings, "Synthetic added warning <img src=x>"];
  view.rerender(
    <SessionWarnings warnings={next} locale="de" t={de} announceNew />,
  );
  expect(region.textContent).toBe("Synthetic added warning <img src=x>");
  expect(region.querySelector("img")).toBeNull();
  view.rerender(
    <SessionWarnings warnings={[...next]} locale="de" t={de} announceNew />,
  );
  expect(screen.getByRole("status")).toBe(region);
  expect(region.textContent).toBe("Synthetic added warning <img src=x>");
  view.rerender(
    <SessionWarnings warnings={[]} locale="de" t={de} announceNew />,
  );
  expect(screen.getByRole("status")).toBe(region);
  expect(region.textContent).toBe("");
  view.rerender(<SessionWarnings warnings={[...next]} locale="de" t={de} />);
  expect(screen.queryByRole("status")).toBeNull();
});

describe("recording status in the sidebar (synthetic IPC)", () => {
  it.each([
    { value: 94.6, expected: "CPU: 95 % · hohe Auslastung", high: true },
    { value: 20, expected: "CPU: 20 %", high: false },
    { value: null, expected: "CPU: nicht gemessen", high: false },
  ])(
    "keeps CPU state available inside computer details ($expected)",
    async ({ value, expected, high }) => {
      render(
        <RecordingStatus
          locale="de"
          paused={false}
          durationMs={1000}
          parts={[]}
          warningCount={0}
          microphoneSilent={false}
          microphoneMuted={false}
          backlogMs={0}
          getSystemLoad={vi
            .fn()
            .mockResolvedValue({ ...load, cpuPercent: value })}
          onOpen={() => undefined}
        />,
      );
      const summary = screen.getByText("Rechnerdetails");
      await userEvent.click(summary);
      const details = summary.closest("details")!;
      expect(details.closest(".sidebar-recording-facts")).toBeNull();
      const reading = await within(details).findByText(expected);
      expect(reading.hasAttribute("data-high")).toBe(high);
    },
  );
  it("names the newest recognized words and cuts long text at a word", () => {
    expect(recentText([part(30, "Später"), part(0, "Früher")])).toBe("Später");
    const long = "Wort ".repeat(40).trim() + " Ende";
    const excerpt = recentText([part(0, long)], 30);
    expect(excerpt.startsWith("…")).toBe(true);
    expect(excerpt.endsWith("Ende")).toBe(true);
    expect(excerpt.length).toBeLessThanOrEqual(31);
    expect(excerpt).not.toMatch(/^…\S*ort /);
    expect(recentText([{ ...part(0, "  "), text: "  " }])).toBe("");
  });

  it("shows time, high load, audio notices, backlog and the latest words", async () => {
    const onOpen = vi.fn();
    render(
      <RecordingStatus
        locale="de"
        paused={false}
        durationMs={486_000}
        parts={live.parts}
        warningCount={1}
        microphoneSilent={false}
        microphoneMuted={false}
        backlogMs={95_000}
        getSystemLoad={vi.fn().mockResolvedValue(load)}
        onOpen={onOpen}
      />,
    );
    const region = screen.getByRole("region", { name: "Aufnahmestatus" });
    expect(within(region).getByText("Aufnahme läuft")).toBeTruthy();
    expect(within(region).getByText("08:06")).toBeTruthy();
    const cpu = await within(region).findByText("CPU 95 %");
    expect(cpu.hasAttribute("data-high")).toBe(true);
    const ram = within(region).getByText("RAM 96 %");
    expect(ram.parentElement!.hasAttribute("data-high")).toBe(true);
    expect(within(region).getByText("GPU —")).toBeTruthy();
    expect(within(region).getByText("Rechnerdetails").tagName).toBe("SUMMARY");
    expect(within(region).getByText("1 Hinweis zur Aufnahme")).toBeTruthy();
    expect(within(region).getByText("01:35 noch offen")).toBeTruthy();
    expect(within(region).getByText("„Zuletzt gesagter Satz“")).toBeTruthy();
    await userEvent.click(
      within(region).getByRole("button", { name: /Aufnahme läuft/ }),
    );
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it("states a silent microphone and unreadable values instead of guessing", async () => {
    const getSystemLoad = vi.fn().mockResolvedValue({
      cpuPercent: null,
      memoryUsedBytes: null,
      memoryTotalBytes: null,
    });
    render(
      <RecordingStatus
        locale="en"
        paused
        durationMs={1000}
        parts={[]}
        warningCount={0}
        microphoneSilent
        microphoneMuted={false}
        backlogMs={null}
        getSystemLoad={getSystemLoad}
        onOpen={() => undefined}
      />,
    );
    await waitFor(() => expect(getSystemLoad).toHaveBeenCalled());
    expect(screen.getByText("Recording paused")).toBeTruthy();
    expect(screen.getByText("CPU —")).toBeTruthy();
    expect(screen.getByText("RAM —")).toBeTruthy();
    expect(screen.getByText("Microphone is not recording")).toBeTruthy();
    expect(screen.getByText("No text recognized yet.")).toBeTruthy();
  });

  it("does not label a short positive transcription backlog as up to date", () => {
    const view = render(
      <RecordingStatus
        locale="de"
        paused={false}
        durationMs={30_000}
        parts={[]}
        warningCount={0}
        microphoneSilent={false}
        microphoneMuted={false}
        backlogMs={1500}
        onOpen={() => undefined}
      />,
    );
    expect(screen.getByText("00:02 noch offen")).toBeTruthy();
    expect(screen.queryByText("Aktuell")).toBeNull();
    expect(screen.getByText("GPU —")).toBeTruthy();
    view.rerender(
      <RecordingStatus
        locale="de"
        paused={false}
        durationMs={30_000}
        parts={[]}
        warningCount={0}
        microphoneSilent={false}
        microphoneMuted={false}
        backlogMs={0}
        onOpen={() => undefined}
      />,
    );
    expect(screen.getByText("Aktuell")).toBeTruthy();
  });

  it("offers RAM amounts and the GPU limitation in accessible computer details", async () => {
    render(
      <RecordingStatus
        locale="de"
        paused={false}
        durationMs={30_000}
        parts={[]}
        warningCount={0}
        microphoneSilent={false}
        microphoneMuted={false}
        backlogMs={0}
        getSystemLoad={vi.fn().mockResolvedValue(load)}
        onOpen={() => undefined}
      />,
    );
    await screen.findByText("RAM 96 %");
    const summary = screen.getByText("Rechnerdetails");
    expect(summary.tagName).toBe("SUMMARY");
    const details = summary.closest("details")!;
    expect(details.closest(".sidebar-recording-facts")).toBeNull();
    expect(details.open).toBe(false);
    await userEvent.click(summary);
    expect(details.open).toBe(true);
    expect(
      within(details).getByText("Arbeitsspeicher: 15,4/16,0 GiB"),
    ).toBeTruthy();
    expect(
      within(details).getByText("GPU-Auslastung wird derzeit nicht gemessen"),
    ).toBeTruthy();
  });

  it("stays visible on other pages while recording and leads back to the recording", async () => {
    let recording = true;
    const api = {
      ...testServices(),
      getAppState: vi.fn(async () => state(recording)),
      refreshHandy: vi.fn().mockResolvedValue(state(false).handy),
      listDevices: vi.fn().mockResolvedValue([]),
      listMeetings: vi.fn().mockResolvedValue([]),
      getMeeting: vi.fn().mockResolvedValue(live),
      getSystemLoad: vi.fn().mockResolvedValue(load),
      listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
      listTextModels: vi.fn().mockResolvedValue([]),
      getNotesProgress: vi.fn().mockResolvedValue(null),
    } as unknown as DesktopApi;
    render(<App api={api} />);
    const region = await screen.findByRole("region", {
      name: "Aufnahmestatus",
    });
    await within(region).findByText("„Zuletzt gesagter Satz“");
    // The live MeetingView opts into announcements; historical MeetingDetail does not.
    expect(
      screen
        .getAllByRole("status")
        .some(
          (node) =>
            node.classList.contains("sr-only") &&
            node.getAttribute("aria-live") === "polite",
        ),
    ).toBe(true);
    await userEvent.click(
      screen.getByRole("button", { name: "Einstellungen" }),
    );
    expect(screen.getByRole("region", { name: "Aufnahmestatus" })).toBeTruthy();
    await userEvent.click(
      within(screen.getByRole("region", { name: "Aufnahmestatus" })).getByRole(
        "button",
        { name: /Aufnahme läuft/ },
      ),
    );
    await waitFor(() =>
      expect(
        screen
          .getByRole("button", { name: "Meeting" })
          .getAttribute("aria-current"),
      ).toBe("page"),
    );
    // Once the recording ends the card disappears; the computer is no longer read.
    recording = false;
    await waitFor(
      () =>
        expect(
          screen.queryByRole("region", { name: "Aufnahmestatus" }),
        ).toBeNull(),
      { timeout: 8000 },
    );
    const reads = vi.mocked(api.getSystemLoad!).mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 2500));
    expect(vi.mocked(api.getSystemLoad!).mock.calls.length).toBe(reads);
  });
});
