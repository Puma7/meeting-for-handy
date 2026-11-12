import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import CalendarPreparation, { calendarEvents } from "./CalendarPreparation";

function eventStamp(time: number) {
  return new Date(time)
    .toISOString()
    .replace(/[-:]/g, "")
    .replace(/\.\d+Z$/, "Z");
}

describe("local calendar preparation", () => {
  it("reads one-off events, folded literal text and UTC without accepting cancelled or invalid dates", () => {
    const makeEvent = (start: string, fields: string) =>
      `BEGIN:VEVENT\r\nDTSTART:${start}\r\n${fields}\r\nEND:VEVENT\r\n`;
    const calendar =
      "BEGIN:VCALENDAR\r\n" +
      makeEvent(
        "20261110T100000Z",
        "SUMMARY:Projekt\\, Alpha\r\nDESCRIPTION:Erstes Thema\\nZweites \r\n Thema",
      ) +
      makeEvent("20260231T100000Z", "SUMMARY:Invalid") +
      makeEvent("20261110T110000Z", "SUMMARY:Cancelled\r\nSTATUS:CANCELLED") +
      makeEvent("20261110T120000Z", "SUMMARY:Recurring\r\nRRULE:FREQ=DAILY") +
      "END:VCALENDAR";
    expect(calendarEvents(calendar)).toEqual([
      {
        title: "Projekt, Alpha",
        agenda: "Erstes Thema\nZweites Thema",
        startsAt: Date.parse("2026-11-10T10:00:00Z"),
      },
    ]);
  });

  it("applies preparation only after the user chooses it, leaving the agenda local until copied", async () => {
    const stamp = eventStamp(Date.now() + 60_000);
    const file = new File([], "event.ics", { type: "text/calendar" });
    Object.defineProperty(file, "text", {
      value: vi
        .fn()
        .mockResolvedValue(
          `BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:${stamp}\nSUMMARY:Projektplanung\nDESCRIPTION:<b>Agenda</b>\nEND:VEVENT\nEND:VCALENDAR`,
        ),
    });
    const onTitle = vi.fn(),
      onTemplate = vi.fn(),
      onCopyAgenda = vi.fn().mockResolvedValue(undefined);
    render(
      <CalendarPreparation
        locale="de"
        onTitle={onTitle}
        onTemplate={onTemplate}
        onCopyAgenda={onCopyAgenda}
        templates={[{ id: "project", name: "Projekt" }]}
      />,
    );
    await userEvent.click(
      screen.getByText("Meeting aus Kalenderdatei vorbereiten"),
    );
    fireEvent.change(screen.getByLabelText("Kalenderdatei (.ics)"), {
      target: { files: [file] },
    });
    await screen.findByText("Termin in der Nähe: Projektplanung");
    expect(screen.getByText("<b>Agenda</b>").tagName).toBe("PRE");
    expect(onTitle).not.toHaveBeenCalled();
    expect(onCopyAgenda).not.toHaveBeenCalled();
    await userEvent.selectOptions(
      screen.getByLabelText("Auswertungsvorlage"),
      "project",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Titel und Vorlage übernehmen" }),
    );
    expect(onTitle).toHaveBeenCalledWith("Projektplanung");
    expect(onTemplate).toHaveBeenCalledWith("project");
    await userEvent.click(
      screen.getByRole("button", { name: "Agenda kopieren" }),
    );
    expect(onCopyAgenda).toHaveBeenCalledWith("<b>Agenda</b>");
  });

  it("offers upcoming events in time order even after 500 past events in the file", async () => {
    const now = Date.now();
    const event = (time: number, title: string) =>
      `BEGIN:VEVENT\nDTSTART:${eventStamp(time)}\nSUMMARY:${title}\nEND:VEVENT`;
    const calendar = [
      "BEGIN:VCALENDAR",
      ...Array.from({ length: 500 }, (_, index) =>
        event(now - 86_400_000 - index * 60_000, `Past ${index}`),
      ),
      event(now + 120_000, "Later appointment"),
      event(now + 60_000, "Next appointment"),
      "END:VCALENDAR",
    ].join("\n");
    const file = new File([calendar], "history.ics", { type: "text/calendar" });
    Object.defineProperty(file, "text", {
      value: vi.fn().mockResolvedValue(calendar),
    });
    render(
      <CalendarPreparation
        locale="en"
        onTitle={vi.fn()}
        onTemplate={vi.fn()}
        onCopyAgenda={vi.fn()}
        templates={[]}
      />,
    );
    await userEvent.click(
      screen.getByText("Prepare a meeting from a calendar file"),
    );
    fireEvent.change(screen.getByLabelText("Calendar file (.ics)"), {
      target: { files: [file] },
    });
    const choices = within(await screen.findByLabelText("Calendar event"));
    expect(
      choices.getAllByRole("option").map((option) => option.textContent),
    ).toEqual([
      expect.stringContaining("Next appointment"),
      expect.stringContaining("Later appointment"),
    ]);
  });

  it("resolves a known time zone and rejects an ambiguous clock change", () => {
    const calendar = [
      "BEGIN:VCALENDAR",
      "BEGIN:VEVENT",
      "DTSTART;TZID=Europe/Berlin:20261110T100000",
      "SUMMARY:Berlin appointment",
      "END:VEVENT",
      "BEGIN:VEVENT",
      "DTSTART;TZID=Europe/Berlin:20261025T023000",
      "SUMMARY:Ambiguous",
      "END:VEVENT",
      "END:VCALENDAR",
    ].join("\n");
    expect(calendarEvents(calendar)).toEqual([
      {
        title: "Berlin appointment",
        agenda: "",
        startsAt: Date.parse("2026-11-10T09:00:00Z"),
      },
    ]);
  });

  it("rejects floating local times that are missing or duplicated during clock changes", () => {
    const calendar = [
      "BEGIN:VCALENDAR",
      "BEGIN:VEVENT",
      "DTSTART:20260329T023000",
      "SUMMARY:Missing hour",
      "END:VEVENT",
      "BEGIN:VEVENT",
      "DTSTART:20261025T023000",
      "SUMMARY:Duplicated hour",
      "END:VEVENT",
      "BEGIN:VEVENT",
      "DTSTART:20261025T033000",
      "SUMMARY:Unambiguous local time",
      "END:VEVENT",
      "END:VCALENDAR",
    ].join("\n");
    expect(calendarEvents(calendar, "Europe/Berlin")).toEqual([
      {
        title: "Unambiguous local time",
        agenda: "",
        startsAt: Date.parse("2026-10-25T02:30:00Z"),
      },
    ]);
  });
});
