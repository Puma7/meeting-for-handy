export function fitColumn(
  cards: readonly { height: number; manualHeight?: boolean }[],
  availableHeight: number,
  requestedGap?: number,
): { heights: number[]; gap: number; scrollable?: boolean };
