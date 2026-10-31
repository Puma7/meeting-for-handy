import { FolderOpen, RefreshCw } from "lucide-react";
import type { AppState, DesktopApi, Locale, Theme } from "../types";
import type { MessageKey } from "../i18n";
import RetentionPanel from "./RetentionPanel";
import { useState, type ReactNode } from "react";
import UpdatePanel from "./UpdatePanel";

/**
 * Presents read-only program paths and delegates native selection/import to the parent.
 * Displayed path text does not grant execution permission or start capture.
 */
export default function SettingsView({
  api,
  app,
  locale,
  theme,
  t,
  busy,
  blocked,
  connected,
  refreshingHandy,
  preferencesPersistable,
  handyPath,
  onChooseHandy,
  onRefreshHandy,
  onRefreshDevices,
  onImportLegacy,
  onRebuildIndex,
  onLocale,
  onTheme,
  textModelSettings,
  speakerModelSettings,
}: {
  api: DesktopApi;
  app: AppState | null;
  locale: Locale;
  theme: Theme;
  t: Record<MessageKey, string>;
  busy: boolean;
  blocked: boolean;
  connected: boolean;
  refreshingHandy: boolean;
  preferencesPersistable: boolean;
  handyPath: string;
  onChooseHandy: () => void;
  onRefreshHandy: () => void;
  onRefreshDevices: () => void;
  onImportLegacy: () => void;
  onRebuildIndex: () => void;
  onLocale: (locale: Locale) => void;
  onTheme: (theme: Theme) => void;
  textModelSettings?: ReactNode;
  /** Renders persistent setup with its asynchronous alerts outside the disclosure. */
  speakerModelSettings?: (statusContainer: HTMLElement | null) => ReactNode;
}) {
  const de = locale === "de";
  const unavailable = busy || blocked || !connected;
  const [maintenanceStatusRoot, setMaintenanceStatusRoot] =
    useState<HTMLDivElement | null>(null);
  const [speakerStatusRoot, setSpeakerStatusRoot] =
    useState<HTMLDivElement | null>(null);
  return (
    <>
      <div className="settings-sections">
        {!preferencesPersistable && (
          <div className="notice warning">{t.preferencesError}</div>
        )}
        <details className="settings-disclosure" open={!app?.handy?.executable}>
          <summary>{t.installation}</summary>
          <p>{t.executableHelp}</p>
          <label>
            {t.executable}
            <input
              value={handyPath || app?.handy?.executable || ""}
              readOnly
              placeholder={de ? "Automatisch erkennen" : "Detect automatically"}
            />
          </label>
          <div className="settings-actions">
            <button
              className="secondary"
              disabled={
                unavailable || refreshingHandy || !api.chooseHandyExecutable
              }
              onClick={onChooseHandy}
            >
              <FolderOpen size={15} />
              {de ? "Handy-Programm auswählen …" : "Choose Handy program …"}
            </button>
            <button
              className="secondary"
              disabled={unavailable || refreshingHandy}
              onClick={onRefreshHandy}
            >
              <RefreshCw size={15} />
              {t.refresh}
            </button>
          </div>
          <p>
            {t.version}:{" "}
            {app?.handy?.version ??
              (app?.handy?.executable ? t.handyVersionMissing : t.notFound)}
          </p>
          <p className="muted">
            {de
              ? "Eine andere Programmdatei wird im Dateidialog des Betriebssystems gewählt und geprüft. Manuelle Freigaben gelten für diesen App-Start; ein eingetippter Pfad erteilt keine Freigabe."
              : "Choose another program in the operating system's file dialog. Manual approvals last for this app session; a typed path does not grant permission to start it."}
          </p>
        </details>
        {textModelSettings && (
          <section id="text-model-settings" tabIndex={-1}>
            <h2>{de ? "Textmodelle" : "Text models"}</h2>
            {textModelSettings}
          </section>
        )}
        {speakerModelSettings && (
          <>
            <div ref={setSpeakerStatusRoot} />
            <details className="settings-disclosure">
              <summary>
                {de
                  ? "Sprecherfunktionen · optional"
                  : "Speaker features · optional"}
              </summary>
              {speakerModelSettings(speakerStatusRoot)}
            </details>
          </>
        )}
        <section>
          <h2>{t.theme}</h2>
          <div className="settings-grid">
            <label>
              {t.language}
              <select
                value={locale}
                onChange={(e) => onLocale(e.target.value as Locale)}
              >
                <option value="de">Deutsch</option>
                <option value="en">English</option>
              </select>
            </label>
            <label>
              {t.theme}
              <select
                value={theme}
                onChange={(e) => onTheme(e.target.value as Theme)}
              >
                <option value="system">{t.systemTheme}</option>
                <option value="light">{t.light}</option>
                <option value="dark">{t.dark}</option>
              </select>
            </label>
          </div>
        </section>
        <details className="settings-disclosure">
          <summary>{de ? "Audiogeräte" : "Audio devices"}</summary>
          <button
            className="secondary"
            disabled={unavailable}
            onClick={onRefreshDevices}
          >
            <RefreshCw size={15} />
            {t.refreshDevices}
          </button>
        </details>
        <details className="settings-disclosure">
          <summary>{t.storage}</summary>
          <p>{t.legacyHelp}</p>
          {app?.dataDirectory && (
            <code className="storage-path">{app.dataDirectory}</code>
          )}
        </details>
      </div>
      <UpdatePanel
        api={api}
        locale={locale}
        blocked={blocked || Boolean(app?.notesRunning)}
      />
      <div ref={setMaintenanceStatusRoot} />
      <details className="settings-disclosure">
        <summary>{de ? "Daten und Wartung" : "Data and maintenance"}</summary>
        {api.rebuildSearchIndex && (
          <section className="legacy-panel">
            <h2>{de ? "Lokaler Suchindex" : "Local search index"}</h2>
            <p className="muted">
              {de
                ? "Der Suchindex beschleunigt große Historien. Er wird lokal aus deinen Sitzungen aufgebaut; Originale werden nicht geändert."
                : "The local index speeds up large histories. Rebuilding never changes original sessions."}
            </p>
            <button
              className="secondary"
              disabled={unavailable}
              onClick={onRebuildIndex}
            >
              <RefreshCw size={15} />
              {de ? "Suchindex aktualisieren" : "Update search index"}
            </button>
          </section>
        )}
        <RetentionPanel
          api={api}
          locale={locale}
          models={app?.handy?.models ?? []}
          statusContainer={maintenanceStatusRoot}
        />
      </details>
      <section className="legacy-panel">
        <h2>{t.legacy}</h2>
        <button
          className="secondary"
          disabled={unavailable || !api.chooseLegacyManifest}
          onClick={onImportLegacy}
        >
          <FolderOpen size={15} />
          {de
            ? "Sitzungsdatei auswählen und nur lesbar öffnen …"
            : "Choose session file and open read-only …"}
        </button>
        <details className="help-disclosure">
          <summary>{de ? "Hinweise zum Öffnen" : "Opening guidance"}</summary>
          <p className="muted">{t.legacyHelp}</p>
          <p className="muted">
            {de
              ? "Wähle die vorhandene session.json im Dateidialog. Abbrechen verändert nichts; die Originaldateien bleiben erhalten."
              : "Choose the existing session.json in the file dialog. Cancelling changes nothing; original files are preserved."}
          </p>
        </details>
      </section>
    </>
  );
}
