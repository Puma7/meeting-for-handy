import { Check, Copy, Download, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "../api";
import type {
  DesktopApi,
  LocalModelSetupInfo,
  Locale,
  ModelPullProgress,
} from "../types";

type Props = {
  api: DesktopApi;
  locale: Locale;
  connected: boolean;
  disabled?: boolean;
  /** Applies a detected local server and model to the text model settings. */
  onUse: (endpoint: string, model: string) => void;
};

const GIB = 1024 * 1024 * 1024;

/**
 * One-click local text models: finds Ollama and LM Studio on this computer, suggests a model
 * size from installed memory and can ask a local Ollama to download it. Nothing is
 * downloaded or installed without a click.
 */
export default function LocalModelSetup({
  api,
  locale,
  connected,
  disabled = false,
  onUse,
}: Props) {
  const de = locale === "de";
  const [info, setInfo] = useState<LocalModelSetupInfo | null>(null),
    [loading, setLoading] = useState(false),
    [error, setError] = useState<string | null>(null),
    [pull, setPull] = useState<ModelPullProgress | null>(null),
    [copied, setCopied] = useState(false);
  const mounted = useRef(true);
  useEffect(() => {
    // Restored on every setup: StrictMode runs setup, cleanup and setup again.
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const check = useCallback(async () => {
    if (!api.localModelSetup || !connected) return;
    setLoading(true);
    setError(null);
    try {
      const next = await api.localModelSetup();
      if (mounted.current) setInfo(next);
    } catch (e) {
      if (mounted.current) setError(errorMessage(e));
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [api, connected]);
  useEffect(() => {
    void check();
  }, [check]);
  // A download keeps running in the app when this view closes; reopening reconnects to it.
  useEffect(() => {
    if (!api.getModelPullProgress || !connected) return;
    void api
      .getModelPullProgress()
      .then((current) => {
        if (mounted.current && current) setPull(current);
      })
      .catch((e) => {
        if (mounted.current) setError(errorMessage(e));
      });
  }, [api, connected]);
  // Follow a running download; a finished one refreshes the model lists.
  useEffect(() => {
    if (pull?.status !== "running" || !api.getModelPullProgress) return;
    const timer = setInterval(() => {
      void api.getModelPullProgress?.().then((next) => {
        if (!mounted.current || !next) return;
        setPull(next);
        if (next.status === "complete") void check();
      });
    }, 1000);
    return () => clearInterval(timer);
  }, [pull?.status, api, check]);

  const recommended = info?.recommendation.model ?? null;
  const ollama = info?.providers.find((p) => p.id === "ollama");
  const lmStudio = info?.providers.find((p) => p.id === "lmstudio");
  const memory =
    info?.totalMemoryBytes != null
      ? `${Math.round(info.totalMemoryBytes / GIB)} GB`
      : de
        ? "unbekannt"
        : "unknown";
  const command = recommended ? `ollama pull ${recommended}` : "";

  /** Requests an explicitly selected model download only while setup actions are unlocked. */
  async function startPull() {
    if (disabled || !recommended || !api.pullOllamaModel) return;
    setError(null);
    try {
      setPull(await api.pullOllamaModel(recommended));
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  return (
    <div className="local-model-setup">
      <div className="local-model-heading">
        <strong>
          {de ? "Lokales Textmodell einrichten" : "Set up a local text model"}
        </strong>
        <button
          className="icon-button bordered"
          title={de ? "Erneut prüfen" : "Check again"}
          aria-label={de ? "Erneut prüfen" : "Check again"}
          disabled={loading || !connected}
          onClick={() => void check()}
        >
          <RefreshCw size={15} className={loading ? "spinning" : undefined} />
        </button>
      </div>
      {info && (
        <p className="muted">
          {de ? "Arbeitsspeicher" : "Memory"}: {memory} ·{" "}
          {info.recommendation.tier === "none"
            ? de
              ? "Für eine lokale Live-Zusammenfassung zu wenig. Besser nach dem Meeting auswerten oder einen Firmen-Endpunkt nutzen."
              : "Too little for a local live summary. Analyse after the meeting or use a company endpoint instead."
            : `${de ? "Empfehlung" : "Suggested"}: ${recommended}${
                info.recommendation.tier === "small"
                  ? de
                    ? " (klein; kann während der Aufnahme langsam sein)"
                    : " (small; may be slow while recording)"
                  : info.recommendation.tier === "large"
                    ? de
                      ? " (größere Modelle nur bewusst wählen)"
                      : " (choose larger models deliberately)"
                    : ""
              }`}
        </p>
      )}
      {error && (
        <p className="notice warning" role="alert">
          {error}
        </p>
      )}
      {info &&
        [ollama, lmStudio].map(
          (provider) =>
            provider && (
              <div className="local-provider" key={provider.id}>
                <div className="local-provider-title">
                  <strong>{provider.name}</strong>
                  <span
                    className={provider.reachable ? "chip ok" : "chip"}
                    role="status"
                  >
                    {provider.reachable
                      ? de
                        ? "läuft"
                        : "running"
                      : de
                        ? "nicht gefunden"
                        : "not found"}
                  </span>
                </div>
                {provider.id === "lmstudio" &&
                  Boolean(provider.loadedModels?.length) && (
                    <div className="local-provider-loaded">
                      <ul
                        aria-label={de ? "Geladene Modelle" : "Loaded models"}
                      >
                        {provider.loadedModels?.map(
                          ({ model, contextLength }) => (
                            <li key={model}>
                              {model} · {de ? "geladen" : "loaded"}
                              {contextLength !== null && (
                                <>
                                  {" "}
                                  · {contextLength.toLocaleString(locale)}{" "}
                                  {de ? "Tokens Kontext" : "context tokens"}
                                </>
                              )}
                            </li>
                          ),
                        )}
                      </ul>
                      <p className="muted">
                        {de
                          ? "Für eine neue HandyMeeting-Auswertung: Falls das Modell gerade nicht gebraucht wird, in LM Studio entladen und den Server laufen lassen."
                          : "For a new HandyMeeting analysis, unload the model in LM Studio when it is idle and leave the server running."}
                      </p>
                    </div>
                  )}
                {provider.reachable && provider.models.length > 0 && (
                  <div className="local-provider-models">
                    {(recommended && provider.recommendedInstalled
                      ? [
                          provider.models.find(
                            (m) =>
                              m === recommended ||
                              m === `${recommended}:latest`,
                          ) ?? recommended,
                        ]
                      : provider.models.slice(0, 6)
                    ).map((model) => (
                      <button
                        key={model}
                        title={
                          de
                            ? "Diesen Dienst und dieses Modell für die Textauswertung auswählen."
                            : "Select this service and model for text analysis."
                        }
                        className="secondary"
                        disabled={disabled}
                        onClick={() => {
                          if (!disabled) onUse(provider.endpoint, model);
                        }}
                      >
                        <Check size={14} />
                        {de ? "Verwenden" : "Use"}: {model}
                      </button>
                    ))}
                  </div>
                )}
                {provider.id === "ollama" &&
                  recommended &&
                  !provider.recommendedInstalled && (
                    <div className="local-provider-action">
                      {provider.reachable ? (
                        pull?.status === "running" ? (
                          <>
                            <span role="status">
                              {de ? "Lädt" : "Downloading"} {pull.model}
                              {pull.total > 0
                                ? ` · ${Math.floor((pull.completed / pull.total) * 100)} %`
                                : ` · ${pull.detail}`}
                            </span>
                            <button
                              className="secondary"
                              onClick={() => void api.cancelModelPull?.()}
                              title={
                                de
                                  ? "Den laufenden Modell-Download abbrechen."
                                  : "Cancel the current model download."
                              }
                            >
                              <X size={14} />
                              {de ? "Abbrechen" : "Cancel"}
                            </button>
                          </>
                        ) : (
                          <button
                            className="primary"
                            disabled={disabled || !api.pullOllamaModel}
                            onClick={() => void startPull()}
                            title={
                              de
                                ? "Dieses Modell über Ollama herunterladen."
                                : "Download this model through Ollama."
                            }
                          >
                            <Download size={14} />
                            {de
                              ? `${recommended} in Ollama laden`
                              : `Download ${recommended} in Ollama`}
                          </button>
                        )
                      ) : (
                        <p className="muted">
                          {de
                            ? "Ollama installieren (ollama.com/download), starten und dann im Terminal ausführen:"
                            : "Install Ollama (ollama.com/download), start it and run in a terminal:"}{" "}
                          <code>{command}</code>{" "}
                          <button
                            className="icon-button"
                            title={de ? "Befehl kopieren" : "Copy command"}
                            aria-label={de ? "Befehl kopieren" : "Copy command"}
                            onClick={() =>
                              void api.copyText(command).then(() => {
                                if (mounted.current) setCopied(true);
                              })
                            }
                          >
                            {copied ? <Check size={14} /> : <Copy size={14} />}
                          </button>
                        </p>
                      )}
                      {pull?.status === "failed" && (
                        <p className="notice warning" role="alert">
                          {pull.error}
                        </p>
                      )}
                    </div>
                  )}
                {provider.id === "lmstudio" && !provider.reachable && (
                  <p className="muted">
                    {de
                      ? `LM Studio öffnen, das Modell „${info.recommendation.lmStudioSearch ?? "Gemma 4 E4B"}“ herunterladen und unter „Developer“ den lokalen Server starten (Port 1234). Ein geöffnetes LM Studio allein ist noch nicht erreichbar.`
                      : `Open LM Studio, download "${info.recommendation.lmStudioSearch ?? "Gemma 4 E4B"}" and start the local server under "Developer" (port 1234). An open LM Studio window alone is not reachable yet.`}
                  </p>
                )}
                {provider.id === "lmstudio" &&
                  provider.reachable &&
                  !provider.models.length && (
                    <p className="muted">
                      {de
                        ? `LM Studio läuft, bietet aber noch kein Modell an. Das Modell „${info.recommendation.lmStudioSearch ?? "Gemma 4 E4B"}“ in LM Studio herunterladen und laden, dann erneut prüfen.`
                        : `LM Studio is running but offers no model yet. Download and load "${info.recommendation.lmStudioSearch ?? "Gemma 4 E4B"}" in LM Studio, then check again.`}
                    </p>
                  )}
              </div>
            ),
        )}
    </div>
  );
}
