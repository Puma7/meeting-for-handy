import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import {
  WORKSPACE_OUTPUT_LANGUAGES,
  type DesktopApi,
  type Locale,
  type WorkspaceOutputLanguage,
} from "../types";
import workspaceSystem from "../../../../crates/meeting-notes/src/workspace/prompts/system.en.txt?raw";
import workspaceDefault from "../../../../crates/meeting-notes/src/workspace/prompts/presentation.en.txt?raw";
import workspaceTasks from "../../../../crates/meeting-notes/src/workspace/prompts/tasks.en.txt?raw";
import workspaceFacts from "../../../../crates/meeting-notes/src/workspace/prompts/facts.en.txt?raw";
import workspaceReconcile from "../../../../crates/meeting-notes/src/workspace/prompts/reconcile.en.txt?raw";
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
  workspaceLanguage,
  setWorkspaceLanguage,
  workspacePrompt,
  setWorkspacePrompt,
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
  workspaceLanguage: WorkspaceOutputLanguage;
  setWorkspaceLanguage: (value: WorkspaceOutputLanguage) => void;
  workspacePrompt: string | null;
  setWorkspacePrompt: (value: string | null) => void;
}) {
  const de = locale === "de";
  const languages: Record<WorkspaceOutputLanguage, string> = {
    auto: de ? "Automatisch aus dem Transkript" : "Automatic from transcript",
    de: "Deutsch",
    en: "English",
    ru: "Русский",
    zh: "中文",
  };
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
      <div className="settings-model-intro">
        <p className="muted">
          {de
            ? "Wähle, womit HandyMeeting deine Texte auswertet."
            : "Choose which service analyzes your meeting text."}
        </p>
        {mode === "local" && (
          <button
            type="button"
            className="primary settings-model-detect"
            title={
              de
                ? "Nach laufenden lokalen Diensten und ihren verfügbaren Modellen suchen."
                : "Look for running local services and their available models."
            }
            disabled={disabled || !connected}
            aria-expanded={discoveryOpen}
            onClick={() => setDiscoveryOpen((open) => !open)}
          >
            {de ? "Ollama / LM Studio erkennen" : "Detect Ollama / LM Studio"}
          </button>
        )}
      </div>
      {mode === "local" && discoveryOpen && (
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
      <div className="settings-grid">
        <label>
          {t.processing}
          <select
            value={mode}
            title={
              de
                ? "Auswählen, wo der Meetingtext ausgewertet wird."
                : "Choose where meeting text is analyzed."
            }
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
            title={
              de
                ? "Adresse des ausgewählten Textmodelldienstes."
                : "Address of the selected text model service."
            }
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
                title={
                  de
                    ? "Ein vom Dienst gemeldetes Textmodell auswählen."
                    : "Choose a text model reported by the service."
                }
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
              title={
                de
                  ? "Den genauen Modellnamen des Dienstes eingeben."
                  : "Enter the exact model name used by the service."
              }
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
                title={
                  de
                    ? "Den Modellnamen selbst eingeben."
                    : "Enter the model name yourself."
                }
                checked={manualModel}
                disabled={disabled}
                onChange={(e) => setManualModel(e.target.checked)}
              />
              {t.manualTextModel}
            </label>
            <details className="local-service-help">
              <summary title={t.setupHelp}>{t.setupHelp}</summary>
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
        </>
      )}
      {mode === "local" && (
        <details className="workspace-prompt-settings">
          <summary>
            {de
              ? "Erweiterte Auswertung: Sprache und Prompt"
              : "Advanced analysis: language and prompt"}
          </summary>
          <p className="muted">
            {de
              ? "Empfohlen: Gemma 4 12B oder größer. Bei knapper Leistung: Gemma 4 E4B."
              : "Recommended: Gemma 4 12B or larger. On less powerful machines: Gemma 4 E4B."}
          </p>
          <label>
            {de ? "Ausgabesprache" : "Output language"}
            <select
              value={workspaceLanguage}
              disabled={disabled}
              onChange={(event) =>
                setWorkspaceLanguage(
                  event.target.value as WorkspaceOutputLanguage,
                )
              }
            >
              {WORKSPACE_OUTPUT_LANGUAGES.map((language) => (
                <option key={language} value={language}>
                  {languages[language]}
                </option>
              ))}
            </select>
          </label>
          <label>
            {de
              ? "Anweisungen für Stil und Schwerpunkte"
              : "Instructions for style and priorities"}
            <textarea
              value={workspacePrompt ?? workspaceDefault.trim()}
              maxLength={600}
              rows={4}
              spellCheck={false}
              disabled={disabled}
              onChange={(event) => setWorkspacePrompt(event.target.value)}
            />
          </label>
          <button
            type="button"
            className="secondary"
            disabled={disabled || workspacePrompt === null}
            onClick={() => setWorkspacePrompt(null)}
          >
            {de ? "Standard wiederherstellen" : "Restore defaults"}
          </button>
          <p className="muted">
            {de
              ? "Änderungen gelten beim nächsten Start der erweiterten Auswertung. Ein leeres Feld verwendet den Standard. Belege und Statusregeln gelten weiterhin."
              : "Changes apply the next time you start advanced analysis. A blank field uses the default. Evidence and status rules still apply."}
          </p>
          <details>
            <summary>
              {de
                ? "Standardprompt anzeigen (Englisch)"
                : "Show default prompt (English)"}
            </summary>
            <pre lang="en">
              {[
                workspaceSystem,
                workspaceTasks,
                workspaceFacts,
                workspaceReconcile,
                workspaceDefault,
              ]
                .map((part) => part.trim())
                .join("\n\n")}
            </pre>
          </details>
        </details>
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
