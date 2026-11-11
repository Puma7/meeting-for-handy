import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api";
import type {
  Locale,
  ProtectedProfileReceipt,
  ProtectedProfileStatus,
} from "../types";

export default function ProtectedProfile({
  locale,
  disabled,
  readStatus,
  create,
  open,
}: {
  locale: Locale;
  disabled?: boolean;
  readStatus: () => Promise<ProtectedProfileStatus>;
  create: () => Promise<ProtectedProfileReceipt | null>;
  open?: () => Promise<boolean | null>;
}) {
  const de = locale === "de";
  const [expanded, setExpanded] = useState(false);
  const [status, setStatus] = useState<ProtectedProfileStatus | null>(null);
  const [statusError, setStatusError] = useState("");
  const [receipt, setReceipt] = useState<ProtectedProfileReceipt | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<"copy" | "open" | null>(null);
  const [opened, setOpened] = useState(false);
  const running = useRef(false);

  useEffect(() => {
    if (!expanded) return;
    let current = true;
    setStatus(null);
    setStatusError("");
    void readStatus().then(
      (next) => {
        if (current) setStatus(next);
      },
      (failure: unknown) => {
        if (current) setStatusError(errorMessage(failure));
      },
    );
    return () => {
      current = false;
    };
  }, [expanded, readStatus]);

  async function run(action: "copy" | "open") {
    if (disabled || running.current) return;
    running.current = true;
    setBusy(action);
    setError("");
    setOpened(false);
    try {
      if (action === "copy") {
        const result = await create();
        if (result) setReceipt(result);
      } else if (await open?.()) {
        setOpened(true);
      }
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      running.current = false;
      setBusy(null);
    }
  }

  return (
    <details
      className="settings-disclosure"
      onToggle={(event) => setExpanded(event.currentTarget.open)}
    >
      <summary>
        {de ? "Sitzungsspeicher verschlüsseln" : "Encrypt session storage"}
      </summary>
      {statusError ? (
        <p role="alert">{statusError}</p>
      ) : status === null ? (
        expanded && (
          <p role="status">{de ? "Speicher prüfen …" : "Checking storage …"}</p>
        )
      ) : !status.supported ? (
        <p>
          {de
            ? "Dafür brauchst du Windows mit EFS und ein geeignetes NTFS-Laufwerk. Für einzelne Sitzungen steht die verschlüsselte Sicherung zur Verfügung."
            : "This requires Windows with EFS and a suitable NTFS drive. You can use an encrypted backup for individual meetings."}
        </p>
      ) : status.active ? (
        <p role="status">
          {de
            ? "Windows-Dateiverschlüsselung (EFS) ist für dieses Sitzungsprofil aktiv."
            : "Windows file encryption (EFS) is active for this meeting profile."}
        </p>
      ) : (
        <>
          <p>
            {de
              ? "Kopiere deine Sitzungsdateien in ein neues Profil auf einem NTFS-Laufwerk. Dort baut die App den Suchindex neu auf. Windows-Dateiverschlüsselung (EFS) schützt die Inhalte über dein Windows-Konto."
              : "Copy your session files into a new profile on an NTFS drive. The app rebuilds the search index there. Windows file encryption (EFS) protects the contents through your Windows account."}
          </p>
          <p>
            {de
              ? "Das Originalprofil bleibt erhalten. Private Arbeitsbereichsentwürfe aus diesem Fenster findest du weiterhin dort. Künftige Änderungen gelten für das jeweils geöffnete Profil."
              : "The original profile is kept. Private workspace drafts from this window remain available there. Future changes apply to the profile you have open."}
          </p>
          <div className="session-row-actions">
            <button
              className="secondary"
              type="button"
              disabled={disabled || busy !== null}
              onClick={() => void run("copy")}
            >
              {de
                ? "Verschlüsseltes Profil erstellen"
                : "Create encrypted profile"}
            </button>
            {open && (
              <button
                className="secondary"
                type="button"
                disabled={disabled || busy !== null}
                onClick={() => void run("open")}
              >
                {de
                  ? "Verschlüsseltes Profil öffnen"
                  : "Open encrypted profile"}
              </button>
            )}
          </div>
          {busy && (
            <p role="status">
              {busy === "copy"
                ? de
                  ? "Profil kopieren und prüfen …"
                  : "Copying and verifying profile …"
                : de
                  ? "Geschütztes Profil öffnen …"
                  : "Opening encrypted profile …"}
            </p>
          )}
          {error && <p role="alert">{error}</p>}
          {receipt && (
            <p role="status">
              {de
                ? "Verschlüsseltes Profil geprüft."
                : "Encrypted profile verified."}{" "}
              <code className="storage-path">{receipt.path}</code>
            </p>
          )}
          {opened && (
            <p role="status">
              {de
                ? "Das geschützte Profil startet in einem neuen Fenster. Schließe dieses Fenster, sobald du dort weiterarbeitest."
                : "The encrypted profile starts in a new window. Close this window when you are ready to continue there."}
            </p>
          )}
        </>
      )}
    </details>
  );
}
