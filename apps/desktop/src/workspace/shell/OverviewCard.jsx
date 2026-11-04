import { useWorkspaceText } from "./workspaceLocale.jsx";
import { useRef, useState } from "react";
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
    menu = useRef(null),
    clickOrigin = useRef(null);
  const [menuPosition, setMenuPosition] = useState({ left: 10, top: 10 });
  /** Toggle card actions while retaining the triggering button for focus restoration. */
  function toggleMenu(event) {
    const popup = menu.current;
    if (popup.matches(":popover-open")) {
      popup.hidePopover();
      return;
    }
    popup.showPopover();
    const anchor = event.currentTarget.getBoundingClientRect();
    const bounds = popup.getBoundingClientRect();
    setMenuPosition({
      left: Math.max(
        10,
        Math.min(innerWidth - bounds.width - 10, anchor.right - bounds.width),
      ),
      top: Math.max(
        10,
        Math.min(innerHeight - bounds.height - 10, anchor.bottom + 6),
      ),
    });
  }
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
            {t("Hier einfügen:")}
            {t(moduleName(moving))}
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
          <div className="card-placement-menu">
            <IconButton
              label={`${t(moduleName(card.id))} ${t("anordnen")}`}
              aria-haspopup="true"
              aria-controls={`placement-${card.id}`}
              onClick={toggleMenu}
            >
              <MoreHorizontal size={15} />
            </IconButton>
            <div
              ref={menu}
              id={`placement-${card.id}`}
              popover="auto"
              className="placement-popover"
              style={menuPosition}
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
                    aria-label={`${t(moduleName(card.id))} Kartenhöhe`}
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
            </div>
          </div>
          <IconButton
            label={`${t(moduleName(card.id))} aus Überblick entfernen`}
            onClick={() => onHide(card.id)}
          >
            <X size={14} />
          </IconButton>
          <IconButton
            label={`${t(moduleName(card.id))} vergrößern`}
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
          aria-label={`${t(moduleName(card.id))} Höhe ändern`}
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
