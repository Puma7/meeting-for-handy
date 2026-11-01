import {
  AudioLines,
  FolderClock,
  Moon,
  Settings,
  Sparkles,
  Sun,
} from "lucide-react";
import type { ReactNode } from "react";
import type { AppState, Locale, Theme } from "../types";
import type { MessageKey } from "../i18n";
export type Page = "meeting" | "history" | "detail" | "notes" | "settings";
type Props = {
  app: AppState | null;
  page: Page;
  locale: Locale;
  theme: Theme;
  t: Record<MessageKey, string>;
  navigate: (page: Page) => Promise<void>;
  setTheme: (theme: Theme) => void;
  setLocale: (locale: Locale) => void;
  /** Recording health, shown on every page while a recording runs. */
  recordingStatus?: ReactNode;
  updateAction?: ReactNode;
};
/**
 * Renders page navigation and locale/theme controls; the parent retains ownership of
 * meeting selection and navigation side effects.
 */
export default function Sidebar({
  app,
  page,
  locale,
  theme,
  t,
  navigate,
  setTheme,
  setLocale,
  recordingStatus,
  updateAction,
}: Props) {
  const dark =
    theme === "dark" ||
    (theme === "system" &&
      (window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false));
  return (
    <aside className="sidebar">
      <div className="brand">
        <img src="/handymeeting.svg" width="34" height="34" alt="" />
        <div>
          <strong>Meeting for Handy</strong>
          {app?.version && <small>{app.version}</small>}
        </div>
      </div>
      <nav aria-label={locale === "de" ? "Hauptnavigation" : "Main navigation"}>
        {(
          [
            ["meeting", AudioLines],
            ["history", FolderClock],
            ["notes", Sparkles],
            ["settings", Settings],
          ] as const
        ).map(([id, Icon]) => (
          <button
            key={id}
            className={
              page === id || (page === "detail" && id === "history")
                ? "nav-item selected"
                : "nav-item"
            }
            aria-current={
              page === id || (page === "detail" && id === "history")
                ? "page"
                : undefined
            }
            title={t[id]}
            onClick={() => void navigate(id)}
          >
            <Icon size={19} />
            {t[id]}
          </button>
        ))}
      </nav>
      <div className="sidebar-bottom">
        {recordingStatus}
        {updateAction}
        <div className="appearance-row">
          <button
            className="icon-button"
            title={dark ? t.light : t.dark}
            aria-label={dark ? t.light : t.dark}
            onClick={() => setTheme(dark ? "light" : "dark")}
          >
            {dark ? <Sun size={17} /> : <Moon size={17} />}
          </button>
          <select
            aria-label={t.language}
            value={locale}
            onChange={(event) => setLocale(event.target.value as Locale)}
          >
            <option value="de">Deutsch</option>
            <option value="en">English</option>
          </select>
        </div>
      </div>
    </aside>
  );
}
