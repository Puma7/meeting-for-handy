import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import SavedWorkspaceHistory from "./SavedWorkspaceHistory";
import type { SavedWorkspace, WorkspaceHistoryListing } from "../types";

/** Unlocked historical output, its literal evidence and event are independent of the current run. */
function snapshot(id: string, text: string): SavedWorkspace {
  return {
    runId: id,
    promptVersion: "handymeeting-workspace-10",
    model: "synthetic",
    createdAt: "2026-11-01T10:00:00Z",
    updatedAt: null,
    entityCount: 1,
    workspace: {
      revision: 1,
      updatedAt: null,
      context: { revision: 0, agenda: [], sharedNotes: [], overrides: [] },
      overviewRefs: [],
      entities: [
        {
          id: "e_old",
          revision: 1,
          kind: "fact",
          text,
          status: "recorded",
          attention: "normal",
          owner: null,
          due: null,
          humanLocked: false,
          evidence: [{ sourceId: "s_old", quote: "Literal saved source." }],
        },
      ],
      events: [
        {
          id: "v_old",
          entityId: "e_old",
          revision: 1,
          atMs: 30000,
          text: `${text} history`,
          status: "recorded",
          evidence: [],
        },
      ],
      originals: [
        {
          id: "s_old",
          source: "system",
          startMs: 0,
          endMs: 30000,
          text: "Literal saved source.",
        },
      ],
    },
  };
}
/** Keep list metadata separate from the selected result payload. */
function listing(...values: SavedWorkspace[]): WorkspaceHistoryListing {
  return {
    entries: values.map(({ workspace: _workspace, ...entry }) => entry),
    unreadableEntries: 0,
  };
}

it("ignores a late older response after another saved view was selected", async () => {
  const a = snapshot("a", "Earlier A"),
    b = snapshot("b", "Earlier B");
  let release!: (v: SavedWorkspace) => void;
  const pending = new Promise<SavedWorkspace>((resolve) => {
    release = resolve;
  });
  const list = vi.fn(async () => listing(a, b));
  const load = vi.fn((_meeting: string, id: string) =>
    id === "a" ? pending : Promise.resolve(b),
  );
  const mounted = render(
    <SavedWorkspaceHistory
      meetingId="meeting"
      list={list}
      load={load}
      language="en"
    />,
  );
  try {
    const choose = await screen.findByRole("combobox", { name: "Saved view" });
    await vi.waitFor(() =>
      expect(load).toHaveBeenCalledWith("meeting", "a", a.promptVersion),
    );
    fireEvent.change(choose, { target: { value: `b/${b.promptVersion}` } });
    await screen.findByText("Earlier B");
    await act(async () => release(a));
    expect(screen.queryByText("Earlier A")).toBeNull();
    expect(screen.getByText("Earlier B history")).toBeTruthy();
  } finally {
    mounted.unmount();
  }
});

it("does not display a payload belonging to another saved run", async () => {
  const wanted = snapshot("a", "Wanted"),
    wrong = snapshot("other", "Wrong run");
  const mounted = render(
    <SavedWorkspaceHistory
      meetingId="meeting"
      list={async () => listing(wanted)}
      load={async () => wrong}
      language="en"
    />,
  );
  try {
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "Error: The saved view does not match the selection.",
    );
    expect(screen.queryByText("Wrong run")).toBeNull();
  } finally {
    mounted.unmount();
  }
});

it("reports unreadable retained versions and native read failures without inventing an empty success", async () => {
  const saved = snapshot("a", "Unread");
  const values = listing(saved);
  values.unreadableEntries = 1;
  const mounted = render(
    <SavedWorkspaceHistory
      meetingId="meeting"
      list={async () => values}
      load={async () => {
        throw new Error("Synthetic read failure");
      }}
      language="en"
    />,
  );
  try {
    await screen.findByText("Error: Synthetic read failure");
    expect(
      screen.getByText(
        "Some saved views could not be read. Their files are retained.",
      ),
    ).toBeTruthy();
    expect(screen.queryByText("Unread")).toBeNull();
  } finally {
    mounted.unmount();
  }
});

it("keeps all older items reachable across bounded pages and leaves the saved payload unchanged", async () => {
  const saved = snapshot("a", "Saved");
  saved.workspace.entities = Array.from({ length: 52 }, (_, i) => ({
    ...saved.workspace.entities[0],
    id: `e_${i}`,
    text: `Saved item ${i}`,
  }));
  saved.entityCount = 52;
  const before = JSON.stringify(saved);
  const mounted = render(
    <SavedWorkspaceHistory
      meetingId="meeting"
      list={async () => listing(saved)}
      load={async () => saved}
      language="en"
    />,
  );
  try {
    await screen.findByText("Saved item 0");
    const section = screen.getByRole("region", { name: "Saved items" });
    for (
      let step = 0;
      step < 10 && !within(section).queryByText("Saved item 51");
      step++
    ) {
      fireEvent.click(within(section).getByRole("button", { name: "Next" }));
    }
    expect(within(section).getByText("Saved item 51")).toBeTruthy();
    expect(JSON.stringify(saved)).toBe(before);
    expect(
      within(section).queryByRole("button", { name: /Save|Complete|Reopen/ }),
    ).toBeNull();
  } finally {
    mounted.unmount();
  }
});
