import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import SessionWarnings from "./views/SessionWarnings";
import TranscriptBody from "./views/TranscriptBody";
import NotesProgressPanel from "./views/NotesProgressPanel";
import { de, en } from "./i18n";
import { describeWarning, renderTranscript } from "./domain";
import { testServices } from "./test-services";
import type { DesktopApi, Meeting, NotesProgress } from "./types";
// Full-App cases render the whole UI with its status polling. On slow CI runners
// (macos-15-intel) they exceeded the 5 s default; focused component files keep it.
vi.setConfig({ testTimeout: 15_000 });

// Synthetic IPC only. No device, clipboard, real file manager or model service is opened.
const warning =
  "system: audio driver reported a discontinuity; affected recording ranges can be incomplete.";
const meeting: Meeting = {
  id: "synthetic-a",
  title: "Same title",
  startedAt: "2026-10-26T06:00:00+01:00",
  durationMs: 7200000,
  status: "complete",
  modelId: "synthetic",
  warnings: [warning],
  parts: Array.from({ length: 480 }, (_, i) => ({
    file: `part-${i}.wav`,
    source: i % 2 ? "microphone" : "system",
    startSample: Math.floor(i / 2) * 480000,
    sampleCount: 480000,
    status: "complete",
    text: `Original section ${i} <img src=x onerror=alert(1)>`,
  })),
};
/**
 * Creates two same-titled synthetic meetings with distinct IDs and a long literal
 * transcript so navigation/export tests can detect wrong-session binding or truncation.
 */
function fixture() {
  const other = {
    ...meeting,
    id: "synthetic-b",
    startedAt: "2026-10-26T08:00:00+01:00",
  };
  const api = {
    ...testServices(),
    getAppState: vi.fn().mockResolvedValue({
      version: "synthetic",
      handy: null,
      recording: false,
      activeMeeting: meeting,
    }),
    listDevices: vi.fn().mockResolvedValue([]),
    listMeetings: vi.fn().mockResolvedValue([meeting, other]),
    getMeeting: vi.fn(async (id) => (id === other.id ? other : meeting)),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    listTextModels: vi.fn().mockResolvedValue([]),
    copyText: vi.fn().mockResolvedValue(undefined),
  } as unknown as DesktopApi;
  return { api, other };
}
beforeEach(() => localStorage.clear());

it("explains known capture interruptions, retains literal technical/unknown warnings and never executes their text", async () => {
  const unknown = "<img src=x onerror=alert(1)> retained diagnostic";
  const original = [warning, warning, unknown];
  const { container } = render(
    <SessionWarnings warnings={original} locale="de" t={de} />,
  );
  expect(screen.getAllByText(/Computerton: Der Audiotreiber/)).toHaveLength(1);
  expect(screen.getByText(unknown)).toBeTruthy();
  expect(container.querySelector("img")).toBeNull();
  // Capture caveats start visible; original technical messages remain optional.
  const notice = container.querySelector<HTMLDetailsElement>(
    "details.session-warnings",
  );
  expect(notice?.open).toBe(true);
  const technical = container.querySelector<HTMLDetailsElement>(
    ".session-warnings details",
  );
  expect(technical?.open).toBe(false);
  await userEvent.click(screen.getByText(de.technicalDetails));
  expect(technical?.open).toBe(true);
  expect(screen.getByText(warning)).toBeTruthy();
  expect(original).toEqual([warning, warning, unknown]);
  const copied = renderTranscript(meeting, "de");
  expect(copied).toContain("Teile dieser Spur können fehlen");
  expect(copied).toContain("Original section 479");
  expect(renderTranscript(meeting, "en")).toContain(
    "System audio: The audio driver",
  );
});

it("translates the 1.0.5 driver discontinuity wording instead of showing it twice in English", () => {
  // 1.0.5 changed the native message; the old pattern no longer matched it.
  const current =
    "microphone: audio driver reported a discontinuity. Missing audio remains missing; trustworthy microphone clock gaps are represented by explicit silence, not recovered speech. Check the affected range.";
  render(<SessionWarnings warnings={[current]} locale="de" t={de} />);
  expect(screen.getByText(/Mikrofon: Der Audiotreiber/)).toBeTruthy();
  expect(screen.getByText(current)).toBeTruthy();
  expect(screen.getAllByText(/discontinuity/)).toHaveLength(1);
});

it("names where the driver reported an interruption and explains the usual cause", async () => {
  const timed =
    "system: audio driver reported a discontinuity at 03:12 of the saved track. Missing audio remains missing; trustworthy microphone clock gaps are represented by explicit silence, not recovered speech. Check the affected range.";
  render(<SessionWarnings warnings={[timed]} locale="de" t={de} />);
  expect(
    screen.getByText(
      "Computerton: Der Audiotreiber hat bei 03:12 eine Unterbrechung gemeldet. Ab dort können Teile dieser Spur fehlen.",
    ),
  ).toBeTruthy();
  await userEvent.click(screen.getByText(de.technicalDetails));
  expect(screen.getByText(/lokales Textmodell/)).toBeTruthy();
  expect(screen.getByText(timed)).toBeTruthy();
  // An unknown time format is not interpreted; the notice stays as recorded.
  const odd = timed.replace("03:12", "soon");
  expect(describeWarning(odd, "en")).toBe(odd);
});

it("retains the first and last original sections of a two-hour transcript in a keyboard-scrollable region", () => {
  const { container } = render(
    <TranscriptBody parts={meeting.parts} locale="en" t={en} />,
  );
  expect(screen.getByRole("region", { name: en.live }).tabIndex).toBe(0);
  expect(container.querySelectorAll(".transcript-part")).toHaveLength(480);
  expect(screen.getByText(meeting.parts[0].text!)).toBeTruthy();
  expect(screen.getByText(meeting.parts[479].text!)).toBeTruthy();
  expect(container.querySelector("img")).toBeNull();
});

it("shows the newest sections first on request and keeps that choice", async () => {
  localStorage.clear();
  const parts = meeting.parts.slice(0, 3);
  const order = (container: HTMLElement) =>
    [...container.querySelectorAll(".transcript-part p")].map(
      (item) => item.textContent,
    );
  const first = render(<TranscriptBody parts={parts} locale="de" t={de} />);
  expect(order(first.container)).toEqual(parts.map((part) => part.text));
  const toggle = screen.getByRole("button", { name: "Neueste oben" });
  expect(toggle.getAttribute("aria-pressed")).toBe("false");
  await userEvent.click(toggle);
  expect(toggle.getAttribute("aria-pressed")).toBe("true");
  expect(order(first.container)).toEqual(
    [...parts].reverse().map((part) => part.text),
  );
  // The sections themselves are unchanged; a new view reads the stored order.
  expect(parts.map((part) => part.text)).toEqual(
    meeting.parts.slice(0, 3).map((part) => part.text),
  );
  first.unmount();
  const second = render(<TranscriptBody parts={parts} locale="de" t={de} />);
  expect(order(second.container)[0]).toBe(parts[2].text);
  localStorage.clear();
});

it("searches the transcript without changing it and renders matches literally", async () => {
  const parts = [
    {
      ...meeting.parts[0],
      file: "a.wav",
      text: "Budget <b>Freitag</b> klären",
    },
    { ...meeting.parts[1], file: "b.wav", text: "Anderes Thema" },
  ];
  const { container } = render(
    <TranscriptBody parts={parts} locale="de" t={de} />,
  );
  await userEvent.type(
    screen.getByLabelText("Im Transkript suchen"),
    "freitag",
  );
  expect(screen.getByText("1 von 2 Abschnitten")).toBeTruthy();
  expect(container.querySelectorAll(".transcript-part")).toHaveLength(1);
  expect(container.querySelector("mark")?.textContent).toBe("Freitag");
  expect(container.querySelector("b")).toBeNull();
  expect(parts[0].text).toBe("Budget <b>Freitag</b> klären");
  await userEvent.clear(screen.getByLabelText("Im Transkript suchen"));
  expect(container.querySelectorAll(".transcript-part")).toHaveLength(2);
});

it("opens and exports the exact selected historical meeting, never a same-named live session", async () => {
  const { api, other } = fixture();
  render(<App api={api} />);
  await screen.findByRole("button", { name: de.openFolder });
  await waitFor(() =>
    expect(
      document.querySelector<HTMLDetailsElement>(".setup-details")?.open,
    ).toBe(false),
  );
  await userEvent.click(screen.getByRole("button", { name: de.history }));
  const row = screen.getAllByRole("button", { name: de.open })[1];
  await userEvent.click(row);
  await screen.findByRole("heading", { name: de.meetingDetail });
  await userEvent.click(screen.getByRole("button", { name: de.openFolder }));
  expect(api.openMeetingFolder).toHaveBeenCalledWith(other.id);
  await userEvent.click(
    screen.getByRole("button", { name: de.exportTranscript }),
  );
  await screen.findByText(/exports\/transcript-synthetic.txt/);
  // Markdown is the default export format.
  expect(api.exportTranscript).toHaveBeenCalledWith(other.id, "de", "md");
  expect(api.exportTranscript).not.toHaveBeenCalledWith(
    meeting.id,
    expect.anything(),
  );
  expect(api.copyText).not.toHaveBeenCalled();
});

it("reports export failure without claiming success or altering the selected transcript", async () => {
  const { api } = fixture();
  vi.mocked(api.exportTranscript).mockRejectedValue(
    "Synthetic storage full; originals retained",
  );
  render(<App api={api} />);
  await screen.findByRole("button", { name: de.exportTranscript });
  await userEvent.click(
    screen.getByRole("button", { name: de.exportTranscript }),
  );
  await screen.findByRole("alert");
  expect(screen.queryByText(/Transkript gespeichert:/)).toBeNull();
  expect(screen.getByText(meeting.parts[479].text!)).toBeTruthy();
});

it("exports Markdown and JSON for the selected historical session without shortening or rewriting its text", async () => {
  const { api, other } = fixture();
  render(<App api={api} />);
  await screen.findByRole("button", { name: de.openFolder });
  await userEvent.click(screen.getByRole("button", { name: de.history }));
  await userEvent.click(screen.getAllByRole("button", { name: de.open })[1]);
  await screen.findByRole("heading", { name: de.meetingDetail });
  const format = screen.getByRole("combobox", { name: de.exportFormat });
  expect((format as HTMLSelectElement).value).toBe("md");
  for (const selected of ["md", "json"] as const) {
    await userEvent.selectOptions(format, selected);
    await userEvent.click(
      screen.getByRole("button", { name: de.exportTranscript }),
    );
    await waitFor(() =>
      expect(api.exportTranscript).toHaveBeenCalledWith(
        other.id,
        "de",
        selected,
      ),
    );
  }
  await userEvent.selectOptions(format, "txt");
  await userEvent.click(
    screen.getByRole("button", { name: de.exportTranscript }),
  );
  await waitFor(() =>
    expect(api.exportTranscript).toHaveBeenLastCalledWith(other.id, "de"),
  );
  expect(screen.getByText(other.parts[0].text!)).toBeTruthy();
  expect(screen.getByText(other.parts[479].text!)).toBeTruthy();
  expect(document.querySelector(".transcript-part img")).toBeNull();
});

it("does not leave an earlier success message after a later export fails", async () => {
  const { api } = fixture();
  render(<App api={api} />);
  await userEvent.click(
    await screen.findByRole("button", { name: de.exportTranscript }),
  );
  await screen.findByText(/exports\/transcript-synthetic.txt/);
  vi.mocked(api.exportTranscript).mockRejectedValue(
    "Synthetic disk write failure",
  );
  await userEvent.selectOptions(
    screen.getByRole("combobox", { name: de.exportFormat }),
    "json",
  );
  await userEvent.click(
    screen.getByRole("button", { name: de.exportTranscript }),
  );
  await screen.findByRole("alert");
  expect(screen.queryByText(/Transkript gespeichert:/)).toBeNull();
  expect(screen.getByText(meeting.parts[479].text!)).toBeTruthy();
});

it("keeps background-job ownership, truthful confirmed progress and exact open/cancel actions in the compact panel", async () => {
  const progress: NotesProgress = {
    jobId: "own-job",
    runId: null,
    meetingId: "own-meeting",
    status: "running",
    stage: "requesting",
    stepName: "Tasks",
    stepIndex: 3,
    totalSteps: 3,
    batchIndex: 1,
    totalBatches: 2,
    confirmedTasks: 2,
    totalTasks: 6,
    elapsedMs: 65000,
    error: null,
    cancelRequested: false,
  };
  const cancel = vi.fn(),
    open = vi.fn();
  const { rerender } = render(
    <NotesProgressPanel
      progress={progress}
      elapsedMs={65000}
      cancelPending={false}
      feedback={null}
      pollError={null}
      meetingTitle="Owned meeting"
      t={de}
      onCancel={cancel}
      onOpen={open}
      onDismiss={vi.fn()}
      onResume={null}
    />,
  );
  const panel = screen.getByRole("status", { name: de.notesTask });
  expect(within(panel).getByText("01:05")).toBeTruthy();
  expect(
    (within(panel).getByRole("progressbar") as HTMLProgressElement).value,
  ).toBe(2);
  await userEvent.click(
    within(panel).getByRole("button", { name: de.openAnalysisMeeting }),
  );
  await userEvent.click(
    within(panel).getByRole("button", { name: de.cancelAnalysis }),
  );
  expect(open).toHaveBeenCalledTimes(1);
  expect(cancel).toHaveBeenCalledTimes(1);
  rerender(
    <NotesProgressPanel
      progress={{ ...progress, status: "complete" }}
      elapsedMs={65000}
      cancelPending={false}
      feedback={{ kind: "complete", meetingId: "own-meeting" }}
      pollError={null}
      meetingTitle="Owned meeting"
      t={de}
      onCancel={cancel}
      onOpen={open}
      onDismiss={vi.fn()}
      onResume={null}
    />,
  );
  expect(within(panel).getByText(de.notesComplete)).toBeTruthy();
  expect(
    (within(panel).getByRole("progressbar") as HTMLProgressElement).value,
  ).toBe(6);
  expect(within(panel).getByText("100%")).toBeTruthy();
  expect(within(panel).getByText("01:05")).toBeTruthy();
});

it("scrolls to a search source once while polling preserves the reader position", () => {
  const scroll = vi.fn();
  const old = Element.prototype.scrollIntoView;
  Element.prototype.scrollIntoView = scroll;
  try {
    const view = render(
      <TranscriptBody
        parts={meeting.parts}
        locale="en"
        t={en}
        selectedFile="part-1.wav"
      />,
    );
    expect(scroll).toHaveBeenCalledTimes(1);
    view.rerender(
      <TranscriptBody
        parts={[...meeting.parts]}
        locale="en"
        t={en}
        selectedFile="part-1.wav"
      />,
    );
    expect(scroll).toHaveBeenCalledTimes(1);
    view.rerender(
      <TranscriptBody
        parts={meeting.parts}
        locale="en"
        t={en}
        selectedFile="part-2.wav"
      />,
    );
    expect(scroll).toHaveBeenCalledTimes(2);
  } finally {
    Element.prototype.scrollIntoView = old;
  }
});
