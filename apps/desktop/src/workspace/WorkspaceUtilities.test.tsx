import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { LiveNotesStatus } from "../types";
import AnalysisDiagnostics, {
  analysisDiagnostics,
} from "./AnalysisDiagnostics";
import MeetingQuestions, { meetingAnswers } from "./MeetingQuestions";
import type { WorkspaceState } from "./WorkspaceBridge";
import { workspaceStatusText } from "./shell/liveWorkspace.mjs";

function status(): LiveNotesStatus {
  return {
    meetingId: "PRIVATE-SESSION",
    status: "updating",
    running: true,
    error: "PRIVATE-KEY in C:\\private\\meeting",
    pendingChars: 800,
    intervalMs: 120000,
    nextUpdateInMs: null,
    lastTrigger: null,
    view: null,
    preparingModel: true,
  };
}

it("copies precisely the displayed diagnostic and leaves private fields out", async () => {
  const copy = vi.fn(async () => {});
  const sample = {
    ...status(),
    meetingTitle: "PRIVATE-TITLE",
    quote: "PRIVATE-QUOTE",
  };
  render(
    <AnalysisDiagnostics
      status={sample}
      pendingAudioMs={500}
      locale="de"
      copy={copy}
      readVersion={async () => "1.2.0"}
    />,
  );
  const report = (await screen.findByText(/"version": "1\.2\.0"/)).textContent!;
  expect(copy).not.toHaveBeenCalled();
  expect(report).not.toContain("PRIVATE");
  expect(report).not.toContain("private");
  expect(JSON.parse(report)).toMatchObject({
    phase: "modelPreparation",
    hasError: true,
    pendingAudioMs: 500,
    pendingTextCharacters: 800,
  });
  fireEvent.click(screen.getByRole("button", { name: "Diagnose kopieren" }));
  expect(copy).toHaveBeenCalledExactlyOnceWith(report);
  expect(await screen.findByText("Diagnose kopiert.")).toBeTruthy();
  expect(
    analysisDiagnostics(
      null,
      Infinity,
      "PRIVATE-PATH",
      "Windows private detail",
    ),
  ).toMatchObject({ version: null, pendingAudioMs: null, platform: "windows" });
});

it("distinguishes local preparation, recognition priority and RAM pressure", () => {
  const preparing = status();
  expect(workspaceStatusText(preparing, "en")).toBe(
    "Preparing the local model",
  );
  const waiting = {
    ...preparing,
    status: "waiting" as const,
    preparingModel: false,
    waitingForMemory: true,
  };
  expect(workspaceStatusText(waiting, "de")).toMatch(/freien Arbeitsspeicher/);
  expect(
    workspaceStatusText({ ...waiting, waitingForTranscript: true }, "en"),
  ).toBe("Waiting for transcription capacity");
  expect(
    workspaceStatusText(
      { ...waiting, status: "finished", running: false },
      "en",
    ),
  ).toBe("Analysis saved");
});

function workspace(): WorkspaceState {
  const task = {
    id: "task",
    revision: 2,
    kind: "task",
    status: "completed",
    attention: "normal",
    text: "Anleitung fertiggestellt",
    owner: null,
    due: null,
    humanLocked: false,
    evidence: [{ sourceId: "s_current", quote: "Die Anleitung ist fertig." }],
  };
  return {
    revision: 2,
    updatedAt: null,
    context: { revision: 0, agenda: [], sharedNotes: [], overrides: [] },
    entities: [
      task,
      {
        ...task,
        id: "invented",
        status: "open",
        evidence: [{ sourceId: "s_current", quote: "erfundener Inhalt" }],
      },
      {
        ...task,
        id: "withdrawn-source",
        status: "open",
        evidence: [{ sourceId: "s_old", quote: "Die alte Anleitung fehlt." }],
      },
      { ...task, id: "excluded", status: "open" },
    ],
    originals: [
      {
        id: "s_current",
        source: "microphone",
        startMs: 0,
        endMs: 1000,
        text: "Die Anleitung ist fertig.",
      },
      {
        id: "s_old",
        source: "microphone",
        startMs: 0,
        endMs: 1000,
        text: "Die alte Anleitung fehlt.",
      },
    ],
    events: [],
    overviewRefs: [],
    contextExcludedIds: ["excluded"],
    supersededSources: ["s_old"],
  };
}

it("keeps completed tasks out of the open list and refuses altered or superseded evidence", () => {
  const current = workspace();
  const openSource = vi.fn();
  render(
    <MeetingQuestions
      workspace={current}
      locale="de"
      openSource={openSource}
    />,
  );
  expect(screen.getByRole("status").textContent).toMatch(
    /keinen belegten Treffer/,
  );
  const filter = screen.getByRole("combobox");
  fireEvent.change(filter, { target: { value: "completed" } });
  expect(screen.getByText("Anleitung fertiggestellt")).toBeTruthy();
  fireEvent.click(
    screen.getByRole("button", { name: /Die Anleitung ist fertig/ }),
  );
  expect(openSource).toHaveBeenCalledExactlyOnceWith("s_current");
  expect(meetingAnswers(current, "sources", "alte Anleitung").total).toBe(0);
  expect(meetingAnswers(current, "sources", "Anleitung").total).toBe(1);
});
