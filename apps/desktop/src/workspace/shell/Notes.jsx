import { useWorkspaceText } from "./workspaceLocale.jsx";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  GripHorizontal,
  LockKeyhole,
  Pencil,
  Sparkles,
} from "lucide-react";
import {
  clampNotesHeight,
  decodeNotesHeight,
  defaultNotesHeight,
  draggedNotesHeight,
  notesHeightLimit,
  resizedNotesPreference,
} from "./notesHeight.mjs";

/** Keep private notes separate from released protocol text and allow viewport-bounded resizing. */
export function Notes({ ctx }) {
  const t = useWorkspaceText();
  const [kind, setKind] = useState("private");
  const [open, setOpen] = useState(true);
  const key = ctx.storageKey ? `${ctx.storageKey}.notes-height.v1` : null;
  const [loaded] = useState(() => {
    try {
      return decodeNotesHeight(key ? localStorage.getItem(key) : null);
    } catch {
      return decodeNotesHeight("{");
    }
  });
  const [preferredHeight, setPreferredHeight] = useState(loaded.height);
  const [heightBlocked, setHeightBlocked] = useState(loaded.blocked);
  const [draftHeight, setDraftHeight] = useState(null);
  const pointer = useRef(null),
    dock = useRef(null);
  const [availableHeight, setAvailableHeight] = useState(Infinity);
  useLayoutEffect(() => {
    const host = dock.current.parentElement;
    let fixed = [];
    /** Recalculate the available notes-pane limit without treating viewport changes as user preferences. */
    function measure() {
      const room =
        host.getBoundingClientRect().height -
        fixed.reduce(
          (total, element) => total + element.getBoundingClientRect().height,
          0,
        ) -
        200;
      setAvailableHeight(Math.floor(room));
    }
    const observer = new ResizeObserver(measure);
    /** Replace observer targets when notices join or leave the fixed workspace siblings. */
    function refreshSiblings() {
      fixed = Array.from(host.children).filter(
        (element) =>
          element !== dock.current &&
          !element.classList.contains("workspace-content") &&
          getComputedStyle(element).position !== "absolute",
      );
      observer.disconnect();
      observer.observe(host);
      fixed.forEach((element) => observer.observe(element));
      measure();
    }
    const mutations = new MutationObserver(refreshSiblings);
    mutations.observe(host, { childList: true });
    refreshSiblings();
    return () => {
      mutations.disconnect();
      observer.disconnect();
    };
  }, [ctx.snapshot, ctx.format]);
  const viewportHeight = ctx.viewportHeight ?? window.innerHeight;
  const height = clampNotesHeight(
    draftHeight ?? preferredHeight,
    viewportHeight,
    availableHeight,
  );
  const maximum = notesHeightLimit(viewportHeight, availableHeight);
  useEffect(() => setOpen(!ctx.compact), [ctx.compact]);
  useEffect(() => {
    if (key && !heightBlocked) {
      try {
        localStorage.setItem(
          key,
          JSON.stringify({ version: 1, height: preferredHeight }),
        );
      } catch {
        setHeightBlocked(true);
      }
    }
  }, [key, preferredHeight, heightBlocked]);
  /** Restore the notes preference when a pointer resize is cancelled. */
  function cancelResize() {
    pointer.current = null;
    setDraftHeight(null);
  }
  /** Resize notes from the keyboard while retaining enough meeting workspace. */
  function resizeKey(event) {
    const values = {
      ArrowUp: height + 24,
      ArrowDown: height - 24,
      Home: 96,
      End: maximum,
    };
    if (event.key === "Escape") {
      cancelResize();
      return;
    }
    if (values[event.key] !== undefined) {
      event.preventDefault();
      setPreferredHeight((previous) =>
        resizedNotesPreference(
          previous,
          height,
          clampNotesHeight(values[event.key], viewportHeight, availableHeight),
        ),
      );
    }
  }
  return (
    <section
      className={`notes-dock notes-resizable ${open ? "notes-open" : ""}`}
      aria-label={t("Notizblock")}
      ref={dock}
      style={{ "--notes-height": `${height}px` }}
    >
      {open && (
        <div
          className="notes-size-grip"
          role="separator"
          aria-label={t("Notizbereich Höhe ändern")}
          aria-describedby="notes-resize-help"
          aria-orientation="horizontal"
          tabIndex={0}
          aria-valuenow={height}
          aria-valuemin={96}
          aria-valuemax={maximum}
          title={t(
            "Notizen nach oben größer ziehen · Doppelklick: Standardhöhe",
          )}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            event.currentTarget.setPointerCapture(event.pointerId);
            pointer.current = { y: event.clientY, height, final: height };
          }}
          onPointerMove={(event) => {
            if (!pointer.current) return;
            const next = draggedNotesHeight(
              pointer.current.height,
              pointer.current.y,
              event.clientY,
              viewportHeight,
              availableHeight,
            );
            pointer.current.final = next;
            setDraftHeight(next);
          }}
          onPointerUp={() => {
            if (pointer.current) {
              const { height: start, final } = pointer.current;
              setPreferredHeight((previous) =>
                resizedNotesPreference(previous, start, final),
              );
            }
            cancelResize();
          }}
          onPointerCancel={cancelResize}
          onLostPointerCapture={cancelResize}
          onKeyDown={resizeKey}
          onDoubleClick={() => setPreferredHeight(defaultNotesHeight)}
        >
          <GripHorizontal size={20} />
        </div>
      )}
      <span id="notes-resize-help" className="sr-only">
        {t(
          "Nach oben größer ziehen. Pfeil hoch oder runter ändert die Höhe. Pos1: klein, Ende: groß. Doppelklick: Standardhöhe.",
        )}
        {heightBlocked &&
          t(
            " Die Höhe bleibt nur für diese Ansicht erhalten; der frühere Browserstand wird bewahrt.",
          )}
      </span>
      <div className="notes-heading">
        <button
          className="notes-toggle"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
        >
          <Pencil size={14} />
          <b>{t("Notizen")}</b>
          {open ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
        </button>
        <div className="segmented notes-tabs">
          <button
            className={kind === "private" ? "active" : ""}
            aria-pressed={kind === "private"}
            onClick={() => {
              setKind("private");
              setOpen(true);
            }}
          >
            <LockKeyhole size={12} />
            {t("Privat")}
          </button>
          <button
            className={kind === "protocol" ? "active" : ""}
            aria-pressed={kind === "protocol"}
            onClick={() => {
              setKind("protocol");
              setOpen(true);
            }}
          >
            {t("Fürs Protokoll")}
          </button>
        </div>
        <span className="notes-privacy">
          {kind === "private" ? (
            <>
              <LockKeyhole size={11} />
              {t("Nicht im Protokoll")}
            </>
          ) : (
            t("Eigene Protokollnotiz")
          )}
        </span>
        <button
          className="subtle-button chat-open"
          disabled={ctx.chatAvailable === false}
          title={
            ctx.chatAvailable === false
              ? t(
                  "Chat wird gesondert angeschlossen. Die lokale Kachelauswertung ist davon unabhängig.",
                )
              : undefined
          }
          onClick={() => ctx.openModal({ type: "chat" })}
        >
          <Sparkles size={13} />
          {t("Meeting fragen")}
        </button>
      </div>
      {open && (
        <div className="notes-input-wrap">
          <textarea
            aria-label={
              kind === "private" ? t("Private Notiz") : t("Protokollnotiz")
            }
            aria-describedby="notes-enter-help"
            rows={2}
            maxLength={12000}
            placeholder={
              kind === "private"
                ? t("Was möchtest du für dich festhalten?")
                : t("Was soll im Protokoll stehen?")
            }
            value={ctx.state.notes[kind]}
            onChange={(event) => ctx.setNote(kind, event.target.value)}
            onBlur={() => ctx.commitNote(kind)}
          />
          <span id="notes-enter-help" className="notes-key-hint">
            {t("Enter: neue Zeile")}
          </span>
          <span className="save-status">
            {ctx.storageBlocked
              ? t("Nur für diese Ansicht")
              : t("Im Browser gesichert")}
          </span>
        </div>
      )}
    </section>
  );
}
