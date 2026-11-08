import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import App from "./App";
import TranscriptBody from "./views/TranscriptBody";
import MeetingLabels from "./views/MeetingLabels";
import AudioCopiesPanel from "./views/AudioCopiesPanel";
import TextModelSettings from "./views/TextModelSettings";
import { testServices } from "./test-services";
import { de } from "./i18n";
import { hydrateLiveTranscript, mergeLiveTranscript } from "./live-transcript";
import type {
  AppState,
  AudioPart,
  DesktopApi,
  HandyModel,
  Meeting,
} from "./types";

// Synthetic IPC and text only; no device, user recording, model or provider is opened.
const parts: AudioPart[] = Array.from({ length: 50 }, (_, i) => ({
  file: `microphone-${i}.wav`,
  source: "microphone",
  startSample: i * 480_000,
  sampleCount: 480_000,
  confirmedSampleCount: 480_000,
  text: `Vollständiger Originalabschnitt ${i}`,
  status: "complete",
}));
const meeting: Meeting = {
  id: "synthetic-transcript",
  title: "Synthetic feedback",
  startedAt: "2026-10-28T18:00:00Z",
  durationMs: 1_500_000,
  modelId: "synthetic",
  status: "streaming",
  warnings: [],
  parts,
};
afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

it("locks a discovery panel that was opened before an analysis started", async () => {
  const endpoint = vi.fn();
  const model = vi.fn();
  const api = {
    ...testServices(),
    localModelSetup: vi.fn(async () => ({
      totalMemoryBytes: null,
      recommendation: { tier: "unknown", model: null, lmStudioSearch: null },
      providers: [
        {
          id: "lmstudio",
          name: "LM Studio",
          endpoint: "http://127.0.0.1:1235/v1/",
          reachable: true,
          models: ["synthetic"],
          recommendedInstalled: false,
          loadedModels: [{ model: "synthetic", contextLength: 8192 }],
        },
      ],
    })),
  } as unknown as DesktopApi;
  const props = {
    api,
    locale: "de" as const,
    t: de,
    connected: true,
    disabled: false,
    mode: "local" as const,
    setMode: vi.fn(),
    endpoint: "http://127.0.0.1:1234/v1/",
    setEndpoint: endpoint,
    textModel: "synthetic",
    setTextModel: model,
    apiKey: "",
    setApiKey: vi.fn(),
    useSavedKey: false,
    setUseSavedKey: vi.fn(),
    localModels: ["synthetic"],
    modelsLoading: false,
    modelsLoaded: true,
    modelsError: null,
    refreshModels: vi.fn(),
    manualModel: false,
    setManualModel: vi.fn(),
    workspaceLanguage: "auto" as const,
    setWorkspaceLanguage: vi.fn(),
    workspacePrompt: null,
    setWorkspacePrompt: vi.fn(),
  };
  const view = render(<TextModelSettings {...props} />);
  fireEvent.click(
    screen.getByRole("button", { name: "Ollama / LM Studio erkennen" }),
  );
  expect(
    await screen.findByText("synthetic · geladen · 8.192 Tokens Kontext"),
  ).toBeTruthy();
  expect(
    screen.getByText(/in LM Studio entladen und den Server laufen lassen/),
  ).toBeTruthy();
  const use = await screen.findByRole("button", {
    name: "Verwenden: synthetic",
  });
  expect((use as HTMLButtonElement).disabled).toBe(false);
  view.rerender(<TextModelSettings {...props} disabled />);
  expect((use as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(use);
  expect(endpoint).not.toHaveBeenCalled();
  expect(model).not.toHaveBeenCalled();
});

it("keeps the beginning of a live transcript available when bounded previews arrive", async () => {
  let state: AppState = {
    version: "1.0.6",
    recording: true,
    handy: null,
    activeMeeting: {
      ...meeting,
      parts: parts.slice(-16),
      livePreviewLimited: true,
    },
  };
  const api: DesktopApi = {
    ...testServices(),
    getAppState: vi.fn(async () => state),
    listDevices: vi.fn(async () => []),
    listMeetings: vi.fn(async () => []),
    getMeeting: vi.fn(async () => meeting),
  } as unknown as DesktopApi;
  render(<App api={api} />);
  await screen.findByText("Vollständiger Originalabschnitt 0");
  const next = {
    ...parts[0],
    file: "microphone-50.wav",
    startSample: 50 * 480_000,
    text: "Neuer bestätigter Abschnitt",
  };
  state = {
    ...state,
    activeMeeting: {
      ...meeting,
      parts: [...parts.slice(-15), next],
      livePreviewLimited: true,
    },
  };
  await screen.findByText("Neuer bestätigter Abschnitt", {}, { timeout: 5000 });
  expect(screen.getByText("Vollständiger Originalabschnitt 0")).toBeTruthy();
  const input = screen.getByRole("searchbox", { name: "Im Transkript suchen" });
  fireEvent.change(input, { target: { value: "Originalabschnitt 0" } });
  expect(screen.getByText("Vollständiger", { exact: false })).toBeTruthy();
});

it("marks the original Unicode match without shifting its start", () => {
  const { container } = render(
    <TranscriptBody
      parts={[{ ...parts[0], text: "İstanbul Budget" }]}
      locale="de"
      t={de}
    />,
  );
  fireEvent.change(screen.getByRole("searchbox"), {
    target: { value: "Budget" },
  });
  expect(container.querySelector("mark")?.textContent).toBe("Budget");
  expect(container.querySelector(".transcript-part p")?.textContent).toBe(
    "İstanbul Budget",
  );
});

it("keeps refreshing old sections throughout catch-up", async () => {
  vi.useFakeTimers();
  let full = {
    ...meeting,
    status: "catching-up",
    parts: [
      { ...parts[0], text: null, transcriptSequence: 1 },
      ...parts.slice(1),
    ],
  };
  const api = {
    ...testServices(),
    getAppState: vi.fn(async () => ({
      version: "1.0.6",
      recording: false,
      handy: null,
      activeMeeting: {
        ...full,
        parts: full.parts.slice(-16),
        livePreviewLimited: true,
      },
    })),
    listDevices: vi.fn(async () => []),
    listMeetings: vi.fn(async () => []),
    getMeeting: vi.fn(async () => full),
  } as unknown as DesktopApi;
  render(<App api={api} />);
  await act(async () => {});
  full = {
    ...full,
    parts: [
      {
        ...full.parts[0],
        text: "Spät erkannter Anfang",
        transcriptSequence: 2,
      },
      ...full.parts.slice(1),
    ],
  };
  await act(async () => {
    await vi.advanceTimersByTimeAsync(5001);
  });
  expect(screen.getByText("Spät erkannter Anfang")).toBeTruthy();
});

it("does not roll a hydrated transcript back to an older bounded preview", () => {
  const latest = {
    ...meeting,
    parts: [{ ...parts[0], transcriptSequence: 9, text: "Neu bestätigt" }],
  };
  const older = {
    ...meeting,
    livePreviewLimited: true,
    parts: [{ ...parts[0], transcriptSequence: 8, text: "Älterer Stand" }],
  };
  expect(mergeLiveTranscript(latest, older).parts[0].text).toBe(
    "Neu bestätigt",
  );
});

it.each([undefined, 80, 90])(
  "keeps newer text when an older full publication has audio sequence %s",
  (snapshotSequence) => {
    const latest = {
      ...meeting,
      snapshotSequence: 90,
      parts: [{ ...parts[0], transcriptSequence: 9, text: "Neu bestätigt" }],
    };
    const stale = {
      ...meeting,
      snapshotSequence,
      livePreviewLimited: false,
      parts: [{ ...parts[0], transcriptSequence: 8, text: "Veraltet" }],
    };
    const merged = mergeLiveTranscript(latest, stale);
    expect(merged.parts[0].text).toBe("Neu bestätigt");
    expect(merged.snapshotSequence).toBe(90);
  },
);

it.each([89, 90])(
  "accepts newer ASR text independently of audio sequence %i",
  (snapshotSequence) => {
    const latest = {
      ...meeting,
      snapshotSequence: 90,
      parts: [{ ...parts[0], transcriptSequence: 9, text: "Bisher bestätigt" }],
    };
    const next = {
      ...meeting,
      snapshotSequence,
      parts: [{ ...parts[0], transcriptSequence: 10, text: "Neue Erkennung" }],
    };
    const merged = mergeLiveTranscript(latest, next);
    expect(merged.parts[0].text).toBe("Neue Erkennung");
    expect(merged.snapshotSequence).toBe(90);
  },
);

it("keeps VIP actions out of the rename form and cancels without changing metadata", () => {
  const save = vi.fn();
  render(
    <MeetingLabels
      api={{ editMeetingMetadata: save } as unknown as DesktopApi}
      meeting={meeting}
      locale="de"
      onChanged={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Meeting umbenennen" }));
  expect(screen.queryByRole("button", { name: "Als VIP anheften" })).toBeNull();
  fireEvent.change(screen.getByRole("textbox", { name: "Meetingname" }), {
    target: { value: "Unconfirmed title" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Abbrechen" }));
  expect(save).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Als VIP anheften" })).toBeTruthy();
});

it("hides compact audio pagination when no further page exists", async () => {
  const api = {
    listAudioCopies: vi.fn(async () => ({ items: [], more: false })),
  } as unknown as DesktopApi;
  render(<AudioCopiesPanel api={api} locale="de" models={[]} job={null} />);
  await waitFor(() => expect(api.listAudioCopies).toHaveBeenCalled());
  expect(screen.queryByRole("button", { name: "Zurück" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Weiter" })).toBeNull();
});

it("refreshes catch-up text outside the recent preview and rejects late foreign hydration", () => {
  const current = {
    ...meeting,
    snapshotSequence: 80,
    parts: [
      { ...parts[0], text: null, transcriptSequence: 0 },
      ...parts.slice(1),
    ],
  };
  const full = {
    ...meeting,
    snapshotSequence: 81,
    parts: [
      { ...parts[0], text: "Später erkannter Anfang", transcriptSequence: 81 },
      ...parts.slice(1),
    ],
  };
  const hydrated = hydrateLiveTranscript(current, full)!;
  expect(hydrated.parts[0].text).toBe("Später erkannter Anfang");
  expect(hydrated.snapshotSequence).toBe(81);
  const preview = {
    ...meeting,
    snapshotSequence: 82,
    livePreviewLimited: true,
    parts: parts.slice(-16),
  };
  const merged = mergeLiveTranscript(hydrated, preview);
  expect(merged.parts[0].text).toBe("Später erkannter Anfang");
  expect(
    hydrateLiveTranscript(merged, { ...full, id: "another-meeting" }),
  ).toBe(merged);
  const stale = {
    ...full,
    snapshotSequence: 79,
    parts: [{ ...parts[0], text: "Alter Stand", transcriptSequence: 79 }],
  };
  expect(hydrateLiveTranscript(merged, stale)!.parts[0].text).toBe(
    "Später erkannter Anfang",
  );
  const oldTextWithSameAudioSequence = {
    ...stale,
    snapshotSequence: merged.snapshotSequence,
  };
  expect(
    hydrateLiveTranscript(merged, oldTextWithSameAudioSequence)!.parts[0].text,
  ).toBe("Später erkannter Anfang");
});

it("clears an audio-copy confirmation when its row becomes the previous page's hidden lookahead", async () => {
  const copies = Array.from({ length: 26 }, (_, i) => ({
    runId: `copy-${i}`,
    meetingId: meeting.id,
    title: `Copy ${i}`,
    codec: "flac" as const,
    createdAt: meeting.startedAt,
    sections: 1,
  }));
  const start = vi.fn(async () => "synthetic-job");
  const list = vi.fn(async (offset: number) => ({
    items: copies.slice(offset, offset + 25),
    more: offset + 25 < copies.length,
  }));
  render(
    <AudioCopiesPanel
      api={
        {
          listAudioCopies: list,
          transcribeAudioCopy: start,
        } as unknown as DesktopApi
      }
      locale="en"
      models={
        [
          {
            id: "synthetic",
            name: "Synthetic model",
            isDownloaded: true,
            supportsStreaming: true,
          },
        ] as HandyModel[]
      }
      job={null}
    />,
  );
  await screen.findByText("Copy 24");
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByText("Copy 25");
  fireEvent.click(
    screen.getByRole("button", { name: "Prepare new transcription" }),
  );
  fireEvent.change(screen.getByLabelText("Model for the new transcription"), {
    target: { value: "synthetic" },
  });
  fireEvent.click(screen.getByRole("checkbox"));
  expect(
    (
      screen.getByRole("button", {
        name: "Start new local transcription",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Previous" }));
  await screen.findByText("Copy 0");
  expect(screen.queryByRole("heading", { name: "Copy 25" })).toBeNull();
  expect(
    screen.queryByRole("button", { name: "Start new local transcription" }),
  ).toBeNull();
  expect(start).not.toHaveBeenCalled();
  fireEvent.click(
    screen.getAllByRole("button", { name: "Prepare new transcription" })[0],
  );
  expect(
    (
      screen.getByRole("button", {
        name: "Start new local transcription",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
});

it.each([25, 26])(
  "uses page metadata rather than an empty next page for %i audio copies",
  async (count) => {
    const copies = Array.from({ length: count }, (_, i) => ({
      runId: `copy-${i}`,
      meetingId: meeting.id,
      title: `Copy ${i}`,
      codec: "flac" as const,
      createdAt: meeting.startedAt,
      sections: 1,
    }));
    const list = vi.fn(async (offset: number) => ({
      items: copies.slice(offset, offset + 25),
      more: offset + 25 < copies.length,
    }));
    render(
      <AudioCopiesPanel
        api={{ listAudioCopies: list } as unknown as DesktopApi}
        locale="de"
        models={[]}
        job={null}
      />,
    );
    await screen.findByText("Copy 24");
    if (count === 25) {
      expect(screen.queryByRole("button", { name: "Weiter" })).toBeNull();
    } else {
      fireEvent.click(screen.getByRole("button", { name: "Weiter" }));
      await screen.findByText("Copy 25");
      expect(screen.queryByText("Copy 24")).toBeNull();
      expect(
        (screen.getByRole("button", { name: "Weiter" }) as HTMLButtonElement)
          .disabled,
      ).toBe(true);
      fireEvent.click(screen.getByRole("button", { name: "Zurück" }));
      await screen.findByText("Copy 0");
      expect(list).toHaveBeenNthCalledWith(2, 25);
    }
  },
);
