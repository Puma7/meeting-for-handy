import { expect, it, vi } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import MeetingLabels from "./views/MeetingLabels";
import SpeakerLabels from "./views/SpeakerLabels";
import HistorySearch from "./views/HistorySearch";
import type { DesktopApi, Meeting, SearchPage } from "./types";
it.each([
  [
    "de",
    "VIP hält das Meeting oben und schützt vor automatischem Aufräumen. Manuelles Löschen bleibt möglich.",
  ],
  [
    "en",
    "VIP keeps the meeting at the top and protects it from automatic cleanup. Manual deletion is still possible.",
  ],
] as const)(
  "explains VIP protection visibly before activation and after pinning (%s)",
  async (locale, explanation) => {
    const edit = vi.fn();
    const api = { editMeetingMetadata: edit } as unknown as DesktopApi;
    const { rerender } = render(
      <MeetingLabels
        api={api}
        meeting={meeting}
        locale={locale}
        onChanged={vi.fn()}
      />,
    );
    for (const pinned of [false, true]) {
      rerender(
        <MeetingLabels
          api={api}
          meeting={{
            ...meeting,
            userMetadata: {
              revision: 0,
              title: null,
              pinned,
              audioRemoved: false,
            },
          }}
          locale={locale}
          onChanged={vi.fn()}
        />,
      );
      const help = screen.getByText(explanation);
      expect(help.hidden).toBe(false);
      expect(help.closest("[hidden], details:not([open])")).toBeNull();
      const button = screen.getByRole("button", { pressed: pinned });
      expect(button.getAttribute("aria-describedby")).toBe(help.id);
      button.focus();
      expect(document.activeElement).toBe(button);
      expect(edit).not.toHaveBeenCalled();
    }
  },
);
const meeting: Meeting = {
  id: "0123456789abcdef0123456789abcdef",
  title: "Original name",
  startedAt: "2026-10-27T06:00:00Z",
  durationMs: 30000,
  status: "complete",
  modelId: "synthetic",
  warnings: [],
  parts: [
    {
      file: "audio/system-0.wav",
      source: "system",
      startSample: 0,
      sampleCount: 480000,
      status: "complete",
      text: "Anna, please read the report.",
    },
  ],
};
it("renames an active meeting without changing its time or transcript and keeps VIP on stale save failure", async () => {
  const edit = vi.fn().mockRejectedValue(new Error("Metadata changed; reload"));
  const changed = vi.fn();
  const original = JSON.stringify(meeting);
  const active = {
    ...meeting,
    status: "recording",
    userMetadata: {
      revision: 7,
      title: null,
      pinned: true,
      audioRemoved: false,
    },
  };
  render(
    <MeetingLabels
      api={{ editMeetingMetadata: edit } as unknown as DesktopApi}
      meeting={active}
      locale="en"
      onChanged={changed}
    />,
  );
  await userEvent.click(screen.getByRole("button", { name: "Rename meeting" }));
  await userEvent.clear(screen.getByRole("textbox", { name: "Meeting name" }));
  await userEvent.type(
    screen.getByRole("textbox", { name: "Meeting name" }),
    "Renamed live meeting",
  );
  await userEvent.click(screen.getByRole("button", { name: "Save name" }));
  await screen.findByRole("alert");
  expect(edit).toHaveBeenCalledWith(
    meeting.id,
    7,
    "Renamed live meeting",
    true,
  );
  expect(changed).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: /VIP/ })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(
    screen.getByRole("button", { name: /VIP/ }).getAttribute("aria-pressed"),
  ).toBe("true");
  expect(JSON.stringify(meeting)).toBe(original);
});
it("does not infer the speaker from a mentioned name and requires explicit confirmation with a literal quote", async () => {
  const doc = {
    revision: 0,
    enabled: false,
    assignments: [],
    sources: { "audio/system-0.wav": "synthetic-current-source-hash" },
  };
  const get = vi.fn().mockResolvedValue(doc);
  const save = vi.fn().mockResolvedValue({ ...doc, revision: 1 });
  render(
    <SpeakerLabels
      api={
        {
          getSpeakerLabels: get,
          saveSpeakerLabels: save,
        } as unknown as DesktopApi
      }
      meeting={meeting}
      locale="en"
    />,
  );
  await screen.findByLabelText("Use confirmed assignments");
  expect(save).not.toHaveBeenCalled();
  await userEvent.selectOptions(
    screen.getByLabelText("Original section"),
    "audio/system-0.wav",
  );
  const button = screen.getByRole("button", {
    name: "Explicitly confirm assignment",
  });
  expect((button as HTMLButtonElement).disabled).toBe(true);
  await userEvent.type(
    screen.getByLabelText("Speaker name confirmed by you"),
    "Confirmed participant",
  );
  await userEvent.type(
    screen.getByLabelText("Literal quote from this section"),
    "Invented evidence",
  );
  expect((button as HTMLButtonElement).disabled).toBe(true);
  await userEvent.clear(
    screen.getByLabelText("Literal quote from this section"),
  );
  await userEvent.type(
    screen.getByLabelText("Literal quote from this section"),
    "please read the report",
  );
  await userEvent.click(button);
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith(meeting.id, 0, true, [
      {
        sourceId: "audio/system-0.wav",
        textSha256: "synthetic-current-source-hash",
        name: "Confirmed participant",
        evidence: "please read the report",
      },
    ]),
  );
});
it("ignores late search responses and opens the source section of the current hit", async () => {
  let resolveOld!: (page: SearchPage) => void;
  const old = new Promise<SearchPage>((resolve) => {
    resolveOld = resolve;
  });
  const page = (title: string): SearchPage => ({
    items: [
      {
        id: meeting.id,
        title,
        startedAt: meeting.startedAt,
        durationMs: 30000,
        status: "complete",
        modelId: "synthetic",
        pinned: false,
        audioRemoved: false,
        metadataRevision: 0,
        sourceId: "audio/system-0.wav",
        startMs: 0,
        endMs: 30000,
        excerpt: "literal evidence",
      },
    ],
    total: 1,
    offset: 0,
    limit: 25,
  });
  const search = vi
    .fn()
    .mockImplementation((query: string) =>
      query === "" ? old : Promise.resolve(page("Current result")),
    );
  const open = vi.fn();
  render(
    <HistorySearch
      api={{ searchMeetings: search } as unknown as DesktopApi}
      locale="en"
      onOpen={open}
    />,
  );
  await waitFor(() => expect(search).toHaveBeenCalledWith("", 0, 25));
  await userEvent.type(screen.getByRole("searchbox"), "report");
  await screen.findByText("Current result");
  await act(async () => {
    resolveOld(page("Stale result"));
    await old;
  });
  expect(screen.queryByText("Stale result")).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Open" }));
  expect(open).toHaveBeenCalledWith(meeting.id, "audio/system-0.wav");
});
it("shows enabled confirmed labels beside unchanged sections only", async () => {
  const doc = {
    revision: 2,
    enabled: true,
    assignments: [
      {
        sourceId: "audio/system-0.wav",
        textSha256: "current",
        name: "Confirmed participant",
        evidence: "please read the report",
      },
      {
        sourceId: "audio/other.wav",
        textSha256: "older-text",
        name: "Stale label",
        evidence: "x",
      },
    ],
    sources: { "audio/system-0.wav": "current", "audio/other.wav": "changed" },
  };
  const labels = vi.fn();
  const api = {
    getSpeakerLabels: vi.fn().mockResolvedValue(doc),
    saveSpeakerLabels: vi.fn(),
  } as unknown as DesktopApi;
  const { rerender } = render(
    <SpeakerLabels api={api} meeting={meeting} locale="en" onLabels={labels} />,
  );
  await waitFor(() =>
    expect(labels).toHaveBeenLastCalledWith(meeting.id, {
      "audio/system-0.wav": "Confirmed participant",
    }),
  );
  (api.getSpeakerLabels as ReturnType<typeof vi.fn>).mockResolvedValue({
    ...doc,
    enabled: false,
  });
  rerender(
    <SpeakerLabels
      key="off"
      api={api}
      meeting={meeting}
      locale="en"
      onLabels={labels}
    />,
  );
  await waitFor(() => expect(labels).toHaveBeenLastCalledWith(meeting.id, {}));
});
it("reloads label hashes when recognition revises a section", async () => {
  const get = vi.fn().mockResolvedValue({
    revision: 1,
    enabled: true,
    assignments: [],
    sources: {},
  });
  const api = {
    getSpeakerLabels: get,
    saveSpeakerLabels: vi.fn(),
  } as unknown as DesktopApi;
  const { rerender } = render(
    <SpeakerLabels api={api} meeting={meeting} locale="en" />,
  );
  await waitFor(() => expect(get).toHaveBeenCalledTimes(1));
  const revised = {
    ...meeting,
    parts: meeting.parts.map((p) => ({ ...p, text: `${p.text} More words.` })),
  };
  rerender(<SpeakerLabels api={api} meeting={revised} locale="en" />);
  await waitFor(() => expect(get).toHaveBeenCalledTimes(2));
});
it("reloads label hashes after a same-length correction early in a long section", async () => {
  const get = vi.fn().mockResolvedValue({
    revision: 1,
    enabled: true,
    assignments: [],
    sources: {},
  });
  const api = {
    getSpeakerLabels: get,
    saveSpeakerLabels: vi.fn(),
  } as unknown as DesktopApi;
  const long = "A".repeat(200) + " confirmed ending of the section";
  const original = {
    ...meeting,
    parts: meeting.parts.map((p) => ({ ...p, text: long })),
  };
  const { rerender } = render(
    <SpeakerLabels api={api} meeting={original} locale="en" />,
  );
  await waitFor(() => expect(get).toHaveBeenCalledTimes(1));
  const corrected = {
    ...meeting,
    parts: meeting.parts.map((p) => ({ ...p, text: "B" + long.slice(1) })),
  };
  rerender(<SpeakerLabels api={api} meeting={corrected} locale="en" />);
  await waitFor(() => expect(get).toHaveBeenCalledTimes(2));
});
