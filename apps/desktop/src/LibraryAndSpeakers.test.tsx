import { expect, it, vi } from "vitest";
import {
  render,
  screen,
  waitFor,
  act,
  fireEvent,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import MeetingLabels from "./views/MeetingLabels";
import SpeakerLabels from "./views/SpeakerLabels";
import HistorySearch, { initialHistorySearch } from "./views/HistorySearch";
import type { DesktopApi, Meeting, SearchPage } from "./types";
it.each([
  [
    "de",
    "VIP hält das Meeting oben in der Historie und schützt vor automatischem Aufräumen. Manuelles Löschen bleibt möglich.",
    "Oben in der Historie anheften und vor automatischem Aufräumen schützen. Manuelles Löschen bleibt möglich.",
    "VIP lösen und Schutz vor automatischem Aufräumen aufheben.",
  ],
  [
    "en",
    "VIP keeps the meeting at the top of history and protects it from automatic cleanup. Manual deletion is still possible.",
    "Pin to the top of history and protect from automatic cleanup. Manual deletion is still possible.",
    "Unpin VIP and remove protection from automatic cleanup.",
  ],
] as const)(
  "explains VIP protection visibly before activation and after pinning (%s)",
  async (locale, explanation, pinAction, unpinAction) => {
    const unpinned = {
      revision: 1,
      title: null,
      pinned: false,
      audioRemoved: false,
    };
    const edit = vi.fn().mockResolvedValue(unpinned);
    const changed = vi.fn();
    const api = { editMeetingMetadata: edit } as unknown as DesktopApi;
    const { rerender } = render(
      <MeetingLabels
        api={api}
        meeting={meeting}
        locale={locale}
        onChanged={changed}
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
          onChanged={changed}
        />,
      );
      const help = screen.getByText(explanation);
      expect(help.hidden).toBe(false);
      expect(help.closest("[hidden], details:not([open])")).toBeNull();
      const button = screen.getByRole("button", { pressed: pinned });
      expect(button.getAttribute("aria-describedby")).toBe(help.id);
      expect(button.getAttribute("title")).toBe(
        pinned ? unpinAction : pinAction,
      );
      button.focus();
      expect(document.activeElement).toBe(button);
      expect(edit).not.toHaveBeenCalled();
    }
    await userEvent.click(screen.getByRole("button", { pressed: true }));
    expect(edit).toHaveBeenCalledExactlyOnceWith(meeting.id, 0, null, false);
    expect(changed).toHaveBeenCalledExactlyOnceWith(unpinned);
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

it("returns to a valid indexed page when deletion empties the last page", async () => {
  let matches: SearchPage["items"] = Array.from({ length: 26 }, (_, index) => ({
    ...meeting,
    id: `synthetic-search-${index + 1}`,
    title: `Indexed hit ${index + 1}`,
    pinned: false,
    audioRemoved: false,
    metadataRevision: 0,
    sourceId: "audio/system-0.wav",
    startMs: 0,
    endMs: 30000,
    excerpt: "synthetic report",
  }));
  const search = vi.fn(
    async (_query: string, offset: number, limit: number) => ({
      items: matches.slice(offset, offset + limit),
      total: matches.length,
      offset,
      limit,
    }),
  );
  const onStateChange = vi.fn();
  render(
    <HistorySearch
      api={{ searchMeetings: search } as unknown as DesktopApi}
      locale="en"
      initialState={{ ...initialHistorySearch, query: "report" }}
      onStateChange={onStateChange}
      onOpen={vi.fn()}
      onErase={(target) => {
        matches = matches.filter((item) => item.id !== target.id);
        window.dispatchEvent(new Event("library-index-updated"));
      }}
    />,
  );
  await screen.findByText("Indexed hit 25");
  await userEvent.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByText("Indexed hit 26");
  await userEvent.click(
    screen.getByRole("button", {
      name: "Permanently delete meeting: Indexed hit 26",
    }),
  );
  expect(await screen.findByText("Indexed hit 1")).toBeTruthy();
  expect(screen.queryByText("Indexed hit 26")).toBeNull();
  expect(search.mock.calls).toEqual([
    ["report", 0, 25],
    ["report", 25, 25],
    ["report", 25, 25],
    ["report", 0, 25],
  ]);
  expect(onStateChange).toHaveBeenLastCalledWith(
    expect.objectContaining({ offset: 0 }),
  );
});

it("applies local day and VIP filters to the history and forwards the same scope to transcript search", async () => {
  const inRange = {
    ...meeting,
    title: "VIP within range",
    startedAt: new Date(2026, 9, 25, 23, 30).toISOString(),
    userMetadata: {
      revision: 1,
      title: null,
      pinned: true,
      audioRemoved: false,
    },
  };
  const outside = {
    ...inRange,
    id: "other",
    title: "Following day",
    startedAt: new Date(2026, 9, 26, 0, 0).toISOString(),
  };
  const notVip = {
    ...inRange,
    id: "not-vip",
    title: "Regular meeting",
    userMetadata: { ...inRange.userMetadata, pinned: false },
  };
  const search = vi.fn().mockResolvedValue({
    items: [],
    total: 0,
    offset: 0,
    limit: 25,
  });
  render(
    <HistorySearch
      api={{ searchMeetings: search } as unknown as DesktopApi}
      locale="en"
      nativeMeetings={[inRange, outside, notVip]}
      onOpen={vi.fn()}
    />,
  );
  fireEvent.change(screen.getByLabelText("From"), {
    target: { value: "2026-10-25" },
  });
  fireEvent.change(screen.getByLabelText("Through"), {
    target: { value: "2026-10-25" },
  });
  await userEvent.click(
    screen.getByRole("checkbox", { name: "VIP meetings only" }),
  );
  expect(screen.getByText("VIP within range")).toBeTruthy();
  expect(screen.queryByText("Following day")).toBeNull();
  expect(screen.queryByText("Regular meeting")).toBeNull();
  expect(search).not.toHaveBeenCalled();

  await userEvent.type(screen.getByRole("searchbox"), "report");
  await userEvent.click(
    screen.getByRole("checkbox", { name: "Transcript matches only" }),
  );
  await waitFor(() =>
    expect(search).toHaveBeenLastCalledWith("report", 0, 25, {
      fromMs: new Date(2026, 9, 25).getTime(),
      beforeMs: new Date(2026, 9, 26).getTime(),
      pinnedOnly: true,
      transcriptOnly: true,
    }),
  );
  await screen.findByText(/No matches/);
  search.mockClear();
  fireEvent.change(screen.getByLabelText("From"), {
    target: { value: "2026-10-26" },
  });
  expect(screen.getByRole("alert").textContent).toBe(
    "Choose a valid date range.",
  );
  expect(screen.queryByText(/No matches/)).toBeNull();
  expect(search).not.toHaveBeenCalled();
});

it("runs history questions explicitly with the local scope and opens the quoted meeting and section", async () => {
  const citation = {
    meetingId: meeting.id,
    meetingTitle: "Other meeting",
    sourceId: "system-000000016000.wav",
    startMs: 1000,
    quote: "The delivery is scheduled for Wednesday.",
  };
  const ask = vi
    .fn()
    .mockResolvedValue({ searchedPassages: 2, citations: [citation] });
  const open = vi.fn();
  render(
    <HistorySearch
      api={{ askMeetingHistory: ask } as unknown as DesktopApi}
      locale="en"
      onOpen={vi.fn()}
      nativeMeetings={[
        {
          ...meeting,
          userMetadata: {
            revision: 0,
            title: null,
            pinned: true,
            audioRemoved: false,
          },
        },
      ]}
      onOpenCitation={open}
      historyModel={{
        endpoint: "http://127.0.0.1:1235/v1/",
        model: "synthetic",
      }}
    />,
  );
  await userEvent.click(
    screen.getByRole("checkbox", { name: "VIP meetings only" }),
  );
  await userEvent.click(screen.getByText("Ask your meetings"));
  await userEvent.click(
    screen.getByText("All meetings in the date and VIP filters"),
  );
  await userEvent.click(
    screen.getByRole("checkbox", { name: /Original name/ }),
  );
  await userEvent.type(
    screen.getByLabelText("Question for your meetings"),
    "When is delivery?",
  );
  expect(ask).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Answer locally" }));
  await screen.findByText(citation.quote);
  expect(ask).toHaveBeenCalledWith({
    endpoint: "http://127.0.0.1:1235/v1/",
    model: "synthetic",
    question: "When is delivery?",
    meetingIds: [meeting.id],
    scope: expect.objectContaining({ pinnedOnly: true }),
  });
  await userEvent.click(
    screen.getByRole("button", { name: "Open original passage" }),
  );
  expect(open).toHaveBeenCalledWith(
    citation,
    expect.objectContaining({ question: "When is delivery?" }),
  );
});

it("discards an in-flight answer when the transcript-only scope changes", async () => {
  const citation = {
    meetingId: meeting.id,
    meetingTitle: "Synthetic meeting",
    sourceId: meeting.parts[0].file,
    startMs: 0,
    quote: "Old-scope reply",
  };
  const oldResult = { searchedPassages: 1, citations: [citation] };
  let finishOld!: (result: typeof oldResult) => void;
  const pending = new Promise<typeof oldResult>((resolve) => {
    finishOld = resolve;
  });
  const ask = vi
    .fn()
    .mockReturnValueOnce(pending)
    .mockResolvedValueOnce({ searchedPassages: 0, citations: [] });
  render(
    <HistorySearch
      api={{ askMeetingHistory: ask } as unknown as DesktopApi}
      locale="en"
      onOpen={vi.fn()}
      onOpenCitation={vi.fn()}
      historyModel={{
        endpoint: "http://127.0.0.1:1235/v1/",
        model: "synthetic",
      }}
    />,
  );
  await userEvent.type(screen.getByRole("searchbox"), "delivery");
  await userEvent.click(screen.getByText("Ask your meetings"));
  const question = "When is delivery?";
  await userEvent.type(
    screen.getByLabelText("Question for your meetings"),
    question,
  );
  await userEvent.click(screen.getByRole("button", { name: "Answer locally" }));
  expect(ask).toHaveBeenCalledWith(
    expect.objectContaining({ question, scope: undefined }),
  );

  await userEvent.click(
    screen.getByRole("checkbox", { name: "Transcript matches only" }),
  );
  await act(async () => {
    finishOld(oldResult);
    await pending;
  });
  expect(screen.queryByText(citation.quote)).toBeNull();
  await userEvent.click(screen.getByText("Ask your meetings"));
  await userEvent.type(
    screen.getByLabelText("Question for your meetings"),
    question,
  );
  await userEvent.click(screen.getByRole("button", { name: "Answer locally" }));
  expect(ask).toHaveBeenLastCalledWith(
    expect.objectContaining({
      question,
      scope: expect.objectContaining({ transcriptOnly: true }),
    }),
  );
  expect(await screen.findByText(/contain no supported answer/)).toBeTruthy();
});

it("loads verified audio only after a user requests the exact history result", async () => {
  const sourceId = meeting.parts[0].file;
  const getTranscriptAudio = vi
    .fn()
    .mockRejectedValue(new Error("unavailable"));
  const searchMeetings = vi.fn().mockResolvedValue({
    items: [
      {
        ...meeting,
        sourceId,
        excerpt: "Read the report",
        startMs: 0,
        endMs: 75_000,
      },
    ],
    total: 1,
    limit: 25,
    offset: 0,
  });
  render(
    <HistorySearch
      api={{ getTranscriptAudio, searchMeetings } as unknown as DesktopApi}
      locale="en"
      onOpen={vi.fn()}
    />,
  );
  await userEvent.type(screen.getByRole("searchbox"), "report");
  await screen.findByText("Read the report");
  expect(
    screen.getByText(/Audio preview plays the first 30 seconds of the section/),
  ).toBeTruthy();
  expect(getTranscriptAudio).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole("button", { name: "Load audio preview" }),
  );
  expect(getTranscriptAudio).toHaveBeenCalledExactlyOnceWith(
    meeting.id,
    sourceId,
  );
  expect(
    await screen.findByText(/Local audio preview is unavailable/),
  ).toBeTruthy();
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
