/// <reference types="vite/client" />
import type { ReactElement, ReactNode } from "react";
export function LiveWorkspace(props: {
  adapter: unknown;
  meeting: unknown;
  onBack: () => void;
  selectedFile?: string | null;
  initialEndpoint?: string;
  initialModel?: string;
  mode?: "local" | "cloud";
  theme?: "light" | "dark" | "system";
  onThemeChange?: (theme: "light" | "dark") => void;
  onModelSettings?: () => void;
  onStopRecording?: () => void;
  stopping?: boolean;
  onTogglePause?: () => void;
  paused?: boolean;
  onToggleMute?: () => void;
  muted?: boolean;
  captureError?: string | null;
  actionError?: string | null;
  onDismissError?: () => void;
  notices?: ReactNode;
  language?: "de" | "en";
}): ReactElement;
