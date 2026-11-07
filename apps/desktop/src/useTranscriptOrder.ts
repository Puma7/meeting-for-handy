import { useState } from "react";

const ORDER_KEY = "handymeeting.transcriptOrder";

/** Both meeting views use the same display preference. Stored text is never reordered. */
export function useTranscriptOrder() {
  const [newestFirst, setNewestFirst] = useState(() => {
    try {
      return localStorage.getItem(ORDER_KEY) !== "oldest";
    } catch {
      return true;
    }
  });

  function chooseOrder(newest: boolean) {
    setNewestFirst(newest);
    try {
      localStorage.setItem(ORDER_KEY, newest ? "newest" : "oldest");
    } catch {
      // The current view still reflects the choice if storage is unavailable.
    }
  }

  return [newestFirst, chooseOrder] as const;
}
