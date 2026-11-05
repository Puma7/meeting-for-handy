import { StrictMode } from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import WorkspaceBridge from "./WorkspaceBridge";
import type { DesktopApi, Meeting } from "../types";
import { LiveWorkspace } from "./shell/LiveWorkspace.jsx";
import { fitColumn } from "./shell/fitOverview.mjs";
import { workspaceText } from "./shell/workspaceLocale.jsx";
import MeetingView from "../views/MeetingView";
import { de, en, extraDe, extraEn } from "../i18n";

it.each(["de", "en"] as const)(
  "keeps the classic displayed meeting identity outside closed setup (%s)",
  (locale) => {
    const meeting: Meeting = {
      id: "synthetic-identity",
      title: "Actual displayed meeting",
      startedAt: "2026-11-05T10:00:00Z",
      durationMs: 65000,
      status: "recording",
      modelId: "synthetic",
      parts: [],
      warnings: [],
    };
    const noop = vi.fn();
    const props = {
      t: locale === "de" ? de : en,
      x: locale === "de" ? extraDe : extraEn,
      locale,
      app: null,
      meeting,
      recording: true,
      isStopping: false,
      busy: false,
      connected: false,
      title: "Uncommitted setup draft",
      defaultTitle: "Synthetic",
      devices: [],
      models: [],
      modelId: "synthetic",
      systemId: "synthetic",
      micId: "",
      micEnabled: false,
      controlsAvailable: false,
      savedFile: null,
      copied: false,
      backlog: null,
      recentParts: [],
      onAnalyse: noop,
      onOpenFolder: noop,
      onExport: noop,
      onPause: noop,
      onMute: noop,
      setTitle: noop,
      setModelId: noop,
      setSystemId: noop,
      setMicId: noop,
      setMicEnabled: noop,
      refreshHandy: noop,
      refreshDevices: noop,
      start: noop,
      stop: noop,
      resume: noop,
      copyFullTranscript: noop,
      savedByTrack: () => 0,
    };
    const mounted = render(<MeetingView {...props} />);
    try {
      const label =
        locale === "de" ? "Angezeigtes Meeting" : "Displayed meeting";
      const identity = screen.getByLabelText(label);
      expect(identity.textContent).toBe(meeting.title);
      expect(identity.getAttribute("title")).toBe(meeting.title);
      expect(identity.closest("details")).toBeNull();
      expect(identity.closest(".meeting-quick-actions")).toBeTruthy();
      const setup = mounted.container.querySelector(
        "details.setup-details",
      ) as HTMLDetailsElement;
      expect(setup.open).toBe(false);
      mounted.rerender(
        <MeetingView
          {...props}
          meeting={{ ...meeting, title: "Renamed actual meeting" }}
        />,
      );
      expect(screen.getByLabelText(label).textContent).toBe(
        "Renamed actual meeting",
      );
      expect(screen.getByLabelText(label).textContent).not.toContain(
        props.title,
      );
      expect(setup.open).toBe(false);
      expect(noop).not.toHaveBeenCalled();
    } finally {
      mounted.unmount();
    }
  },
);

it.each([
  ["de", 120000],
  ["en", 120000],
  ["de", 300000],
  ["en", 450000],
] as const)(
  "shows the authoritative running interval after reopening (%s, %s)",
  async (language, intervalMs) => {
    const f = fixture();
    f.status.intervalMs = intervalMs;
    const mounted = render(
      <LiveWorkspace
        language={language}
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    try {
      const selector = screen.getByRole("combobox", {
        name: language === "de" ? "Auswertungsintervall" : "Analysis interval",
      }) as HTMLSelectElement;
      await waitFor(() =>
        expect(selector.value).toBe(String(intervalMs / 60000)),
      );
      expect(selector.selectedOptions[0].textContent).toContain(
        (intervalMs / 60000).toLocaleString(language),
      );
      expect(f.adapter.interval).not.toHaveBeenCalled();
      expect(f.adapter.start).not.toHaveBeenCalled();
      expect(f.adapter.stop).not.toHaveBeenCalled();
    } finally {
      mounted.unmount();
    }
  },
);

it("keeps a rejected interval change visibly authoritative and applies the next confirmed change", async () => {
  const f = fixture();
  f.adapter.interval.mockRejectedValueOnce(
    new Error("Synthetic interval change rejected"),
  );
  f.adapter.interval.mockImplementationOnce(async () => {
    f.status.intervalMs = 180000;
  });
  const mounted = render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  try {
    const selector = screen.getByRole("combobox", {
      name: "Auswertungsintervall",
    }) as HTMLSelectElement;
    await waitFor(() => expect(selector.disabled).toBe(false));
    fireEvent.change(selector, { target: { value: "3" } });
    await screen.findByText("Synthetic interval change rejected");
    expect(selector.value).toBe("2");
    fireEvent.change(selector, { target: { value: "3" } });
    await waitFor(() => expect(f.adapter.interval).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(selector.value).toBe("3"));
    expect(f.adapter.interval.mock.calls).toEqual([[180000], [180000]]);
    expect(f.adapter.start).not.toHaveBeenCalled();
    expect(f.adapter.stop).not.toHaveBeenCalled();
  } finally {
    mounted.unmount();
  }
});

it("forwards changing global notices without resetting the selected workspace tab", async () => {
  const f = fixture();
  const api = {
    getLiveNotes: f.adapter.status,
    startLiveNotes: f.adapter.start,
    stopLiveNotes: f.adapter.stop,
    refreshLiveNotes: f.adapter.refresh,
    setLiveNotesInterval: f.adapter.interval,
    setWorkspaceContext: vi.fn(),
    localModelSetup: f.adapter.models,
  } as unknown as DesktopApi;
  const props = {
    api,
    meeting: f.meeting as unknown as Meeting,
    endpoint: "http://127.0.0.1:1235/v1/",
    model: "synthetic",
    onBack: vi.fn(),
  };
  const mounted = render(<WorkspaceBridge {...props} />);
  try {
    const host = mounted.container.querySelector(".native-workspace-host")!;
    await waitFor(() =>
      expect(host.shadowRoot?.querySelector('[role="tablist"]')).toBeTruthy(),
    );
    const root = host.shadowRoot!.querySelector(".prototype") as HTMLElement;
    const shadow = within(root);
    fireEvent.click(shadow.getByRole("tab", { name: "Vorbereitung" }));
    mounted.rerender(
      <WorkspaceBridge
        {...props}
        notices={
          <p role="status" className="notice">
            Synthetic erasure remains incomplete; preserved remnants need
            review.
          </p>
        }
      />,
    );
    expect(
      shadow
        .getByText(
          "Synthetic erasure remains incomplete; preserved remnants need review.",
        )
        .getAttribute("role"),
    ).toBe("status");
    expect(
      shadow
        .getByRole("tab", { name: "Vorbereitung" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(host.shadowRoot!.querySelector(".prototype")).toBe(root);
    mounted.rerender(<WorkspaceBridge {...props} />);
    expect(
      shadow.queryByText(/Synthetic erasure remains incomplete/),
    ).toBeNull();
    expect(
      shadow
        .getByRole("tab", { name: "Vorbereitung" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(f.adapter.start).not.toHaveBeenCalled();
    expect(f.adapter.stop).not.toHaveBeenCalled();
  } finally {
    mounted.unmount();
  }
});

describe("Status polling error recovery", () => {
  it.each([
    ["de", true],
    ["de", false],
    ["en", true],
    ["en", false],
  ] as const)(
    "shows saved verification failures in %s with readable fallback=%s and clears them after recovery",
    async (language, readable) => {
      vi.useFakeTimers();
      const f = fixture();
      Object.assign(f.status, {
        error: null,
        running: false,
        status: "stopped",
      });
      Object.assign(f.status.view, { damagedRuns: 2 });
      if (!readable) Object.assign(f.status.view, { workspace: null });
      const mounted = render(
        <LiveWorkspace
          language={language}
          adapter={f.adapter}
          meeting={f.meeting}
          onBack={() => {}}
        />,
      );
      try {
        await act(async () => {});
        const label =
          language === "en" ? "Saved analyses" : "Gespeicherte Auswertungen";
        const warning = screen.getByRole("status", { name: label });
        expect(warning.textContent).toContain(
          language === "en"
            ? "could not be verified"
            : "konnten nicht geprüft werden",
        );
        expect(warning.textContent).toContain(
          language === "en"
            ? readable
              ? "An earlier readable revision is displayed."
              : "Saved results are currently unavailable."
            : readable
              ? "Ein früherer lesbarer Stand wird angezeigt."
              : "Gespeicherte Ergebnisse sind derzeit nicht verfügbar.",
        );
        expect(warning.textContent).toContain(
          language === "en"
            ? "Files are kept unchanged"
            : "Die Dateien bleiben unverändert erhalten",
        );
        if (readable)
          expect(
            screen.getAllByText("Welche Freigabe fehlt?").length,
          ).toBeGreaterThan(0);
        Object.assign(f.status.view, { damagedRuns: 0 });
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2000);
        });
        expect(screen.queryByRole("status", { name: label })).toBeNull();
        expect(f.adapter.start).not.toHaveBeenCalled();
        expect(f.adapter.stop).not.toHaveBeenCalled();
        expect(f.adapter.context).not.toHaveBeenCalled();
      } finally {
        mounted.unmount();
        vi.useRealTimers();
      }
    },
  );
  it("clears a transient poll banner on the next successful status read", async () => {
    vi.useFakeTimers();
    const f = fixture();
    f.adapter.status.mockRejectedValueOnce(
      new Error("Synthetic poll unavailable"),
    );
    const mounted = render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    try {
      await act(async () => {});
      expect(screen.getByText(/Synthetic poll unavailable/)).toBeTruthy();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(2000);
      });
      expect(screen.queryByText(/Synthetic poll unavailable/)).toBeNull();
      expect(f.adapter.stop).not.toHaveBeenCalled();
      expect(f.adapter.start).not.toHaveBeenCalled();
    } finally {
      mounted.unmount();
      vi.useRealTimers();
    }
  });
  it.each(["error", "string"] as const)(
    "keeps a failed note publication readable and retains its draft across a poll retry for %s",
    async (kind) => {
      vi.useFakeTimers();
      const f = fixture();
      f.status.status = "waiting";
      const message = "Synthetic note publication refused";
      f.adapter.context.mockRejectedValue(
        kind === "error" ? new Error(message) : message,
      );
      const mounted = render(
        <LiveWorkspace
          adapter={f.adapter}
          meeting={f.meeting}
          onBack={() => {}}
        />,
      );
      try {
        await act(async () => {});
        fireEvent.click(screen.getByRole("button", { name: "Fürs Protokoll" }));
        const note = screen.getByRole("textbox", { name: "Protokollnotiz" });
        fireEvent.change(note, {
          target: { value: "New synthetic protocol draft" },
        });
        fireEvent.blur(note);
        await act(async () => {});
        expect(f.adapter.context).toHaveBeenCalledTimes(1);
        expect(screen.getByText(message)).toBeTruthy();
        expect((note as HTMLTextAreaElement).value).toBe(
          "New synthetic protocol draft",
        );
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2000);
        });
        expect(f.adapter.context).toHaveBeenCalledTimes(2);
        expect(screen.getByText(message)).toBeTruthy();
        expect((note as HTMLTextAreaElement).value).toBe(
          "New synthetic protocol draft",
        );
        expect(f.adapter.start).not.toHaveBeenCalled();
        expect(f.adapter.stop).not.toHaveBeenCalled();
      } finally {
        mounted.unmount();
        vi.useRealTimers();
      }
    },
  );
  it("shows a poll Error message without its JavaScript Error prefix", async () => {
    vi.useFakeTimers();
    const f = fixture();
    f.adapter.status.mockRejectedValueOnce(
      new Error("Synthetic poll unavailable"),
    );
    const mounted = render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    try {
      await act(async () => {});
      expect(screen.getByText("Synthetic poll unavailable")).toBeTruthy();
      expect(
        screen.queryByText("Error: Synthetic poll unavailable"),
      ).toBeNull();
    } finally {
      mounted.unmount();
      vi.useRealTimers();
    }
  });
  it.each(["error", "string", "number"] as const)(
    "keeps a failed manual action visible through poll failure and recovery for %s",
    async (kind) => {
      vi.useFakeTimers();
      const f = fixture();
      const read = f.adapter.status.getMockImplementation()!;
      let unavailable = false;
      f.adapter.status.mockImplementation(async () => {
        if (unavailable) throw new Error("Synthetic poll unavailable");
        return read();
      });
      const message = kind === "number" ? "503" : "Synthetic refresh refused";
      f.adapter.refresh.mockRejectedValueOnce(
        kind === "error"
          ? new Error(message)
          : kind === "number"
            ? 503
            : message,
      );
      const mounted = render(
        <LiveWorkspace
          adapter={f.adapter}
          meeting={f.meeting}
          onBack={() => {}}
        />,
      );
      try {
        await act(async () => {});
        unavailable = true;
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2000);
        });
        fireEvent.click(screen.getByRole("button", { name: "Aktualisieren" }));
        await act(async () => {});
        expect(screen.getByText(message)).toBeTruthy();
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2000);
        });
        expect(screen.getByText(message)).toBeTruthy();
        unavailable = false;
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2000);
        });
        expect(screen.getByText(message)).toBeTruthy();
        expect(f.adapter.refresh).toHaveBeenCalledTimes(1);
        expect(f.adapter.stop).not.toHaveBeenCalled();
      } finally {
        mounted.unmount();
        vi.useRealTimers();
      }
    },
  );
  it.each(["error", "string", "number"] as const)(
    "keeps a failed human override visible without silently retrying it for %s",
    async (kind) => {
      vi.useFakeTimers();
      const f = fixture();
      const message = kind === "number" ? "503" : "Synthetic override refused";
      f.adapter.context.mockRejectedValueOnce(
        kind === "error"
          ? new Error(message)
          : kind === "number"
            ? 503
            : message,
      );
      const mounted = render(
        <LiveWorkspace
          adapter={f.adapter}
          meeting={f.meeting}
          onBack={() => {}}
        />,
      );
      try {
        await act(async () => {});
        fireEvent.click(screen.getByRole("tab", { name: "Impulse" }));
        fireEvent.click(screen.getByRole("button", { name: "Besprochen" }));
        await act(async () => {});
        expect(screen.getByText(message)).toBeTruthy();
        f.adapter.status.mockRejectedValueOnce(
          new Error("Synthetic poll unavailable"),
        );
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2000);
        });
        expect(screen.getByText(message)).toBeTruthy();
        await act(async () => {
          await vi.advanceTimersByTimeAsync(2000);
        });
        expect(screen.getByText(message)).toBeTruthy();
        expect(f.adapter.context).toHaveBeenCalledTimes(1);
        expect(f.adapter.stop).not.toHaveBeenCalled();
      } finally {
        mounted.unmount();
        vi.useRealTimers();
      }
    },
  );
});

it("localizes every built-in module and its conversation-history controls", () => {
  for (const label of [
    "Transkript",
    "Live-Überblick",
    "Wichtige Details",
    "Gesprächsverlauf",
    "Vorbereitung",
    "Gesprächsimpulse",
    "Jetzt relevant",
  ]) {
    expect(workspaceText("en", label), label).not.toBe(label);
  }
  const f = fixture();
  render(
    <LiveWorkspace
      adapter={f.adapter}
      meeting={f.meeting}
      onBack={() => {}}
      language="en"
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Arrange" }));
  const grip = screen.getByRole("button", {
    name: "Conversation history move",
  });
  expect(grip).toBeTruthy();
  expect(
    screen.queryByRole("button", { name: "Gesprächsverlauf move" }),
  ).toBeNull();
  expect(
    screen.getByRole("button", { name: "Conversation history arrange" }),
  ).toBeTruthy();
  const picker = screen.getByRole("combobox", {
    name: "Conversation history replace module",
  });
  expect(
    within(picker).getByRole("option", { name: "Conversation history" }),
  ).toBeTruthy();
});

it("translates only dictionary entries and preserves inherited object names", () => {
  for (const name of ["constructor", "toString", "valueOf", "__proto__"]) {
    expect(workspaceText("en", name)).toBe(name);
  }
  expect(workspaceText("en", "Transkript")).toBe("Transcript");
});

it.each(["constructor", "toString", "valueOf", "__proto__", "Transkript"])(
  "keeps the saved custom layout name literal in English: %s",
  (name) => {
    const f = fixture();
    localStorage.setItem(
      "handymeeting.workspace.layout-catalog.v1",
      JSON.stringify({
        defaultId: "custom-abc",
        custom: [{ id: "custom-abc", base: "verlauf" }],
        names: { "custom-abc": name },
      }),
    );
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
        language="en"
      />,
    );
    const select = screen.getByRole("combobox", {
      name: "Overview layout",
    }) as HTMLSelectElement;
    expect(select.selectedOptions[0].text).toBe(`${name} · Default`);
  },
);

beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
function fixture() {
  const context = {
    revision: 7,
    agenda: [{ id: "e_agenda", revision: 1, text: "Budget klären" }],
    sharedNotes: [
      { id: "h_protocol_0", revision: 3, text: "Gesicherte Protokollnotiz" },
    ],
    overrides: [] as object[],
  };
  const item = {
    id: "e_impulse",
    revision: 1,
    kind: "impulse",
    text: "Welche Freigabe fehlt?",
    status: "open",
    attention: "clarification",
    owner: null,
    due: null,
    evidence: [{ sourceId: "s_one", quote: "Die Freigabe bleibt noch offen." }],
    humanLocked: false,
  };
  const workspace = {
    revision: 2,
    context,
    entities: [item],
    events: [],
    originals: [
      {
        id: "s_one",
        source: "system",
        startMs: 0,
        endMs: 30000,
        text: "Die Freigabe bleibt noch offen.",
      },
    ],
    overviewRefs: [],
  };
  const status = {
    status: "updating",
    running: true,
    intervalMs: 120000,
    updateElapsedMs: 210000,
    pendingChars: 180,
    view: { runId: "run-current", model: "synthetic-test", workspace },
    workspaceContext: context,
  };
  const adapter = {
    status: vi.fn(async () => {
      const value = structuredClone(status);
      value.workspaceContext = {
        overrides: value.workspaceContext.overrides,
        sharedNotes: value.workspaceContext.sharedNotes,
        agenda: value.workspaceContext.agenda,
        revision: value.workspaceContext.revision,
      };
      return value;
    }),
    context: vi.fn(async (value: typeof context) => {
      status.workspaceContext = { ...value, revision: value.revision + 1 };
      workspace.context = status.workspaceContext;
      const last = value.overrides.at(-1) as { status: string } | undefined;
      if (last) {
        item.status = last.status;
        item.revision++;
        item.humanLocked = true;
      }
      return true;
    }),
    start: vi.fn(),
    stop: vi.fn(),
    refresh: vi.fn(),
    interval: vi.fn(),
    models: vi.fn(),
  };
  const meeting = {
    id: "synthetic-workspace-ui",
    title: "Synthetisches Prüfmeeting",
    parts: [
      {
        file: "generated-one",
        source: "system",
        startSample: 0,
        sampleCount: 480000,
        status: "complete",
        text: "Die Freigabe bleibt noch offen.",
      },
      {
        file: "generated-two",
        source: "microphone",
        startSample: 480000,
        sampleCount: 480000,
        status: "complete",
        text: "Robin schreibt den Bericht.",
      },
    ],
  };
  return { adapter, meeting, status };
}
describe("Native workspace boundary", () => {
  it("restores native shared notes without replacing private text or submitting a stale browser cache", async () => {
    const f = fixture();
    localStorage.setItem(
      `handymeeting.workspace.v1.${f.meeting.id}`,
      JSON.stringify({
        version: 1,
        nativeContextRunId: "run-other",
        nativeContextRevision: 7,
        topics: [
          { id: "e_unsynced", text: "Lokales Thema erhalten", shared: true },
        ],
        history: [],
        attachments: [],
        notes: { private: "PRIVATE_ONLY", protocol: "Veralteter Entwurf" },
        preparation: { goal: "PRIVATE_GOAL", agenda: "" },
      }),
    );
    render(
      <StrictMode>
        <LiveWorkspace
          adapter={f.adapter}
          meeting={f.meeting}
          onBack={() => {}}
        />
      </StrictMode>,
    );
    await screen.findAllByText("Welche Freigabe fehlt?");
    fireEvent.click(screen.getByRole("button", { name: "Fürs Protokoll" }));
    expect(
      (
        screen.getByRole("textbox", {
          name: "Protokollnotiz",
        }) as HTMLTextAreaElement
      ).value,
    ).toBe("Gesicherte Protokollnotiz");
    expect(f.adapter.context).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Privat" }));
    expect(
      (
        screen.getByRole("textbox", {
          name: "Private Notiz",
        }) as HTMLTextAreaElement
      ).value,
    ).toBe("PRIVATE_ONLY");
    expect(JSON.stringify(f.adapter.status.mock.calls)).not.toContain(
      "PRIVATE_ONLY",
    );
    fireEvent.click(screen.getByRole("tab", { name: "Vorbereitung" }));
    const preserved = screen
      .getByRole("button", { name: "Lokales Thema erhalten" })
      .closest("article")!;
    expect(
      (
        within(preserved).getByRole("checkbox", {
          name: "Freigeben",
        }) as HTMLInputElement
      ).checked,
    ).toBe(false);
    expect(f.adapter.context).not.toHaveBeenCalled();
  });
  it("renders English workspace controls and the original audio preview", async () => {
    const f = fixture();
    localStorage.setItem(
      `handymeeting.workspace.v1.${f.meeting.id}.notes-height.v1`,
      "unreadable synthetic height",
    );
    render(
      <LiveWorkspace
        adapter={{ ...f.adapter, sourceAudio: vi.fn() }}
        meeting={f.meeting}
        language="en"
        onBack={() => {}}
      />,
    );
    expect(
      await screen.findByRole("tab", { name: "Preparation" }),
    ).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Transcript" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Overview" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Prompts" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Standard view" })).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Stop analysis only" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Arrange" }));
    expect(
      screen.getAllByRole("button", { name: / remove from overview$/ }).length,
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByRole("button", { name: / expand$/ }).length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByLabelText(/card height$/).length).toBeGreaterThan(0);
    expect(
      screen.getAllByRole("separator", { name: /resize height$/ }).length,
    ).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    fireEvent.click(screen.getByRole("tab", { name: "Transcript" }));
    expect(
      screen.getByRole("textbox", { name: "Transcript search" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "For the minutes" }));
    expect(screen.getByRole("textbox", { name: "Protocol note" })).toBeTruthy();
    expect(
      screen.getByText(/The height is kept only for this view/),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Preparation" }));
    expect(screen.getByRole("textbox", { name: "My goal" })).toBeTruthy();
    expect(
      screen.getByRole("textbox", { name: "Private preparation" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Prompts" }));
    fireEvent.click(
      screen.getByRole("button", { name: /Die Freigabe bleibt noch offen/ }),
    );
    expect(
      screen.getByRole("dialog", { name: "Original evidence" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Load audio preview" }),
    ).toBeTruthy();
  });
  it("keeps a dismissible foreground action error separate from recording health", async () => {
    const f = fixture();
    const dismiss = vi.fn();
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={{ ...f.meeting, status: "recording" }}
        language="en"
        onBack={() => {}}
        actionError="Synthetic export failed"
        onDismissError={dismiss}
      />,
    );
    expect(await screen.findByText("Synthetic export failed")).toBeTruthy();
    expect(screen.queryByText(/Check recording/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss message" }));
    expect(dismiss).toHaveBeenCalledOnce();
    expect(f.adapter.stop).not.toHaveBeenCalled();
  });
  it("does not stop or restart a slow update and coalesces a manual refresh", async () => {
    const f = fixture();
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={{
          ...f.meeting,
          modelId: "parakeet-ultra",
          status: "recording",
          pendingAudioMs: 15000,
        }}
        onBack={() => {}}
      />,
    );
    expect(screen.getByText("Aufnahme läuft")).toBeTruthy();
    expect(
      screen.getByText("Abschnittstranskription · offenes Audio 00:15"),
    ).toBeTruthy();
    await screen.findByText(
      "Auswertung läuft · 3:30 · weiterer Text folgt danach · Belege bis 00:30 · 180 Zeichen noch nicht ausgewertet",
    );
    fireEvent.click(screen.getByRole("button", { name: "Aktualisieren" }));
    await waitFor(() => expect(f.adapter.refresh).toHaveBeenCalledTimes(1));
    expect(f.adapter.stop).not.toHaveBeenCalled();
    expect(f.adapter.start).not.toHaveBeenCalled();
  });
  it("retains a discussed impulse and appends its reopen action with the actual revision", async () => {
    const f = fixture();
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "Impulse" }));
    fireEvent.click(screen.getByRole("button", { name: "Besprochen" }));
    await screen.findByRole("button", { name: "Wieder öffnen" });
    expect(f.adapter.context.mock.calls[0][0].overrides).toEqual([
      {
        id: "e_impulse",
        expectedRevision: 1,
        status: "discussed",
        saved: false,
      },
    ]);
    fireEvent.click(screen.getByRole("button", { name: "Wieder öffnen" }));
    await waitFor(() => expect(f.adapter.context).toHaveBeenCalledTimes(2));
    expect(f.adapter.context.mock.calls[1][0].overrides).toEqual([
      {
        id: "e_impulse",
        expectedRevision: 1,
        status: "discussed",
        saved: false,
      },
      { id: "e_impulse", expectedRevision: 2, status: "open", saved: false },
    ]);
    expect(screen.getByText("Welche Freigabe fehlt?")).toBeTruthy();
  });
  it("filters a custom transcript interval at section boundaries", async () => {
    const f = fixture();
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "Transkript" }));
    const panel = within(screen.getByRole("tabpanel", { name: "Transkript" }));
    fireEvent.change(
      panel.getByRole("combobox", { name: "Transkriptzeitraum" }),
      {
        target: { value: "custom" },
      },
    );
    fireEvent.change(panel.getByRole("spinbutton", { name: "Von (Min.)" }), {
      target: { value: "0.5" },
    });
    expect(panel.queryByText("Die Freigabe bleibt noch offen.")).toBeNull();
    expect(panel.getByText("Robin schreibt den Bericht.")).toBeTruthy();
  });
  it("filters actual transcript tracks and keeps search when expanded", async () => {
    const f = fixture();
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    fireEvent.click(await screen.findByRole("tab", { name: "Transkript" }));
    fireEvent.change(screen.getByRole("combobox", { name: "Transkriptspur" }), {
      target: { value: "microphone" },
    });
    const panel = screen.getByRole("tabpanel", { name: "Transkript" });
    expect(
      within(panel).queryByText("Die Freigabe bleibt noch offen."),
    ).toBeNull();
    expect(within(panel).getByText("Robin schreibt den Bericht.")).toBeTruthy();
    fireEvent.change(
      screen.getByRole("textbox", { name: "Transkript durchsuchen" }),
      { target: { value: "absent" } },
    );
    expect(
      within(panel).getByText(
        "Für diese Auswahl kein bestätigter Transkriptabschnitt.",
      ),
    ).toBeTruthy();
  });
});

it("preserves a pending topic revocation through native publication and reopening", async () => {
  const f = fixture();
  const mounted = render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(await screen.findByRole("tab", { name: "Vorbereitung" }));
  let share = screen.getByRole("checkbox", {
    name: "Freigeben",
  }) as HTMLInputElement;
  expect(share.checked).toBe(true);
  fireEvent.click(share);
  expect(share.checked).toBe(false);
  expect(f.adapter.context).not.toHaveBeenCalled();
  f.status.workspaceContext.revision++;
  await waitFor(
    () => expect(f.adapter.status.mock.calls.length).toBeGreaterThan(1),
    { timeout: 4000 },
  );
  expect(
    (screen.getByRole("checkbox", { name: "Freigeben" }) as HTMLInputElement)
      .checked,
  ).toBe(false);
  mounted.unmount();
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(await screen.findByRole("tab", { name: "Vorbereitung" }));
  share = screen.getByRole("checkbox", {
    name: "Freigeben",
  }) as HTMLInputElement;
  expect(share.checked).toBe(false);
  f.status.status = "waiting";
  await waitFor(() => expect(f.adapter.context).toHaveBeenCalledOnce(), {
    timeout: 4000,
  });
  expect(f.adapter.context.mock.calls[0][0].agenda).toEqual([]);
  expect(share.checked).toBe(false);
}, 10000);

it("isolates native workspace styles and returns capture warnings without changing recording", async () => {
  const f = fixture();
  const back = vi.fn();
  const api = {
    getMeeting: async () => f.meeting,
    getLiveNotes: f.adapter.status,
    startLiveNotes: f.adapter.start,
    stopLiveNotes: f.adapter.stop,
    refreshLiveNotes: f.adapter.refresh,
    setLiveNotesInterval: f.adapter.interval,
    setWorkspaceContext: vi.fn(),
    localModelSetup: f.adapter.models,
  } as unknown as DesktopApi;
  const meeting = {
    ...f.meeting,
    status: "recording",
    captureIncomplete: true,
    warnings: ["synthetic capture warning"],
  } as unknown as Meeting;
  const mounted = render(
    <WorkspaceBridge
      api={api}
      meeting={meeting}
      endpoint="http://127.0.0.1:1235/v1/"
      model="google/gemma-4-e4b"
      onBack={back}
    />,
  );
  const host = mounted.container.querySelector(".native-workspace-host")!;
  await waitFor(() =>
    expect(host.shadowRoot?.textContent).toContain("Aufnahme läuft"),
  );
  const shadow = within(host.shadowRoot as unknown as HTMLElement);
  expect(host.shadowRoot?.querySelectorAll("style").length).toBe(1);
  expect(mounted.container.querySelector(".prototype")).toBeNull();
  fireEvent.click(shadow.getByRole("button", { name: /Aufnahme prüfen/ }));
  expect(back).toHaveBeenCalledTimes(1);
  expect(f.adapter.stop).not.toHaveBeenCalled();
});

it("keeps transcript available and does not repeat original text in the timeline when analysis is unavailable", async () => {
  const f = fixture();
  f.adapter.status.mockRejectedValue(
    new Error("Lokaler Dienst nicht verfügbar"),
  );
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  const layout = () =>
    screen.getByRole("combobox", {
      name: "Überblick-Layout",
    }) as HTMLSelectElement;
  expect(layout().value).toBe("verlauf");
  fireEvent.change(layout(), { target: { value: "regie" } });
  expect(layout().value).toBe("regie");
  fireEvent.change(layout(), { target: { value: "verlauf" } });
  expect(layout().value).toBe("verlauf");
  await screen.findByText(/Lokaler Dienst nicht verfügbar/);
  fireEvent.click(screen.getByRole("tab", { name: "Verlauf" }));
  const timeline = screen.getByRole("tabpanel", { name: "Verlauf" });
  expect(
    within(timeline).queryByText("Die Freigabe bleibt noch offen."),
  ).toBeNull();
  expect(within(timeline).getByText(/belegte Entwicklungen/)).toBeTruthy();
  fireEvent.click(screen.getByRole("tab", { name: "Transkript" }));
  const transcript = screen.getByRole("tabpanel", { name: "Transkript" });
  expect(
    within(transcript).getByText("Robin schreibt den Bericht."),
  ).toBeTruthy();
  expect(f.adapter.start).not.toHaveBeenCalled();
});

it("uses the original source range to focus the matching transcript after opening a model citation", async () => {
  const f = fixture();
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(screen.getByRole("tab", { name: "Impulse" }));
  fireEvent.click(
    await screen.findByRole("button", {
      name: /Die Freigabe bleibt noch offen/,
    }),
  );
  fireEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "Im Transkript ansehen",
    }),
  );
  const transcript = screen.getByRole("tabpanel", { name: "Transkript" });
  expect(
    within(transcript)
      .getByText("Die Freigabe bleibt noch offen.")
      .closest("article")
      ?.classList.contains("source-focused"),
  ).toBe(true);
});

it("requests WAV bytes only on demand and revokes them when the source closes", async () => {
  const f = fixture();
  const load = vi.fn(async (_file: string) => new ArrayBuffer(44));
  const create = vi.fn(() => "blob:synthetic-preview");
  const revoke = vi.fn();
  const originalCreate = Object.getOwnPropertyDescriptor(
    URL,
    "createObjectURL",
  );
  const originalRevoke = Object.getOwnPropertyDescriptor(
    URL,
    "revokeObjectURL",
  );
  Object.defineProperty(URL, "createObjectURL", {
    configurable: true,
    value: create,
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    configurable: true,
    value: revoke,
  });
  try {
    render(
      <LiveWorkspace
        adapter={{ ...f.adapter, sourceAudio: load }}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("tab", { name: "Impulse" }));
    fireEvent.click(
      await screen.findByRole("button", {
        name: /Die Freigabe bleibt noch offen/,
      }),
    );
    expect(load).not.toHaveBeenCalled();
    const source = screen.getByRole("dialog");
    fireEvent.click(
      within(source).getByRole("button", { name: "Audiovorschau laden" }),
    );
    await within(source).findByLabelText(
      "Audiovorschau des Originalabschnitts",
    );
    expect(load).toHaveBeenCalledExactlyOnceWith("generated-one");
    expect((create.mock.calls[0] as unknown[])[0]).toBeInstanceOf(Blob);
    fireEvent.click(
      within(source).getByRole("button", { name: "Im Transkript ansehen" }),
    );
    expect(revoke).toHaveBeenCalledExactlyOnceWith("blob:synthetic-preview");
  } finally {
    if (originalCreate)
      Object.defineProperty(URL, "createObjectURL", originalCreate);
    else Reflect.deleteProperty(URL, "createObjectURL");
    if (originalRevoke)
      Object.defineProperty(URL, "revokeObjectURL", originalRevoke);
    else Reflect.deleteProperty(URL, "revokeObjectURL");
  }
});

it("uses the current shared model configuration and blocks cloud starts in the workspace", async () => {
  const f = fixture();
  f.status.running = false;
  const start = f.adapter.start.mockResolvedValue({});
  const props = { adapter: f.adapter, meeting: f.meeting, onBack: () => {} };
  const view = render(
    <LiveWorkspace
      {...props}
      mode="cloud"
      initialEndpoint="https://cloud.invalid/v1/"
      initialModel="cloud-choice"
    />,
  );
  expect(
    (
      screen.getByRole("button", {
        name: "Erweiterte Auswertung nur lokal verfügbar",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(start).not.toHaveBeenCalled();
  view.rerender(
    <LiveWorkspace
      {...props}
      mode="local"
      initialEndpoint="http://127.0.0.1:11434/v1/"
      initialModel="shared-choice"
    />,
  );
  const button = screen.getByRole("button", {
    name: "Lokale Auswertung starten",
  }) as HTMLButtonElement;
  await waitFor(() => expect(button.disabled).toBe(false));
  fireEvent.click(button);
  await waitFor(() =>
    expect(start).toHaveBeenCalledWith(
      expect.objectContaining({
        endpoint: "http://127.0.0.1:11434/v1/",
        model: "shared-choice",
        mode: "local",
        cloudConsent: false,
        apiKey: "",
      }),
    ),
  );
});

it("saves a named copy with its widths and opens it as the default layout", async () => {
  const f = fixture();
  const view = () => (
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />
  );
  const first = render(view());
  const divider = screen.getByRole("separator", { name: /Spalten 1 und 2/ });
  fireEvent.keyDown(divider, { key: "ArrowRight" });
  expect(divider.getAttribute("aria-valuenow")).toBe("52");
  fireEvent.click(screen.getByRole("button", { name: "Anordnen" }));
  fireEvent.click(screen.getByText("Layouts verwalten"));
  fireEvent.change(screen.getByRole("textbox", { name: "Layoutname" }), {
    target: { value: "Kompakt" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Als neues Layout speichern" }),
  );
  expect(
    screen.getByRole("combobox", { name: "Überblick-Layout" }).textContent,
  ).toContain("Kompakt");
  fireEvent.click(screen.getByRole("button", { name: "Anordnen" }));
  fireEvent.click(screen.getByText("Layouts verwalten"));
  fireEvent.click(screen.getByRole("button", { name: "Als Standard öffnen" }));
  first.unmount();
  render(view());
  const select = screen.getByRole("combobox", {
    name: "Überblick-Layout",
  }) as HTMLSelectElement;
  expect(select.selectedOptions[0].text).toBe("Kompakt · Standard");
  expect(
    screen
      .getByRole("separator", { name: /Spalten 1 und 2/ })
      .getAttribute("aria-valuenow"),
  ).toBe("52");
});

it("shows a relevant result only once inside the live overview", async () => {
  const f = fixture();
  const workspace = f.status.view.workspace;
  (workspace.overviewRefs as string[]).push(workspace.entities[0].id);
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  const overview = within(
    screen.getByRole("region", { name: "Live-Überblick" }),
  );
  expect(
    await overview.findByRole("heading", { name: "Jetzt relevant" }),
  ).toBeTruthy();
  expect(overview.getAllByText("Welche Freigabe fehlt?")).toHaveLength(1);
  expect(screen.queryByRole("region", { name: "Jetzt relevant" })).toBeNull();
});

it.each([
  [false, "de"],
  [true, "de"],
  [false, "en"],
  [true, "en"],
] as const)(
  "moves a card using the pointer grip (stacked=%s, language=%s)",
  (stacked, language) => {
    const f = fixture();
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        language={language}
        onBack={() => {}}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: language === "en" ? "Arrange" : "Anordnen",
      }),
    );
    const left = screen.getByLabelText(
      language === "en" ? "Overview column 1" : "Überblick Spalte 1",
    );
    const right = screen.getByLabelText(
      language === "en" ? "Overview column 2" : "Überblick Spalte 2",
    );
    vi.spyOn(left, "getBoundingClientRect").mockReturnValue(
      new DOMRect(0, 0, 400, 600),
    );
    vi.spyOn(right, "getBoundingClientRect").mockReturnValue(
      new DOMRect(410, 0, 400, 600),
    );
    if (stacked) {
      const grid = left.parentElement!;
      grid.style.display = "flex";
      vi.spyOn(grid, "getBoundingClientRect").mockReturnValue(
        new DOMRect(0, 0, 400, 1100),
      );
      vi.mocked(left.getBoundingClientRect).mockReturnValue(new DOMRect());
      vi.mocked(right.getBoundingClientRect).mockReturnValue(new DOMRect());
      [...grid.querySelectorAll(".overview-card")].forEach((card, index) =>
        vi
          .spyOn(card, "getBoundingClientRect")
          .mockReturnValue(new DOMRect(0, 200 * index, 400, 180)),
      );
    }
    const grip = within(right).getByRole("button", {
      name:
        language === "en"
          ? "Conversation prompts move"
          : "Gesprächsimpulse verschieben",
    });
    Object.defineProperty(grip, "setPointerCapture", { value: vi.fn() });
    const pointer = (type: string, x: number, y: number) =>
      fireEvent(
        grip,
        Object.assign(
          new MouseEvent(type, {
            bubbles: true,
            button: 0,
            clientX: x,
            clientY: y,
          }),
          { isPrimary: true, pointerId: 1 },
        ),
      );
    pointer("pointerdown", 450, 450);
    pointer("pointermove", 250, 30);
    expect(
      screen.getByText(
        language === "en"
          ? "Insert here: Conversation prompts"
          : "Hier einfügen: Gesprächsimpulse",
      ),
    ).toBeTruthy();
    pointer("pointerup", 250, 30);
    expect(
      within(left).getByRole("button", {
        name:
          language === "en"
            ? "Conversation prompts move"
            : "Gesprächsimpulse verschieben",
      }),
    ).toBeTruthy();
    expect(
      within(right).queryByRole("button", {
        name:
          language === "en"
            ? "Conversation prompts move"
            : "Gesprächsimpulse verschieben",
      }),
    ).toBeNull();
    expect(
      screen
        .getByText(
          language === "en"
            ? "Conversation prompts dropped in column 1."
            : "Gesprächsimpulse in Spalte 1 abgelegt.",
        )
        .getAttribute("aria-live"),
    ).toBe("polite");
  },
);

it("gives a manually resized card real space in a crowded column", () => {
  const cards = [
    { height: 288 },
    { height: 288 },
    { height: 312 },
    { height: 288 },
  ];
  const available = 520;
  expect(fitColumn(cards, available).heights[0]).toBeLessThan(125);
  const resized = fitColumn(
    [{ height: 192, manualHeight: true }, ...cards.slice(1)],
    available,
  );
  expect(resized.heights[0]).toBe(192);
  expect(resized.heights[3]).toBeGreaterThanOrEqual(72);
  expect(
    resized.heights.reduce((a: number, b: number) => a + b, 0) +
      3 * resized.gap,
  ).toBeCloseTo(available);
});

it.each([false, true])(
  "uses the configured credential mode for workspace starts (saved=%s)",
  async (useSavedKey) => {
    const f = fixture();
    f.status.running = false;
    f.status.status = "off";
    const api = {
      getLiveNotes: f.adapter.status,
      startLiveNotes: f.adapter.start,
      stopLiveNotes: f.adapter.stop,
      refreshLiveNotes: f.adapter.refresh,
      setLiveNotesInterval: f.adapter.interval,
      setWorkspaceContext: vi.fn(),
      localModelSetup: f.adapter.models,
    } as unknown as DesktopApi;
    const { container } = render(
      <WorkspaceBridge
        api={api}
        meeting={f.meeting as unknown as Meeting}
        endpoint="http://127.0.0.1:1235/v1/"
        model="synthetic"
        apiKey="synthetic-fixture-key"
        useSavedKey={useSavedKey}
        onBack={() => {}}
      />,
    );
    const shadow = within(
      container.querySelector(".native-workspace-host")!
        .shadowRoot as unknown as HTMLElement,
    );
    const start = await shadow.findByRole("button", {
      name: "Lokale Auswertung starten",
    });
    await waitFor(() => expect(start.hasAttribute("disabled")).toBe(false));
    fireEvent.click(start);
    await waitFor(() =>
      expect(f.adapter.start).toHaveBeenCalledWith(
        expect.objectContaining({
          apiKey: useSavedKey ? "" : "synthetic-fixture-key",
          useSavedKey,
          mode: "local",
          cloudConsent: false,
        }),
      ),
    );
  },
);

it("commits protocol notes after inference and retains a newer uncommitted draft on reopen", async () => {
  const f = fixture();
  const mounted = render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Fürs Protokoll" }),
  );
  let note = screen.getByRole("textbox", {
    name: "Protokollnotiz",
  }) as HTMLTextAreaElement;
  await waitFor(() => expect(note.value).toBe("Gesicherte Protokollnotiz"));
  fireEvent.change(note, { target: { value: "Committed protocol edit" } });
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 2100));
  });
  expect(f.adapter.context).not.toHaveBeenCalled();
  fireEvent.blur(note);
  await act(async () => {});
  expect(f.adapter.context).not.toHaveBeenCalled();
  mounted.unmount();
  const reloaded = render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Fürs Protokoll" }),
  );
  note = screen.getByRole("textbox", {
    name: "Protokollnotiz",
  }) as HTMLTextAreaElement;
  await waitFor(() => expect(note.value).toBe("Committed protocol edit"));
  fireEvent.change(note, { target: { value: "New uncommitted draft" } });
  f.status.status = "waiting";
  await waitFor(() => expect(f.adapter.context).toHaveBeenCalledOnce(), {
    timeout: 4000,
  });
  expect(
    f.adapter.context.mock.calls[0][0].sharedNotes.map((n) => n.text).join(""),
  ).toBe("Committed protocol edit");
  expect(note.value).toBe("New uncommitted draft");
  reloaded.unmount();
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Fürs Protokoll" }),
  );
  await waitFor(() =>
    expect(
      (
        screen.getByRole("textbox", {
          name: "Protokollnotiz",
        }) as HTMLTextAreaElement
      ).value,
    ).toBe("New uncommitted draft"),
  );
}, 10000);

it("does not acknowledge a context write deferred by the native inference gate", async () => {
  const f = fixture();
  f.status.status = "waiting";
  f.adapter.context.mockImplementationOnce(async () => false);
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Fürs Protokoll" }),
  );
  const note = screen.getByRole("textbox", {
    name: "Protokollnotiz",
  }) as HTMLTextAreaElement;
  await waitFor(() => expect(note.value).toBe("Gesicherte Protokollnotiz"));
  fireEvent.change(note, { target: { value: "Queued at native race" } });
  fireEvent.blur(note);
  await waitFor(() => expect(f.adapter.context).toHaveBeenCalledOnce());
  expect(f.status.workspaceContext.revision).toBe(7);
  await waitFor(() => expect(f.adapter.context).toHaveBeenCalledTimes(2), {
    timeout: 4000,
  });
  expect(f.adapter.context.mock.calls[1][0].revision).toBe(7);
  expect(f.status.workspaceContext.sharedNotes[0].text).toBe(
    "Queued at native race",
  );
});

it.each([
  { name: "note plus JSON overhead", note: "x".repeat(8000), topics: [] },
  { name: "JSON-escaped text", note: '\\"\n'.repeat(1500), topics: [] },
  {
    name: "shared agenda plus note",
    note: "x".repeat(2000),
    topics: Array.from({ length: 7 }, (_, i) => ({
      id: `e_budget_${i}`,
      revision: 1,
      text: "a".repeat(900),
    })),
  },
])(
  "keeps an oversized $name local and lets a later topic revocation reach native state",
  async ({ note: text, topics }) => {
    const f = fixture();
    f.status.status = "waiting";
    f.status.workspaceContext.agenda.push(...topics);
    const mounted = render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Fürs Protokoll" }),
    );
    const note = screen.getByRole("textbox", {
      name: "Protokollnotiz",
    }) as HTMLTextAreaElement;
    await waitFor(() => expect(note.value).toBe("Gesicherte Protokollnotiz"));
    fireEvent.change(note, { target: { value: text } });
    fireEvent.blur(note);
    await act(async () => {});
    expect(f.adapter.context.mock.calls.length).toBe(0);
    expect(
      screen.getByText(/Bitte Text kürzen oder weniger Themen freigeben/),
    ).toBeTruthy();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 2100));
    });
    expect(f.adapter.context.mock.calls.length).toBe(0);
    fireEvent.click(screen.getByRole("tab", { name: "Vorbereitung" }));
    fireEvent.click(screen.getAllByRole("checkbox", { name: "Freigeben" })[0]);
    await waitFor(() => expect(f.adapter.context.mock.calls.length).toBe(1), {
      timeout: 4000,
    });
    expect(
      f.status.workspaceContext.agenda.some((x) => x.id === "e_agenda"),
    ).toBe(false);
    expect(
      f.status.workspaceContext.sharedNotes.map((x) => x.text).join(""),
    ).toBe("Gesicherte Protokollnotiz");
    mounted.unmount();
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Fürs Protokoll" }),
    );
    const restored = screen.getByRole("textbox", {
      name: "Protokollnotiz",
    }) as HTMLTextAreaElement;
    await waitFor(() => expect(restored.value).toBe(text));
    fireEvent.change(restored, {
      target: { value: "Short corrected protocol" },
    });
    fireEvent.blur(restored);
    await waitFor(() =>
      expect(f.status.workspaceContext.sharedNotes[0].text).toBe(
        "Short corrected protocol",
      ),
    );
  },
  15000,
);

it("recovers a cached over-budget pending protocol without sending it or losing a revocation", async () => {
  const f = fixture();
  f.status.status = "waiting";
  const text = "x".repeat(8000);
  localStorage.setItem(
    `handymeeting.workspace.v1.${f.meeting.id}`,
    JSON.stringify({
      version: 1,
      topics: [
        {
          ...f.status.workspaceContext.agenda[0],
          shared: false,
          pending: true,
        },
      ],
      notes: { private: "PRIVATE", protocol: "Newer local draft" },
      protocolRevision: 5,
      protocolShare: { text, revision: 4, pending: true },
      preparation: { goal: "", agenda: "" },
      history: [],
      attachments: [],
      impulseStates: {},
      nativeContextRunId: "run-current",
      nativeContextRevision: 7,
    }),
  );
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  await waitFor(() => expect(f.adapter.context.mock.calls.length).toBe(1));
  const sent = f.adapter.context.mock.calls[0][0];
  expect(sent.sharedNotes.map((n) => n.text).join("")).toBe(
    "Gesicherte Protokollnotiz",
  );
  expect(sent.agenda).toEqual([]);
  fireEvent.click(screen.getByRole("button", { name: "Fürs Protokoll" }));
  expect(
    (
      screen.getByRole("textbox", {
        name: "Protokollnotiz",
      }) as HTMLTextAreaElement
    ).value,
  ).toBe("Newer local draft");
  const retained = JSON.parse(
    localStorage.getItem(`handymeeting.workspace.v1.${f.meeting.id}`)!,
  );
  expect(
    retained.history.some(
      (entry: { content: string }) => entry.content === text,
    ),
  ).toBe(true);
  expect(retained.notes.private).toBe("PRIVATE");
});

it("admits a valid Unicode protocol using native scalar accounting with shared agenda and chunk overhead", async () => {
  const f = fixture();
  f.status.status = "waiting";
  f.status.workspaceContext.agenda.push(
    ...Array.from({ length: 5 }, (_, i) => ({
      id: `e_unicode_${i}`,
      revision: 1,
      text: "a".repeat(900),
    })),
  );
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Fürs Protokoll" }),
  );
  const note = screen.getByRole("textbox", { name: "Protokollnotiz" });
  fireEvent.change(note, { target: { value: "🙂".repeat(1800) } });
  fireEvent.blur(note);
  await waitFor(() => expect(f.adapter.context.mock.calls.length).toBe(1));
  const sent = f.adapter.context.mock.calls[0][0];
  const serialized = JSON.stringify([sent.agenda, sent.sharedNotes]);
  expect(serialized.length).toBeGreaterThan(8000);
  expect(Array.from(serialized).length).toBeLessThanOrEqual(8000);
  expect(sent.sharedNotes.map((x) => x.text).join("")).toBe("🙂".repeat(1800));
  expect(sent.sharedNotes).toHaveLength(2);
});

it("rejects a newly over-budget topic share but preserves the draft and permits a topic withdrawal", async () => {
  const f = fixture();
  f.status.status = "waiting";
  f.status.workspaceContext.sharedNotes = Array.from({ length: 5 }, (_, i) => ({
    id: `h_protocol_${i}`,
    revision: 3,
    text: "x".repeat(Math.min(1600, 7500 - i * 1600)),
  }));
  render(
    <LiveWorkspace adapter={f.adapter} meeting={f.meeting} onBack={() => {}} />,
  );
  fireEvent.click(await screen.findByRole("tab", { name: "Vorbereitung" }));
  const text = "New topic " + "a".repeat(170);
  fireEvent.change(
    screen.getByRole("textbox", { name: "Neuer Gesprächspunkt" }),
    { target: { value: text } },
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Gesprächspunkt hinzufügen" }),
  );
  const checkboxes = screen.getAllByRole("checkbox", {
    name: "Freigeben",
  }) as HTMLInputElement[];
  fireEvent.click(checkboxes[1]);
  expect(checkboxes[1].checked).toBe(false);
  expect(screen.getByText(text)).toBeTruthy();
  expect(
    screen.getByText(/Bitte Text kürzen oder weniger Themen freigeben/),
  ).toBeTruthy();
  expect(f.adapter.context.mock.calls.length).toBe(0);
  fireEvent.click(checkboxes[0]);
  await waitFor(() => expect(f.adapter.context.mock.calls.length).toBe(1), {
    timeout: 4000,
  });
  expect(f.adapter.context.mock.calls[0][0].agenda).toEqual([]);
  expect(
    f.status.workspaceContext.sharedNotes.map((x) => x.text).join(""),
  ).toHaveLength(7500);
});

it.each(["de", "en"] as const)(
  "reports a rejected manual status change and permits an explicit later retry in %s",
  async (language) => {
    const f = fixture();
    f.adapter.context.mockImplementationOnce(async () => false);
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        language={language}
        onBack={() => {}}
      />,
    );
    fireEvent.click(
      await screen.findByRole("tab", {
        name: language === "de" ? "Impulse" : "Prompts",
      }),
    );
    const label = language === "de" ? "Besprochen" : "Discussed";
    fireEvent.click(screen.getByRole("button", { name: label }));
    await waitFor(() => expect(f.adapter.context).toHaveBeenCalledOnce());
    expect(f.status.view.workspace.entities[0].status).toBe("open");
    expect(
      await screen.findByText(
        language === "de"
          ? "Die Änderung wurde nicht übernommen. Bitte aktuellen Auswertungsstand abwarten und erneut versuchen."
          : "The change was not applied. Wait for the current analysis state and try again.",
      ),
    ).toBeTruthy();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 2100));
    });
    expect(f.adapter.context).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: label }));
    await waitFor(() => expect(f.adapter.context).toHaveBeenCalledTimes(2));
    expect(f.status.view.workspace.entities[0].status).toBe("discussed");
    await screen.findByRole("button", {
      name: language === "de" ? "Wieder öffnen" : "Reopen",
    });
    expect(
      screen.queryByText(
        language === "de"
          ? "Die Änderung wurde nicht übernommen. Bitte aktuellen Auswertungsstand abwarten und erneut versuchen."
          : "The change was not applied. Wait for the current analysis state and try again.",
      ),
    ).toBeNull();
  },
);

it("translates pending and unavailable audio-preview states", async () => {
  const f = fixture();
  let fail!: (error: Error) => void;
  const sourceAudio = vi.fn(
    () =>
      new Promise<Uint8Array>((_, reject) => {
        fail = reject;
      }),
  );
  render(
    <LiveWorkspace
      adapter={{ ...f.adapter, sourceAudio }}
      meeting={f.meeting}
      language="en"
      onBack={() => {}}
    />,
  );
  fireEvent.click(await screen.findByRole("tab", { name: "Prompts" }));
  fireEvent.click(
    screen.getByRole("button", { name: /Die Freigabe bleibt noch offen/ }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Load audio preview" }));
  expect(screen.getByRole("button", { name: "Loading audio …" })).toBeTruthy();
  await act(async () => fail(new Error("synthetic unavailable")));
  expect(
    await screen.findByText(/Local audio preview is unavailable/),
  ).toBeTruthy();
});

it.each(["de", "en"] as const)(
  "separates dynamic workspace labels in %s",
  async (locale) => {
    const f = fixture();
    const task = {
      ...f.status.view.workspace.entities[0],
      id: "e_task",
      kind: "task",
      text: "Bericht schreiben",
      owner: "Robin",
      due: "Freitag",
    };
    (f.status.view.workspace.entities as unknown[]).push(task);
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        language={locale}
        onBack={() => {}}
      />,
    );
    fireEvent.click(
      screen.getByRole("tab", {
        name: locale === "de" ? "Transkript" : "Transcript",
      }),
    );
    expect(
      screen.getByRole("option", {
        name: locale === "de" ? "Letzte 5 Min." : "Last 5 min",
      }),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("tab", {
        name: locale === "de" ? "Impulse" : "Prompts",
      }),
    );
    expect(
      await screen.findByText(locale === "de" ? "von 1" : "of 1"),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Details" }));
    expect(
      await screen.findByText(
        locale === "de"
          ? "Zuständig: Robin · Termin: Freitag"
          : "Owner: Robin · Due: Freitag",
      ),
    ).toBeTruthy();
  },
);

it.each(["de", "en"] as const)(
  "separates empty-column drop labels in %s",
  async (language) => {
    const f = fixture();
    localStorage.setItem(
      "handymeeting.workspace.user-layout.v1.verlauf.layout.v1",
      JSON.stringify({
        version: 1,
        columns: 4,
        widths: [25, 25, 25, 25],
        cards: [{ id: "summary", column: 0, height: 240 }],
      }),
    );
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        language={language}
        onBack={() => {}}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: language === "de" ? "Anordnen" : "Arrange",
      }),
    );
    fireEvent.click(
      screen.getByRole("button", {
        name:
          language === "de"
            ? "Live-Überblick verschieben"
            : "Live overview move",
      }),
    );
    for (let column = 1; column <= 4; column++)
      expect(
        screen.getByRole("button", {
          name:
            language === "de"
              ? `Hier in Spalte ${column} ablegen`
              : `Drop into column ${column} here`,
        }),
      ).toBeTruthy();
  },
);
