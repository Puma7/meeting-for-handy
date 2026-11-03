export const modules = [
  ["transcript", "Transkript"],
  ["summary", "Live-Überblick"],
  ["details", "Wichtige Details"],
  ["changes", "Gesprächsverlauf"],
  ["topics", "Vorbereitung"],
  ["impulses", "Gesprächsimpulse"],
  ["relevant", "Jetzt relevant"],
];
export const moduleName = (id) =>
  modules.find((item) => item[0] === id)?.[1] || "";
export const clampHeight = (height) =>
  Math.max(180, Math.min(1200, Math.round(height / 12) * 12));
export const clampManualHeight = (height) =>
  Math.max(72, Math.min(1200, Math.round(height)));
export function presetLayout(variant) {
  const rows =
    variant === "verlauf"
      ? [
          ["relevant", 0, 192],
          ["changes", 0, 492],
          ["summary", 1, 288],
          ["transcript", 1, 288],
          ["topics", 1, 312],
          ["impulses", 1, 288],
        ]
      : [
          ["transcript", 0, 636],
          ["summary", 1, 288],
          ["details", 1, 180],
          ["changes", 1, 180],
          ["topics", 2, 312],
          ["impulses", 2, 288],
        ];
  const columns = variant === "verlauf" ? 2 : 3;
  return {
    version: 1,
    columns,
    widths: Array(columns).fill(100 / columns),
    cards: rows.map(([id, column, height]) => ({ id, column, height })),
  };
}
export function decodeLayout(raw, variant) {
  const fallback = presetLayout(variant);
  if (raw === null) return { layout: fallback, blocked: false };
  try {
    if (typeof raw !== "string" || raw.length > 20000) throw Error();
    const value = JSON.parse(raw);
    if (
      value.version !== 1 ||
      ![2, 3, 4].includes(value.columns) ||
      !Array.isArray(value.cards) ||
      value.cards.length > modules.length ||
      !Array.isArray(value.widths) ||
      value.widths.length !== value.columns
    )
      throw Error();
    if (
      value.widths.some((n) => !Number.isFinite(n) || n < 20 || n > 80) ||
      Math.abs(value.widths.reduce((a, b) => a + b, 0) - 100) > 0.01
    )
      throw Error();
    const seen = new Set();
    const cards = value.cards.map((card) => {
      if (
        !modules.some(([id]) => id === card.id) ||
        seen.has(card.id) ||
        !Number.isInteger(card.column) ||
        card.column < 0 ||
        card.column >= value.columns ||
        !Number.isInteger(card.height) ||
        card.height < (card.manualHeight === true ? 72 : 180) ||
        card.height > 1200
      )
        throw Error();
      seen.add(card.id);
      return {
        id: card.id,
        column: card.column,
        height: card.height,
        ...(card.manualHeight === true ? { manualHeight: true } : {}),
      };
    });
    return {
      layout: {
        version: 1,
        columns: value.columns,
        widths: [...value.widths],
        cards,
      },
      blocked: false,
    };
  } catch {
    return { layout: fallback, blocked: true };
  }
}
export function moveCard(layout, id, column, beforeId = null) {
  const card = layout.cards.find((item) => item.id === id);
  if (
    !card ||
    !Number.isInteger(column) ||
    column < 0 ||
    column >= layout.columns ||
    id === beforeId
  )
    return layout;
  const cards = layout.cards.filter((item) => item.id !== id);
  let index = cards.findIndex(
    (item) => item.id === beforeId && item.column === column,
  );
  if (index < 0) {
    index = cards.map((item) => item.column).lastIndexOf(column);
    index = index < 0 ? cards.length : index + 1;
  }
  cards.splice(index, 0, { ...card, column });
  return { ...layout, cards };
}
export function stepCard(layout, id, direction) {
  const card = layout.cards.find((item) => item.id === id);
  if (!card) return layout;
  if (direction === "left" || direction === "right")
    return moveCard(layout, id, card.column + (direction === "left" ? -1 : 1));
  const column = layout.cards.filter((item) => item.column === card.column);
  const index = column.findIndex((item) => item.id === id);
  if (direction === "up")
    return index > 0
      ? moveCard(layout, id, card.column, column[index - 1].id)
      : layout;
  return index < column.length - 1
    ? moveCard(layout, id, card.column, column[index + 2]?.id)
    : layout;
}
export function setColumns(layout, columns) {
  if (![2, 3, 4].includes(columns) || columns === layout.columns) return layout;
  return {
    ...layout,
    columns,
    widths: Array(columns).fill(100 / columns),
    cards: layout.cards.map((card) => ({
      ...card,
      column: Math.min(card.column, columns - 1),
    })),
  };
}
// Resolve the visible gap against a list excluding the dragged card. This avoids
// shifting the destination index when moving down within the same column.
export function insertionTarget(
  layout,
  movingId,
  column,
  anchorId = null,
  edge = "after",
) {
  if (
    !layout.cards.some((card) => card.id === movingId) ||
    column < 0 ||
    column >= layout.columns ||
    anchorId === movingId
  )
    return null;
  const cards = layout.cards.filter(
    (card) => card.column === column && card.id !== movingId,
  );
  if (anchorId === null)
    return {
      column,
      beforeId: null,
      anchorId: cards.at(-1)?.id ?? null,
      edge: "after",
    };
  const index = cards.findIndex((card) => card.id === anchorId);
  if (index < 0) return null;
  return {
    column,
    beforeId: edge === "before" ? anchorId : (cards[index + 1]?.id ?? null),
    anchorId,
    edge,
  };
}
export function resizeColumns(layout, boundary, delta) {
  if (boundary < 0 || boundary >= layout.columns - 1 || !Number.isFinite(delta))
    return layout;
  const widths = [...layout.widths];
  const total = widths[boundary] + widths[boundary + 1];
  widths[boundary] = Math.max(
    20,
    Math.min(total - 20, widths[boundary] + delta),
  );
  widths[boundary + 1] = total - widths[boundary];
  return { ...layout, widths };
}
export function toggleCard(layout, id) {
  if (!modules.some(([key]) => key === id)) return layout;
  if (layout.cards.some((card) => card.id === id))
    return { ...layout, cards: layout.cards.filter((card) => card.id !== id) };
  const heights = Array.from({ length: layout.columns }, (_, i) =>
    layout.cards
      .filter((card) => card.column === i)
      .reduce((sum, card) => sum + card.height, 0),
  );
  const column = heights.indexOf(Math.min(...heights));
  return {
    ...layout,
    cards: [
      ...layout.cards,
      { id, column, height: id === "relevant" ? 192 : 312 },
    ],
  };
}
export function replaceCard(layout, id, replacement) {
  if (id === replacement || !modules.some(([key]) => key === replacement))
    return layout;
  const index = layout.cards.findIndex((card) => card.id === id);
  if (index < 0) return layout;
  const otherIndex = layout.cards.findIndex((card) => card.id === replacement);
  const cards = [...layout.cards];
  if (otherIndex < 0) cards[index] = { ...cards[index], id: replacement };
  else {
    const first = cards[index],
      second = cards[otherIndex];
    cards[index] = { ...second, column: first.column };
    cards[otherIndex] = { ...first, column: second.column };
  }
  return { ...layout, cards };
}

export function gapAnchor(rects, y) {
  const next = rects.find((rect) => y < rect.top + rect.height / 2);
  return next
    ? { anchorId: next.id, edge: "before" }
    : { anchorId: rects.at(-1)?.id ?? null, edge: "after" };
}
