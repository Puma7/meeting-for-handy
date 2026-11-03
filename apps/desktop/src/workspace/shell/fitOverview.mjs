// Stored heights remain preferences. Only the rendered column is fitted to its viewport.
export function fitColumn(cards, availableHeight, requestedGap = 12) {
  if (!cards.length) return { heights: [], gap: requestedGap };
  const viewport = Math.max(
    0,
    Number.isFinite(availableHeight) ? availableHeight : 0,
  );
  // Keep the expand button reachable when a very short viewport cannot fit all headers.
  const minimumTotal = cards.length * 72 + requestedGap * (cards.length - 1);
  const available = Math.max(viewport, minimumTotal);
  const gap = requestedGap;
  const room = Math.max(0, available - gap * (cards.length - 1));
  const manual = cards.slice(0, -1).some((card) => card.manualHeight);
  const minimum = Math.min(manual ? 72 : 120, room / cards.length);
  const upper = cards
    .slice(0, -1)
    .map((card) => Math.max(minimum, card.height));
  const budget = Math.max(0, room - minimum * cards.length);
  const requested = upper.map((height) => height - minimum);
  const manualTotal = requested.reduce(
    (sum, extra, index) => sum + (cards[index].manualHeight ? extra : 0),
    0,
  );
  const autoTotal = requested.reduce(
    (sum, extra, index) => sum + (cards[index].manualHeight ? 0 : extra),
    0,
  );
  const manualScale = manualTotal ? Math.min(1, budget / manualTotal) : 1;
  const autoScale = autoTotal
    ? Math.min(1, Math.max(0, budget - manualTotal) / autoTotal)
    : 1;
  const heights = requested.map(
    (extra, index) =>
      minimum + extra * (cards[index].manualHeight ? manualScale : autoScale),
  );
  heights.push(
    Math.max(0, room - heights.reduce((sum, height) => sum + height, 0)),
  );
  return { heights, gap, scrollable: minimumTotal > viewport };
}
