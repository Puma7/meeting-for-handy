export function formatMeetingDates(value: string, language?: string): string;
export function formatMeetingDue(
  value: string,
  language?: string,
  quotes?: readonly string[],
): string;
export function formatMeetingDue(
  value: null,
  language?: string,
  quotes?: readonly string[],
): null;
export interface HistoryFrame {
  id: string;
  entityId: string;
  revision: number;
  atMs: number;
  text: string;
  human?: boolean;
}
export function eventAdditions(
  events: readonly HistoryFrame[],
): Map<string, { text: string; isAddition: boolean }>;
