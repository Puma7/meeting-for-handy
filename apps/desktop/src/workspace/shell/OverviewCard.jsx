import { useWorkspaceText } from "./workspaceLocale.jsx";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  GripHorizontal,
  GripVertical,
  Maximize2,
  MoreHorizontal,
  X,
} from "lucide-react";
import { IconButton } from "./ui.jsx";
import { clampManualHeight, moduleName, modules } from "./overviewLayout.mjs";

/** Keep placement actions reachable above clipped cards, including WebViews without native popovers. */
function PlacementMenu({ id, label, children }) {
  const menu = useRef(null),
    trigger = useRef(null),
    openAtPress = useRef(null);
  const [nativePopover] = useState(
    () =>
      typeof HTMLElement.prototype.showPopover === "function" &&
      typeof HTMLElement.prototype.hidePopover === "function",
  );
  const [open, setOpen] = useState(false);
  const [portalRoot, setPortalRoot] = useState(null);
  const [position, setPosition] = useState({ left: 10, top: 10 });
  useEffect(() => {
    if (!nativePopover) return;
    const popup = menu.current;
    const sync = () => setOpen(popup.matches(":popover-open"));
    popup.addEventListener("toggle", sync);
    return () => popup.removeEventListener("toggle", sync);
  }, [nativePopover]);
  useLayoutEffect(() => {
    if (!open) return;
    const popup = menu.current;
    const anchor = trigger.current.getBoundingClientRect();
    const bounds = popup.getBoundingClientRect();
    setPosition({
      left: Math.max(
        10,
        Math.min(innerWidth - bounds.width - 10, anchor.right - bounds.width),
      ),
      top: Math.max(
        10,
        Math.min(innerHeight - bounds.height - 10, anchor.bottom + 6),
      ),
    });
    if (nativePopover) return;
    popup.querySelector("button, select")?.focus();
    const dismissOutside = (event) => {
      const path = event.composedPath();
      if (!path.includes(popup) && !path.includes(trigger.current))
        setOpen(false);
    };
    const escape = (event) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      trigger.current.focus();
    };
    document.addEventListener("pointerdown", dismissOutside, true);
    document.addEventListener("focusin", dismissOutside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", dismissOutside, true);
      document.removeEventListener("focusin", dismissOutside);
      document.removeEventListener("keydown", escape);
    };
  }, [open, nativePopover]);
  /** Preserve native pointer light-dismiss semantics; fallback dismissal never queries unsupported selectors. */
  function toggleMenu(event) {
    if (!nativePopover) {
      setPortalRoot(event.currentTarget.closest(".prototype") || document.body);
      setOpen(!open);
      return;
    }
    const popup = menu.current;
    const pointerActivation =
      event.detail > 0 || event.nativeEvent?.pointerType;
    const wasOpen = pointerActivation
      ? (openAtPress.current ?? popup.matches(":popover-open"))
      : popup.matches(":popover-open");
    openAtPress.current = null;
    if (wasOpen) {
      if (popup.matches(":popover-open")) popup.hidePopover();
      setOpen(false);
    } else {
      popup.showPopover();
      setOpen(true);
    }
  }
  const popup = (
    <div
      ref={menu}
      id={id}
      popover={nativePopover ? "auto" : undefined}
      hidden={nativePopover ? undefined : !open}
      className={`placement-popover${nativePopover ? "" : " placement-fallback"}${!nativePopover && open ? " placement-open" : ""}`}
      style={position}
    >
      {children}
    </div>
  );
  return (
    <div className="card-placement-menu">
      <IconButton
        label={label}
        aria-haspopup="true"
        aria-controls={id}
        aria-expanded={open}
        onPointerDown={(event) => {
          trigger.current = event.currentTarget;
          if (nativePopover)
            openAtPress.current = menu.current.matches(":popover-open");
        }}
        onPointerCancel={() => {
          openAtPress.current = null;
        }}
        onClick={(event) => {
          trigger.current = event.currentTarget;
          toggleMenu(event);
        }}
      >
        <MoreHorizontal size={15} />
      </IconButton>
      {!nativePopover && open && portalRoot
        ? createPortal(popup, portalRoot)
        : popup}
    </div>
  );
}

/** Render a dashboard card with compact controls, expansion and explicit layout-edit actions. */
export function OverviewCard({
  card,
  renderedHeight,
  autoFill,
  hasNext,
  onResizePreview,
  editing,
  moving,
  target,
  children,
  onLift,
  onMove,
  gripEvents,
  onHide,
  onResize,
  onReplace,
  availableModules = modules,
  onExpand,
  onKeyMove,
}) {
  const t = useWorkspaceText();
  const pointer = useRef(null),
    clickOrigin = useRef(null);
  const height = renderedHeight ?? card.height;
  return (
    <div
      className={`overview-card ${moving === card.id ? "card-moving" : ""}`}
      data-module={card.id}
      data-auto-fill={autoFill || undefined}
      style={{ height }}
    >
      {target?.anchorId === card.id && (
        <div
          className={`drop-insertion drop-${target.edge}`}
          data-drop-position={target.edge}
          aria-hidden="true"
        >
          <span>
            {t("Hier einfügen:")} {t(moduleName(moving))}
          </span>
        </div>
      )}
      {editing && (
        <div className="card-layout-tools">
          <button
            className="card-grab"
            data-handle-id={card.id}
            aria-label={`${t(moduleName(card.id))} ${t("verschieben")}`}
            aria-pressed={moving === card.id}
            aria-describedby="layout-keyboard-help"
            {...gripEvents}
            onPointerDown={(event) => {
              clickOrigin.current = { x: event.clientX, y: event.clientY };
              gripEvents.onPointerDown(event);
            }}
            onClick={(event) => {
              const origin = clickOrigin.current;
              clickOrigin.current = null;
              if (
                !event.detail ||
                !origin ||
                Math.hypot(event.clientX - origin.x, event.clientY - origin.y) <
                  5
              )
                onLift(moving === card.id ? null : card.id);
            }}
            onKeyDown={(event) => onKeyMove(event, card.id)}
          >
            <GripVertical size={14} />
            <span>{t(moduleName(card.id))}</span>
          </button>
          <PlacementMenu
            id={`placement-${card.id}`}
            label={`${t(moduleName(card.id))} ${t("anordnen")}`}
          >
            <span>{t("Verschieben")}</span>
            <div className="placement-directions">
              {[
                ["up", ArrowUp, t("Nach oben")],
                ["down", ArrowDown, t("Nach unten")],
                ["left", ArrowLeft, t("Spalte nach links")],
                ["right", ArrowRight, t("Spalte nach rechts")],
              ].map(([direction, Symbol, label]) => (
                <IconButton
                  key={direction}
                  label={`${t(moduleName(card.id))}: ${t(label)}`}
                  onClick={() => onMove(card.id, direction)}
                >
                  <Symbol size={15} />
                </IconButton>
              ))}
            </div>
            {autoFill ? (
              <span className="auto-height-label">
                {t("Höhe · bis zu den Notizen")}
              </span>
            ) : (
              <label>
                {t("Höhe")}
                <select
                  aria-label={`${t(moduleName(card.id))} ${t("Kartenhöhe")}`}
                  value={card.height}
                  onChange={(event) =>
                    onResize(card.id, Number(event.target.value))
                  }
                >
                  {[...new Set([216, 312, 432, 636, card.height])]
                    .sort((a, b) => a - b)
                    .map((value) => (
                      <option key={value} value={value}>
                        {value} px
                        {value === 216
                          ? t(" · Kompakt")
                          : value === 636
                            ? t(" · Groß")
                            : ""}
                      </option>
                    ))}
                </select>
              </label>
            )}
            <label>
              {t("Modul")}
              <select
                aria-label={`${t(moduleName(card.id))} ${t("Modul austauschen")}`}
                value={card.id}
                onChange={(event) => onReplace(card.id, event.target.value)}
              >
                {availableModules.map(([id, label]) => (
                  <option key={id} value={id}>
                    {t(label)}
                  </option>
                ))}
              </select>
            </label>
            <small>{t("Bereits sichtbare Module tauschen den Platz.")}</small>
          </PlacementMenu>
          <IconButton
            label={`${t(moduleName(card.id))} ${t("aus Überblick entfernen")}`}
            onClick={() => onHide(card.id)}
          >
            <X size={14} />
          </IconButton>
          <IconButton
            label={`${t(moduleName(card.id))} ${t("vergrößern")}`}
            onClick={() => onExpand(card.id, t(moduleName(card.id)))}
          >
            <Maximize2 size={14} />
          </IconButton>
        </div>
      )}
      {children}
      {hasNext && (
        <div
          className="card-resize"
          role="separator"
          aria-label={`${t(moduleName(card.id))} ${t("Höhe ändern")}`}
          aria-orientation="horizontal"
          tabIndex={0}
          aria-valuenow={height}
          aria-valuemin={72}
          aria-valuemax={1200}
          onPointerDown={(event) => {
            event.preventDefault();
            event.currentTarget.setPointerCapture(event.pointerId);
            pointer.current = {
              y: event.clientY,
              height: height,
              final: height,
            };
          }}
          onPointerMove={(event) => {
            if (!pointer.current) return;
            const next = clampManualHeight(
              pointer.current.height + event.clientY - pointer.current.y,
            );
            pointer.current.final = next;
            onResizePreview(next);
          }}
          onPointerUp={() => {
            if (pointer.current?.final !== pointer.current?.height)
              onResize(card.id, pointer.current.final);
            pointer.current = null;
            onResizePreview(null);
          }}
          onLostPointerCapture={() => {
            pointer.current = null;
            onResizePreview(null);
          }}
          onPointerCancel={() => {
            pointer.current = null;
            onResizePreview(null);
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowUp" || event.key === "ArrowDown") {
              event.preventDefault();
              onResize(
                card.id,
                clampManualHeight(
                  height + (event.key === "ArrowUp" ? -24 : 24),
                ),
              );
            }
          }}
        >
          <GripHorizontal size={17} />
        </div>
      )}
    </div>
  );
}
