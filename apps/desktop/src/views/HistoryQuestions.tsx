import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowRight } from "lucide-react";
import { errorMessage } from "../api";
import { clock } from "../domain";
import type {
  DesktopApi,
  HistoryAnswer,
  HistoryCitation,
  HistoryQuestionRequest,
  Locale,
  MeetingSummary,
  SearchScope,
} from "../types";

export type HistoryModel = Omit<
  HistoryQuestionRequest,
  "question" | "scope" | "meetingIds"
>;
export type HistoryQuestionResult = {
  question: string;
  answer: HistoryAnswer;
  meetingIds: string[];
};

export function historyQuestionUsesMeeting(
  result: HistoryQuestionResult,
  matches: (id: string) => boolean,
) {
  return (
    result.meetingIds.some(matches) ||
    result.answer.citations.some((hit) => matches(hit.meetingId))
  );
}

export default function HistoryQuestions({
  ask,
  locale,
  model,
  scope,
  invalidDates,
  hiddenMeetingIds,
  initialResult,
  choices,
  meetingId,
  onOpen,
}: {
  ask: NonNullable<DesktopApi["askMeetingHistory"]>;
  locale: Locale;
  model?: HistoryModel;
  scope?: SearchScope;
  invalidDates: boolean;
  hiddenMeetingIds?: ReadonlySet<string>;
  initialResult?: HistoryQuestionResult;
  choices?: MeetingSummary[];
  meetingId?: string;
  onOpen: (citation: HistoryCitation, result: HistoryQuestionResult) => void;
}) {
  const de = locale === "de";
  const [question, setQuestion] = useState(initialResult?.question ?? "");
  const [answer, setAnswer] = useState<HistoryQuestionResult | null>(
    initialResult ?? null,
  );
  const [initiallyOpen] = useState(Boolean(initialResult));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedIds, setSelection] = useState(initialResult?.meetingIds ?? []);
  const [findMeeting, setFindMeeting] = useState("");
  const vipIds = useMemo(
    () =>
      scope?.pinnedOnly && choices
        ? new Set(
            choices
              .filter((choice) => choice.userMetadata?.pinned)
              .map((choice) => choice.id),
          )
        : undefined,
    [scope?.pinnedOnly, choices],
  );
  const outsideScope = useCallback(
    (id: string) =>
      Boolean(hiddenMeetingIds?.has(id) || (vipIds && !vipIds.has(id))),
    [hiddenMeetingIds, vipIds],
  );
  const selection = selectedIds.filter((id) => !outsideScope(id));
  const selectionRemoved = selection.length !== selectedIds.length;
  const answerRemoved = Boolean(
    answer && historyQuestionUsesMeeting(answer, outsideScope),
  );
  useEffect(() => {
    if (selectionRemoved)
      setSelection((ids) => ids.filter((id) => !outsideScope(id)));
    if (answerRemoved) setAnswer(null);
  }, [selectionRemoved, answerRemoved, outsideScope]);
  const candidates = choices?.filter(
    (meeting) =>
      !outsideScope(meeting.id) &&
      (selection.includes(meeting.id) ||
        meeting.title.toLowerCase().includes(findMeeting.trim().toLowerCase())),
  );
  const visibleChoices = candidates && [
    ...candidates.filter((meeting) => selection.includes(meeting.id)),
    ...candidates
      .filter((meeting) => !selection.includes(meeting.id))
      .slice(0, 12),
  ];
  const meetingIds = meetingId ? [meetingId] : selection;
  const visibleResult = answerRemoved ? null : answer;
  const visible = visibleResult?.answer;
  return (
    <details className="history-questions" open={initiallyOpen}>
      <summary>
        {meetingId
          ? de
            ? "Dieses Meeting befragen"
            : "Ask this meeting"
          : de
            ? "Meetings befragen"
            : "Ask your meetings"}
      </summary>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          if (busy || !model || invalidDates || !question.trim()) return;
          setBusy(true);
          setError(null);
          setAnswer(null);
          try {
            const response = await ask({
              ...model,
              question: question.trim(),
              scope,
              ...(meetingIds.length ? { meetingIds } : {}),
            });
            setAnswer({ question, answer: response, meetingIds });
          } catch (failure) {
            setError(errorMessage(failure));
          } finally {
            setBusy(false);
          }
        }}
      >
        <p>
          {de
            ? "Die Frage wird auf diesem Computer ausgewertet. Die Antwort besteht aus Originalzitaten."
            : "The question is processed on this computer. Answers consist of original quotes."}
        </p>
        {visibleChoices && (
          <details>
            <summary>
              {selection.length
                ? de
                  ? `${selection.length} Meetings ausgewählt`
                  : `${selection.length} meetings selected`
                : de
                  ? "Alle Meetings im gewählten Zeitraum und VIP-Filter"
                  : "All meetings in the date and VIP filters"}
            </summary>
            <fieldset disabled={busy}>
              <legend>
                {de
                  ? "Bis zu zwölf Meetings auswählen"
                  : "Select up to twelve meetings"}
              </legend>
              <label>
                {de ? "Meetingtitel suchen" : "Find meeting by title"}
                <input
                  type="search"
                  value={findMeeting}
                  onChange={(event) => setFindMeeting(event.target.value)}
                />
              </label>
              {visibleChoices.map((item) => {
                const selected = selection.includes(item.id);
                return (
                  <label key={item.id}>
                    <input
                      type="checkbox"
                      checked={selected}
                      disabled={!selected && selection.length >= 12}
                      onChange={() => {
                        setSelection(
                          selected
                            ? selection.filter((id) => id !== item.id)
                            : [...selection, item.id],
                        );
                        setAnswer(null);
                        setError(null);
                      }}
                    />
                    {item.title} ·{" "}
                    {new Date(item.startedAt).toLocaleString(locale)}
                  </label>
                );
              })}
              {selection.length > 0 && (
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    setSelection([]);
                    setAnswer(null);
                    setError(null);
                  }}
                >
                  {de ? "Auswahl zurücksetzen" : "Clear selection"}
                </button>
              )}
            </fieldset>
          </details>
        )}
        {!model && (
          <p>
            {de
              ? "Wähle in den Einstellungen ein lokales Textmodell."
              : "Choose a local text model in Settings."}
          </p>
        )}
        <label>
          {de ? "Frage an deine Meetings" : "Question for your meetings"}
          <input
            type="text"
            value={question}
            maxLength={400}
            disabled={busy}
            onChange={(event) => {
              setQuestion(event.target.value);
              setAnswer(null);
              setError(null);
            }}
            placeholder={
              de
                ? "Was wurde in den Meetings zum Liefertermin gesagt?"
                : "What did the meetings say about the delivery date?"
            }
          />
        </label>
        <button
          type="submit"
          className="secondary"
          disabled={busy || !model || invalidDates || !question.trim()}
        >
          {de ? "Lokal beantworten" : "Answer locally"}
        </button>
      </form>
      <div role="status">
        {busy
          ? de
            ? "Das lokale Modell prüft passende Stellen …"
            : "The local model is checking matching passages …"
          : visible &&
            (de
              ? `${visible.searchedPassages} Originalstellen berücksichtigt.`
              : `${visible.searchedPassages} original passages considered.`)}
      </div>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      {visible?.citations.length === 0 && (
        <p>
          {de
            ? "In den gefundenen Originalstellen gibt es keine belegte Antwort."
            : "The retrieved passages contain no supported answer."}
        </p>
      )}
      {visibleResult &&
        visibleResult.answer.citations.map((hit, index) => (
          <article
            className="session-row"
            key={`${hit.meetingId}:${hit.sourceId}:${index}`}
          >
            <div>
              <strong>
                {hit.meetingTitle} · {clock(hit.startMs)}
              </strong>
              <blockquote>{hit.quote}</blockquote>
            </div>
            <button
              type="button"
              className="secondary"
              onClick={() => onOpen(hit, visibleResult)}
            >
              <ArrowRight size={15} aria-hidden="true" />
              {de ? "Originalstelle öffnen" : "Open original passage"}
            </button>
          </article>
        ))}
    </details>
  );
}
