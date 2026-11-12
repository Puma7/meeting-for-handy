import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../api";
import { ACTIVE_POLL_MS } from "../jobPolling";
import type {
  Locale,
  ProtectedProfileProvider,
  ProtectedProfileReceipt,
  ProtectedProfileStatus,
} from "../types";

type ProfileAction = "copy" | "open";

const providerText = {
  windowsEfs: {
    de: {
      create:
        "Kopiere deine Sitzungsdateien in ein neues Profil auf einem NTFS-Laufwerk. Dort baut die App den Suchindex neu auf. Windows-Dateiverschlüsselung (EFS) schützt die Inhalte über dein Windows-Konto.",
      active:
        "Windows-Dateiverschlüsselung (EFS) ist für dieses Sitzungsprofil aktiv.",
      unavailable:
        "Dafür brauchst du Windows mit EFS und ein geeignetes NTFS-Laufwerk. Für einzelne Sitzungen steht die verschlüsselte Sicherung zur Verfügung.",
    },
    en: {
      create:
        "Copy your session files into a new profile on an NTFS drive. The app rebuilds the search index there. Windows file encryption (EFS) protects the contents through your Windows account.",
      active:
        "Windows file encryption (EFS) is active for this meeting profile.",
      unavailable:
        "This requires Windows with EFS and a suitable NTFS drive. You can use an encrypted backup for individual meetings.",
    },
  },
  macosImage: {
    de: {
      create:
        "Kopiere deine Sitzungsdateien in ein verschlüsseltes macOS-Disk-Image. Dort baut die App den Suchindex neu auf. Zum Öffnen brauchst du die Passphrase.",
      active:
        "Dieses Sitzungsprofil liegt in einem verschlüsselten macOS-Disk-Image.",
      unavailable:
        "Das macOS-Werkzeug für verschlüsselte Disk-Images ist nicht verfügbar.",
    },
    en: {
      create:
        "Copy your session files into an encrypted macOS disk image. The app rebuilds the search index there. You will need the passphrase to open the profile.",
      active:
        "This meeting profile is stored in an encrypted macOS disk image.",
      unavailable: "The macOS tool for encrypted disk images is unavailable.",
    },
  },
  linuxGocryptfs: {
    de: {
      create:
        "Kopiere deine Sitzungsdateien in ein mit gocryptfs verschlüsseltes Profil. Dort baut die App den Suchindex neu auf. Zum Öffnen brauchst du die Passphrase und FUSE.",
      active: "Dieses Sitzungsprofil ist mit gocryptfs verschlüsselt.",
      unavailable:
        "Installiere gocryptfs und richte FUSE ein, um ein verschlüsseltes Sitzungsprofil zu erstellen oder zu öffnen. Für einzelne Sitzungen steht die verschlüsselte Sicherung zur Verfügung.",
    },
    en: {
      create:
        "Copy your session files into a profile encrypted with gocryptfs. The app rebuilds the search index there. You will need the passphrase and FUSE to open the profile.",
      active: "This meeting profile is encrypted with gocryptfs.",
      unavailable:
        "Install gocryptfs and configure FUSE to create or open an encrypted meeting profile. You can use an encrypted backup for individual meetings.",
    },
  },
} satisfies Record<
  ProtectedProfileProvider,
  Record<Locale, { create: string; active: string; unavailable: string }>
>;

export default function ProtectedProfile({
  locale,
  disabled,
  readStatus,
  create,
  cancel,
  open,
}: {
  locale: Locale;
  disabled?: boolean;
  readStatus: () => Promise<ProtectedProfileStatus>;
  create: (passphrase?: string) => Promise<ProtectedProfileReceipt | null>;
  cancel?: () => Promise<boolean>;
  open?: (passphrase?: string) => Promise<boolean | null>;
}) {
  const de = locale === "de";
  const [expanded, setExpanded] = useState(false);
  const [status, setStatus] = useState<ProtectedProfileStatus | null>(null);
  const [statusError, setStatusError] = useState("");
  const [statusRetry, setStatusRetry] = useState(0);
  const [receipt, setReceipt] = useState<ProtectedProfileReceipt | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<ProfileAction | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [opened, setOpened] = useState(false);
  const [passwordAction, setPasswordAction] = useState<ProfileAction | null>(
    null,
  );
  const passphrase = useRef<HTMLInputElement>(null);
  const repeated = useRef<HTMLInputElement>(null);
  const running = useRef(false);
  const attempt = useRef(0);
  const observedCopy = useRef(false);
  const busyAction = busy ?? (status?.migration ? "copy" : null);
  const cancellationPending = cancelling || status?.migration === "cancelling";
  const provider = status?.provider ?? "windowsEfs";
  const copy = providerText[provider][locale];
  const temporaryDrafts = de
    ? "Private Arbeitsbereichsentwürfe in diesem geschützten Fenster sind vorübergehend. Gespeicherte Sitzungsdateien liegen im verschlüsselten Profil."
    : "Private workspace drafts in this protected window are temporary. Saved session files are stored in the encrypted profile.";

  useEffect(() => {
    if (!expanded) return;
    let current = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setStatus(null);
    setStatusError("");
    async function readMigrationStatus() {
      try {
        const next = await readStatus();
        if (!current) return;
        if (!next.migration && !running.current) {
          if (observedCopy.current) attempt.current += 1;
          setCancelling(false);
        }
        observedCopy.current = Boolean(next.migration);
        setStatus(next);
      } catch (failure) {
        if (!current) return;
        if (!observedCopy.current) setStatusError(errorMessage(failure));
      }
      if (observedCopy.current) {
        timer = setTimeout(() => void readMigrationStatus(), ACTIVE_POLL_MS);
      }
    }
    void readMigrationStatus();
    return () => {
      current = false;
      observedCopy.current = false;
      clearTimeout(timer);
    };
  }, [expanded, readStatus, statusRetry]);

  async function run(action: ProfileAction, secret?: string) {
    if (disabled || running.current || observedCopy.current) return;
    running.current = true;
    attempt.current += 1;
    setBusy(action);
    setError("");
    setOpened(false);
    if (action === "copy") setReceipt(null);
    try {
      if (action === "copy") {
        const result = await (secret === undefined ? create() : create(secret));
        if (result) setReceipt(result);
      } else if (await (secret === undefined ? open?.() : open?.(secret))) {
        setOpened(true);
      }
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      running.current = false;
      setBusy(null);
      setCancelling(false);
    }
  }

  async function requestCancellation() {
    if (!cancel || cancellationPending || busyAction !== "copy") return;
    const current = attempt.current;
    const stillActive = () =>
      attempt.current === current && (running.current || observedCopy.current);
    setCancelling(true);
    setError("");
    try {
      const accepted = await cancel();
      if (!accepted && stillActive()) setCancelling(false);
    } catch (failure) {
      if (stillActive()) {
        setCancelling(false);
        setError(errorMessage(failure));
      }
    }
  }

  function clearPasswordFields() {
    if (passphrase.current) passphrase.current.value = "";
    if (repeated.current) repeated.current.value = "";
  }

  function requestAction(action: ProfileAction) {
    if (disabled || running.current || observedCopy.current) return;
    if (provider === "windowsEfs") {
      void run(action);
      return;
    }
    clearPasswordFields();
    setError("");
    setOpened(false);
    setPasswordAction(action);
  }

  return (
    <details
      className="settings-disclosure"
      onToggle={(event) => {
        const next = event.currentTarget.open;
        setExpanded(next);
        if (!next) {
          clearPasswordFields();
          setPasswordAction(null);
        }
      }}
    >
      <summary>
        {de ? "Sitzungsspeicher verschlüsseln" : "Encrypt session storage"}
      </summary>
      {statusError ? (
        <>
          <p role="alert">{statusError}</p>
          <button
            type="button"
            className="secondary"
            onClick={() => setStatusRetry((count) => count + 1)}
          >
            {de ? "Speicher erneut prüfen" : "Check storage again"}
          </button>
        </>
      ) : status === null ? (
        expanded && (
          <p role="status">{de ? "Speicher prüfen …" : "Checking storage …"}</p>
        )
      ) : status.active ? (
        <>
          <p role="status">{copy.active}</p>
          {provider === "macosImage" && <p>{temporaryDrafts}</p>}
        </>
      ) : !status.supported ? (
        <p>{copy.unavailable}</p>
      ) : (
        <>
          <p>{copy.create}</p>
          <p>
            {de
              ? "Das Originalprofil bleibt erhalten. Private Arbeitsbereichsentwürfe aus diesem Fenster findest du weiterhin dort. Künftige Änderungen gelten für das jeweils geöffnete Profil."
              : "The original profile is kept. Private workspace drafts from this window remain available there. Future changes apply to the profile you have open."}
          </p>
          {provider === "macosImage" && <p>{temporaryDrafts}</p>}
          <div className="session-row-actions">
            <button
              className="secondary"
              type="button"
              disabled={disabled || busyAction !== null}
              onClick={() => requestAction("copy")}
            >
              {de
                ? "Verschlüsseltes Profil erstellen"
                : "Create encrypted profile"}
            </button>
            {open && (
              <button
                className="secondary"
                type="button"
                disabled={disabled || busyAction !== null}
                onClick={() => requestAction("open")}
              >
                {de
                  ? "Verschlüsseltes Profil öffnen"
                  : "Open encrypted profile"}
              </button>
            )}
            {busyAction === "copy" && cancel && (
              <button
                className="secondary"
                type="button"
                disabled={cancellationPending}
                onClick={() => void requestCancellation()}
              >
                {de ? "Profilkopie abbrechen" : "Cancel profile copy"}
              </button>
            )}
          </div>
          {passwordAction && (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                if (disabled || running.current || !passphrase.current) return;
                const secret = passphrase.current.value;
                if (
                  !secret ||
                  /[\r\n\u0000]/.test(secret) ||
                  new TextEncoder().encode(secret).length > 1024
                ) {
                  setError(
                    de
                      ? "Gib eine Passphrase ohne Zeilenumbruch mit höchstens 1024 Bytes ein."
                      : "Enter a passphrase of up to 1024 bytes without line breaks.",
                  );
                  return;
                }
                if (
                  passwordAction === "copy" &&
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
                clearPasswordFields();
                setPasswordAction(null);
                void run(passwordAction, secret);
              }}
            >
              <fieldset disabled={disabled || busyAction !== null}>
                <legend>
                  {passwordAction === "copy"
                    ? de
                      ? "Passphrase festlegen"
                      : "Set a passphrase"
                    : de
                      ? "Verschlüsseltes Profil entsperren"
                      : "Unlock encrypted profile"}
                </legend>
                <label>
                  Passphrase
                  <input
                    type="password"
                    ref={passphrase}
                    autoComplete="off"
                    maxLength={1024}
                    required
                  />
                </label>
                {passwordAction === "copy" && (
                  <label>
                    {de ? "Passphrase wiederholen" : "Repeat passphrase"}
                    <input
                      type="password"
                      ref={repeated}
                      autoComplete="off"
                      maxLength={1024}
                      required
                    />
                  </label>
                )}
                <div className="session-row-actions">
                  <button type="submit" className="secondary">
                    {passwordAction === "copy"
                      ? de
                        ? "Bestätigen und erstellen"
                        : "Confirm and create"
                      : de
                        ? "Entsperren und öffnen"
                        : "Unlock and open"}
                  </button>
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => {
                      clearPasswordFields();
                      setPasswordAction(null);
                      setError("");
                    }}
                  >
                    {de ? "Abbrechen" : "Cancel"}
                  </button>
                </div>
              </fieldset>
            </form>
          )}
          {busyAction && (
            <p role="status">
              {busyAction === "copy"
                ? cancellationPending
                  ? de
                    ? "Profilkopie wird abgebrochen …"
                    : "Cancelling profile copy …"
                  : de
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
