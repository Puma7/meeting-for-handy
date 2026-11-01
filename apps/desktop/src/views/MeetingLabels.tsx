import { useEffect, useId, useState } from "react";
import { Pencil, Pin, Save, X } from "lucide-react";
import type { DesktopApi, Meeting, MeetingMetadata, Locale } from "../types";
import { errorMessage } from "../api";
/**
 * Edits revision-checked display metadata while keeping VIP protection out of the rename form.
 * A failed native save leaves the draft and original metadata available for correction.
 */
export default function MeetingLabels({
  api,
  meeting,
  locale,
  onChanged,
}: {
  api: DesktopApi;
  meeting: Meeting;
  locale: Locale;
  onChanged: (metadata: MeetingMetadata) => void;
}) {
  const pinHelpId = useId();
  const [editing, setEditing] = useState(false),
    [title, setTitle] = useState(meeting.title),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setEditing(false);
    setTitle(meeting.title);
    setError(null);
  }, [meeting.id]);
  const de = locale === "de";
  const meta = meeting.userMetadata ?? {
    revision: 0,
    title: null,
    pinned: false,
    audioRemoved: false,
  };
  async function save(nextTitle: string | null, pinned: boolean) {
    if (!api.editMeetingMetadata || busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await api.editMeetingMetadata(
        meeting.id,
        meta.revision,
        nextTitle,
        pinned,
      );
      onChanged(next);
      setEditing(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  if (!api.editMeetingMetadata) return null;
  return (
    <section
      className="meeting-labels"
      aria-label={
        de ? "Meetingname und VIP-Schutz" : "Meeting name and VIP protection"
      }
    >
      {editing ? (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void save(title.trim(), meta.pinned);
          }}
        >
          <label>
            {de ? "Meetingname" : "Meeting name"}
            <input
              autoFocus
              value={title}
              maxLength={200}
              disabled={busy}
              onChange={(e) => setTitle(e.target.value)}
            />
          </label>
          <button className="secondary" disabled={busy || !title.trim()}>
            <Save size={15} />
            {de ? "Namen speichern" : "Save name"}
          </button>
          <button
            type="button"
            className="text-button"
            disabled={busy}
            onClick={() => setEditing(false)}
          >
            <X size={15} />
            {de ? "Abbrechen" : "Cancel"}
          </button>
        </form>
      ) : (
        <button
          className="text-button"
          title={
            de
              ? "Anzeigenamen ändern; Aufnahme und Transkript bleiben erhalten."
              : "Change the display name; recording and transcript are kept."
          }
          disabled={busy}
          onClick={() => {
            setTitle(meeting.title);
            setEditing(true);
          }}
        >
          <Pencil size={15} />
          {de ? "Meeting umbenennen" : "Rename meeting"}
        </button>
      )}
      {!editing && (
        <button
          className={meta.pinned ? "secondary pinned" : "text-button"}
          aria-pressed={meta.pinned}
          aria-describedby={pinHelpId}
          title={
            de
              ? "Oben in der Historie anheften und vor automatischem Aufräumen schützen. Manuelles Löschen bleibt möglich."
              : "Pin to the top of history and protect from automatic cleanup. Manual deletion is still possible."
          }
          disabled={busy || Boolean(meeting.metadataError)}
          onClick={() => void save(meta.title, !meta.pinned)}
        >
          <Pin size={15} />
          {meta.pinned
            ? de
              ? "VIP · vor Aufräumen geschützt"
              : "VIP · protected from cleanup"
            : de
              ? "Als VIP anheften"
              : "Pin as VIP"}
        </button>
      )}
      {!editing && (
        <p id={pinHelpId} className="muted">
          {de
            ? "VIP hält das Meeting oben und schützt vor automatischem Aufräumen. Manuelles Löschen bleibt möglich."
            : "VIP keeps the meeting at the top and protects it from automatic cleanup. Manual deletion is still possible."}
        </p>
      )}
      {(error || meeting.metadataError) && (
        <p role="alert" className="inline-error">
          {error ?? meeting.metadataError}
        </p>
      )}
      {meta.audioRemoved && (
        <p className="muted">
          {de
            ? "Audio wurde lokal archiviert. Transkript und Auswertungen bleiben erhalten; die Wiederherstellung prüft das Archiv erneut."
            : "Audio was archived locally. The transcript and analyses are retained; restoration verifies the archive again."}
        </p>
      )}
    </section>
  );
}
