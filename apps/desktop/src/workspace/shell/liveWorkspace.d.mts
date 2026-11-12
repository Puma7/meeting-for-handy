import type { LiveNotesStatus, Locale } from "../../types";

export function workspaceStatusText(
  status: LiveNotesStatus | null | undefined,
  language?: Locale,
): string;
