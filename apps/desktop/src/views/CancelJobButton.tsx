import { useState } from "react";
import { Square } from "lucide-react";
import { errorMessage } from "../api";

/** Mounted for one running job so a late rejection cannot appear on its successor. */
export default function CancelJobButton({
  label,
  requested,
  cancel,
}: {
  label: string;
  requested: boolean;
  cancel: () => Promise<unknown>;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <button
        className="secondary"
        disabled={pending || requested}
        onClick={async () => {
          setError(null);
          setPending(true);
          try {
            await cancel();
          } catch (failure) {
            setError(errorMessage(failure));
          } finally {
            setPending(false);
          }
        }}
      >
        <Square size={14} />
        {label}
      </button>
      {error && !requested && (
        <span className="inline-error" role="alert">
          {error}
        </span>
      )}
    </>
  );
}
