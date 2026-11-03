import { useEffect, useRef } from "react";
import {
  Maximize2,
  X,
  ArrowUpRight,
  Check,
  Clock3,
  CircleHelp,
  Info,
} from "lucide-react";
import { transcript, formatTime } from "./data.mjs";

export function IconButton({ label, children, className = "", ...props }) {
  return (
    <button
      type="button"
      className={`icon-button ${className}`}
      aria-label={label}
      title={label}
      {...props}
    >
      {children}
    </button>
  );
}

export function Status({ kind = "open", children }) {
  const Symbol =
    kind === "done"
      ? Check
      : kind === "question"
        ? CircleHelp
        : kind === "info"
          ? Info
          : Clock3;
  return (
    <span className={`status ${kind}`}>
      <Symbol size={12} aria-hidden="true" />
      {children}
    </span>
  );
}

export function Panel({
  title,
  subtitle,
  icon: Symbol,
  children,
  onExpand,
  actions,
  className = "",
}) {
  return (
    <section className={`panel ${className}`} aria-label={title}>
      <div className="panel-heading">
        <h2>
          {Symbol && <Symbol size={16} aria-hidden="true" />}
          {title}
          {subtitle && <span className="heading-meta">{subtitle}</span>}
        </h2>
        <div className="panel-actions">
          {actions}
          {onExpand && (
            <IconButton label={`${title} vergrößern`} onClick={onExpand}>
              <Maximize2 size={15} />
            </IconButton>
          )}
        </div>
      </div>
      {children}
    </section>
  );
}

export function SourceButton({ id, onOpen, text = false }) {
  const row = transcript.find((item) => item.id === id);
  if (!row) return null;
  return (
    <button
      type="button"
      className={`source-link ${text ? "source-with-text" : ""}`}
      onClick={() => onOpen(id)}
      title={`${id}: ${row.text}`}
    >
      {text ? "Beleg" : id}
      <ArrowUpRight size={11} aria-hidden="true" />
    </button>
  );
}

export function Quote({ id, onOpen }) {
  const row = transcript.find((item) => item.id === id);
  return row ? (
    <div className="quote">
      <span>„{row.text}“</span>
      <SourceButton id={id} onOpen={onOpen} />
    </div>
  ) : null;
}

export function Modal({ title, onClose, children, wide = false }) {
  const ref = useRef(null);
  useEffect(() => {
    const dialog = ref.current;
    const opener = dialog.getRootNode().activeElement || document.activeElement;
    dialog.showModal();
    return () => {
      dialog.close();
      if (opener?.isConnected) opener.focus();
    };
  }, []);
  return (
    <dialog
      ref={ref}
      className={wide ? "wide-modal" : ""}
      aria-label={title}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === ref.current) onClose();
      }}
    >
      <div className="modal-heading">
        <h2>{title}</h2>
        <IconButton label="Dialog schließen" onClick={onClose}>
          <X size={19} />
        </IconButton>
      </div>
      <div className="modal-content">{children}</div>
    </dialog>
  );
}

export function TimeLabel({ row }) {
  return (
    <span className="time-label">
      {formatTime(row.start)}–{formatTime(row.end)}
    </span>
  );
}
