import { useRef, useState } from "react";
import { errorMessage } from "../api";
import type { BackupCredential, Locale } from "../types";

export type BackupResult = { file: string; recoveryKey?: string };

export default function EncryptedBackup({
  locale,
  restore,
  disabled,
  run,
  cancel,
  receipt,
}: {
  locale: Locale;
  restore?: boolean;
  disabled?: boolean;
  run: (
    credential: string,
    kind: BackupCredential,
  ) => Promise<BackupResult | null>;
  cancel?: () => Promise<boolean>;
  receipt?: {
    result: BackupResult | null;
    setResult: (next: BackupResult) => void;
    busy: boolean;
    cancelling: boolean;
  };
}) {
  const de = locale === "de";
  const passphrase = useRef<HTMLInputElement>(null);
  const repeated = useRef<HTMLInputElement>(null);
  const started = useRef(false);
  const [localBusy, setBusy] = useState(false);
  const [localCancelling, setCancelling] = useState(false);
  const [kind, setKind] = useState<BackupCredential>("passphrase");
  const [error, setError] = useState("");
  const [localResult, setLocalResult] = useState<BackupResult | null>(null);
  const result = receipt ? receipt.result : localResult;
  const busy = receipt?.busy ?? localBusy;
  const cancelling = receipt?.cancelling ?? localCancelling;
  const setResult = receipt?.setResult ?? setLocalResult;
  const action = restore
    ? de
      ? "Verschlüsselte Sitzung wiederherstellen"
      : "Recover an encrypted meeting"
    : de
      ? "Verschlüsselte Kopie erstellen"
      : "Create an encrypted copy";
  return (
    <details className="settings-disclosure">
      <summary>{action}</summary>
      <p>
        {restore
          ? de
            ? "Die App prüft die Sicherung und stellt sie als separates, lesbares Profil wieder her. Wähle dafür einen privaten Ordner außerhalb der aktiven Sitzungsablage."
            : "The app verifies the backup and restores it as a separate readable profile. Choose a private folder outside the active meeting storage."
          : de
            ? "Die App erstellt eine geschützte .hmvault-Datei. Das Original und sein Suchindex bleiben in der bisherigen Ablage lesbar. Zur Wiederherstellung genügt die Passphrase oder der separat angezeigte Schlüssel."
            : "The app creates a protected .hmvault file. The original and its search index remain readable in the existing storage. Recover it with the passphrase or the separately displayed key."}
      </p>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          if (disabled || started.current || !passphrase.current) return;
          const secret = passphrase.current.value;
          if (restore) setLocalResult(null);
          setError("");
          if (
            !restore &&
            (Array.from(secret).length < 12 ||
              secret !== repeated.current?.value)
          ) {
            setError(
              de
                ? "Verwende mindestens zwölf Zeichen und wiederhole dieselbe Passphrase."
                : "Use at least twelve characters and enter the same passphrase twice.",
            );
            return;
          }
          if (!secret) return;
          passphrase.current.value = "";
          if (repeated.current) repeated.current.value = "";
          started.current = true;
          setBusy(true);
          try {
            const completed = await run(secret, kind);
            if (completed) setResult(completed);
          } catch (failure) {
            setError(errorMessage(failure));
          } finally {
            started.current = false;
            setBusy(false);
            setCancelling(false);
          }
        }}
      >
        <fieldset disabled={disabled || busy}>
          {restore && (
            <label>
              {de ? "Entsperren mit" : "Unlock with"}
              <select
                value={kind}
                onChange={(event) => {
                  setKind(
                    event.target.value === "recoveryKey"
                      ? "recoveryKey"
                      : "passphrase",
                  );
                  if (passphrase.current) passphrase.current.value = "";
                }}
              >
                <option value="passphrase">Passphrase</option>
                <option value="recoveryKey">
                  {de ? "Wiederherstellungsschlüssel" : "Recovery key"}
                </option>
              </select>
            </label>
          )}
          <label>
            {kind === "recoveryKey"
              ? de
                ? "Wiederherstellungsschlüssel"
                : "Recovery key"
              : "Passphrase"}
            <input
              type="password"
              ref={passphrase}
              required
              maxLength={1024}
              autoComplete={
                kind === "recoveryKey"
                  ? "off"
                  : restore
                    ? "current-password"
                    : "new-password"
              }
            />
          </label>
          {!restore && (
            <label>
              {de ? "Passphrase wiederholen" : "Repeat passphrase"}
              <input
                type="password"
                ref={repeated}
                required
                maxLength={1024}
                autoComplete="new-password"
              />
            </label>
          )}
          <button type="submit" className="secondary">
            {action}
          </button>
        </fieldset>
      </form>
      {busy && (
        <div className="session-row-actions">
          <p role="status">
            {cancelling
              ? de
                ? "Abbruch angefordert …"
                : "Cancellation requested …"
              : de
                ? "Vorgang läuft …"
                : "Working …"}
          </p>
          {cancel && (
            <button
              className="secondary"
              type="button"
              disabled={cancelling}
              onClick={async () => {
                try {
                  if ((await cancel()) && !receipt)
                    setCancelling(started.current);
                } catch (failure) {
                  setError(errorMessage(failure));
                }
              }}
            >
              {de ? "Abbrechen" : "Cancel"}
            </button>
          )}
        </div>
      )}
      {error && <p role="alert">{error}</p>}
      {result && (
        <p role="status">
          {restore
            ? de
              ? "Wiederhergestelltes Profil"
              : "Recovered profile"
            : de
              ? "Geprüfte Sicherung gespeichert"
              : "Verified backup saved"}
          {" · "}
          <code className="storage-path">{result.file}</code>
        </p>
      )}
      {restore && result && (
        <p>
          {de
            ? "Das Profil enthält lesbare Sitzungsdateien. Setze zum Öffnen im Terminal "
            : "This profile contains readable session files. To open it, set "}
          <code>HANDYMEETING_DATA_DIR</code>
          {de
            ? " auf den angezeigten Profilordner und starte Meeting for Handy aus demselben Terminal."
            : " to the displayed profile folder in your terminal, then launch Meeting for Handy from that terminal."}
        </p>
      )}
      {result?.recoveryKey && (
        <div>
          <p>
            {de
              ? "Bewahre den Schlüssel getrennt von der Sicherung auf. Die App speichert ihn nicht."
              : "Keep this key separately from the backup. The app does not save it."}
          </p>
          <label>
            {de ? "Wiederherstellungsschlüssel" : "Recovery key"}
            <input
              readOnly
              value={result.recoveryKey}
              onFocus={(event) => event.target.select()}
            />
          </label>
          <button
            type="button"
            className="secondary"
            onClick={() => setResult({ file: result.file })}
          >
            {de ? "Schlüssel ausblenden" : "Hide recovery key"}
          </button>
        </div>
      )}
    </details>
  );
}
