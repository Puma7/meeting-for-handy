import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import type { DesktopApi, Locale } from "../types";
import type { MessageKey } from "../i18n";
import CredentialPanel from "./CredentialPanel";
import LocalModelSetup from "./LocalModelSetup";

/** One shared connection form for settings, live notes and post-meeting work. No run starts here. */
export default function TextModelSettings({
  api,
  t,
  locale,
  connected,
  disabled,
  mode,
  setMode,
  endpoint,
  setEndpoint,
  textModel,
  setTextModel,
  apiKey,
  setApiKey,
  useSavedKey,
  setUseSavedKey,
  localModels,
  modelsLoading,
  modelsLoaded,
  modelsError,
  refreshModels,
  manualModel,
  setManualModel,
}: {
  api: DesktopApi;
  t: Record<MessageKey, string>;
  locale: Locale;
  connected: boolean;
  disabled: boolean;
  mode: "local" | "cloud";
  setMode: (mode: "local" | "cloud") => void;
  endpoint: string;
  setEndpoint: (value: string) => void;
  textModel: string;
  setTextModel: (value: string) => void;
  apiKey: string;
  setApiKey: (value: string) => void;
  useSavedKey: boolean;
  setUseSavedKey: (value: boolean) => void;
  localModels: string[];
  modelsLoading: boolean;
  modelsLoaded: boolean;
  modelsError: string | null;
  refreshModels: () => void;
  manualModel: boolean;
  setManualModel: (value: boolean) => void;
}) {
  const de = locale === "de";
  const [discoveryOpen, setDiscoveryOpen] = useState(false);
  const [usedServer, setUsedServer] = useState(0);
  useEffect(() => {
    if (!usedServer) return;
    const timer = setTimeout(refreshModels, 0);
    return () => clearTimeout(timer);
    // Refresh after the parent commits the newly chosen route, including the same address.
  }, [usedServer]);
  return (
    <section
      className="text-model-settings"
      aria-label={de ? "Textmodell-Verbindung" : "Text model connection"}
    >
      <h3>{de ? "Verbindung und Textmodell" : "Connection and text model"}</h3>
      <p className="muted">
        {de
          ? "Für Live-Zusammenfassung und Nachbearbeitung."
          : "For live summaries and post-meeting analysis."}
      </p>
      <div className="settings-grid">
        <label>
          {t.processing}
          <select
            value={mode}
            disabled={disabled}
            onChange={(e) => setMode(e.target.value as "local" | "cloud")}
          >
            <option value="local">{t.local}</option>
            <option value="cloud">{t.cloud}</option>
          </select>
        </label>
        <label>
          {mode === "local" ? t.localEndpoint : t.endpoint}
          <input
            type="url"
            value={endpoint}
            disabled={disabled}
            onChange={(e) => setEndpoint(e.target.value)}
            autoComplete="off"
          />
        </label>
        <label className="text-model-choice">
          {t.textModel}
          {mode === "local" && !manualModel ? (
            <div className="inline-control">
              <select
                aria-label={t.textModel}
                value={textModel}
                disabled={disabled || modelsLoading}
                onChange={(e) => setTextModel(e.target.value)}
              >
                <option value="">
                  {modelsLoading ? t.loadingTextModels : t.selectTextModel}
                </option>
                {textModel && !localModels.includes(textModel) && (
                  <option value={textModel} disabled>
                    {textModel} · {t.notFound}
                  </option>
                )}
                {localModels.map((model) => (
                  <option key={model} value={model}>
                    {model}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="icon-button bordered"
                onClick={refreshModels}
                disabled={disabled || modelsLoading || !connected}
                title={t.refreshTextModels}
                aria-label={t.refreshTextModels}
              >
                <RefreshCw
                  size={17}
                  className={modelsLoading ? "spinning" : undefined}
                />
              </button>
            </div>
          ) : (
            <input
              aria-label={t.textModel}
              value={textModel}
              disabled={disabled}
              onChange={(e) => setTextModel(e.target.value)}
              autoComplete="off"
            />
          )}
        </label>
      </div>
      {mode === "local" && (
        <>
          <div className="model-options-row">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={manualModel}
                disabled={disabled}
                onChange={(e) => setManualModel(e.target.checked)}
              />
              {t.manualTextModel}
            </label>
            <button
              type="button"
              className="text-button"
              disabled={disabled || !connected}
              aria-expanded={discoveryOpen}
              onClick={() => setDiscoveryOpen((open) => !open)}
            >
              {de ? "Ollama / LM Studio erkennen" : "Detect Ollama / LM Studio"}
            </button>
            <details className="local-service-help">
              <summary>{t.setupHelp}</summary>
              <p>{t.localServiceHelp}</p>
              <p>{t.firstModelLoad}</p>
            </details>
          </div>
          {modelsError && (
            <p className="notice warning" role="status">
              {modelsError}{" "}
              {de
                ? "Starte deinen lokalen Modelldienst und aktualisiere die Liste. Einrichtungshilfe: „Ollama / LM Studio erkennen“."
                : "Start your local model server and refresh the list. For setup, choose “Detect Ollama / LM Studio”."}
            </p>
          )}
          {modelsLoaded && !localModels.length && (
            <p className="muted">{t.noTextModels}</p>
          )}
          {textModel &&
            !manualModel &&
            modelsLoaded &&
            !localModels.includes(textModel) && (
              <p className="inline-error">{t.textModelUnavailable}</p>
            )}
          {discoveryOpen && (
            <LocalModelSetup
              api={api}
              locale={locale}
              connected={connected}
              disabled={disabled}
              onUse={(address, model) => {
                if (disabled) return;
                setApiKey("");
                setUseSavedKey(false);
                setManualModel(false);
                setEndpoint(address);
                setTextModel(model);
                setUsedServer((count) => count + 1);
              }}
            />
          )}
        </>
      )}
      <CredentialPanel
        api={api}
        endpoint={endpoint}
        mode={mode}
        connected={connected}
        disabled={disabled}
        apiKey={apiKey}
        setApiKey={setApiKey}
        useSavedKey={useSavedKey}
        setUseSavedKey={setUseSavedKey}
        t={t}
      />
    </section>
  );
}
