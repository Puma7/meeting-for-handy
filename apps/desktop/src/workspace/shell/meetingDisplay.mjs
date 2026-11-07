// Presentation only: native records, original transcript and evidence are never rewritten.
const germanStems = [
  "erst",
  "zweit",
  "dritt",
  "viert",
  "fünft",
  "sechst",
  "siebt",
  "acht",
  "neunt",
  "zehnt",
  "elft",
  "zwölft",
  "dreizehnt",
  "vierzehnt",
  "fünfzehnt",
  "sechzehnt",
  "siebzehnt",
  "achtzehnt",
  "neunzehnt",
  "zwanzigst",
  "einundzwanzigst",
  "zweiundzwanzigst",
  "dreiundzwanzigst",
  "vierundzwanzigst",
  "fünfundzwanzigst",
  "sechsundzwanzigst",
  "siebenundzwanzigst",
  "achtundzwanzigst",
  "neunundzwanzigst",
  "dreißigst",
  "einunddreißigst",
];
const ordinal = new Map();
germanStems.forEach((stem, i) => {
  for (const ending of ["e", "en", "er", "em", "es"])
    ordinal.set(stem + ending, i + 1);
});
const germanOrdinalKeys = [...ordinal.keys()];
const englishOrdinals = [
  "first",
  "second",
  "third",
  "fourth",
  "fifth",
  "sixth",
  "seventh",
  "eighth",
  "ninth",
  "tenth",
  "eleventh",
  "twelfth",
  "thirteenth",
  "fourteenth",
  "fifteenth",
  "sixteenth",
  "seventeenth",
  "eighteenth",
  "nineteenth",
  "twentieth",
  "twenty-first",
  "twenty-second",
  "twenty-third",
  "twenty-fourth",
  "twenty-fifth",
  "twenty-sixth",
  "twenty-seventh",
  "twenty-eighth",
  "twenty-ninth",
  "thirtieth",
  "thirty-first",
];
englishOrdinals.forEach((word, i) => ordinal.set(word, i + 1));
const months = new Map();
[
  ["januar", "january"],
  ["februar", "february"],
  ["märz", "maerz", "march"],
  ["april"],
  ["mai", "may"],
  ["juni", "june"],
  ["juli", "july"],
  ["august"],
  ["september"],
  ["oktober", "october"],
  ["november"],
  ["dezember", "december"],
].forEach((names, i) => names.forEach((name) => months.set(name, i + 1)));
const patternWord = (word) =>
  word.replaceAll("und", "\\s*und\\s*").replaceAll("-", "[-\\s]");
const words = [...ordinal.keys()]
  .sort((a, b) => b.length - a.length)
  .map(patternWord)
  .join("|");
const monthWords = [
  ...months.keys(),
  ...germanOrdinalKeys.filter((word) => ordinal.get(word) <= 12),
]
  .sort((a, b) => b.length - a.length)
  .map(patternWord)
  .join("|");
const namedMonthWords = [...months.keys()].join("|");
const dayWords = `(?:${words}|\\d{1,2}(?:st|nd|rd|th)?)`;
const formatters = new Map();
const nonDatePrefix =
  /\b(?:version|build|preis|betrag|price|amount|cost)\s*[:=]?\s*$/i;
const dayFirst = new RegExp(
  `\\b(${dayWords})(?:\\.\\s*|\\s+(?:(?:of|des)\\s+)?)(${monthWords})(?:\\s*,?\\s+(\\d{4}))?\\b`,
  "giu",
);
const monthFirst = new RegExp(
  `\\b(${namedMonthWords})\\s+(?:the\\s+)?(${dayWords})(?:,?\\s+(\\d{4}))?\\b`,
  "giu",
);

/** Validate calendar components without supplying a missing year to the displayed date. */
function calendarDate(day, month, year, language) {
  if (
    !Number.isInteger(day) ||
    !Number.isInteger(month) ||
    (year != null && (year < 1 || year > 9999))
  )
    return null;
  const checkedYear = year ?? 2000; // Leap-year validation only; absent year stays absent.
  const date = new Date(0);
  date.setUTCFullYear(checkedYear, month - 1, day);
  date.setUTCHours(0, 0, 0, 0);
  if (
    date.getUTCFullYear() !== checkedYear ||
    date.getUTCMonth() !== month - 1 ||
    date.getUTCDate() !== day
  )
    return null;
  const locale = language === "en" ? "en-US" : "de-DE";
  const key = `${locale}:${year != null}`;
  if (!formatters.has(key))
    formatters.set(
      key,
      new Intl.DateTimeFormat(locale, {
        day: "2-digit",
        month: "2-digit",
        ...(year != null ? { year: "numeric" } : {}),
        timeZone: "UTC",
      }),
    );
  return formatters.get(key).format(date);
}

/** Read a literal ordinal or decimal day; never resolve relative dates with the current clock. */
function dayNumber(word) {
  const normalized = word.toLowerCase();
  return (
    ordinal.get(normalized) ??
    ordinal.get(normalized.replace(/\s/g, "")) ??
    ordinal.get(normalized.replace(/\s/g, "-")) ??
    Number(word.replace(/(?:st|nd|rd|th)$/i, ""))
  );
}

/** Recognize explicit dotted clock context before interpreting the same digits as a date. */
function clockContext(text, index, raw) {
  return (
    /^\d{1,2}\.\d{1,2}\.?$/.test(raw) &&
    (/^\s*(?:uhr\b|[ap]\.?m\.?(?=\s|$|[,;.!?]))/i.test(
      text.slice(index + raw.length),
    ) ||
      /\b(?:um|at)\s*$/i.test(text.slice(0, index)))
  );
}

/** Distinguish numeric calendar mentions from amounts and clock times in generated prose. */
function calendarContext(text, index, raw, assumeDate) {
  if (
    nonDatePrefix.test(text.slice(0, index)) ||
    clockContext(text, index, raw)
  )
    return false;
  return (
    assumeDate ||
    text.trim() === raw ||
    /(?:\bam|\bzum|\bbis|\bon|\bby|\bdue|\bdeadline|\btermin|\bdate)\s*:?\s*$/i.test(
      text.slice(0, index),
    )
  );
}

/** Format recognized, valid calendar mentions; ambiguous conjunctions and invalid dates remain literal. */
function dates(value, language, assumeDate) {
  if (typeof value !== "string") return value;
  let text = value;
  text = text.replace(dayFirst, (raw, day, month, year, index) =>
    months.has(month.toLowerCase()) ||
    calendarContext(text, index, raw, assumeDate)
      ? (calendarDate(
          dayNumber(day),
          months.get(month.toLowerCase()) ?? dayNumber(month),
          year ? Number(year) : null,
          language,
        ) ?? raw)
      : raw,
  );
  text = text.replace(
    monthFirst,
    (raw, month, day, year) =>
      calendarDate(
        dayNumber(day),
        months.get(month.toLowerCase()),
        year ? Number(year) : null,
        language,
      ) ?? raw,
  );
  text = text.replace(
    /\b(\d{4})-(\d{2})-(\d{2})\b/g,
    (raw, year, month, day, index) =>
      nonDatePrefix.test(text.slice(0, index))
        ? raw
        : (calendarDate(Number(day), Number(month), Number(year), language) ??
          raw),
  );
  // A dot is the explicit German convention; a slash is the explicit US convention.
  text = text.replace(
    /\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b/g,
    (raw, day, month, year, index) =>
      calendarContext(text, index, raw, assumeDate)
        ? (calendarDate(Number(day), Number(month), Number(year), language) ??
          raw)
        : raw,
  );
  text = text.replace(
    /\b(\d{1,2})\/(\d{1,2})(?:\/(\d{4}))?\b/g,
    (raw, first, second, year, index) => {
      if (!calendarContext(text, index, raw, assumeDate)) return raw;
      if (Number(first) <= 12 && Number(second) <= 12) return raw; // Locale of an ambiguous original numeric form is unknown.
      const [day, month] =
        Number(first) > 12 ? [first, second] : [second, first];
      return (
        calendarDate(
          Number(day),
          Number(month),
          year ? Number(year) : null,
          language,
        ) ?? raw
      );
    },
  );
  text = text.replace(
    /\b(\d{1,2})\.(\d{1,2})(\.)?(?![\d.])/g,
    (raw, day, month, _dot, index) =>
      calendarContext(text, index, raw, assumeDate)
        ? (calendarDate(Number(day), Number(month), null, language) ?? raw)
        : raw,
  );
  return text === value ? value : text.replace(/(\d{2})\.\.(?=\s|$)/g, "$1.");
}

/** Format generated prose only. Callers must keep original transcript and quotations untouched. */
export function formatMeetingDates(value, language = "de") {
  return dates(value, language, false);
}

/** Format the separately grounded due field, retaining uncertainty and relative wording. */
export function formatMeetingDue(value, language = "de", quotes = []) {
  // A model may return only the numeric substring of a literal "9.05 Uhr".
  // Keep the clock context supplied by its evidence even when due omits that suffix.
  const numeric =
    typeof value === "string"
      ? value.match(
          /^\s*(?:(?:bis|by|at|um|before)\s+)?(\d{1,2}\.\d{1,2}\.?)\s*$/i,
        )?.[1]
      : null;
  if (
    numeric &&
    quotes.some((quote) => {
      let index = quote.indexOf(numeric);
      while (index >= 0) {
        if (
          !/\d/.test(quote[index - 1] || "") &&
          clockContext(quote, index, numeric)
        )
          return true;
        index = quote.indexOf(numeric, index + 1);
      }
      return false;
    })
  )
    return value;
  const result = dates(value, language, true);
  if (result === value || typeof result !== "string") return result;
  return result.replace(/^(?:(?:am|bis|zum|den|dem|on|by|at|the|of)\s+)+/i, "");
}

/** Derive append-only event additions across the full history, before search or sort filters. */
export function eventAdditions(events) {
  const previous = new Map();
  const output = new Map();
  const chronological = [...(events || [])].sort(
    (a, b) => a.atMs - b.atMs || a.revision - b.revision,
  );
  for (const event of chronological) {
    const text = (event.text || "").trim().replace(/\s+/g, " ");
    const prior = previous.get(event.entityId);
    const old = (prior?.text || "").trim().replace(/\s+/g, " ");
    const append =
      !event.human &&
      event.entityId &&
      prior &&
      event.revision > prior.revision &&
      old.length >= 24 &&
      old.split(" ").length >= 4 &&
      text.startsWith(old) &&
      /^\s/.test(text.slice(old.length));
    const addition = append ? text.slice(old.length).trim() : "";
    output.set(event.id, {
      text: addition || text,
      isAddition: Boolean(addition),
    });
    if (event.entityId) previous.set(event.entityId, event);
  }
  return output;
}
