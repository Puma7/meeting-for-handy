import { useEffect, useState } from "react";
import type { Locale } from "../types";

const MAX_CALENDAR_BYTES = 512 * 1024;
const HINT_MS = 15 * 60 * 1000;

type CalendarEvent = {
  title: string;
  agenda: string;
  startsAt: number;
};

function calendarText(value: string, limit: number) {
  return value
    .replace(/\\([nN,;\\])/g, (_, escaped: string) =>
      escaped.toLowerCase() === "n" ? "\n" : escaped,
    )
    .replace(/[\u0000-\u0008\u000b-\u001f\u007f]/g, "")
    .trim()
    .slice(0, limit);
}

function namedZoneTime(iso: string, zone: string): number | undefined {
  try {
    const formatter = new Intl.DateTimeFormat("en-CA", {
      timeZone: zone,
      hourCycle: "h23",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
    const localAt = (instant: number) => {
      const fields = Object.fromEntries(
        formatter.formatToParts(instant).map((part) => [part.type, part.value]),
      );
      return `${fields.year.padStart(4, "0")}-${fields.month}-${fields.day}T${fields.hour}:${fields.minute}:${fields.second}`;
    };
    const target = Date.parse(iso + "Z");
    const candidates = new Set<number>();
    for (const probe of [target - 86_400_000, target, target + 86_400_000]) {
      const offset = Date.parse(localAt(probe) + "Z") - probe;
      const candidate = target - offset;
      if (localAt(candidate) === iso) candidates.add(candidate);
    }
    return candidates.size === 1 ? candidates.values().next().value : undefined;
  } catch {
    return undefined;
  }
}

/** Accept dated, one-off events and reject unknown or ambiguous zone times. */
export function calendarEvents(
  contents: string,
  localZone = Intl.DateTimeFormat().resolvedOptions().timeZone,
): CalendarEvent[] {
  if (contents.length > MAX_CALENDAR_BYTES) return [];
  const lines = contents.replace(/\r?\n[ \t]/g, "").split(/\r?\n/);
  if (lines[0]?.replace(/^\uFEFF/, "").toUpperCase() !== "BEGIN:VCALENDAR")
    return [];
  const result: CalendarEvent[] = [];
  let event: Partial<CalendarEvent> | null = null;
  let unsupported = false;
  let alarmDepth = 0;
  for (const line of lines) {
    const colon = line.indexOf(":");
    if (colon < 0) continue;
    const [property, ...params] = line.slice(0, colon).split(";");
    const name = property.toUpperCase();
    const value = line.slice(colon + 1);
    const marker = value.toUpperCase();
    if (!event && name === "METHOD" && marker === "CANCEL") return [];
    if (name === "BEGIN" && marker === "VEVENT") {
      event = {};
      alarmDepth = 0;
      unsupported = false;
    } else if (event && name === "END" && marker === "VEVENT") {
      if (
        !unsupported &&
        !alarmDepth &&
        event.title &&
        event.startsAt !== undefined
      ) {
        result.push({
          title: event.title,
          agenda: event.agenda ?? "",
          startsAt: event.startsAt,
        });
      }
      event = null;
    } else if (event && name === "BEGIN") {
      alarmDepth++;
    } else if (event && name === "END" && alarmDepth) {
      alarmDepth--;
    } else if (event && !alarmDepth) {
      if (name === "SUMMARY")
        event.title = calendarText(value, 200).replace(/\s+/g, " ");
      if (name === "DESCRIPTION") event.agenda = calendarText(value, 4000);
      if (
        ["RRULE", "RDATE", "RECURRENCE-ID"].includes(name) ||
        (name === "STATUS" && marker === "CANCELLED")
      ) {
        unsupported = true;
      }
      if (name === "DTSTART") {
        const match = /^(\d{4})(\d\d)(\d\d)T(\d\d)(\d\d)(\d\d)(Z?)$/.exec(
          value,
        );
        const zones = params.filter((param) => /^TZID=/i.test(param));
        if (!match || zones.length > 1 || (zones.length && match[7])) {
          unsupported = true;
          continue;
        }
        const [, y, m, d, h, min, sec, zone] = match;
        const iso = `${y}-${m}-${d}T${h}:${min}:${sec}`;
        const calendar = new Date(iso + "Z");
        const named = zones[0]?.slice(5).replace(/^"(.*)"$/, "$1");
        if (
          !Number.isFinite(calendar.getTime()) ||
          !calendar.toISOString().startsWith(iso) ||
          (zones.length && !named)
        ) {
          unsupported = true;
          continue;
        }
        const resolved = zone
          ? calendar.getTime()
          : namedZoneTime(iso, named ?? localZone);
        if (resolved === undefined) {
          unsupported = true;
          continue;
        }
        event.startsAt = resolved;
      }
    }
  }
  return result.sort((a, b) => a.startsAt - b.startsAt);
}

export default function CalendarPreparation({
  locale,
  onTitle,
  onCopyAgenda,
  templates,
  onTemplate,
  disabled,
}: {
  locale: Locale;
  onTitle: (title: string) => void;
  onCopyAgenda: (agenda: string) => Promise<void>;
  templates: { id: string; name: string }[];
  onTemplate: (id: string) => void;
  disabled?: boolean;
}) {
  const de = locale === "de";
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [chosen, setChosen] = useState("");
  const [now, setNow] = useState(Date.now);
  const [message, setMessage] = useState("");
  const [reading, setReading] = useState(false);
  const [template, setTemplate] = useState("");
  const event = chosen === "" ? undefined : events[Number(chosen)];
  useEffect(() => {
    if (!events.length) return;
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, [events.length]);
  const nearby = events.find(
    (item) => Math.abs(item.startsAt - now) <= HINT_MS,
  );
  return (
    <details className="full history-questions">
      <summary>
        {de
          ? "Meeting aus Kalenderdatei vorbereiten"
          : "Prepare a meeting from a calendar file"}
        {nearby && (de ? " · Termin in der Nähe" : " · Meeting around now")}
      </summary>
      <p>
        {de
          ? "Lies eine lokale .ics-Datei ein. Du entscheidest selbst, wann die Aufnahme startet."
          : "Read a local .ics file. You choose when recording starts."}
      </p>
      <label>
        {de ? "Kalenderdatei (.ics)" : "Calendar file (.ics)"}
        <input
          type="file"
          accept=".ics,text/calendar"
          disabled={reading || disabled}
          onChange={async (change) => {
            const input = change.currentTarget;
            const file = input.files?.[0];
            input.value = "";
            if (!file) return;
            setEvents([]);
            setChosen("");
            setMessage("");
            setReading(true);
            try {
              if (file.size > MAX_CALENDAR_BYTES) throw new Error();
              const time = Date.now();
              const results = calendarEvents(await file.text())
                .filter((item) => item.startsAt >= time - HINT_MS)
                .slice(0, 30);
              setNow(time);
              setEvents(results);
              if (results.length) setChosen("0");
              else
                setMessage(
                  de
                    ? "Keine unterstützten bevorstehenden Einzeltermine. Serien, ganztägige Termine und unklare Zeitzonen werden ausgelassen."
                    : "No supported upcoming single events. Recurring events, all-day events and ambiguous time zones are skipped.",
                );
            } catch {
              setMessage(
                de
                  ? "Die Kalenderdatei ist nicht lesbar oder zu groß."
                  : "The calendar file is unreadable or too large.",
              );
            } finally {
              setReading(false);
            }
          }}
        />
      </label>
      <p role="status">
        {message ||
          (nearby &&
            (de
              ? `Termin in der Nähe: ${nearby.title}`
              : `Meeting around now: ${nearby.title}`))}
      </p>
      {event && (
        <>
          <label>
            {de ? "Kalendertermin" : "Calendar event"}
            <select
              value={chosen}
              onChange={(change) => {
                setChosen(change.target.value);
                setMessage("");
              }}
            >
              {events.map((item, index) => (
                <option key={index} value={index}>
                  {new Date(item.startsAt).toLocaleString(locale)} ·{" "}
                  {item.title}
                </option>
              ))}
            </select>
          </label>
          {event.agenda && (
            <pre className="calendar-agenda">{event.agenda}</pre>
          )}
          <label>
            {de ? "Auswertungsvorlage" : "Analysis template"}
            <select
              value={template}
              onChange={(change) => setTemplate(change.target.value)}
            >
              <option value="">
                {de ? "Aktuelle Vorlage beibehalten" : "Keep current template"}
              </option>
              {templates.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
          </label>
          <div className="session-row-actions">
            <button
              type="button"
              className="secondary"
              disabled={disabled}
              onClick={() => {
                onTitle(event.title);
                if (templates.some(({ id }) => id === template))
                  onTemplate(template);
                setMessage(
                  de
                    ? "Vorbereitung übernommen."
                    : "Meeting preparation applied.",
                );
              }}
            >
              {de ? "Titel und Vorlage übernehmen" : "Apply title and template"}
            </button>
            {event.agenda && (
              <button
                type="button"
                className="secondary"
                onClick={async () => {
                  try {
                    await onCopyAgenda(event.agenda);
                    setMessage(de ? "Agenda kopiert." : "Agenda copied.");
                  } catch {
                    setMessage(
                      de
                        ? "Agenda konnte nicht kopiert werden."
                        : "Could not copy the agenda.",
                    );
                  }
                }}
              >
                {de ? "Agenda kopieren" : "Copy agenda"}
              </button>
            )}
          </div>
        </>
      )}
    </details>
  );
}
