import { StrictMode } from "react";
import {
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

it.each([false, true])(
  "moves a card using the pointer grip (stacked=%s)",
  (stacked) => {
    const f = fixture();
    render(
      <LiveWorkspace
        adapter={f.adapter}
        meeting={f.meeting}
        onBack={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Anordnen" }));
    const left = screen.getByLabelText("Überblick Spalte 1");
    const right = screen.getByLabelText("Überblick Spalte 2");
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
      name: "Gesprächsimpulse verschieben",
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
    pointer("pointerup", 250, 30);
    expect(
      within(left).getByRole("button", {
        name: "Gesprächsimpulse verschieben",
      }),
    ).toBeTruthy();
    expect(
      within(right).queryByRole("button", {
        name: "Gesprächsimpulse verschieben",
      }),
    ).toBeNull();
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
