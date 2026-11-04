import { useState } from "react";

// A card and its full tab share view settings; meeting data stays in the existing
// separately versioned state. Local fallback keeps the component reusable.
/** Restore independent per-panel controls and persist user changes without overwriting invalid caches. */
export function usePanelSettings(ctx, panel, initial) {
  const [fallback, setFallback] = useState(initial);
  const value = ctx?.panelViews?.[panel] ?? fallback;
  function patch(update) {
    if (ctx?.setPanelView) ctx.setPanelView(panel, update);
    else setFallback((previous) => ({ ...previous, ...update }));
  }
  return [value, patch];
}
