import { useWorkspaceText } from "./workspaceLocale.jsx";
import { Fragment, useEffect, useReducer, useRef, useState } from "react";
import {
  Check,
  CircleHelp,
  GripVertical,
  LayoutGrid,
  Plus,
  RotateCcw,
  Undo2,
} from "lucide-react";
import { IconButton } from "./ui.jsx";
import { OverviewCard } from "./OverviewCard.jsx";
import { fitColumn } from "./fitOverview.mjs";
import { useOverviewSize } from "./useOverviewSize.jsx";
import {
  clampManualHeight,
  decodeLayout,
  gapAnchor,
  insertionTarget,
  moduleName,
  modules,
  moveCard,
  presetLayout,
  replaceCard,
  resizeColumns,
  setColumns,
  stepCard,
  toggleCard,
} from "./overviewLayout.mjs";

/** Apply layout operations immutably while preserving valid card identities and column bounds. */
function reducer(state, action) {
  if (action.type === "restore") return action.state;
  if (action.type === "undo")
    return state.past.length
      ? { layout: state.past.at(-1), past: state.past.slice(0, -1) }
      : state;
  const layout = action.update(state.layout);
  return layout === state.layout
    ? state
    : { layout, past: [...state.past.slice(-29), state.layout] };
}

/** Render the configurable dashboard with pointer/keyboard movement and independent card resizing. */
export function CustomOverview({
  variant,
  storageKey,
  renderPanel,
  onExpand,
  layoutPicker,
  mergeRelevant,
  manageLayout,
  layoutLabel,
  isDefaultLayout,
}) {
  const t = useWorkspaceText();
  /** Merge visible presets with retained preferences without duplicate card identifiers. */
  function combineCards(layout) {
    if (!mergeRelevant) return layout;
    const hasSummary = layout.cards.some(({ id }) => id === "summary");
    return {
      ...layout,
      cards: layout.cards.flatMap((card) =>
        card.id !== "relevant"
          ? [card]
          : hasSummary
            ? []
            : [{ ...card, id: "summary" }],
      ),
    };
  }
  const availableModules = mergeRelevant
    ? modules.filter(([id]) => id !== "relevant")
    : modules;
  const [loaded] = useState(() => {
    try {
      return decodeLayout(
        localStorage.getItem(`${storageKey}.layout.v1`),
        variant,
      );
    } catch {
      return { layout: presetLayout(variant), blocked: true };
    }
  });
  const [state, dispatch] = useReducer(reducer, {
    layout: combineCards(loaded.layout),
    past: [],
  });
  const [blocked, setBlocked] = useState(loaded.blocked);
  const [editing, setEditing] = useState(false);
  const [layoutName, setLayoutName] = useState("");
  const [layoutMessage, setLayoutMessage] = useState("");
  /** Persist a completed layout edit and report storage failure instead of silently losing it. */
  function saveLayout(action) {
    const name = layoutName.trim();
    if (action !== "default" && !name) {
      setLayoutMessage(t("Bitte einen Layoutnamen eingeben."));
      return;
    }
    const error = manageLayout(action, name, layout);
    if (!error) {
      setLayoutName("");
      setLayoutMessage(
        action === "default"
          ? t("Standardlayout gespeichert.")
          : t("Layout gespeichert."),
      );
    } else {
      setLayoutMessage(error);
    }
  }
  const [moving, setMoving] = useState(null);
  const [target, setTarget] = useState(null);
  const [announcement, setAnnouncement] = useState("");
  const [draftWidths, setDraftWidths] = useState(null);
  const grid = useRef(null),
    resize = useRef(null),
    lifted = useRef(null),
    cardDrag = useRef(null);
  const { layout } = state;
  const size = useOverviewSize(grid);
  const [heightPreview, setHeightPreview] = useState(null);
  useEffect(() => {
    if (!blocked) {
      try {
        localStorage.setItem(`${storageKey}.layout.v1`, JSON.stringify(layout));
      } catch {
        setBlocked(true);
      }
    }
  }, [layout, storageKey, blocked]);
  function update(fn) {
    dispatch({ type: "update", update: fn });
  }
  /** Restore keyboard focus to the moved card after the DOM order changes. */
  function focusGrip(id) {
    requestAnimationFrame(() =>
      grid.current?.querySelector(`[data-handle-id="${id}"]`)?.focus(),
    );
  }
  /** Apply a destination and announce the card's new location for assistive technology. */
  function move(id, direction) {
    update((previous) => stepCard(previous, id, direction));
    setTarget(null);
    setAnnouncement(`${t(moduleName(id))} ${t("verschoben.")}`);
    focusGrip(id);
  }
  /** Begin a card move with the current origin retained for cancellation. */
  function lift(id) {
    if (!moving && id) lifted.current = state;
    setMoving(id);
    setTarget(null);
    setAnnouncement(
      id
        ? `${t(moduleName(id))} ${t("aufgenommen. Mit Pfeiltasten verschieben, Enter zum Ablegen, Escape zum Abbrechen.")}`
        : t("Modul abgelegt."),
    );
  }
  /** Clear drag state and restore focus without changing the completed layout. */
  function end() {
    setMoving(null);
    setTarget(null);
    lifted.current = null;
  }
  /** Update the visible insertion marker for the current drop target. */
  function hover(id, column, anchorId, edge) {
    const next = insertionTarget(layout, id, column, anchorId, edge);
    setTarget((previous) =>
      previous?.column === next?.column &&
      previous?.anchorId === next?.anchorId &&
      previous?.edge === next?.edge
        ? previous
        : next,
    );
    if (next)
      setAnnouncement(
        `${t(moduleName(id))}: ${edge === "before" ? t("oberhalb") : t("unterhalb")} ${anchorId ? t(moduleName(anchorId)) : t("am Spaltenende")}, ${t("Spalte")} ${column + 1}.`,
      );
  }
  /** Commit the currently resolved insertion target and finish the move. */
  function drop(column, anchorId = null, edge = "after", id = moving) {
    const destination = insertionTarget(layout, id, column, anchorId, edge);
    if (!destination) {
      end();
      return;
    }
    update((previous) => moveCard(previous, id, column, destination.beforeId));
    setAnnouncement(
      t("{module} in Spalte {n} abgelegt.")
        .replace("{module}", () => t(moduleName(id)))
        .replace("{n}", String(column + 1)),
    );
    focusGrip(id);
    end();
  }
  /** Resolve a client-space pointer against currently rendered columns and cards. */
  function pointerDestination(event, id) {
    const columns = [
      ...grid.current.querySelectorAll(":scope > .overview-column"),
    ];
    /** Check whether a pointer falls inside a measured client-space rectangle. */
    function inside(element) {
      const rect = element.getBoundingClientRect();
      return (
        event.clientX >= rect.left &&
        event.clientX <= rect.right &&
        event.clientY >= rect.top &&
        event.clientY <= rect.bottom
      );
    }
    /** Measure visible card rectangles used for between-card placement. */
    function cardRects(elements) {
      return elements.flatMap((element, column) =>
        [...element.children]
          .filter((card) => card.dataset.module && card.dataset.module !== id)
          .map((card) => {
            const { top, height } = card.getBoundingClientRect();
            return { id: card.dataset.module, top, height, column };
          }),
      );
    }
    if (getComputedStyle(grid.current).display === "flex") {
      if (!inside(grid.current)) return null;
      const rects = cardRects(columns);
      const anchor = gapAnchor(rects, event.clientY);
      const destination = rects.find((rect) => rect.id === anchor.anchorId);
      return destination ? { column: destination.column, ...anchor } : null;
    }
    const column = columns.findIndex(inside);
    if (column < 0) return null;
    return {
      column,
      ...gapAnchor(cardRects([columns[column]]), event.clientY),
    };
  }

  /** Bind pointer and keyboard move controls to one stable card identifier. */
  function gripEvents(id) {
    return {
      onPointerDown(event) {
        if (!event.isPrimary || event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        cardDrag.current = {
          id,
          x: event.clientX,
          y: event.clientY,
          active: false,
        };
      },
      onPointerMove(event) {
        const drag = cardDrag.current;
        if (drag?.id !== id) return;
        if (
          !drag.active &&
          Math.hypot(event.clientX - drag.x, event.clientY - drag.y) < 5
        )
          return;
        if (!drag.active) {
          drag.active = true;
          lift(id);
        }
        const next = pointerDestination(event, id);
        if (next) hover(id, next.column, next.anchorId, next.edge);
        else setTarget(null);
      },
      onPointerUp(event) {
        const drag = cardDrag.current;
        cardDrag.current = null;
        if (!drag?.active) return;
        const next = pointerDestination(event, id);
        if (next) drop(next.column, next.anchorId, next.edge, id);
        else end();
      },
      onPointerCancel() {
        cardDrag.current = null;
        end();
      },
    };
  }
  /** Resolve directional or confirmation keys without starting an unrelated native action. */
  function keyboardMove(event, id) {
    if (event.key === "Escape" && moving === id) {
      event.preventDefault();
      if (lifted.current) dispatch({ type: "restore", state: lifted.current });
      end();
      setAnnouncement(t("Verschieben abgebrochen."));
      focusGrip(id);
    } else if (
      moving === id &&
      ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key)
    ) {
      event.preventDefault();
      move(id, event.key.slice(5).toLowerCase());
    }
  }
  const widths = draftWidths ?? layout.widths;
  const columnCards = Array.from({ length: layout.columns }, (_, column) =>
    layout.cards.filter((card) => card.column === column),
  );
  const fitted = columnCards.map((cards) =>
    size && !size.stacked
      ? fitColumn(
          cards.map((card) =>
            heightPreview?.id === card.id
              ? { ...card, height: heightPreview.height, manualHeight: true }
              : card,
          ),
          size.height - 16,
        )
      : null,
  );
  const columnsStyle = widths
    .map(
      (value, i) =>
        `minmax(0,${value}fr)${i < widths.length - 1 ? " 10px" : ""}`,
    )
    .join(" ");
  return (
    <div
      className={`custom-overview ${editing ? "layout-editing" : ""} ${moving ? "layout-dragging" : ""}`}
    >
      <div className="overview-toolbar">
        <div className="overview-toolbar-title">
          <LayoutGrid size={15} />
          <b>{moving ? t(moduleName(moving)) : t("Dein Überblick")}</b>
          <span>
            {moving
              ? t("wird verschoben")
              : `${layout.cards.length} ${t("Module")}`}
          </span>
        </div>
        <div className="overview-toolbar-actions">
          {layoutPicker}
          {editing && manageLayout && (
            <details className="module-picker layout-manager">
              <summary>{t("Layouts verwalten")}</summary>
              <div>
                <b>{layoutLabel}</b>
                <label>
                  {t("Layoutname")}{" "}
                  <input
                    value={layoutName}
                    maxLength={50}
                    onChange={(event) => setLayoutName(event.target.value)}
                  />
                </label>
                <button type="button" onClick={() => saveLayout("copy")}>
                  {t("Als neues Layout speichern")}
                </button>
                <button type="button" onClick={() => saveLayout("rename")}>
                  {t("Aktuelles Layout umbenennen")}
                </button>
                <button
                  type="button"
                  disabled={isDefaultLayout}
                  onClick={() => saveLayout("default")}
                >
                  {t("Als Standard öffnen")}
                </button>
                <p role="status">{layoutMessage}</p>
              </div>
            </details>
          )}
          {editing && (
            <>
              <select
                className="column-count"
                aria-label={t("Anzahl Spalten")}
                value={layout.columns}
                onChange={(event) => {
                  end();
                  update((previous) =>
                    setColumns(previous, Number(event.target.value)),
                  );
                }}
              >
                {[2, 3, 4].map((n) => (
                  <option key={n} value={n}>
                    {n} {t("Spalten")}
                  </option>
                ))}
              </select>
              <details className="module-picker">
                <summary>
                  <Plus size={14} />
                  {t("Module")}
                </summary>
                <div>
                  <b>{t("Module im Überblick")}</b>
                  {availableModules.map(([id, label]) => (
                    <label key={id}>
                      <input
                        type="checkbox"
                        checked={layout.cards.some((card) => card.id === id)}
                        onChange={() => {
                          end();
                          update((previous) => toggleCard(previous, id));
                        }}
                      />
                      {t(label)}
                    </label>
                  ))}
                  <p>
                    {t(
                      "Ausblenden ändert keine Meetinginhalte. Notizen bleiben unten.",
                    )}
                  </p>
                  <button
                    className="text-button layout-reset"
                    onClick={() => {
                      end();
                      update(() => combineCards(presetLayout(variant)));
                    }}
                  >
                    <RotateCcw size={13} />
                    {t("Voreinstellung wiederherstellen")}
                  </button>
                </div>
              </details>
              <IconButton
                label={t("Letzte Layoutänderung rückgängig")}
                disabled={!state.past.length}
                onClick={() => {
                  end();
                  dispatch({ type: "undo" });
                }}
              >
                <Undo2 size={15} />
              </IconButton>
              <details className="layout-help">
                <summary aria-label={t("Hilfe zum Anordnen")}>
                  <CircleHelp size={16} />
                </summary>
                <div>
                  <b>{t("Deinen Überblick anordnen")}</b>
                  <p>
                    {t(
                      "Am Griff ziehen. Die blaue Linie zeigt den Einfügeplatz ober- oder unterhalb einer Karte.",
                    )}
                  </p>
                  <p>
                    {t(
                      "Spaltentrenner und Kartenunterkante verändern die Größe. Die unterste Karte füllt automatisch bis zu den Notizen. Tastatur: Griff aktivieren, Pfeiltasten, Enter zum Ablegen, Escape zum Abbrechen.",
                    )}
                  </p>
                </div>
              </details>
            </>
          )}
          <button
            className={`layout-toggle ${editing ? "primary-button" : "subtle-button"}`}
            onClick={() => {
              setEditing(!editing);
              end();
            }}
          >
            {editing ? <Check size={14} /> : <GripVertical size={14} />}
            {editing ? t("Fertig") : t("Anordnen")}
          </button>
        </div>
      </div>
      <p className="sr-only" id="layout-keyboard-help">
        {t(
          "Griff aktivieren, mit Pfeiltasten verschieben, Enter zum Ablegen, Escape zum Abbrechen.",
        )}
      </p>
      {blocked && (
        <p className="layout-storage-warning">
          {t(
            "Anordnung nur für diese Ansicht. Der frühere Browserstand bleibt unverändert.",
          )}
        </p>
      )}
      <p className="sr-only" role="status" aria-live="polite">
        {announcement}
      </p>
      <div
        className="custom-overview-grid"
        ref={grid}
        style={{ "--overview-columns": columnsStyle }}
      >
        {Array.from({ length: layout.columns }, (_, column) => (
          <Fragment key={column}>
            <div
              className={`overview-column ${target?.column === column ? "column-targeted" : ""}`}
              aria-label={`${t("Überblick Spalte")} ${column + 1}`}
              style={
                fitted[column]
                  ? {
                      gap: fitted[column].gap,
                      overflowY: fitted[column].scrollable ? "auto" : "hidden",
                    }
                  : undefined
              }
            >
              {columnCards[column].map((card, index) => (
                <OverviewCard
                  key={card.id}
                  renderedHeight={fitted[column]?.heights[index]}
                  hasNext={index < columnCards[column].length - 1}
                  autoFill={Boolean(
                    fitted[column] && index === columnCards[column].length - 1,
                  )}
                  onResizePreview={(height) =>
                    setHeightPreview(
                      height === null ? null : { id: card.id, height },
                    )
                  }
                  card={card}
                  editing={editing}
                  moving={moving}
                  target={target}
                  onLift={lift}
                  onMove={move}
                  gripEvents={gripEvents(card.id)}
                  onKeyMove={keyboardMove}
                  onExpand={onExpand}
                  onHide={(id) => {
                    end();
                    update((previous) => toggleCard(previous, id));
                  }}
                  availableModules={availableModules}
                  onReplace={(id, replacement) => {
                    end();
                    update((previous) =>
                      replaceCard(previous, id, replacement),
                    );
                  }}
                  onResize={(id, height) =>
                    update((previous) => ({
                      ...previous,
                      cards: previous.cards.map((card) =>
                        card.id === id
                          ? {
                              ...card,
                              height: clampManualHeight(height),
                              manualHeight: true,
                            }
                          : card,
                      ),
                    }))
                  }
                >
                  {renderPanel(card.id)}
                </OverviewCard>
              ))}
              {editing &&
                moving &&
                !layout.cards.some(
                  (card) => card.column === column && card.id !== moving,
                ) && (
                  <button
                    className="column-drop-zone drop-active"
                    onClick={() => drop(column)}
                  >
                    {t("Hier in Spalte")}
                    {column + 1} {t("ablegen")}
                  </button>
                )}
            </div>
            {column < layout.columns - 1 && (
              <div
                className="column-resize"
                role="separator"
                aria-label={`${t("Spalten")} ${column + 1} ${t("und")} ${column + 2} ${t("Breite ändern")}`}
                aria-orientation="vertical"
                tabIndex={0}
                aria-valuenow={Math.round(widths[column])}
                aria-valuemin={20}
                aria-valuemax={Math.round(
                  widths[column] + widths[column + 1] - 20,
                )}
                onPointerDown={(event) => {
                  event.preventDefault();
                  event.currentTarget.setPointerCapture(event.pointerId);
                  resize.current = {
                    x: event.clientX,
                    layout,
                    width: Math.max(
                      1,
                      grid.current.clientWidth - (layout.columns - 1) * 10,
                    ),
                    final: layout.widths,
                  };
                }}
                onPointerMove={(event) => {
                  if (!resize.current) return;
                  const next = resizeColumns(
                    resize.current.layout,
                    column,
                    ((event.clientX - resize.current.x) /
                      resize.current.width) *
                      100,
                  );
                  resize.current.final = next.widths;
                  setDraftWidths(next.widths);
                }}
                onPointerUp={() => {
                  if (resize.current) {
                    const next = resize.current.final;
                    update((previous) => ({ ...previous, widths: next }));
                  }
                  resize.current = null;
                  setDraftWidths(null);
                }}
                onPointerCancel={() => {
                  resize.current = null;
                  setDraftWidths(null);
                }}
                onKeyDown={(event) => {
                  if (["ArrowLeft", "ArrowRight"].includes(event.key)) {
                    event.preventDefault();
                    update((previous) =>
                      resizeColumns(
                        previous,
                        column,
                        (event.key === "ArrowLeft" ? -1 : 1) *
                          (event.shiftKey ? 10 : 2),
                      ),
                    );
                  }
                }}
              >
                <span />
              </div>
            )}
          </Fragment>
        ))}
      </div>
    </div>
  );
}
