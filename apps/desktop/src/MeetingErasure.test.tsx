import { afterEach, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import {
  act,
  cleanup,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import EraseMeetingDialog from "./views/EraseMeetingDialog";
import HistorySearch from "./views/HistorySearch";
import App from "./App";
import PendingErasures from "./views/PendingErasures";
import { testServices } from "./test-services";
import { useNotesJob } from "./useNotesJob";
import type { DesktopApi, ErasurePreview, MeetingSummary } from "./types";

const dialogTargets = vi.hoisted(() => [] as MeetingSummary[]);
vi.mock("./views/EraseMeetingDialog", async (importOriginal) => {
  const original =
    await importOriginal<typeof import("./views/EraseMeetingDialog")>();
  return {
    default: (props: Parameters<typeof original.default>[0]) => {
      dialogTargets.push(props.meeting);
      return <original.default {...props} />;
    },
  };
});

afterEach(cleanup);
afterEach(() => vi.useRealTimers());
afterEach(() => {
  dialogTargets.length = 0;
});
const meeting: MeetingSummary = {
  id: "b".repeat(32),
  title: "Synthetic two-second meeting",
  startedAt: "2026-10-30T07:00:00Z",
  durationMs: 2000,
  status: "recorded",
  modelId: "synthetic",
};
const plan: ErasurePreview = {
  meetingId: meeting.id,
  fingerprint: "synthetic-file-inventory",
  files: 8,
  bytes: 128000,
  rebuildSearch: true,
};

it.each(["failed", "cancelled"])(
  "keeps only confirmation metadata at the dialog boundary after %s detail erasure",
  async (outcome) => {
    localStorage.clear();
    const full = Object.freeze({
      ...meeting,
      parts: Object.freeze([
        {
          source: "microphone",
          startSample: 0,
          sampleCount: 16000,
          text: "Synthetic transcript must not be owned by confirmation",
          status: "complete",
        },
      ]),
      warnings: Object.freeze(["Synthetic private recording warning"]),
      metadataError: "Synthetic content-bearing diagnostic",
    });
    let deleting = false;
    const props = fixture();
    const api = {
      ...testServices(),
      ...props.api,
      getAppState: vi.fn().mockResolvedValue({
        version: "synthetic",
        handy: null,
        recording: false,
        activeMeeting: null,
      }),
      listDevices: vi.fn().mockResolvedValue([]),
      listMeetings: vi.fn().mockResolvedValue([meeting]),
      getMeeting: vi.fn().mockResolvedValue(full),
      listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
      getNotesProgress: vi.fn().mockResolvedValue(null),
      listPendingErasures: vi.fn().mockResolvedValue([]),
      eraseMeetingNow: vi.fn(async () => {
        deleting = true;
        return "erase-job";
      }),
      getMaintenanceProgress: vi.fn(async () => ({
        jobId: deleting ? "erase-job" : "preview-job",
        kind: "meeting-erasure",
        status: deleting ? outcome : "complete",
        confirmedTasks: 0,
        totalTasks: 8,
        elapsedMs: 1,
        skipped: 0,
        error: deleting ? "Synthetic erasure terminal" : null,
        cancelRequested: outcome === "cancelled",
      })),
    } as unknown as DesktopApi;
    render(<App api={api} />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Historie/ }),
    );
    const row = (await screen.findByText(meeting.title)).closest(
      ".session-row",
    )!;
    await userEvent.click(
      within(row as HTMLElement).getByRole("button", { name: "Öffnen" }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Meeting löschen" }),
    );
    await userEvent.type(
      await screen.findByRole("textbox", {
        name: "Zur Bestätigung LÖSCHEN eingeben",
      }),
      "LÖSCHEN",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Jetzt endgültig löschen" }),
    );
    await screen.findByText("Synthetic erasure terminal");
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    expect(
      screen.queryByText(
        "Meetinginhalte vollständig aus der HandyMeeting-Ablage entfernt.",
      ),
    ).toBeNull();
    expect(dialogTargets.length).toBeGreaterThan(0);
    for (const target of dialogTargets) {
      expect(Object.keys(target).sort()).toEqual([
        "durationMs",
        "id",
        "modelId",
        "startedAt",
        "status",
        "title",
      ]);
      expect(target).not.toBe(full);
    }
    expect(full.parts[0].text).toBe(
      "Synthetic transcript must not be owned by confirmation",
    );
    expect(full.warnings).toEqual(["Synthetic private recording warning"]);
    expect(api.eraseMeetingNow).toHaveBeenCalledOnce();
  },
);

it.each(["close button", "Escape"])(
  "returns focus to the History erasure opener after closing with %s",
  async (action) => {
    const props = fixture();
    const api = {
      ...testServices(),
      ...props.api,
      getAppState: vi.fn(async () => ({
        version: "synthetic",
        handy: null,
        recording: false,
        activeMeeting: null,
      })),
      listDevices: vi.fn().mockResolvedValue([]),
      listMeetings: vi.fn().mockResolvedValue([meeting]),
      listPendingErasures: vi.fn().mockResolvedValue([]),
    } as unknown as DesktopApi;
    render(
      <StrictMode>
        <App api={api} />
      </StrictMode>,
    );
    await userEvent.click(
      await screen.findByRole("button", { name: /Historie/ }),
    );
    const opener = await screen.findByRole("button", {
      name: `Meeting dauerhaft löschen: ${meeting.title}`,
    });
    await userEvent.click(opener);
    await screen.findByRole("textbox", {
      name: "Zur Bestätigung LÖSCHEN eingeben",
    });
    expect(
      screen.getByRole("alertdialog").contains(document.activeElement),
    ).toBe(true);
    expect(document.querySelector("main")?.inert).toBe(true);
    if (action === "Escape") await userEvent.keyboard("{Escape}");
    else
      await userEvent.click(screen.getByRole("button", { name: "Schließen" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(opener));
    expect(document.querySelector("main")?.inert).toBe(false);
    expect(api.eraseMeetingNow).not.toHaveBeenCalled();
  },
);

it.each(["failed", "cancelled"] as const)(
  "keeps all retained meetings in desktop history after a %s post-erasure rebuild",
  async (outcome) => {
    let erased = false;
    let job = "preview-job";
    const retained = [
      { ...meeting, id: "c".repeat(32), title: "Synthetic retained first" },
      { ...meeting, id: "d".repeat(32), title: "Synthetic retained second" },
    ];
    const hit = (item: MeetingSummary, sourceId: string | null = null) => ({
      ...item,
      pinned: false,
      audioRemoved: false,
      metadataRevision: 0,
      sourceId,
      startMs: sourceId ? 0 : null,
      endMs: sourceId ? 1000 : null,
      excerpt: sourceId ? "Synthetic indexed needle quote" : "",
    });
    const api = {
      ...testServices(),
      getAppState: vi.fn(async () => ({
        version: "synthetic",
        handy: null,
        recording: false,
        activeMeeting: null,
      })),
      listDevices: vi.fn().mockResolvedValue([]),
      listMeetings: vi.fn(async () =>
        erased ? retained : [meeting, ...retained],
      ),
      searchMeetings: vi.fn(async (query: string) => {
        const items = query.trim()
          ? [hit(retained[0], "section-one")]
          : erased
            ? outcome === "failed"
              ? []
              : [hit(retained[0])]
            : [meeting, ...retained].map((item) => hit(item));
        return { items, total: items.length, offset: 0, limit: 25 };
      }),
      previewMeetingErasure: vi.fn(async () => (job = "preview-job")),
      getMeetingErasurePreview: vi.fn().mockResolvedValue(plan),
      eraseMeetingNow: vi.fn(async () => {
        erased = true;
        return (job = "erase-job");
      }),
      getMaintenanceProgress: vi.fn(async () => ({
        jobId: job,
        kind: "meeting-erasure",
        status: "complete",
        confirmedTasks: 8,
        totalTasks: 8,
        elapsedMs: 1,
        skipped: 0,
        error: null,
        cancelRequested: false,
      })),
      listPendingErasures: vi.fn().mockResolvedValue([]),
      rebuildSearchIndex:
        outcome === "failed"
          ? vi
              .fn()
              .mockRejectedValue(new Error("Synthetic index rebuild failure"))
          : vi.fn().mockResolvedValue("cancelled-index-job"),
      getLibraryProgress: vi.fn(async () => ({
        jobId: "cancelled-index-job",
        kind: "search-index",
        status: outcome,
        confirmedTasks: 1,
        totalTasks: 2,
        elapsedMs: 1,
        skipped: 0,
        error: null,
        cancelRequested: outcome === "cancelled",
      })),
      cancelMaintenance: vi.fn().mockResolvedValue(true),
    } as unknown as DesktopApi;
    render(<App api={api} />);
    await userEvent.click(
      await screen.findByRole("button", { name: /Historie/ }),
    );
    await userEvent.click(
      await screen.findByRole("button", {
        name: `Meeting dauerhaft löschen: ${meeting.title}`,
      }),
    );
    await userEvent.type(
      await screen.findByRole("textbox", {
        name: "Zur Bestätigung LÖSCHEN eingeben",
      }),
      "LÖSCHEN",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Jetzt endgültig löschen" }),
    );
    await screen.findByText(
      "Meetinginhalte vollständig aus der HandyMeeting-Ablage entfernt.",
    );
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole("heading", { name: "Historie" }),
      ),
    );
    expect(api.rebuildSearchIndex).toHaveBeenCalledOnce();
    const query = screen.getByRole("searchbox");
    await userEvent.type(query, "needle");
    await screen.findByText("Synthetic indexed needle quote");
    expect(screen.queryByText(retained[1].title)).toBeNull();
    await userEvent.clear(query);
    expect(await screen.findByText(retained[0].title)).toBeTruthy();
    expect(await screen.findByText(retained[1].title)).toBeTruthy();
    expect(screen.queryByText(meeting.title)).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: "Aktualisieren" }),
    );
    expect(await screen.findByText(retained[1].title)).toBeTruthy();
  },
);
function fixture() {
  let active = "preview-job";
  const api = {
    previewMeetingErasure: vi.fn().mockResolvedValue(active),
    getMeetingErasurePreview: vi.fn().mockResolvedValue(plan),
    eraseMeetingNow: vi.fn().mockImplementation(async () => {
      active = "erase-job";
      return active;
    }),
    getMaintenanceProgress: vi.fn().mockImplementation(async () => ({
      jobId: active,
      kind: "meeting-erasure",
      status: "complete",
      confirmedTasks: 8,
      totalTasks: 8,
      elapsedMs: 5,
      skipped: 0,
      error: null,
      cancelRequested: false,
    })),
    cancelMaintenance: vi.fn().mockResolvedValue(true),
  } as unknown as DesktopApi;
  return {
    api,
    onStart: vi.fn(),
    onDeleted: vi.fn(),
    onStopped: vi.fn(),
    onClose: vi.fn(),
  };
}

it.each(["preview", "erase"])(
  "can close after a stopped pending %s command rejects without a job ID",
  async (phase) => {
    const props = fixture();
    let reject!: (error: Error) => void;
    const pending = new Promise<string>((_, fail) => {
      reject = fail;
    });
    if (phase === "preview")
      props.api.previewMeetingErasure = vi.fn(() => pending);
    else props.api.eraseMeetingNow = vi.fn(() => pending);
    render(<EraseMeetingDialog {...props} meeting={meeting} locale="en" />);
    if (phase === "erase") {
      await userEvent.type(
        await screen.findByRole("textbox", { name: "Type DELETE to confirm" }),
        "DELETE",
      );
      await userEvent.click(
        screen.getByRole("button", { name: "Permanently delete now" }),
      );
    } else
      await waitFor(() =>
        expect(props.api.previewMeetingErasure).toHaveBeenCalledOnce(),
      );
    const stop = screen.getByRole("button", {
      name: phase === "preview" ? "Cancel check" : "Stop remaining deletion",
    });
    await userEvent.click(stop);
    expect((stop as HTMLButtonElement).disabled).toBe(true);
    await act(async () =>
      reject(new Error("Synthetic pending command rejected")),
    );
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Synthetic pending command rejected",
    );
    const close = screen.getByRole("button", { name: "Close" });
    expect((close as HTMLButtonElement).disabled).toBe(false);
    expect(props.api.cancelMaintenance).not.toHaveBeenCalled();
    expect(props.onDeleted).not.toHaveBeenCalled();
    expect(props.onStopped).toHaveBeenCalledTimes(phase === "erase" ? 1 : 0);
    await userEvent.click(close);
    expect(props.onClose).toHaveBeenCalledOnce();
  },
);

it("paginates authoritative history without an index and clamps a removed final page", async () => {
  const native = Array.from({ length: 26 }, (_, i) => ({
    ...meeting,
    id: i.toString(16).padStart(32, "0"),
    title: `Synthetic retained row ${i + 1}`,
  }));
  const api = {
    searchMeetings: vi
      .fn()
      .mockResolvedValue({ items: [], total: 0, offset: 0, limit: 25 }),
  } as unknown as DesktopApi;
  const refresh = vi.fn();
  const props = {
    api,
    locale: "de" as const,
    nativeMeetings: native,
    onOpen: vi.fn(),
    onRefresh: refresh,
  };
  const view = render(<HistorySearch {...props} />);
  expect(await screen.findByText(native[0].title)).toBeTruthy();
  expect(screen.queryByText(native[25].title)).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Weiter" }));
  expect(await screen.findByText(native[25].title)).toBeTruthy();
  expect(screen.queryByText(native[0].title)).toBeNull();
  view.rerender(
    <HistorySearch {...props} hiddenMeetingIds={new Set([native[25].id])} />,
  );
  expect(await screen.findByText(native[0].title)).toBeTruthy();
  expect(screen.queryByText(native[25].title)).toBeNull();
  expect(screen.queryByRole("button", { name: "Weiter" })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Aktualisieren" }));
  expect(refresh).toHaveBeenCalledOnce();
  expect(api.searchMeetings).not.toHaveBeenCalled();
});

it("keeps an older VIP on the first history page before newer unpinned meetings", async () => {
  const recent = Array.from({ length: 26 }, (_, i) => ({
    ...meeting,
    id: (i + 1).toString(16).padStart(32, "0"),
    title: `Synthetic recent ${i + 1}`,
  }));
  const vip = {
    ...meeting,
    id: "e".repeat(32),
    title: "Synthetic older VIP",
    startedAt: "2026-10-29T07:00:00Z",
    userMetadata: {
      revision: 1,
      title: null,
      pinned: true,
      audioRemoved: false,
    },
  };
  const native = [...recent, vip];
  const originalIds = native.map((item) => item.id);
  Object.freeze(native);
  const api = { searchMeetings: vi.fn() } as unknown as DesktopApi;
  const view = render(
    <HistorySearch
      api={api}
      locale="de"
      nativeMeetings={native}
      onOpen={vi.fn()}
    />,
  );
  expect(await screen.findByText(vip.title)).toBeTruthy();
  expect(
    Array.from(
      view.container.querySelectorAll(".session-row strong"),
      (node) => node.textContent,
    )[0],
  ).toBe(vip.title);
  expect(screen.getByLabelText("VIP")).toBeTruthy();
  await userEvent.click(screen.getByRole("button", { name: "Weiter" }));
  expect(screen.queryByText(vip.title)).toBeNull();
  expect(screen.getByText(recent[24].title)).toBeTruthy();
  expect(screen.getByText(recent[25].title)).toBeTruthy();
  expect(native.map((item) => item.id)).toEqual(originalIds);
  expect(api.searchMeetings).not.toHaveBeenCalled();
});

it("uses the index's date and ID tie order within each VIP group without mutating summaries", () => {
  const row = (id: string, title: string, date: string, pinned: boolean) => ({
    ...meeting,
    id: id.repeat(32),
    title,
    startedAt: date,
    userMetadata: { revision: 1, title: null, pinned, audioRemoved: false },
  });
  const older = "2026-10-29T07:00:00Z",
    newer = "2026-10-30T07:00:00Z";
  const native = [
    row("d", "Synthetic unpinned tie D", newer, false),
    row("b", "Synthetic VIP tie B", older, true),
    row("e", "Synthetic oldest ordinary", older, false),
    row("a", "Synthetic VIP tie A", older, true),
    row("c", "Synthetic unpinned tie C", newer, false),
    row("f", "Synthetic newest VIP", newer, true),
  ];
  const original = JSON.stringify(native);
  Object.freeze(native);
  const view = render(
    <HistorySearch
      api={{ searchMeetings: vi.fn() } as unknown as DesktopApi}
      locale="de"
      nativeMeetings={native}
      onOpen={vi.fn()}
    />,
  );
  expect(
    Array.from(
      view.container.querySelectorAll(".session-row strong"),
      (node) => node.textContent,
    ),
  ).toEqual([
    "Synthetic newest VIP",
    "Synthetic VIP tie A",
    "Synthetic VIP tie B",
    "Synthetic unpinned tie C",
    "Synthetic unpinned tie D",
    "Synthetic oldest ordinary",
  ]);
  expect(JSON.stringify(native)).toBe(original);
});

it("keeps observing a started deletion when delayed cancellation rejects", async () => {
  const props = fixture();
  let resolve!: (id: string) => void;
  let started = false;
  props.api.eraseMeetingNow = vi.fn().mockImplementation(
    () =>
      new Promise<string>((done) => {
        resolve = done;
      }),
  );
  props.api.cancelMaintenance = vi
    .fn()
    .mockRejectedValue(new Error("Synthetic cancellation IPC failure"));
  props.api.getMaintenanceProgress = vi.fn().mockImplementation(async () => ({
    jobId: started ? "erase-job" : "preview-job",
    status: "complete",
    confirmedTasks: 8,
    totalTasks: 8,
    elapsedMs: 0,
    skipped: 0,
    error: null,
    cancelRequested: false,
  }));
  render(<EraseMeetingDialog {...props} meeting={meeting} locale="en" />);
  await userEvent.type(
    await screen.findByRole("textbox", { name: "Type DELETE to confirm" }),
    "DELETE",
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Permanently delete now" }),
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Stop remaining deletion" }),
  );
  started = true;
  await act(async () => resolve("erase-job"));
  await waitFor(() =>
    expect(props.api.cancelMaintenance).toHaveBeenCalledWith("erase-job"),
  );
  expect(props.onStopped).not.toHaveBeenCalled();
  await waitFor(() => expect(props.onDeleted).toHaveBeenCalledOnce());
  expect(props.api.eraseMeetingNow).toHaveBeenCalledOnce();
});

it("bounds failed deletion status polls, keeps admission and can inspect cancellation later", async () => {
  vi.useFakeTimers();
  const props = fixture();
  let deleting = false,
    recovered = false,
    terminalStatus = "unrecognized";
  props.api.eraseMeetingNow = vi.fn().mockImplementation(async () => {
    deleting = true;
    return "erase-job";
  });
  props.api.getMaintenanceProgress = vi.fn().mockImplementation(async () => {
    if (deleting && !recovered) throw new Error("Synthetic IPC read failure");
    return {
      jobId: deleting ? "erase-job" : "preview-job",
      status: deleting ? terminalStatus : "complete",
      confirmedTasks: 0,
      totalTasks: 8,
      elapsedMs: 0,
      skipped: 0,
      error: null,
      cancelRequested: deleting,
    };
  });
  render(<EraseMeetingDialog {...props} meeting={meeting} locale="en" />);
  await act(() => vi.advanceTimersByTimeAsync(0));
  fireEvent.change(
    screen.getByRole("textbox", { name: "Type DELETE to confirm" }),
    { target: { value: "DELETE" } },
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Permanently delete now" }),
  );
  await act(() => vi.advanceTimersByTimeAsync(10000));
  const calls = vi.mocked(props.api.getMaintenanceProgress!).mock.calls.length;
  expect(calls).toBe(4);
  expect(props.api.cancelMaintenance).toHaveBeenCalledWith("erase-job");
  await act(() => vi.advanceTimersByTimeAsync(10000));
  expect(props.api.getMaintenanceProgress).toHaveBeenCalledTimes(calls);
  expect(props.onStopped).not.toHaveBeenCalled();
  expect(props.onDeleted).not.toHaveBeenCalled();
  expect(props.api.eraseMeetingNow).toHaveBeenCalledOnce();
  expect(
    screen.queryByRole("button", { name: "Permanently delete now" }),
  ).toBeNull();
  recovered = true;
  screen.getByRole("button", { name: "Check job status again" }).focus();
  await act(async () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Check job status again" }),
    ),
  );
  expect(screen.getByRole("alertdialog").contains(document.activeElement)).toBe(
    true,
  );
  await act(() => vi.advanceTimersByTimeAsync(10000));
  expect(props.onStopped).not.toHaveBeenCalled();
  terminalStatus = "cancelled";
  await act(async () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Check job status again" }),
    ),
  );
  expect(props.onStopped).toHaveBeenCalledOnce();
  expect(props.onDeleted).not.toHaveBeenCalled();
});

it("discloses unattributable atomic remnants before confirming erasure", async () => {
  const props = fixture();
  props.api.getMeetingErasurePreview = vi
    .fn()
    .mockResolvedValue({ ...plan, preservedAtomicRemnants: 2 });
  render(<EraseMeetingDialog {...props} meeting={meeting} locale="en" />);
  expect(
    await screen.findByText(/2 interrupted records cannot be assigned/),
  ).toBeTruthy();
  expect(props.api.eraseMeetingNow).not.toHaveBeenCalled();
});

it("offers direct permanent deletion on the exact short meeting in searchable history", async () => {
  const remove = vi.fn();
  render(
    <HistorySearch
      api={
        {
          searchMeetings: vi.fn().mockResolvedValue({
            items: [
              {
                ...meeting,
                pinned: false,
                audioRemoved: false,
                metadataRevision: 0,
                sourceId: null,
                startMs: null,
                endMs: null,
                excerpt: "",
              },
            ],
            offset: 0,
            total: 1,
            limit: 25,
          }),
        } as unknown as DesktopApi
      }
      locale="de"
      onOpen={vi.fn()}
      onErase={remove}
    />,
  );
  await screen.findByText(meeting.title);
  await userEvent.click(
    screen.getByRole("button", {
      name: `Meeting dauerhaft löschen: ${meeting.title}`,
    }),
  );
  expect(remove).toHaveBeenCalledWith(
    expect.objectContaining({ id: meeting.id, durationMs: 2000 }),
  );
});

it("requires the exact confirmation word and binds the call to the inspected meeting", async () => {
  const props = fixture();
  render(<EraseMeetingDialog {...props} meeting={meeting} locale="de" />);
  const input = await screen.findByRole("textbox", {
    name: "Zur Bestätigung LÖSCHEN eingeben",
  });
  const button = screen.getByRole("button", {
    name: "Jetzt endgültig löschen",
  });
  expect((button as HTMLButtonElement).disabled).toBe(true);
  await userEvent.type(input, "ja");
  await userEvent.type(input, "{enter}");
  expect(props.api.eraseMeetingNow).not.toHaveBeenCalled();
  await userEvent.clear(input);
  await userEvent.type(input, "LÖSCHEN");
  await userEvent.click(button);
  await waitFor(() => expect(props.onDeleted).toHaveBeenCalledOnce());
  expect(props.api.eraseMeetingNow).toHaveBeenCalledWith(
    "preview-job",
    meeting.id,
    "LÖSCHEN",
  );
  expect(props.onStart).toHaveBeenCalledOnce();
});

it("does not erase on preview cancellation and cancels a job ID that arrives late", async () => {
  const props = fixture();
  let resolve!: (id: string) => void;
  props.api.previewMeetingErasure = vi.fn().mockImplementation(
    () =>
      new Promise<string>((done) => {
        resolve = done;
      }),
  );
  render(<EraseMeetingDialog {...props} meeting={meeting} locale="de" />);
  await waitFor(() =>
    expect(props.api.previewMeetingErasure).toHaveBeenCalledOnce(),
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Prüfung abbrechen" }),
  );
  await act(async () => resolve("late-preview-job"));
  await waitFor(() =>
    expect(props.api.cancelMaintenance).toHaveBeenCalledWith(
      "late-preview-job",
    ),
  );
  expect(props.api.eraseMeetingNow).not.toHaveBeenCalled();
  expect(props.onDeleted).not.toHaveBeenCalled();
});

it("rejects a preview for another meeting rather than reusing its permission", async () => {
  const props = fixture();
  props.api.getMeetingErasurePreview = vi
    .fn()
    .mockResolvedValue({ ...plan, meetingId: "c".repeat(32) });
  render(<EraseMeetingDialog {...props} meeting={meeting} locale="de" />);
  await screen.findByRole("alert");
  expect(
    screen.queryByRole("button", { name: "Jetzt endgültig löschen" }),
  ).toBeNull();
  expect(props.api.eraseMeetingNow).not.toHaveBeenCalled();
});

it("keeps cancellation and partial failure explicit and never reports failed deletion as complete", async () => {
  const props = fixture();
  props.api.getMaintenanceProgress = vi.fn().mockImplementation(async () => ({
    jobId:
      props.api.eraseMeetingNow &&
      vi.mocked(props.api.eraseMeetingNow).mock.calls.length
        ? "erase-job"
        : "preview-job",
    status:
      props.api.eraseMeetingNow &&
      vi.mocked(props.api.eraseMeetingNow).mock.calls.length
        ? "failed"
        : "complete",
    confirmedTasks: 1,
    totalTasks: 8,
    error: "Synthetic locked file; remaining data retained",
  }));
  render(<EraseMeetingDialog {...props} meeting={meeting} locale="en" />);
  await userEvent.type(
    await screen.findByRole("textbox", { name: "Type DELETE to confirm" }),
    "DELETE",
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Permanently delete now" }),
  );
  await screen.findByText("Synthetic locked file; remaining data retained");
  expect(props.onDeleted).not.toHaveBeenCalled();
  expect(props.onStopped).toHaveBeenCalledOnce();
  expect(screen.getByText(/Files already removed stay deleted/)).toBeTruthy();
});

it("starts focus on the safe control, traps keyboard focus and Escape never confirms deletion", async () => {
  const props = fixture();
  render(<EraseMeetingDialog {...props} meeting={meeting} locale="en" />);
  const input = await screen.findByRole("textbox", {
    name: "Type DELETE to confirm",
  });
  const close = screen.getByRole("button", { name: "Close" });
  expect(document.activeElement).toBe(close);
  await userEvent.type(input, "DELETE");
  const remove = screen.getByRole("button", { name: "Permanently delete now" });
  remove.focus();
  await userEvent.tab();
  expect(document.activeElement).toBe(input);
  await userEvent.keyboard("{Escape}");
  expect(props.onClose).toHaveBeenCalledOnce();
  expect(props.api.eraseMeetingNow).not.toHaveBeenCalled();
});

it.each(["de", "en"] as const)(
  "does not claim unfinished deletion when pending erasure listing fails (%s)",
  async (locale) => {
    const resume = vi.fn();
    const erase = vi.fn();
    const api = {
      listPendingErasures: vi
        .fn()
        .mockRejectedValueOnce(new Error("Synthetic pending-list read failure"))
        .mockResolvedValueOnce([]),
      eraseMeetingNow: erase,
    } as unknown as DesktopApi;
    const props = { api, locale, onResume: resume };
    const view = render(<PendingErasures {...props} revision={0} />);
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Synthetic pending-list read failure",
    );
    expect(
      screen.getByRole("heading", {
        name:
          locale === "de"
            ? "Unterbrochene Löschungen konnten nicht gelesen werden"
            : "Interrupted deletions could not be read",
      }),
    ).toBeTruthy();
    expect(
      screen.queryByText(
        locale === "de"
          ? "Löschung noch nicht vollständig abgeschlossen"
          : "Deletion is not yet complete",
      ),
    ).toBeNull();
    expect(
      screen.queryByText(
        locale === "de"
          ? "Verbleibende Daten können erneut geprüft und nach Bestätigung entfernt werden."
          : "Inspect remaining data and confirm again to finish removing it.",
      ),
    ).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
    expect(resume).not.toHaveBeenCalled();
    expect(erase).not.toHaveBeenCalled();

    view.rerender(<PendingErasures {...props} revision={1} />);
    await waitFor(() => {
      expect(api.listPendingErasures).toHaveBeenCalledTimes(2);
      expect(screen.queryByRole("region")).toBeNull();
    });
  },
);

it.each(["de", "en"] as const)(
  "keeps known unfinished work when pending erasure listing refresh fails (%s)",
  async (locale) => {
    const resume = vi.fn();
    const api = {
      listPendingErasures: vi
        .fn()
        .mockResolvedValueOnce([
          { meetingId: meeting.id, confirmedAt: meeting.startedAt },
        ])
        .mockRejectedValueOnce(
          new Error("Synthetic pending-list refresh failure"),
        ),
    } as unknown as DesktopApi;
    const props = { api, locale, onResume: resume };
    const view = render(<PendingErasures {...props} revision={0} />);
    const action =
      locale === "de" ? "Prüfen und fortsetzen" : "Inspect and continue";
    await screen.findByRole("button", { name: action });
    view.rerender(<PendingErasures {...props} revision={1} />);
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Synthetic pending-list refresh failure",
    );
    expect(
      screen.getByRole("heading", {
        name:
          locale === "de"
            ? "Löschung noch nicht vollständig abgeschlossen"
            : "Deletion is not yet complete",
      }),
    ).toBeTruthy();
    expect(
      screen.queryByText(
        locale === "de"
          ? "Unterbrochene Löschungen konnten nicht gelesen werden"
          : "Interrupted deletions could not be read",
      ),
    ).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: action }));
    expect(resume).toHaveBeenCalledWith(
      expect.objectContaining({ id: meeting.id, status: "erasure-pending" }),
    );
  },
);

it("can resume an interrupted erasure even when the original meeting title is already gone", async () => {
  const resume = vi.fn();
  render(
    <PendingErasures
      api={
        {
          listPendingErasures: vi
            .fn()
            .mockResolvedValue([
              { meetingId: meeting.id, confirmedAt: meeting.startedAt },
            ]),
        } as unknown as DesktopApi
      }
      locale="de"
      revision={0}
      onResume={resume}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "Prüfen und fortsetzen" }),
  );
  expect(resume).toHaveBeenCalledWith(
    expect.objectContaining({ id: meeting.id, status: "erasure-pending" }),
  );
});

it.each([0, 2])(
  "clears deleted state and retains the final warning for %s ambiguous records",
  async (remnants) => {
    localStorage.clear();
    const full = { ...meeting, parts: [], warnings: [] };
    const other = {
      ...meeting,
      id: "c".repeat(32),
      title: "Other meeting retained",
    };
    let erased = false,
      activeJob = "preview-job";
    let deletionCanComplete = false;
    let stale!: (value: unknown) => void;
    let stateCalls = 0;
    const initial = {
      version: "synthetic",
      handy: null,
      recording: false,
      activeMeeting: full,
    };
    const api = {
      ...testServices(),
      getAppState: vi.fn().mockImplementation(() => {
        stateCalls++;
        if (stateCalls === 2)
          return new Promise((done) => {
            stale = done;
          });
        return Promise.resolve(
          erased ? { ...initial, activeMeeting: null } : initial,
        );
      }),
      listDevices: vi.fn().mockResolvedValue([]),
      listMeetings: vi
        .fn()
        .mockImplementation(async () => (erased ? [other] : [meeting, other])),
      getMeeting: vi.fn().mockResolvedValue(full),
      listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
      getNotesProgress: vi.fn().mockResolvedValue(null),
      previewMeetingErasure: vi.fn().mockResolvedValue("preview-job"),
      getMeetingErasurePreview: vi
        .fn()
        .mockResolvedValue({ ...plan, preservedAtomicRemnants: remnants }),
      eraseMeetingNow: vi.fn().mockImplementation(async () => {
        erased = true;
        activeJob = "erase-job";
        return activeJob;
      }),
      getMaintenanceProgress: vi.fn().mockImplementation(async () => ({
        jobId: activeJob,
        status:
          activeJob === "erase-job" && !deletionCanComplete
            ? "running"
            : "complete",
        confirmedTasks: 8,
        totalTasks: 8,
        error: null,
      })),
      cancelMaintenance: vi.fn().mockResolvedValue(true),
      listPendingErasures: vi.fn().mockResolvedValue([]),
      rebuildSearchIndex: vi.fn().mockResolvedValue("index-job"),
    } as unknown as DesktopApi;
    render(<App api={api} />);
    await waitFor(() => expect(stateCalls).toBeGreaterThanOrEqual(2));
    await userEvent.click(screen.getByRole("button", { name: /Historie/ }));
    const row = (await screen.findByText(meeting.title)).closest(
      ".session-row",
    )!;
    await userEvent.click(
      within(row as HTMLElement).getByRole("button", { name: "Öffnen" }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Meeting löschen" }),
    );
    await userEvent.type(
      await screen.findByRole("textbox", {
        name: "Zur Bestätigung LÖSCHEN eingeben",
      }),
      "LÖSCHEN",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Jetzt endgültig löschen" }),
    );
    const dialog = screen.getByRole("alertdialog");
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect((document.querySelector("main") as HTMLElement).inert).toBe(true);
    await userEvent.tab();
    expect(dialog.contains(document.activeElement)).toBe(true);
    deletionCanComplete = true;
    const notice = await screen.findByText(
      /Meetinginhalte vollständig aus der HandyMeeting-Ablage entfernt|unterbrochene Dateien sind nicht sicher zuordenbar/,
    );
    if (remnants)
      expect(notice.textContent).toMatch(
        /2 unterbrochene Dateien sind nicht sicher zuordenbar und bleiben erhalten/,
      );
    else
      expect(notice.textContent).toBe(
        "Meetinginhalte vollständig aus der HandyMeeting-Ablage entfernt.",
      );
    await act(async () => stale(initial));
    expect(screen.queryByText(meeting.title)).toBeNull();
    expect(screen.getByText(other.title)).toBeTruthy();
    expect(api.rebuildSearchIndex).toHaveBeenCalledOnce();
  },
);

it("a confirmed erasure discards delayed analysis results and its saved retry request", async () => {
  let reply!: (value: unknown) => void;
  const resultCallback = vi.fn();
  const api = {
    generateNotes: vi.fn().mockImplementation(
      () =>
        new Promise((done) => {
          reply = done;
        }),
    ),
    getNotesProgress: vi.fn().mockResolvedValue(null),
  } as unknown as DesktopApi;
  const hook = renderHook(() => useNotesJob(api, false, false, resultCallback));
  let pending!: Promise<void>;
  act(() => {
    pending = hook.result.current.run({
      meetingId: meeting.id,
      template: { id: "synthetic", name: "Synthetic", steps: [] },
      endpoint: "http://127.0.0.1:1235/v1/chat/completions",
      model: "synthetic",
      mode: "local",
      cloudConsent: false,
      apiKey: "",
    });
  });
  expect(hook.result.current.lastRequest?.meetingId).toBe(meeting.id);
  act(() => hook.result.current.forget(meeting.id, true));
  await act(async () => {
    reply({ id: "synthetic", meetingId: meeting.id, sections: [] });
    await pending;
  });
  expect(resultCallback).not.toHaveBeenCalled();
  expect(hook.result.current.lastRequest).toBeNull();
  expect(hook.result.current.feedback).toBeNull();
  expect(hook.result.current.progress).toBeNull();
  expect(hook.result.current.busy).toBe(false);
});
