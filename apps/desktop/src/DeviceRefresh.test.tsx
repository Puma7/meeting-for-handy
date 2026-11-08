import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  configure,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { lostSelections, useDeviceRefresh } from "./deviceWatch";
import { testServices } from "./test-services";
import type { AppState, AudioDevice, DesktopApi, Meeting } from "./types";
// Full-App cases: role queries over the whole page are slow on the macos-15-intel runner.
vi.setConfig({ testTimeout: 15_000 });
configure({ asyncUtilTimeout: 5000 });

// Synthetic IPC fixtures only. These tests never open an audio device or a model server.
const output: AudioDevice = {
  id: "out",
  name: "Synthetic speakers",
  source: "system",
  isDefault: true,
};
const builtIn: AudioDevice = {
  id: "mic",
  name: "Synthetic built-in microphone",
  source: "microphone",
  isDefault: true,
};
const headset: AudioDevice = {
  id: "headset",
  name: "Synthetic headset",
  source: "microphone",
};
const alternate: AudioDevice = {
  id: "alternate",
  name: "Synthetic alternate microphone",
  source: "microphone",
};
const live: Meeting = {
  id: "synthetic-live",
  title: "Synthetic live meeting",
  startedAt: "2026-10-29T09:00:00+01:00",
  durationMs: 1000,
  status: "recording",
  modelId: "installed-stream",
  warnings: [],
  parts: [],
};

/** Synthetic ready catalogue; `recording` adds a running capture with a microphone track. */
function state(recording = false, hasMicrophone = true): AppState {
  return {
    version: "1.0.7",
    recording,
    activeMeeting: recording ? live : null,
    captureControl: recording
      ? { paused: false, microphoneMuted: false, hasMicrophone }
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
  };
}

/** Mocked services whose device list and capture state the test changes. */
function fakeApi(devices: () => AudioDevice[], app: () => AppState) {
  return {
    ...testServices(),
    getAppState: vi.fn(async () => app()),
    refreshHandy: vi.fn().mockResolvedValue(state().handy),
    listDevices: vi.fn(async () => devices()),
    startMeeting: vi.fn().mockRejectedValue("Synthetic capture unavailable"),
    stopMeeting: vi.fn(),
    listMeetings: vi.fn().mockResolvedValue([]),
    getMeeting: vi.fn().mockResolvedValue(live),
    resumeMeeting: vi.fn(),
    importLegacy: vi.fn(),
    generateNotes: vi.fn(),
    listAnalyses: vi.fn().mockResolvedValue({ analyses: [], skipped: [] }),
    listTextModels: vi.fn().mockResolvedValue([]),
    getNotesProgress: vi.fn().mockResolvedValue(null),
    cancelNotes: vi.fn().mockResolvedValue(false),
    notesMarkdown: vi.fn(),
    copyText: vi.fn().mockResolvedValue(undefined),
  } as DesktopApi;
}

const microphoneSelect = () =>
  screen.getByLabelText("Mikrofon", {
    selector: "select",
  }) as HTMLSelectElement;

beforeEach(() => localStorage.clear());

describe("audio devices follow connected and removed hardware (synthetic IPC)", () => {
  it("lists a newly connected headset without restarting the app", async () => {
    let devices = [output, builtIn];
    const api = fakeApi(
      () => devices,
      () => state(),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: builtIn.name });
    expect(screen.queryByRole("option", { name: headset.name })).toBeNull();
    devices = [output, builtIn, headset];
    // Returning to the window refreshes the list; the timer does the same in the background.
    fireEvent.focus(window);
    expect(
      await screen.findByRole("option", { name: headset.name }),
    ).toBeTruthy();
    devices = [output, builtIn];
    await userEvent.click(
      screen.getByRole("button", { name: "Audiogeräte aktualisieren" }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("option", { name: headset.name })).toBeNull(),
    );
  });

  it("takes a removed device back after its notice was closed, unless another one was chosen", async () => {
    let devices = [output, builtIn, alternate, headset];
    const api = fakeApi(
      () => devices,
      () => state(),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    const unplug = async () => {
      devices = [output, builtIn, alternate];
      fireEvent.focus(window);
      await screen.findByText(/„Synthetic headset“ ist nicht mehr verfügbar/);
    };
    const plugIn = async () => {
      devices = [output, builtIn, alternate, headset];
      fireEvent.focus(window);
      await screen.findByRole("option", { name: headset.name });
    };
    // Closing the notice keeps the record, so the reconnected headset is chosen again.
    await unplug();
    await userEvent.click(
      screen.getByRole("button", { name: "Hinweis schließen" }),
    );
    expect(screen.queryByText(/ist nicht mehr verfügbar/)).toBeNull();
    await plugIn();
    await waitFor(() => expect(microphoneSelect().value).toBe(headset.id));
    // A microphone chosen while the headset was away stays chosen.
    await unplug();
    await userEvent.selectOptions(microphoneSelect(), alternate.id);
    await plugIn();
    await waitFor(() =>
      expect(screen.queryByText(/ist nicht mehr verfügbar/)).toBeNull(),
    );
    expect(microphoneSelect().value).toBe(alternate.id);
  });

  it("takes the only microphone back after another one was picked automatically", async () => {
    let devices = [output, headset];
    const api = fakeApi(
      () => devices,
      () => state(),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    devices = [output];
    fireEvent.focus(window);
    await screen.findByText(/kein anderes Gerät dieser Art/);
    // A new microphone is selected automatically, not by the user, and the notice says so ...
    devices = [output, builtIn];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(builtIn.id));
    expect(
      await screen.findByText(
        /„Synthetic headset“ ist nicht mehr verfügbar. Ausgewählt ist jetzt „Synthetic built-in microphone“/,
      ),
    ).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", { name: "Hinweis schließen" }),
    );
    // ... so the headset takes over again when it returns.
    devices = [output, builtIn, headset];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(headset.id));
  });

  it("keeps the first lost microphone across successive automatic fallbacks", async () => {
    let devices = [output, builtIn, alternate, headset];
    const api = fakeApi(
      () => devices,
      () => state(),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    // The headset goes, the default stands in; then the stand-in goes as well.
    devices = [output, builtIn, alternate];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(builtIn.id));
    devices = [output, alternate];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(alternate.id));
    expect(
      await screen.findByText(
        /„Synthetic headset“ ist nicht mehr verfügbar. Ausgewählt ist jetzt „Synthetic alternate microphone“/,
      ),
    ).toBeTruthy();
    devices = [output, alternate, headset];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(headset.id));
  });

  it("keeps a microphone the user picked even when it was the automatic choice", async () => {
    let devices = [output, builtIn, alternate, headset];
    const api = fakeApi(
      () => devices,
      () => state(),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    devices = [output, builtIn, alternate];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(builtIn.id));
    await screen.findByText(
      /„Synthetic headset“ ist nicht mehr verfügbar. Ausgewählt ist jetzt „Synthetic built-in microphone“/,
    );
    // The user tries another microphone, which the notice then names, and then picks the
    // automatic one again.
    await userEvent.selectOptions(microphoneSelect(), alternate.id);
    await screen.findByText(
      /Ausgewählt ist jetzt „Synthetic alternate microphone“/,
    );
    fireEvent.focus(window);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(
      screen.getByText(/Ausgewählt ist jetzt „Synthetic alternate microphone“/),
    ).toBeTruthy();
    await userEvent.selectOptions(microphoneSelect(), builtIn.id);
    devices = [output, builtIn, alternate, headset];
    fireEvent.focus(window);
    await screen.findByRole("option", { name: headset.name });
    await waitFor(() =>
      expect(screen.queryByText(/ist nicht mehr verfügbar/)).toBeNull(),
    );
    expect(microphoneSelect().value).toBe(builtIn.id);
  });

  it("reports a removed microphone, chooses another one and takes it back on reconnect", async () => {
    let devices = [output, builtIn, headset];
    const api = fakeApi(
      () => devices,
      () => state(),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    devices = [output, builtIn];
    fireEvent.focus(window);
    expect(
      await screen.findByText(
        "Das Mikrofon „Synthetic headset“ ist nicht mehr verfügbar. Ausgewählt ist jetzt „Synthetic built-in microphone“.",
      ),
    ).toBeTruthy();
    expect(microphoneSelect().value).toBe(builtIn.id);
    devices = [output, builtIn, headset];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(headset.id));
    expect(screen.queryByText(/ist nicht mehr verfügbar/)).toBeNull();
  });

  it("warns when the recording microphone disappears and keeps the recording's choice", async () => {
    let devices = [output, builtIn, headset];
    let recording = false;
    let hasMicrophone = true;
    const api = fakeApi(
      () => devices,
      () => state(recording, hasMicrophone),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    recording = true;
    await waitFor(() =>
      expect(document.querySelector(".state.recording")).not.toBeNull(),
    );
    devices = [output, builtIn];
    fireEvent.focus(window);
    // The list alone does not prove that the track stopped; the wording says so.
    const listed = await screen.findByText(
      /Das Mikrofon „Synthetic headset“ ist während der Aufnahme aus der Geräteliste verschwunden/,
    );
    expect(listed.closest("[role=alert]")).toBeTruthy();
    expect(listed.textContent).toContain(
      "bereits gespeicherter Ton bleibt erhalten",
    );
    // A closed notice does not hide the later native confirmation, which keeps the name.
    await userEvent.click(
      screen.getByRole("button", { name: "Hinweis schließen" }),
    );
    hasMicrophone = false;
    const confirmed = await screen.findByText(
      /Das Mikrofon „Synthetic headset“ ist während der Aufnahme verschwunden. Diese Spur nimmt nicht mehr auf/,
    );
    expect(confirmed.textContent).toContain("Der Computerton läuft weiter");
  });

  it("keeps the notice for a microphone lost while the recording was starting", async () => {
    let devices = [output, builtIn, headset];
    let recording = false;
    let started: (meeting: Meeting) => void = () => undefined;
    const api = fakeApi(
      () => devices,
      () => state(recording, false),
    );
    vi.mocked(api.startMeeting).mockImplementationOnce(
      () => new Promise((resolve) => (started = resolve)),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Aufnahme starten",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole("button", { name: "Aufnahme starten" }));
    await waitFor(() =>
      expect(api.startMeeting).toHaveBeenCalledWith(
        expect.objectContaining({ microphoneDeviceId: headset.id }),
      ),
    );
    // The headset leaves the list before the capture reports the recording ...
    devices = [output, builtIn];
    fireEvent.focus(window);
    await screen.findByText(
      /Das Mikrofon „Synthetic headset“ ist nicht mehr verfügbar/,
    );
    // ... which then runs without its microphone track from the first snapshot on.
    recording = true;
    started(live);
    const notice = await screen.findByText(
      /Das Mikrofon „Synthetic headset“ ist während der Aufnahme aus der Geräteliste verschwunden/,
    );
    expect(notice.closest("[role=alert]")).toBeTruthy();
    expect(screen.queryByText(/Ausgewählt ist jetzt/)).toBeNull();
    // After the recording the built-in microphone stands in until the headset is back.
    recording = false;
    await waitFor(() => expect(microphoneSelect().value).toBe(builtIn.id));
    devices = [output, builtIn, headset];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(headset.id));
  });

  it("takes a microphone lost during a recording back after the recording ended", async () => {
    let devices = [output, builtIn, headset];
    let recording = false;
    const api = fakeApi(
      () => devices,
      () => state(recording),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    recording = true;
    // The capture indicator, not the meeting status label, proves the recording state.
    await waitFor(() =>
      expect(document.querySelector(".state.recording")).not.toBeNull(),
    );
    devices = [output, builtIn];
    fireEvent.focus(window);
    await screen.findByText(/aus der Geräteliste verschwunden/);
    // The recording ends; the built-in microphone stands in automatically ...
    recording = false;
    await waitFor(() => expect(microphoneSelect().value).toBe(builtIn.id));
    // ... and the headset takes over again once it is back.
    devices = [output, builtIn, headset];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(headset.id));
    expect(screen.queryByText(/Synthetic headset“ ist/)).toBeNull();
  });

  it("warns when the native microphone track ends during a recording", async () => {
    let hasMicrophone = true;
    let recording = false;
    const api = fakeApi(
      () => [output, builtIn],
      () => state(recording, hasMicrophone),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: builtIn.name });
    recording = true;
    await waitFor(() =>
      expect(document.querySelector(".state.recording")).not.toBeNull(),
    );
    hasMicrophone = false;
    expect(
      await screen.findByText(
        /Das Mikrofon „Synthetic built-in microphone“ ist während der Aufnahme verschwunden/,
        undefined,
        { timeout: 5000 },
      ),
    ).toBeTruthy();
  });

  it("keeps a confirmed microphone stop when the device list notices the loss later", async () => {
    let devices = [output, builtIn, headset];
    let recording = false;
    let hasMicrophone = true;
    const api = fakeApi(
      () => devices,
      () => state(recording, hasMicrophone),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    recording = true;
    await waitFor(() =>
      expect(document.querySelector(".state.recording")).not.toBeNull(),
    );
    // The capture reports the stop while the device is still listed ...
    hasMicrophone = false;
    await screen.findByText(
      /„Synthetic headset“ ist während der Aufnahme verschwunden. Diese Spur nimmt nicht mehr auf/,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Hinweis schließen" }),
    );
    // ... and the later list change neither weakens nor reopens that notice.
    const queried = vi.mocked(api.listDevices).mock.calls.length;
    devices = [output, builtIn];
    fireEvent.focus(window);
    await waitFor(() =>
      expect(vi.mocked(api.listDevices).mock.calls.length).toBeGreaterThan(
        queried,
      ),
    );
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByText(/Synthetic headset/)).toBeNull();
  });

  it("takes the only microphone back after a recording, also when none was left meanwhile", async () => {
    let devices = [output, headset];
    let recording = false;
    const api = fakeApi(
      () => devices,
      () => state(recording),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    recording = true;
    await waitFor(() =>
      expect(document.querySelector(".state.recording")).not.toBeNull(),
    );
    devices = [output];
    fireEvent.focus(window);
    await screen.findByText(/aus der Geräteliste verschwunden/);
    // The recording ends while no microphone is connected ...
    recording = false;
    await waitFor(() =>
      expect(document.querySelector(".state.recording")).toBeNull(),
    );
    const calls = vi.mocked(api.listDevices).mock.calls.length;
    fireEvent.focus(window);
    await waitFor(() =>
      expect(vi.mocked(api.listDevices).mock.calls.length).toBeGreaterThan(
        calls,
      ),
    );
    await new Promise((resolve) => setTimeout(resolve, 50));
    // ... then another one stands in automatically until the headset is back.
    devices = [output, builtIn];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(builtIn.id));
    devices = [output, builtIn, headset];
    fireEvent.focus(window);
    await waitFor(() => expect(microphoneSelect().value).toBe(headset.id));
  });

  it("ignores an older device list that arrives after a newer one", async () => {
    let devices = [output, builtIn, headset];
    let release: (items: AudioDevice[]) => void = () => undefined;
    const api = fakeApi(
      () => devices,
      () => state(),
    );
    render(<App api={api} />);
    await screen.findByRole("option", { name: headset.name });
    await userEvent.selectOptions(microphoneSelect(), headset.id);
    // A slow automatic query still sees the headset ...
    vi.mocked(api.listDevices).mockImplementationOnce(
      () => new Promise((resolve) => (release = resolve)),
    );
    fireEvent.focus(window);
    // ... while a newer manual refresh already reports it removed.
    devices = [output, builtIn];
    await userEvent.click(
      screen.getByRole("button", { name: "Audiogeräte aktualisieren" }),
    );
    const notice = await screen.findByText(
      /Das Mikrofon „Synthetic headset“ ist nicht mehr verfügbar/,
    );
    release([output, builtIn, headset]);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(notice.isConnected).toBe(true);
    expect(microphoneSelect().value).toBe(builtIn.id);
    expect(screen.queryByRole("option", { name: headset.name })).toBeNull();
  });

  it("repeats a refresh requested while an earlier query is still running", async () => {
    // The running query may have read the list before the headset was connected.
    const replies: (() => void)[] = [];
    const refresh = vi.fn(
      () => new Promise<void>((resolve) => replies.push(resolve)),
    );
    renderHook(() => useDeviceRefresh(refresh, true));
    act(() => {
      fireEvent.focus(window);
      fireEvent.focus(window);
    });
    expect(refresh).toHaveBeenCalledTimes(1);
    await act(async () => replies[0]());
    expect(refresh).toHaveBeenCalledTimes(2);
    await act(async () => replies[1]());
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("only reports selected devices that were listed before", () => {
    const selected = { system: "out", microphone: "headset" };
    expect(lostSelections([], [output], selected)).toEqual([]);
    expect(
      lostSelections([output, headset], [output, builtIn], selected),
    ).toEqual([{ source: "microphone", id: "headset", name: headset.name }]);
    expect(
      lostSelections([output, headset], [builtIn], {
        system: "out",
        microphone: null,
      }),
    ).toEqual([{ source: "system", id: "out", name: output.name }]);
  });
});
