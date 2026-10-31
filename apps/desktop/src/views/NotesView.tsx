import {
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  Pencil,
  Plus,
  Sparkles,
  Trash2,
  RefreshCw,
} from "lucide-react";
import { uid, meetingLabel } from "../domain";
import type {
  AnalysisResult,
  AnalysisStep,
  AnalysisTemplate,
  Evidence,
  Meeting,
  MeetingSummary,
  Locale,
} from "../types";
import type { MessageKey } from "../i18n";
import type { TemplateAction } from "./TemplateDialog";
import { useEffect, useState, type ReactNode } from "react";
type Props = {
  connectionSettings: ReactNode;
  t: Record<MessageKey, string>;
  meeting: Meeting | null;
  meetingId: string;
  meetings: MeetingSummary[];
  meetingLoading: boolean;
  selectMeeting: (id: string) => void;
  modelReady: boolean;
  /** Local discovery failure remains visible while optional setup is collapsed. */
  modelDiscoveryError?: string | null;
  libraryInvalid: boolean;
  libraryPending: boolean;
  libraryError: string | null;
  browserInvalid: boolean;
  refreshTemplates: () => void;
  importBrowser: (() => void) | null;
  templates: AnalysisTemplate[];
  notesBusy: boolean;
  textModel: string;
  endpoint: string;
  template: AnalysisTemplate;
  resultId: string;
  analyses: AnalysisResult[];
  setTemplateId: (v: string) => void;
  setStepId: (v: string) => void;
  setResultId: (v: string) => void;
  openTemplateAction: (
    kind: TemplateAction["kind"],
    original?: AnalysisTemplate,
  ) => void;
  connected: boolean;
  recording: boolean;
  busy: boolean;
  beginNotes: () => void;
  /** Only render the page action when the persistent status bar has no start action. */
  showStart: boolean;
  result: AnalysisResult | undefined;
  resultSection: AnalysisResult["sections"][number] | undefined;
  selectedStep: AnalysisStep;
  setEditing: (v: AnalysisStep | null) => void;
  inspectStep: (v: AnalysisStep) => void;
  setEditingNew: (v: boolean) => void;
  moveStep: (delta: number) => void;
  updateTemplate: (v: AnalysisTemplate) => Promise<boolean>;
  copied: boolean;
  copyResults: () => Promise<void>;
  showEvidence: (evidence: Evidence) => void;
  locale: Locale;
};
/**
 * Orders post-processing around the selected meeting, shared connection, template and result.
 * Execution and per-run cloud consent remain parent-owned; editing never starts a model call.
 */
export default function NotesView({
  connectionSettings,
  t,
  meeting,
  meetingId,
  meetings,
  meetingLoading,
  selectMeeting,
  modelReady,
  modelDiscoveryError,
  libraryInvalid,
  libraryPending,
  libraryError,
  browserInvalid,
  refreshTemplates,
  importBrowser,
  templates,
  notesBusy,
  textModel,
  endpoint,
  template,
  resultId,
  analyses,
  setTemplateId,
  setStepId,
  setResultId,
  openTemplateAction,
  connected,
  recording,
  busy,
  beginNotes,
  showStart,
  result,
  resultSection,
  selectedStep,
  setEditing,
  inspectStep,
  setEditingNew,
  moveStep,
  updateTemplate,
  copied,
  copyResults,
  showEvidence,
  locale,
}: Props) {
  const [setupOpen, setSetupOpen] = useState(false);
  useEffect(() => {
    if (notesBusy || result) setSetupOpen(false);
  }, [notesBusy, result?.id]);
  const templateLocked =
    notesBusy || libraryInvalid || libraryPending || !connected;
  const builtIn = template.id === "speaker-name-candidates-v1";
  // The shipped prompt carries the speaker evidence rule; duplicate it to change steps.
  const stepsLocked = templateLocked || builtIn;
  const editableTemplateCount = templates.filter(
    (t) => t.id !== "speaker-name-candidates-v1",
  ).length;
  const visibleSteps = result ? result.sections : template.steps;
  const activeId =
    (result ? resultSection?.id : selectedStep.id) ?? visibleSteps[0].id;
  const tabId = (id: string) => "analysis-tab-" + encodeURIComponent(id);
  return (
    <>
      <section className="notes-meeting-picker">
        <label>
          {t.chooseMeeting}
          <select
            value={meetingId}
            disabled={!connected}
            onChange={(event) => selectMeeting(event.target.value)}
          >
            <option value="">{t.selectMeeting}</option>
            {meetings.map((item) => (
              <option key={item.id} value={item.id}>
                {meetingLabel(item, locale)}
              </option>
            ))}
          </select>
        </label>
        {meetingLoading && (
          <p className="muted">
            <RefreshCw size={14} className="spinning" />
            {t.busy}
          </p>
        )}
      </section>
      {!meeting && <div className="notice warning">{t.noMeeting}</div>}
      {meeting && meeting.status !== "complete" && (
        <div className="notice warning">{t.pendingNotes}</div>
      )}
      {browserInvalid && (
        <div role="alert" className="notice error">
          {t.templateError}
        </div>
      )}
      {libraryError && (
        <div role="alert" className="notice error">
          <div>
            <strong>{t.templateLoadFailed}</strong>
            <p>{libraryError}</p>
          </div>
          <button
            className="secondary"
            disabled={libraryPending}
            onClick={refreshTemplates}
          >
            {t.refreshTemplates}
          </button>
        </div>
      )}
      <section className="notes-config">
        {!setupOpen && !notesBusy && modelDiscoveryError && (
          <div role="alert" className="notice warning analysis-discovery-error">
            <p>{modelDiscoveryError}</p>
            <p>
              {locale === "de"
                ? "Starte deinen lokalen Modelldienst und aktualisiere die Liste."
                : "Start your local model server and refresh the list."}
            </p>
            <button className="secondary" onClick={() => setSetupOpen(true)}>
              {locale === "de" ? "Einrichtung öffnen" : "Open setup"}
            </button>
          </div>
        )}
        <details
          className="analysis-setup"
          open={setupOpen}
          onToggle={(event) => setSetupOpen(event.currentTarget.open)}
        >
          <summary>
            <strong>{t.analysisSetup}</strong>
            <span>
              {[
                textModel ||
                  (locale === "de" ? "Textmodell wählen" : "Choose text model"),
                template.name,
              ].join(" · ")}
            </span>
          </summary>
          <div className="analysis-setup-fields">
            {connectionSettings}
            <section
              className="notes-template-settings"
              aria-label={
                locale === "de"
                  ? "Vorlage und Arbeitsauftrag"
                  : "Template and instructions"
              }
            >
              <h3>
                {locale === "de"
                  ? "Vorlage und Arbeitsauftrag"
                  : "Template and instructions"}
              </h3>
              <div className="template-selector">
                <label>
                  {t.template}
                  <select
                    value={template.id}
                    disabled={templateLocked}
                    onChange={(event) => {
                      const next = templates.find(
                        (item) => item.id === event.target.value,
                      )!;
                      setTemplateId(next.id);
                      setStepId(next.steps[0].id);
                      setResultId("");
                    }}
                  >
                    {templates.map((item) => (
                      <option value={item.id} key={item.id}>
                        {item.name}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <details className="template-management">
                <summary>
                  {locale === "de" ? "Vorlagen verwalten" : "Manage templates"}
                </summary>
                <div className="template-actions" aria-label={t.template}>
                  <button
                    className="secondary"
                    disabled={templateLocked || templates.length >= 100}
                    onClick={() => openTemplateAction("new")}
                  >
                    <Plus size={15} />
                    {t.newTemplate}
                  </button>
                  <button
                    className="secondary"
                    disabled={templateLocked || templates.length >= 100}
                    onClick={() =>
                      openTemplateAction(
                        "duplicate",
                        result?.template ?? template,
                      )
                    }
                  >
                    <Copy size={15} />
                    {t.duplicateTemplate}
                  </button>
                  <button
                    className="text-button"
                    disabled={templateLocked || builtIn}
                    onClick={() => openTemplateAction("rename")}
                  >
                    <Pencil size={14} />
                    {t.renameTemplate}
                  </button>
                  <button
                    className="text-button"
                    disabled={
                      templateLocked || builtIn || editableTemplateCount < 2
                    }
                    title={
                      editableTemplateCount < 2
                        ? t.lastTemplate
                        : t.removeTemplate
                    }
                    onClick={() => openTemplateAction("delete")}
                  >
                    <Trash2 size={14} />
                    {t.removeTemplate}
                  </button>
                  <button
                    className="text-button"
                    disabled={notesBusy || libraryPending || !connected}
                    onClick={refreshTemplates}
                  >
                    <RefreshCw
                      size={14}
                      className={libraryPending ? "spinning" : undefined}
                    />
                    {t.refreshTemplates}
                  </button>
                </div>
                <p className="muted">{t.templatesPortable}</p>
                {importBrowser && (
                  <details>
                    <summary>{t.importBrowserTemplates}</summary>
                    <p>{t.browserTemplatesHelp}</p>
                    <button
                      className="secondary"
                      disabled={templateLocked}
                      onClick={importBrowser}
                    >
                      {t.importBrowserTemplates}
                    </button>
                  </details>
                )}
              </details>
            </section>
          </div>
        </details>
        {showStart && (
          <div className="notes-run">
            <span>{meeting ? meetingLabel(meeting, locale) : t.noMeeting}</span>
            <button
              className="primary"
              disabled={
                !connected ||
                !meeting ||
                meetingLoading ||
                meeting.status !== "complete" ||
                recording ||
                notesBusy ||
                busy ||
                !textModel.trim() ||
                !modelReady ||
                !endpoint.trim() ||
                libraryInvalid ||
                libraryPending
              }
              onClick={beginNotes}
            >
              {notesBusy ? (
                <RefreshCw size={17} className="spinning" />
              ) : (
                <Sparkles size={17} />
              )}
              {notesBusy ? t.generating : t.startAnalysis}
            </button>
          </div>
        )}
      </section>
      <section className="analysis-panel">
        {analyses.length > 0 && (
          <label>
            {t.earlier}
            <select
              value={resultId}
              onChange={(event) => {
                setResultId(event.target.value);
                const next = analyses.find(
                  (item) => item.id === event.target.value,
                );
                if (next) setStepId(next.sections[0]?.id ?? "");
              }}
            >
              <option value="">{t.currentTemplate}</option>
              {analyses.map((item) => (
                <option value={item.id} key={item.id}>
                  {item.template.name} ·{" "}
                  {new Date(item.createdAt).toLocaleString(
                    locale === "de" ? "de-DE" : "en-GB",
                  )}
                  {item.currentPrompt === false &&
                    ` · ${t.earlierPrompt} (${t.readOnly})`}
                </option>
              ))}
            </select>
          </label>
        )}

        {result?.currentPrompt === false && (
          <p className="notice warning" role="note">
            {t.earlierPrompt}: {result.promptVersion} · {t.earlierPromptResult}
          </p>
        )}
        {result && (
          <div className="saved-template-notice">
            <details>
              <summary>{t.resultTemplate}</summary>
              <p>{t.historicalTemplateHelp}</p>
            </details>
            <button
              className="secondary"
              onClick={() => {
                setResultId("");
                setStepId(template.steps[0].id);
              }}
            >
              {t.editCurrentTemplate}
            </button>
          </div>
        )}
        <div className="tabs-toolbar">
          <div className="analysis-tabs" role="tablist" aria-label={t.notes}>
            {visibleSteps.map((item, position) => (
              <button
                role="tab"
                key={item.id}
                id={tabId(item.id)}
                aria-controls="analysis-step-panel"
                tabIndex={activeId === item.id ? 0 : -1}
                aria-selected={
                  (result ? resultSection?.id : selectedStep.id) === item.id
                }
                className={
                  (result ? resultSection?.id : selectedStep.id) === item.id
                    ? "analysis-tab active"
                    : "analysis-tab"
                }
                onClick={() => setStepId(item.id)}
                onKeyDown={(event) => {
                  if (
                    ["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                      event.key,
                    )
                  ) {
                    event.preventDefault();
                    const next =
                      event.key === "Home"
                        ? 0
                        : event.key === "End"
                          ? visibleSteps.length - 1
                          : (position +
                              (event.key === "ArrowLeft" ? -1 : 1) +
                              visibleSteps.length) %
                            visibleSteps.length;
                    setStepId(visibleSteps[next].id);
                    event.currentTarget
                      .closest('[role="tablist"]')
                      ?.querySelectorAll<HTMLElement>('[role="tab"]')
                      [next]?.focus();
                  } else if (event.key === "F2") {
                    event.preventDefault();
                    if (result) {
                      const saved = result.template.steps.find(
                        (step) => step.id === item.id,
                      );
                      if (saved) inspectStep(saved);
                    } else if (!stepsLocked) {
                      setEditing({ ...(item as AnalysisStep) });
                      setEditingNew(false);
                    }
                  }
                }}
                onContextMenu={(event) => {
                  if (result || stepsLocked) return;
                  event.preventDefault();
                  setEditing({ ...(item as AnalysisStep) });
                  setEditingNew(false);
                }}
              >
                {item.name}
              </button>
            ))}
          </div>
          {!result && (
            <button
              className="icon-button"
              disabled={stepsLocked || template.steps.length >= 100}
              title={t.addTab}
              aria-label={t.addTab}
              onClick={() => {
                setEditing({
                  id: uid(),
                  name: "",
                  kind: "analysis",
                  prompt: "",
                });
                setEditingNew(true);
              }}
            >
              <Plus size={19} />
            </button>
          )}
        </div>

        {result && (
          <div className="step-actions">
            <button
              className="text-button"
              onClick={() => {
                const saved = result.template.steps.find(
                  (item) => item.id === resultSection?.id,
                );
                if (saved) inspectStep(saved);
              }}
            >
              <Pencil size={14} />
              {t.inspectInstructions}
            </button>
          </div>
        )}
        {!result && (
          <details className="help-disclosure template-editor">
            <summary>
              {locale === "de"
                ? "Arbeitsschritte bearbeiten"
                : "Edit analysis steps"}
            </summary>
            <p className="muted">{t.tabKeyboardHelp}</p>
            <p className="muted">{t.instructionHelp}</p>
            <div className="step-actions">
              <button
                className="text-button"
                disabled={stepsLocked}
                onClick={() => {
                  setEditing({ ...selectedStep });
                  setEditingNew(false);
                }}
              >
                <Pencil size={14} />
                {t.editTab}
              </button>
              <div>
                <button
                  className="icon-button"
                  title={t.moveLeft}
                  aria-label={t.moveLeft}
                  disabled={
                    stepsLocked || template.steps[0].id === selectedStep.id
                  }
                  onClick={() => moveStep(-1)}
                >
                  <ChevronLeft size={16} />
                </button>
                <button
                  className="icon-button"
                  title={t.moveRight}
                  aria-label={t.moveRight}
                  disabled={
                    stepsLocked || template.steps.at(-1)?.id === selectedStep.id
                  }
                  onClick={() => moveStep(1)}
                >
                  <ChevronRight size={16} />
                </button>
                <button
                  className="icon-button"
                  title={t.removeTab}
                  aria-label={t.removeTab}
                  disabled={stepsLocked || template.steps.length < 2}
                  onClick={async () => {
                    const steps = template.steps.filter(
                      (item) => item.id !== selectedStep.id,
                    );
                    if (await updateTemplate({ ...template, steps }))
                      setStepId(steps[0].id);
                  }}
                >
                  <Trash2 size={15} />
                </button>
              </div>
            </div>
          </details>
        )}
        <div
          className="analysis-body"
          role="tabpanel"
          id="analysis-step-panel"
          aria-labelledby={tabId(activeId)}
          tabIndex={0}
        >
          {resultSection && rejectedCount(resultSection) > 0 && (
            <p className="muted rejected-note">
              {rejectedCount(resultSection)} {t.rejectedItems}
            </p>
          )}
          {resultSection ? (
            resultSection.items.length ? (
              <ul className="fact-list">
                {resultSection.items.map((fact, index) => (
                  <li key={index}>
                    <p>{fact.text}</p>
                    {resultSection.kind === "tasks" && (
                      <div className="fact-details">
                        {t.owner}: {fact.owner ?? t.unknown} · {t.due}:{" "}
                        {fact.due ?? t.unknown}
                      </div>
                    )}
                    <div className="evidence-list">
                      {fact.evidence.map((evidence, evidenceIndex) => (
                        <button
                          className="source-button"
                          key={evidenceIndex}
                          onClick={() => showEvidence(evidence)}
                          title={t.evidence}
                        >
                          [{evidence.sourceId}] <span>„{evidence.quote}“</span>
                        </button>
                      ))}
                    </div>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted">{t.noItems}</p>
            )
          ) : (
            <div className="empty">
              <Sparkles size={29} />
              <p>{t.noResults}</p>
            </div>
          )}
        </div>
        <div className="analysis-footer">
          <span>{result ? t.draft : ""}</span>
          <button
            className="secondary"
            disabled={!result || busy}
            onClick={() => result && void copyResults()}
            title={t.markdownHelp}
          >
            {copied ? <Check size={16} /> : <Copy size={16} />}
            <span>{copied ? t.copied : t.copyMarkdown}</span>
          </button>
        </div>
      </section>
    </>
  );
}
function rejectedCount(section: AnalysisResult["sections"][number]): number {
  return Object.values(section.rejected ?? {}).reduce(
    (sum, count) => sum + (count ?? 0),
    0,
  );
}
