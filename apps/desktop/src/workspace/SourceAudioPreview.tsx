import { useWorkspaceText, workspaceText } from "./shell/workspaceLocale.jsx";
import { useEffect, useRef, useState } from "react";

type PreviewState =
  | { status: "idle" | "loading" | "unavailable" }
  | { status: "ready"; bytes: ArrayBuffer };

type PreviewProps = {
  sourceId: string;
  runId?: string;
  load: (sourceId: string, runId?: string) => Promise<ArrayBuffer>;
  loadOnOpen?: boolean;
  locale?: "de" | "en";
};

/** Reset the complete preview lifetime when its source or analysis run changes. */
export default function SourceAudioPreview(props: PreviewProps) {
  return (
    <FileAudioPreview
      key={`${props.runId ?? ""}:${props.sourceId}`}
      {...props}
    />
  );
}

/** Request one bounded verified excerpt on demand and ignore stale results without cancelling transport. */
function FileAudioPreview({
  sourceId,
  runId,
  load,
  loadOnOpen,
  locale,
}: PreviewProps) {
  const translate = useWorkspaceText();
  const t = locale
    ? (value: string) => workspaceText(locale, value)
    : translate;
  const [preview, setPreview] = useState<PreviewState>({
    status: loadOnOpen ? "loading" : "idle",
  });
  const [url, setUrl] = useState<string | null>(null);
  const alive = useRef(true);
  const initialRequest = useRef<Promise<ArrayBuffer> | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  useEffect(() => {
    if (!loadOnOpen) return;
    let current = true;
    // Reuse the initial request across StrictMode's effect replay.
    initialRequest.current ??= runId ? load(sourceId, runId) : load(sourceId);
    void initialRequest.current.then(
      (bytes) => {
        if (current) setPreview({ status: "ready", bytes });
      },
      () => {
        if (current) setPreview({ status: "unavailable" });
      },
    );
    return () => {
      current = false;
    };
  }, [sourceId, runId, load, loadOnOpen]);

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

  if (url && preview.status === "ready")
    return (
      <audio
        controls
        preload="metadata"
        aria-label={t("Audiovorschau des Originalabschnitts")}
        src={url}
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
            const bytes = runId
              ? await load(sourceId, runId)
              : await load(sourceId);
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
