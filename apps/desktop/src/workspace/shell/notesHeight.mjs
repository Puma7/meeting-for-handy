export const defaultNotesHeight = 114;
export function notesHeightLimit(viewportHeight, availableHeight = Infinity) {
  return Math.max(
    96,
    Math.min(
      640,
      Math.floor(viewportHeight * 0.6),
      Math.floor(availableHeight),
    ),
  );
}
export function clampNotesHeight(
  height,
  viewportHeight,
  availableHeight = Infinity,
) {
  return Math.max(
    96,
    Math.min(
      notesHeightLimit(viewportHeight, availableHeight),
      Math.round(height),
    ),
  );
}
export function draggedNotesHeight(
  startHeight,
  startY,
  currentY,
  viewportHeight,
  availableHeight = Infinity,
) {
  return clampNotesHeight(
    startHeight + startY - currentY,
    viewportHeight,
    availableHeight,
  );
}
export function decodeNotesHeight(raw) {
  if (raw === null) return { height: defaultNotesHeight, blocked: false };
  try {
    if (typeof raw !== "string" || raw.length > 256) throw Error();
    const value = JSON.parse(raw);
    if (
      value.version !== 1 ||
      !Number.isInteger(value.height) ||
      value.height < 96 ||
      value.height > 640
    )
      throw Error();
    return { height: value.height, blocked: false };
  } catch {
    return { height: defaultNotesHeight, blocked: true };
  }
}

export function resizedNotesPreference(
  preferredHeight,
  renderedHeight,
  requestedHeight,
) {
  return requestedHeight === renderedHeight ? preferredHeight : requestedHeight;
}
