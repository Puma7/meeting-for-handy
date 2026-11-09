import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import CalendarPreparation, { calendarEvents } from "./CalendarPreparation";

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
    const stamp = new Date(Date.now() + 60_000)
      .toISOString()
      .replace(/[-:]/g, "")
      .replace(/\.\d+Z$/, "Z");
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
});
