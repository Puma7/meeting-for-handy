import { useWorkspaceText } from "./shell/workspaceLocale.jsx";
import { useEffect, useRef, useState } from "react";

type PreviewState =
  | { status: "idle" | "loading" | "unavailable" }
  | { status: "ready"; bytes: ArrayBuffer };

type PreviewProps = {
  file: string;
  load: (file: string) => Promise<ArrayBuffer>;
  offsetSeconds: number;
};

/** Reset the complete preview lifetime when its source file changes. */
export default function SourceAudioPreview(props: PreviewProps) {
  return <FileAudioPreview key={props.file} {...props} />;
}

/** Request one bounded verified excerpt on demand and ignore stale results without cancelling transport. */
function FileAudioPreview({ file, load, offsetSeconds }: PreviewProps) {
  const t = useWorkspaceText();
  const [preview, setPreview] = useState<PreviewState>({ status: "idle" });
  const [url, setUrl] = useState<string | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  useEffect(() => {
    if (preview.status !== "ready") {
      setUrl(null);
      return;
    }
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
          const requestedFile = file;
          setPreview({ status: "loading" });
          try {
            const bytes = await load(requestedFile);
            if (alive.current) setPreview({ status: "ready", bytes });
          } catch {
            if (alive.current) setPreview({ status: "unavailable" });
          }
        }}
      >
        {preview.status === "loading"
          ? t("Audio wird geladen …")
          : t("Audiovorschau laden")}
      </button>
      {preview.status === "unavailable" && (
        <p role="status">
          {t(
            "Für diesen Abschnitt ist keine lokale Audiovorschau verfügbar. Archiviertes Audio kann nach dem Wiederherstellen erneut geprüft werden.",
          )}
        </p>
      )}
    </div>
  );
}
