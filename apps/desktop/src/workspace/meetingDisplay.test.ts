import { describe, expect, it } from "vitest";
import {
  eventAdditions,
  formatMeetingDates,
  formatMeetingDue,
} from "./shell/meetingDisplay.mjs";

describe("grounded calendar presentation", () => {
  it("keeps the source clock context when the model's due field omits Uhr", () => {
    const quotes = ["Jana liefert bis 9.05 Uhr."];
    expect(formatMeetingDue("9.05", "de", quotes)).toBe("9.05");
    expect(formatMeetingDue("9.05", "en", quotes)).toBe("9.05");
    expect(formatMeetingDue("bis 9.05", "en", quotes)).toBe("bis 9.05");
    expect(formatMeetingDue("9.05", "en", ["Jana liefert am 9.05."])).toBe(
      "05/09",
    );
  });
  it.each([
    "bis 9.05 Uhr",
    "bis 9.5 Uhr",
    "9.05am",
    "at 9.05 pm",
    "9.05 p.m.",
    "um 9.05",
    "at 9.05",
    "9:05 Uhr",
  ])("preserves the explicit clock time %s", (raw) => {
    for (const language of ["de", "en"]) {
      expect(formatMeetingDue(raw, language)).toBe(raw);
      expect(formatMeetingDates(raw, language)).toBe(raw);
    }
  });
  it("formats a date next to a dotted clock time without changing the time", () => {
    expect(formatMeetingDates("Abholen am 9.05. um 9.05 Uhr", "en")).toBe(
      "Abholen am 05/09 um 9.05 Uhr",
    );
    expect(formatMeetingDue("am 9.05.", "de")).toBe("09.05.");
  });
  it.each([
    ["dreiundzwanzigsten neunten", "23.09.", "09/23"],
    ["bis zum siebenundzwanzigsten elften", "27.11.", "11/27"],
    ["sieben und zwanzigsten elften", "27.11.", "11/27"],
    ["4. Dezember", "04.12.", "12/04"],
    ["September 23", "23.09.", "09/23"],
    ["September the twenty-third", "23.09.", "09/23"],
    ["twenty third of September", "23.09.", "09/23"],
    ["23. September 2027", "23.09.2027", "09/23/2027"],
    ["2027-09-23", "23.09.2027", "09/23/2027"],
    ["23/09/2027", "23.09.2027", "09/23/2027"],
    ["09/23/2027", "23.09.2027", "09/23/2027"],
    ["29. Februar", "29.02.", "02/29"],
  ])("formats %s without guessing a year", (raw, de, en) => {
    expect(formatMeetingDue(raw, "de")).toBe(de);
    expect(formatMeetingDue(raw, "en")).toBe(en);
  });
  it.each([
    "vierten und zwölften",
    "4/12",
    "29. Februar 2027",
    "31. November",
    "31/02/2027",
    "Freitag",
    "vielleicht nächste Woche",
  ])("preserves an ambiguous, invalid or relative original: %s", (raw) => {
    expect(formatMeetingDue(raw, "de")).toBe(raw);
    expect(formatMeetingDue(raw, "en")).toBe(raw);
  });
  it("formats generated calendar prose without changing decimal amounts or versions", () => {
    expect(
      formatMeetingDates("Abholen am siebenundzwanzigsten elften.", "en"),
    ).toBe("Abholen am 11/27.");
    expect(
      formatMeetingDates("Abholen am siebenundzwanzigsten elften.", "de"),
    ).toBe("Abholen am 27.11.");
    for (const text of [
      "Preis 27.11 Euro",
      "Version 23.09.2027",
      "Version 2027-09-23",
      "The first second of recording",
      "Am vierten und zwölften",
    ])
      expect(formatMeetingDates(text, "en")).toBe(text);
  });
  it("retains uncertainty and an exclusive deadline when formatting a clear date", () => {
    expect(formatMeetingDue("before September 23", "en")).toBe("before 09/23");
    expect(formatMeetingDue("vor dem 23. September", "de")).toBe(
      "vor dem 23.09.",
    );
    expect(formatMeetingDue("vielleicht am 23. September", "de")).toBe(
      "vielleicht am 23.09.",
    );
  });
});

describe("append-only history presentation", () => {
  it("keeps status changes, human events, independent identities and corrections complete", () => {
    const base = {
      entityId: "e_one",
      revision: 1,
      atMs: 1000,
      text: "Eine ausreichend lange bisherige Aussage bleibt erhalten.",
      human: false,
    };
    const events = [
      { ...base, id: "v_one" },
      {
        ...base,
        id: "v_other",
        entityId: "e_other",
        revision: 2,
        atMs: 2000,
        text: `${base.text} Zusatz.`,
      },
      { ...base, id: "v_status", revision: 2, atMs: 3000 },
      {
        ...base,
        id: "v_human",
        revision: 3,
        atMs: 4000,
        human: true,
        text: `${base.text} Menschliche Ergänzung.`,
      },
      {
        ...base,
        id: "v_correction",
        revision: 4,
        atMs: 5000,
        text: "Eine geänderte Aussage korrigiert den bisherigen Stand.",
      },
    ];
    const before = JSON.stringify(events);
    const result = eventAdditions(events);
    for (const event of events)
      expect(result.get(event.id)).toEqual({
        text: event.text,
        isAddition: false,
      });
    expect(JSON.stringify(events)).toBe(before);
  });
});
