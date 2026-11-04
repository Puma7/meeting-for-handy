import { useWorkspaceText } from "./shell/workspaceLocale.jsx";
import { useEffect, useState } from "react";

type PreviewState =
  | { status: "idle" | "loading" | "unavailable" }
  | { status: "ready"; bytes: ArrayBuffer };

/** Request a bounded verified audio excerpt for one original source, cancelling stale preview results. */
export default function SourceAudioPreview({
  file,
  load,
  offsetSeconds,
}: {
  file: string;
  load: (file: string) => Promise<ArrayBuffer>;
  offsetSeconds: number;
}) {
  const t = useWorkspaceText();
  const [preview, setPreview] = useState<PreviewState>({ status: "idle" });
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    if (preview.status !== "ready") return;
    const next = URL.createObjectURL(
      new Blob([preview.bytes], { type: "audio/wav" }),
    );
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [preview]);

  if (offsetSeconds >= 30)
    return (
      <p>
        {t("Diese Stelle liegt hinter der Vorschau der ersten 30 Sekunden.")}
      </p>
    );

  if (url && preview.status === "ready")
    return (
      <audio
        controls
        preload="metadata"
        aria-label={t("Audiovorschau des Originalabschnitts")}
        src={url}
        onLoadedMetadata={({ currentTarget: audio }) => {
          if (offsetSeconds < audio.duration) audio.currentTime = offsetSeconds;
        }}
        onError={() => setPreview({ status: "unavailable" })}
      />
    );

  return (
    <div>
      <button
        type="button"
        disabled={preview.status === "loading"}
        onClick={async () => {
          setPreview({ status: "loading" });
          try {
            setPreview({ status: "ready", bytes: await load(file) });
          } catch {
            setPreview({ status: "unavailable" });
          }
        }}
      >
        {preview.status === "loading"
          ? "Audio wird geladen …"
          : t("Audiovorschau laden")}
      </button>
      {preview.status === "unavailable" && (
        <p role="status">
          Für diesen Abschnitt ist keine lokale Audiovorschau verfügbar.
          Archiviertes Audio kann nach dem Wiederherstellen erneut geprüft
          werden.
        </p>
      )}
    </div>
  );
}
