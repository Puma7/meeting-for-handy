import { expect, it, vi } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { de } from "./i18n";
import SettingsView from "./views/SettingsView";
import type { DesktopApi, RetentionPolicy } from "./types";

// Real SettingsView wiring with synthetic callbacks only; no archive or audio is opened.
function renderChildMaintenance(api: DesktopApi) {
  const noop = () => undefined;
  render(
    <SettingsView
      api={api}
      app={null}
      locale="de"
      theme="system"
      t={de}
      busy={false}
      blocked={false}
      connected
      refreshingHandy={false}
      preferencesPersistable
      handyPath=""
      onChooseHandy={noop}
      onRefreshHandy={noop}
      onRefreshDevices={noop}
      onImportLegacy={noop}
      onRebuildIndex={noop}
      onLocale={noop}
      onTheme={noop}
    />,
  );
  return screen.getByText("Daten und Wartung").closest("details")!;
}
function childPolicy() {
  return {
    revision: 0,
    untouchedDays: 30,
    audioArchiveAfterDays: null,
    archiveAfterDays: null,
    convertAfterDays: null,
    maxVisibleMeetings: null,
    deleteArchivesAfterDays: null,
    codec: "flac",
    ffmpegExecutable: null,
  };
}

it("keeps an interrupted deletion and its typed continuation visible outside closed maintenance", async () => {
  const meeting = "c".repeat(32);
  const resume = vi.fn().mockResolvedValue("synthetic-resumed-job");
  const api = {
    getRetentionPolicy: vi.fn().mockResolvedValue(childPolicy()),
    getMaintenanceProgress: vi.fn().mockResolvedValue(null),
    listArchives: vi.fn().mockResolvedValue({ items: [], more: false }),
    listUnfinishedDeletions: vi.fn().mockResolvedValue([
      {
        logId: "synthetic-log",
        meetingId: meeting,
        deletedFiles: 1,
        totalFiles: 3,
        confirmedAt: "2026-01-01T00:00:00Z",
      },
    ]),
    resumePermanentDeletion: resume,
  } as unknown as DesktopApi;
  const disclosure = renderChildMaintenance(api);
  const warning = await screen.findByText("Unterbrochene endgültige Löschung");
  expect(disclosure.open).toBe(false);
  expect(warning.closest("details")).toBeNull();
  expect(screen.getByText(meeting).closest("details")).toBeNull();
  expect(resume).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole("button", { name: "Fortsetzen prüfen" }),
  );
  const button = screen.getByRole("button", {
    name: "Löschung fortsetzen",
  }) as HTMLButtonElement;
  expect(button.closest("details")).toBeNull();
  expect(button.disabled).toBe(true);
  await userEvent.type(
    screen.getByRole("textbox", {
      name: `Zur Bestätigung „DELETE ${meeting}“ eingeben`,
    }),
    `DELETE ${meeting}`,
  );
  await userEvent.click(button);
  expect(resume).toHaveBeenCalledExactlyOnceWith(
    "synthetic-log",
    `DELETE ${meeting}`,
  );
  expect(disclosure.open).toBe(false);
});

it.each(["restore", "compact"] as const)(
  "keeps a rejected archive %s visible after maintenance closes",
  async (operation) => {
    let reject!: (error: unknown) => void;
    const command = vi.fn().mockImplementation(
      () =>
        new Promise((_, fail) => {
          reject = fail;
        }),
    );
    const entry = {
      archiveId: "a".repeat(32),
      title: "Synthetic archive",
      sessionPresent: false,
      sourceCopyRetained: true,
      receipt: {
        archiveId: "a".repeat(32),
        meetingId: "b".repeat(32),
        kind: "whole-meeting",
        archiveBytes: 1000,
        originalBytes: 2000,
        createdAt: "2026-01-01",
      },
    };
    const api = {
      getRetentionPolicy: vi.fn().mockResolvedValue(childPolicy()),
      getMaintenanceProgress: vi.fn().mockResolvedValue(null),
      listArchives: vi.fn().mockResolvedValue({ items: [entry], more: false }),
      restoreArchive: command,
      compactArchive: command,
    } as unknown as DesktopApi;
    const detail = renderChildMaintenance(api);
    await userEvent.click(detail.querySelector("summary")!);
    await userEvent.click(
      await screen.findByRole("button", {
        name:
          operation === "restore"
            ? "Wiederherstellung prüfen"
            : "Archiv platzsparend aufbewahren",
      }),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name:
          operation === "restore"
            ? "Geprüfte Kopie wiederherstellen"
            : "Geprüftes ZIP behalten, Rohkopie entfernen",
      }),
    );
    expect(command).toHaveBeenCalledOnce();
    await userEvent.click(detail.querySelector("summary")!);
    await act(async () => reject(new Error(`Synthetic ${operation} failed`)));
    const message = await screen.findByText(`Synthetic ${operation} failed`);
    expect(message.closest("details")).toBeNull();
    expect(detail.open).toBe(false);
  },
);

it("keeps an audio-copy catalog error visible outside closed maintenance settings", async () => {
  let reject!: (error: unknown) => void;
  const api = {
    getRetentionPolicy: vi.fn().mockResolvedValue(childPolicy()),
    getMaintenanceProgress: vi.fn().mockResolvedValue(null),
    listAudioCopies: vi.fn().mockImplementation(
      () =>
        new Promise((_, fail) => {
          reject = fail;
        }),
    ),
  } as unknown as DesktopApi;
  const detail = renderChildMaintenance(api);
  await act(async () => reject(new Error("Synthetic audio copy failed")));
  const message = await screen.findByText("Synthetic audio copy failed");
  expect(message.closest("details")).toBeNull();
  expect(detail.open).toBe(false);
});

it("keeps a rejected maintenance save visible after its settings close", async () => {
  const policy: RetentionPolicy = {
    revision: 0,
    untouchedDays: 30,
    audioArchiveAfterDays: null,
    archiveAfterDays: null,
    convertAfterDays: null,
    maxVisibleMeetings: null,
    deleteArchivesAfterDays: null,
    codec: "flac",
    ffmpegExecutable: null,
  };
  let reject!: (e: unknown) => void;
  const api = {
    getRetentionPolicy: vi.fn().mockResolvedValue(policy),
    getMaintenanceProgress: vi.fn().mockResolvedValue(null),
    saveRetentionPolicy: vi.fn().mockImplementation(
      () =>
        new Promise((_, fail) => {
          reject = fail;
        }),
    ),
  } as unknown as DesktopApi;
  const noop = () => undefined;
  render(
    <SettingsView
      api={api}
      app={null}
      locale="de"
      theme="system"
      t={de}
      busy={false}
      blocked={false}
      connected
      refreshingHandy={false}
      preferencesPersistable
      handyPath=""
      onChooseHandy={noop}
      onRefreshHandy={noop}
      onRefreshDevices={noop}
      onImportLegacy={noop}
      onRebuildIndex={noop}
      onLocale={noop}
      onTheme={noop}
    />,
  );
  const detail = screen.getByText("Daten und Wartung").closest("details")!;
  await userEvent.click(detail.querySelector("summary")!);
  const days = await screen.findByLabelText(
    "Meetings zunächst unangetastet lassen · Tage",
  );
  await userEvent.clear(days);
  await userEvent.type(days, "31");
  await userEvent.click(
    await screen.findByRole("button", { name: "Regeln speichern" }),
  );
  expect(api.saveRetentionPolicy).toHaveBeenCalledOnce();
  await userEvent.click(detail.querySelector("summary")!);
  await act(async () => reject(new Error("Synthetic maintenance save failed")));
  const message = await screen.findByText("Synthetic maintenance save failed");
  expect(message.closest("details")).toBeNull();
  expect(detail.open).toBe(false);
});

// Synthetic IPC only: this does not open, archive, remove or restore any file.
it("keeps a running maintenance job and cancellation outside collapsed settings", async () => {
  const policy: RetentionPolicy = {
    revision: 0,
    untouchedDays: 30,
    audioArchiveAfterDays: null,
    archiveAfterDays: null,
    convertAfterDays: null,
    maxVisibleMeetings: null,
    deleteArchivesAfterDays: null,
    codec: "flac",
    ffmpegExecutable: null,
  };
  const cancel = vi.fn().mockResolvedValue(true);
  const api = {
    getRetentionPolicy: vi.fn().mockResolvedValue(policy),
    getMaintenanceProgress: vi.fn().mockResolvedValue({
      jobId: "synthetic-running-archive",
      kind: "archive-restore",
      status: "running",
      confirmedTasks: 1,
      totalTasks: 3,
      elapsedMs: 1200,
      skipped: 0,
      error: null,
      cancelRequested: false,
    }),
    cancelMaintenance: cancel,
  } as unknown as DesktopApi;
  const noop = () => undefined;
  render(
    <SettingsView
      api={api}
      app={null}
      locale="de"
      theme="system"
      t={de}
      busy={false}
      blocked={false}
      connected
      refreshingHandy={false}
      preferencesPersistable
      handyPath=""
      onChooseHandy={noop}
      onRefreshHandy={noop}
      onRefreshDevices={noop}
      onImportLegacy={noop}
      onRebuildIndex={noop}
      onLocale={noop}
      onTheme={noop}
    />,
  );

  const disclosure = screen.getByText("Daten und Wartung").closest("details")!;
  expect(disclosure.open).toBe(false);
  const job = await screen.findByRole("status", {
    name: "Aufbewahrung und Archiv",
  });
  // jsdom does not fully render native details hiding; the DOM ancestry must also
  // prove that neither status nor cancel depends on opening a collapsed ancestor.
  expect(job.closest("details")).toBeNull();
  expect(api.getMaintenanceProgress).toHaveBeenCalledTimes(1);
  await userEvent.click(
    within(job).getByRole("button", {
      name: "Aufbewahrungsauftrag abbrechen",
    }),
  );
  expect(cancel).toHaveBeenCalledExactlyOnceWith("synthetic-running-archive");
  expect(disclosure.open).toBe(false);
});

it("keeps the exact legacy import control visible when settings collapse during an import", async () => {
  const importLegacy = vi.fn();
  const noop = () => undefined;
  const props = {
    api: { chooseLegacyManifest: vi.fn() } as unknown as DesktopApi,
    app: null,
    locale: "de" as const,
    theme: "system" as const,
    t: de,
    busy: false,
    blocked: false,
    connected: true,
    refreshingHandy: false,
    preferencesPersistable: true,
    handyPath: "",
    onChooseHandy: noop,
    onRefreshHandy: noop,
    onRefreshDevices: noop,
    onImportLegacy: importLegacy,
    onRebuildIndex: noop,
    onLocale: noop,
    onTheme: noop,
  };
  const view = render(<SettingsView {...props} />);
  const disclosure = screen.getByText("Daten und Wartung").closest("details")!;
  await userEvent.click(disclosure.querySelector("summary")!);
  const button = screen.getByRole("button", {
    name: "Sitzungsdatei auswählen und nur lesbar öffnen …",
  });
  await userEvent.click(button);
  expect(importLegacy).toHaveBeenCalledOnce();
  view.rerender(<SettingsView {...props} busy />);
  await userEvent.click(disclosure.querySelector("summary")!);
  expect(disclosure.open).toBe(false);
  expect((button as HTMLButtonElement).disabled).toBe(true);
  // Unlike jsdom's partial details rendering, actual DOM ancestry is decisive:
  // closing any disclosure must not hide the only in-progress import control.
  expect(button.closest("details")).toBeNull();
});
