/** Scope renderer preferences to one meeting without exposing its title or filesystem path. */
export const workspaceStorageKey = (meetingId: string) =>
  `handymeeting.workspace.v1.${meetingId}`;
