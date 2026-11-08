import { useEffect, useState } from "react";
import { errorMessage } from "../api";
import { clock } from "../domain";
import { nameCandidates } from "../speaker-proposals";
import type {
  DesktopApi,
  Meeting,
  Locale,
  SpeakerDocument,
  SpeakerAssignment,
} from "../types";
/**
 * Hydrates saved speaker names even while editing controls are closed, publishing only names
 * whose original source hashes still match. Text revisions invalidate stale assignments.
 */
export default function SpeakerLabels({
  api,
  meeting,
  locale,
  onPropose,
  proposeDisabled = false,
  onLabels,
  embedded = false,
}: {
  api: DesktopApi;
  meeting: Meeting;
  locale: Locale;
  onPropose?: () => void;
  /** True while an analysis runs; its mode, model and template must not change. */
  proposeDisabled?: boolean;
  /** Enabled assignments whose original text is unchanged, by source ID; empty when off. */
  onLabels?: (meetingId: string, labels: Record<string, string>) => void;
  embedded?: boolean;
}) {
  const de = locale === "de";
  const [doc, setDoc] = useState<
      (SpeakerDocument & { sources: Record<string, string> }) | null
    >(null),
    [error, setError] = useState<string | null>(null),
    [busy, setBusy] = useState(false);
  const [source, setSource] = useState(""),
    [name, setName] = useState(""),
    [quote, setQuote] = useState("");
  const [candidates, setCandidates] = useState<
    ReturnType<typeof nameCandidates>
  >([]);
  useEffect(() => {
    let active = true;
    setCandidates([]);
    api
      .listAnalyses?.(meeting.id)
      .then((value) => {
        if (active) setCandidates(nameCandidates(value.analyses, meeting));
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [api, meeting.id]);
  useEffect(() => {
    setDoc(null);
    setError(null);
    setSource("");
    setName("");
    setQuote("");
  }, [meeting.id]);
  // Source hashes come from the backend; reload them whenever recognition revises a section,
  // so a label confirmed for earlier text is shown as stale instead of confirmed.
  // A full-text digest catches every revision, including same-length corrections.
  const textSignature = meeting.parts
    .map((p) => `${p.file ?? ""}:${p.status}:${textDigest(p.text ?? "")}`)
    .join("\u0000");
  useEffect(() => {
    let active = true;
    setDoc(null);
    api
      .getSpeakerLabels?.(meeting.id)
      .then((v) => {
        if (active) setDoc(v);
      })
      .catch((e) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [api, meeting.id, textSignature]);
  useEffect(() => {
    if (!onLabels) return;
    const labels: Record<string, string> = {};
    if (doc?.enabled)
      for (const a of doc.assignments)
        if (doc.sources[a.sourceId] === a.textSha256)
          labels[a.sourceId] = a.name;
    onLabels(meeting.id, labels);
  }, [doc, meeting.id, onLabels]);
  if (!api.getSpeakerLabels || !api.saveSpeakerLabels) return null;
  const parts = meeting.parts.filter((p) => p.file && p.text?.trim());
  const selected = parts.find((p) => p.file === source);
  const stale =
    doc?.assignments.filter((a) => doc.sources[a.sourceId] !== a.textSha256) ??
    [];
  async function save(enabled: boolean, assignments: SpeakerAssignment[]) {
    if (!doc || busy) return;
    const target = meeting.id;
    setBusy(true);
    setError(null);
    try {
      await api.saveSpeakerLabels!(target, doc.revision, enabled, assignments);
      const updated = await api.getSpeakerLabels!(target);
      setDoc(updated);
      setName("");
      setQuote("");
      setSource("");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const Container = embedded ? "section" : "details";
  return (
    <Container className="speaker-options">
      {embedded ? (
        <h3>
          {de ? "Sprecher selbst bestätigen" : "Confirm speakers yourself"}
        </h3>
      ) : (
        <summary
          title={
            de
              ? "Sprecher für einzelne Transkriptabschnitte manuell bestätigen."
              : "Manually confirm speakers for individual transcript sections."
          }
        >
          {de
            ? "Sprecher nach dem Meeting zuordnen · optional"
            : "Assign speakers after the meeting · optional"}
        </summary>
      )}
      <p>
        {de
          ? "Eine Namensnennung belegt nicht, wer spricht. Ordne nur Abschnitte zu, deren Sprecher du bestätigen kannst. Bei mehreren Stimmen bleibt die Zuordnung unbekannt. Originaltexte und ältere Zuordnungen bleiben erhalten."
          : "A name mentioned in text does not identify the speaker. Assign a section only when you can confirm who spoke. Leave mixed or uncertain sections unknown. Original text and prior annotations are retained."}
      </p>
      {error && <p role="alert">{error}</p>}
      {onPropose && (
        <button
          className="secondary"
          disabled={busy || proposeDisabled || meeting.status !== "complete"}
          onClick={onPropose}
          title={
            de
              ? "Die Vorlage für unbestätigte Namensvorschläge öffnen."
              : "Open the template for unconfirmed name suggestions."
          }
        >
          {de
            ? "Lokale KI-Namensvorschläge vorbereiten"
            : "Prepare local AI name suggestions"}
        </button>
      )}
      {onPropose && (
        <p>
          {de
            ? "Öffnet eine eigene Auswertungsvorlage. Wähle dort ein lokales Textmodell und starte ausdrücklich. Vorschläge werden nicht automatisch zugeordnet."
            : "Opens a dedicated analysis template. Choose a local text model there and start explicitly. Suggestions are never assigned automatically."}
        </p>
      )}
      {doc && (
        <>
          {candidates.length > 0 && (
            <div>
              <h3>
                {de
                  ? "Genannte Namen · unbestätigte Vorschläge"
                  : "Mentioned names · unconfirmed suggestions"}
              </h3>
              {candidates.slice(0, 100).map((c, i) => (
                <article key={c.analysisId + c.file + i}>
                  <strong>{c.name}</strong>
                  <blockquote>{c.quote}</blockquote>
                  <button
                    disabled={busy}
                    onClick={() => {
                      setSource(c.file);
                      setName(c.name);
                      setQuote(c.quote);
                    }}
                  >
                    {de
                      ? "Nur ins Prüfformular übernehmen"
                      : "Copy only into the review form"}
                  </button>
                </article>
              ))}
            </div>
          )}
          <label>
            <input
              type="checkbox"
              checked={doc.enabled}
              title={
                de
                  ? "Manuell bestätigte Namen im Transkript anzeigen."
                  : "Show manually confirmed names in the transcript."
              }
              disabled={busy || stale.length > 0}
              onChange={(e) => void save(e.target.checked, doc.assignments)}
            />
            {de
              ? "Bestätigte Zuordnungen verwenden"
              : "Use confirmed assignments"}
          </label>
          {stale.length > 0 && (
            <div role="alert">
              <p>
                {de
                  ? "Einige Originalabschnitte haben sich geändert. Ihre alten Zuordnungen sind nicht mehr bestätigt."
                  : "Some original sections changed. Their previous labels are no longer confirmed."}
              </p>
              <button
                disabled={busy}
                onClick={() => {
                  if (
                    window.confirm(
                      de
                        ? "Veraltete Zuordnungen aus der aktuellen Fassung entfernen? Frühere Fassungen bleiben erhalten."
                        : "Remove stale labels from the current revision? Previous revisions are retained.",
                    )
                  )
                    void save(
                      doc.enabled,
                      doc.assignments.filter((a) => !stale.includes(a)),
                    );
                }}
              >
                {de ? "Veraltete Zuordnungen entfernen" : "Remove stale labels"}
              </button>
            </div>
          )}
          <ul>
            {doc.assignments.map((a) => (
              <li key={a.sourceId}>
                <strong>{a.name}</strong> · {a.sourceId}
                <blockquote>{a.evidence}</blockquote>
                <button
                  disabled={busy || stale.length > 0}
                  onClick={() =>
                    void save(
                      doc.enabled,
                      doc.assignments.filter(
                        (old) => old.sourceId !== a.sourceId,
                      ),
                    )
                  }
                >
                  {de ? "Zuordnung entfernen" : "Remove assignment"}
                </button>
              </li>
            ))}
          </ul>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (!selected?.file || !doc.sources[source]) return;
              void save(true, [
                ...doc.assignments.filter((a) => a.sourceId !== source),
                {
                  sourceId: source,
                  textSha256: doc.sources[source],
                  name: name.trim(),
                  evidence: quote,
                },
              ]);
            }}
          >
            <label>
              {de ? "Originalabschnitt" : "Original section"}
              <select
                value={source}
                title={
                  de
                    ? "Den Transkriptabschnitt für die Zuordnung auswählen."
                    : "Choose the transcript section to label."
                }
                disabled={busy}
                onChange={(e) => {
                  setSource(e.target.value);
                  setQuote("");
                  setName("");
                }}
              >
                <option value="">
                  {de ? "Abschnitt wählen" : "Choose section"}
                </option>
                {parts.map((p) => (
                  <option value={p.file!} key={p.file}>
                    {clock(p.startSample / 16)} · {p.source} ·{" "}
                    {p.text?.slice(0, 70)}
                  </option>
                ))}
              </select>
            </label>
            {selected && (
              <>
                <blockquote>{selected.text}</blockquote>
                <label>
                  {de
                    ? "Von dir bestätigter Sprechername"
                    : "Speaker name confirmed by you"}
                  <input
                    value={name}
                    title={
                      de
                        ? "Den Namen der Person eingeben, die hier spricht."
                        : "Enter the name of the person speaking here."
                    }
                    maxLength={80}
                    disabled={busy}
                    onChange={(e) => setName(e.target.value)}
                  />
                </label>
                <label>
                  {de
                    ? "Wörtliches Zitat aus diesem Abschnitt"
                    : "Literal quote from this section"}
                  <textarea
                    value={quote}
                    title={
                      de
                        ? "Ein wörtliches Zitat aus dem angezeigten Abschnitt übernehmen."
                        : "Use an exact quote from the displayed section."
                    }
                    maxLength={2000}
                    disabled={busy}
                    onChange={(e) => setQuote(e.target.value)}
                  />
                </label>
                <button
                  className="secondary"
                  title={
                    de
                      ? "Die geprüfte Zuordnung für diesen Abschnitt speichern."
                      : "Save the confirmed speaker for this section."
                  }
                  disabled={
                    busy ||
                    stale.length > 0 ||
                    !name.trim() ||
                    !quote.trim() ||
                    !selected.text?.includes(quote)
                  }
                >
                  {de
                    ? "Zuordnung ausdrücklich bestätigen"
                    : "Explicitly confirm assignment"}
                </button>
              </>
            )}
          </form>
        </>
      )}
    </Container>
  );
}
/** FNV-1a over the whole text: cheap enough per render, sensitive to any change. */
function textDigest(text: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `${text.length}.${hash.toString(16)}`;
}
