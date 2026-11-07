import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { DesktopApi, Meeting } from "../types";
import { LiveWorkspace } from "./shell/LiveWorkspace.jsx";
import { createPortal } from "react-dom";
import workspaceCss from "./shell/workspace.css?inline";
import overviewCss from "./shell/overview.css?inline";
import densityCss from "./shell/density.css?inline";
import liveCss from "./shell/liveWorkspace.css?inline";

export interface WorkspaceContext {
  revision: number;
  agenda: { id: string; revision: number; text: string }[];
  sharedNotes: { id: string; revision: number; text: string }[];
  overrides: {
    id: string;
    expectedRevision: number;
    status: string;
    saved: boolean;
  }[];
}
export interface WorkspaceEntity {
  id: string;
  revision: number;
  kind: string;
  text: string;
  status: string;
  attention: string;
  owner: string | null;
  due: string | null;
  evidence: { sourceId: string; quote: string }[];
  humanLocked: boolean;
}
export interface WorkspaceState {
  revision: number;
  updatedAt: string | null;
  context: WorkspaceContext;
  entities: WorkspaceEntity[];
  events: {
    id: string;
    entityId: string;
    human?: boolean;
    revision: number;
    atMs: number;
    text: string;
    status: string;
    evidence: { sourceId: string; quote: string }[];
  }[];
  overviewRefs: string[];
  contextExcludedIds?: string[];
  supersededSources?: string[];
  originals: {
    id: string;
    source: string;
    startMs: number;
    endMs: number;
    text: string;
  }[];
}
/** Mount an isolated workspace and adapt desktop APIs without giving text models session or audio objects. */
export default function WorkspaceBridge({
  api,
  meeting,
  onBack,
  endpoint,
  selectedFile,
  model,
  apiKey = "",
  useSavedKey = false,
  outputLanguage,
  workspacePrompt,
  ...presentation
}: {
  api: DesktopApi;
  meeting: Meeting;
  batchTranscription?: boolean;
  onBack: () => void;
  endpoint: string;
  selectedFile?: string | null;
  model: string;
  apiKey?: string;
  useSavedKey?: boolean;
  outputLanguage?: import("../types").WorkspaceOutputLanguage;
  workspacePrompt?: string | null;
  mode?: "local" | "cloud";
  theme?: "light" | "dark" | "system";
  onThemeChange?: (theme: "light" | "dark") => void;
  onModelSettings?: () => void;
  onCopyTranscript?: () => void;
  transcriptCopied?: boolean;
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
}) {
  const host = useRef<HTMLDivElement>(null);
  const [shadow, setShadow] = useState<ShadowRoot | null>(null);
  const available = Boolean(
    api.startLiveNotes &&
      api.getLiveNotes &&
      api.setWorkspaceContext &&
      api.stopLiveNotes &&
      api.refreshLiveNotes &&
      api.setLiveNotesInterval &&
      api.localModelSetup,
  );
  useLayoutEffect(() => {
    if (host.current)
      setShadow(
        host.current.shadowRoot || host.current.attachShadow({ mode: "open" }),
      );
  }, [available]);
  const adapter = useMemo(
    () => ({
      status: (id: string) => api.getLiveNotes!(id, true),
      history: api.listWorkspaceHistory
        ? (id: string) => api.listWorkspaceHistory!(id)
        : undefined,
      historyView: api.readWorkspaceHistory
        ? (id: string, runId: string, version: string) =>
            api.readWorkspaceHistory!(id, runId, version)
        : undefined,
      start: (
        request: Parameters<NonNullable<DesktopApi["startLiveNotes"]>>[0],
      ) =>
        api.startLiveNotes!({
          ...request,
          apiKey: useSavedKey ? "" : apiKey,
          useSavedKey,
          language: outputLanguage ?? request.language,
          ...(workspacePrompt?.trim() ? { workspacePrompt } : {}),
        }),
      stop: () => api.stopLiveNotes!(),
      refresh: () => api.refreshLiveNotes!(),
      interval: (ms: number) => api.setLiveNotesInterval!(ms),
      context: (context: WorkspaceContext, runId: string) =>
        api.setWorkspaceContext!(meeting.id, runId, context),
      models: () => api.localModelSetup!(),
      sourceAudio: api.getSourceAudio
        ? (file: string) => api.getSourceAudio!(meeting.id, file)
        : undefined,
    }),
    [api, meeting.id, apiKey, useSavedKey, outputLanguage, workspacePrompt],
  );
  if (
    !api.startLiveNotes ||
    !api.getLiveNotes ||
    !api.setWorkspaceContext ||
    !api.stopLiveNotes ||
    !api.refreshLiveNotes ||
    !api.setLiveNotesInterval ||
    !api.localModelSetup
  )
    return (
      <div role="status">
        Die native Kachelanbindung ist in diesem Programmstand nicht verfügbar.
        <button onClick={onBack}>Zur Aufnahme</button>
      </div>
    );
  return (
    <div
      ref={host}
      className="native-workspace-host"
      style={{ display: "block", height: "100dvh", width: "100%" }}
    >
      {shadow &&
        createPortal(
          <>
            <style>{`:host{all:initial;display:block;height:100dvh;width:100%}\n${workspaceCss.replaceAll(":root", ":host")}\n${overviewCss}\n${densityCss}\n${liveCss}`}</style>
            <LiveWorkspace
              key={meeting.id}
              adapter={adapter}
              selectedFile={selectedFile}
              meeting={meeting}
              onBack={onBack}
              {...presentation}
              initialEndpoint={endpoint}
              initialModel={model}
            />
          </>,
          shadow,
        )}
    </div>
  );
}
