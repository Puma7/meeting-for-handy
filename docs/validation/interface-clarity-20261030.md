# 30 October 2026 – separate interface clarity follow-up

The recording source cards now share one heading/control/meter structure. In a
rendered synthetic recording the prior 7 px meter offset became 0 px at 1025 px and
800 px viewport widths. At 600 px the sources intentionally stack; no horizontal
page overflow was observed. Both light and dark views were inspected with synthetic
IPC and text only: no actual capture, transcript, model server or credentials.

Audio-level guidance and live-summary options are named disclosures. Model setup,
per-run cloud consent, capture warnings and incomplete-result notices retain their
gates. Post-meeting configuration and step editing start collapsed; settings group
optional setup and maintenance. Original warning text remains available, with an
explanation that a driver flag proves neither an audible gap nor uninterrupted
capture. CPU/RAM use whole-computer percentages, RAM exposes used/total GiB, and
GPU usage remains unmeasured. Positive transcription backlog is no longer labeled
up to date merely because it is below 30 seconds. Existing transcript order and
30-second recording/section behavior are unchanged.

All 226 frontend tests and TypeScript passed. The production frontend build and
`./verify.ps1` passed locally; final source-bound checks are recorded with the PR.
This is presentation work, not a new physical audio, text-model quality, GPU or
endurance proof. Native recording recovery, provider transport, prompts and model
selection policy are unchanged. `docs/UI_GUIDELINES.md` records the shared layout,
action hierarchy, progressive help and honest status conventions.

The analysis page exposes one start action at a time. The persistent status
bar remains the place for progress, cancellation and errors. The original full
report is retained without modification in this interface change.


## Follow-up after the first review

CodeRabbit correctly found that RAM amounts and the GPU limitation were exposed
only through pointer tooltips. They now appear in a keyboard-accessible native
computer-details disclosure. A separate review reproduced a running archive job
and its cancellation control disappearing inside collapsed maintenance settings.
The same single job observer now renders outside that disclosure, retaining its
existing callbacks and cancellation path without duplicate initial polling.
Both new synthetic regressions failed against the previous UI implementation.
These checks do not perform a real archive operation or measure GPU usage.

All 228 frontend tests, TypeScript, the production build and the required
`./verify.ps1` check passed after these fixes. A synthetic browser check opened
computer details with Enter, kept the running job outside closed settings,
and observed cancellation acknowledgement. No real archive was started.

## Visible local-model warning

Review 5710641785 identified the local-model memory warning inside closed
options. A short warning now remains visible before starting: extra memory use
can affect audio, so analysis can wait until recording has ended. The longer
explanation remains under options. The visibility regression failed before the
fix; all 229 frontend tests, TypeScript, the production build and the required
`./verify.ps1` check pass afterward. A synthetic browser check confirmed the
warning is outside the still-closed options.

The historical [macOS ARM job on ea1e080](https://github.com/Puma7/meeting-for-handy/actions/runs/41035957251/job/123996754171)
failed the synthetic reconnect-progress test's 150-ms test deadline. The actual
macOS application uses a 60-second no-progress readiness limit. Neither deadline
was changed or skipped. Fresh complete target CI remains required; this report
does not turn the historical failure into a success or establish hardware quality.

## Single analysis duration and visible capture caveats

The analysis status bar now shows elapsed time once in clock format, retaining
the accessible label, stopped-state duration, error, progress and resume controls.
The duplicate spelled-out seconds row and its unused translation keys/styles are
removed. The existing failure/progress regression was strengthened and failed on
the duplicate display before the fix. A synthetic browser case shows one `00:15`
value alongside a failed analysis and preserved partial progress.

Review 5710797020 also identified capture caveats hidden behind the outer warning
disclosure. That disclosure now starts open; nested original technical messages
remain collapsed. The corresponding existing synthetic regression failed before
the correction. All 229 frontend tests, TypeScript, the production build and the
required `./verify.ps1` check pass for these additions. Provider transport, output
limits and incomplete-response validation are unchanged. No private provider logs
or meeting diagnostics are part of the public source evidence.

## Compact status and legacy import visibility

Full review 5711425553 identified two further UI issues. The normal two-line
sidebar excerpt is now declared with its base styles, before the one-line
short-window and hidden compact-sidebar rules. Synthetic browser checks reproduce
the previous two-line excerpt at both 1280×600 and 600×900; the corrected layout
keeps one line in a short window and hides it in the compact sidebar.

The legacy import control remains outside collapsible maintenance settings,
including its busy/disabled state. Explanatory prose stays under optional opening
guidance, so progress visibility does not require restoring all help text to the
default settings view. A new synthetic regression starts the exact import
callback, closes maintenance settings while busy and verifies that its control
has no collapsed ancestor. It failed before the correction. No actual legacy
file is selected or modified by this test.

These changes affect only React/CSS and this sanitized note. Existing native
and provider behavior, dependencies, review rules and runtime limits are unchanged.

All 230 frontend tests, TypeScript, the production build, frontend formatting
and the required `./verify.ps1` passed for these corrections. Full byte comparison
against the previously published candidate confirms every native source file is
unchanged. The pre-existing historical report sections remain byte-identical;
later validation sections were appended under explicit publication approval.
Target CI and both providers must review the
new head before its review rounds can be complete.


## Review follow-up: errors, new warnings and compact computer details

Full CodeRabbit review 5713217416 found four visibility defects in candidate07.
Idle local-model discovery failures now remain visible while setup is closed.
Maintenance save/apply errors use the existing status outlet outside collapsed
settings. New capture warnings reopen their disclosure; identical polling updates
respect a user's decision to close it. Computer details remain accessible at
600px through a labeled icon and an expanded card above the main content.

Three new synthetic regressions and a strengthened resource-details assertion
fail on the previous implementation and pass after these corrections. For candidate08, all 233
frontend tests, TypeScript, the production build, frontend formatting and the
required ./verify.ps1 pass locally. Browser checks use only synthetic IPC data:
the 600px details card visibly paints RAM amounts and the unmeasured-GPU caveat
without horizontal overflow, and a local-model error is visible outside closed
setup in light and dark views. An initial browser check exposed a stacking-order
mistake; the corrected sidebar layer was checked again visibly. These are UI
checks, not physical capture, local model, GPU or endurance evidence.

Native source, provider transport, prompts, dependencies and review thresholds
are unchanged. The historical full report is now accompanied by this current
summary under the user's explicit permission to publish the complete report for
PR36. No private meeting text, session identity or provider diagnostic is added.
Fresh reviews and CI are still required on the new published head.


## Existing results, nested maintenance failures and dialog layering

The next full review identified discovery failures hidden beside an existing
analysis, child archive/audio-copy errors hidden after maintenance closed, and
the compact sidebar painting above modal dialogs. Discovery errors now remain
visible with a retained result. Archive restore/compaction, catalog failures and
audio-copy errors use the same outside status outlet; standalone panels retain
an inline fallback. The compact sidebar stays above ordinary content at layer2
and below the existing modal layer10.

Four new synthetic assertions failed on candidate08 and pass after these fixes.
All 237 frontend tests and TypeScript/production build pass locally. A 600px
rendered check reproduces the card and sidebar obscuring a modal before the
change, then verifies the dialog paints above them and the open card still
paints above ordinary content after the dialog closes, without horizontal
overflow. The browser and IPC data are synthetic; no real maintenance or model
operation is performed. Native code, dependencies and deadlines are unchanged.


## Candidate10: CPU details at narrow widths

At widths up to 740px, the compact navigation hides the regular recording facts.
The separate computer-details disclosure now includes the measured CPU percentage,
the same high-load threshold and an explicit unavailable state. It remains a
reading of the entire computer, not a measurement of this application's CPU use.
Three synthetic regressions for high, ordinary and unavailable CPU values failed
before the correction. The complete frontend suite now passes 240 tests, together
with TypeScript/production build. Native source and dependency bytes are unchanged.
A rendered synthetic 600px check confirmed the regular facts hidden while the
opened disclosure showed CPU 28%, memory amounts and the unmeasured-GPU limitation,
with no horizontal document overflow. No real hardware reading is claimed.

## Candidate 11: asynchronous setup errors and unfinished deletion

Speaker setup keeps its component mounted when its optional disclosure closes.
The settings view now supplies an alert container outside that disclosure; a
later model-check failure is rendered there without reopening the optional
controls or duplicating the alert. A full-App regression starts a deferred
synthetic check, closes the disclosure and rejects the check. The exact former
published speaker component fails the corrected test at its collapsed alert
ancestor; the new component passes. An earlier version of that fixture lacked
the optional start-speaker capability and is retained as a fixture failure,
not presented as proof of the application defect.

Successfully loaded unfinished permanent deletions also use the visible
maintenance status area, including their meeting identities, progress and
continuation controls. The typed confirmation stays with that visible notice;
loading a record never starts deletion. A synthetic regression loads a stopped
record while maintenance is closed, checks its visible identity, then requires
the exact typed confirmation before invoking its mocked continuation command.
The former notice failed the collapsed-ancestor assertion. The warning uses a
stacked layout so its heading, explanation and confirmation remain readable at
narrow widths instead of competing as flex-row columns.

The previous documentation statement now explicitly applies only to pre-existing
historical report sections; later sections were appended under the user's
publication approval. Every native source remains byte-identical, and candidate
11 also leaves the complete candidate 10 TEST_REPORT.md byte-identical.

The full frontend suite passes 242 tests. TypeScript/production build and frontend
formatting pass. Rendered checks with synthetic IPC at 1280 px and 390 px show
the late error and unfinished-deletion controls outside closed settings, with
no horizontal document overflow or browser console errors. The final destructive
action remains disabled without typed confirmation; no native deletion, model
check, real meeting or audio device is invoked by this browser fixture. Native,
physical-device, model-provider, GPU and endurance evidence is unchanged.

## Candidate 12: keep the remedy and acknowledgement with the setup error

The speaker-check feedback region now includes its archive remedy and archive
completion notice, using the same visible status container as the setup error.
Its inline fallback and all native capability/admission conditions remain
unchanged. A new full-App regression reproduces the hidden archive action on
candidate 11, then checks that both the explicit mocked archive action and its
completion notice remain outside closed settings. No archiving starts merely
because the error arrives. The design guide now requires an actionable error,
its remedy and its acknowledgement to share a visible status region.

The actual candidate 11 Intel CI failed one existing program-selection fixture:
the old model option remained present in the scenario intended to simulate a
failed fresh-state read after a confirmed native choice. The log alone does not
establish a production failure. That fixture armed its one-shot state error
before userEvent prepared the chooser click, allowing an unrelated idle poll to
consume it. The fault is now armed inside the confirmed chooser, and the test
additionally proves that the chooser ran. All existing blocked-start, old-option
and explicit recovery assertions remain; no timeout, assertion or test is removed.
Actual fresh Intel CI is required before calling the platform check successful.

All 243 frontend tests and TypeScript/production build pass locally. Synthetic
rendered checks at 1280 px and 390 px confirm that the archive remedy and its
completion notice remain outside closed speaker settings, without horizontal
overflow or browser console errors. They invoke only a mock archive callback.
Native sources, dependencies, review rules and the complete candidate 11
TEST_REPORT.md remain byte-identical. No real meeting, check archive, model
service or audio device is opened or changed by these fixtures.

## Candidate 13: visible recovery controls and polite live warning additions

Closed analysis setup now keeps the local discovery error, a short next action
and an explicit setup-opening button together. Only the click opens setup. An
invalid cloud endpoint keeps the existing text-model settings action beside the
visible model status while Start remains disabled; the same action is moved
out of the closed options rather than duplicated. No model request or provider
change is triggered by either recovery control.

The live recording view opts into a persistent polite, atomic warning region.
It announces only added warnings after initial render, not the initial warning
list or unchanged polling arrays. Empty polls keep the region mounted, removals
clear the old announcement, and historical meeting warnings do not opt in.
Existing disclosure reopening, deduplication, literal text rendering and manual
closure are retained. This validates DOM accessibility semantics, not actual
screen-reader speech or a real driver event.

All three focused regressions fail on the prior candidate 12: absent discovery
remedy, settings action inside closed cloud options and absent warning live region.
After correction all 246 frontend tests and TypeScript/production build pass.
The complete-App recording regression also verifies the live-view opt-in. Native
sources, dependencies, protection/review settings and the entire published
candidate 12 TEST_REPORT.md remain byte-identical; native suites were not rerun.

Synthetic browser checks confirm the cloud action at 1280 px and the discovery
error/remedy outside closed setup at 390 px, with no horizontal overflow. A
synthetic polling addition populates only the new warning in the polite region;
initial warnings leave it empty. These controls use mocked callbacks and no
real provider, audio device, archive, deletion, filesystem or clipboard action.
Physical recording, permissions, provider, GPU and combined endurance limits
remain open. Current target CI and two fresh clean reviews per provider on this
same final head remain required.


## Integration with the merged portable checks (31 October 2026)

The UI changes were combined with the actual merged PR35 source before merging
PR36. The two already approved and published report additions are retained byte
for byte after their single shared report prefix. No private report preparation
was used.

The first PR35 run on main failed the synthetic startup-progress readiness test
on macOS ARM. The same wall-clock-dependent fixture had already been investigated
and corrected in PR38. This integration carries that reviewed fixture correction
forward while retaining the PR35 passive startup timing API. A scoped synthetic
clock tests both deadline renewal and expiry after progress stops. Release clock
reads, capture time limits and privacy behavior remain unchanged. The failed run
is retained; no actual scheduler trace or physical device cause was established.

Fresh local integration checks pass 352 Rust tests with two existing special
entry points ignored, Clippy with warnings denied, and Rust formatting. The
unchanged UI bytes pass 246 tests, TypeScript, production build and formatting.
The unchanged Python checks run 124 cases: 123 pass and one Unix-only case is
regularly skipped on Windows. These are synthetic checks, not new physical
recording, screen-reader speech, GPU or endurance evidence. A restricted local
rerun could not start some fixture subprocesses or local test servers; the same
suite passed with the access those fixtures require. No checks or deadlines were
relaxed. Current target CI and reviews must be read on this integration head;
earlier reviews remain evidence only for their original heads.

The required project verification also passed: 47 PCM, 35 notes,
28 template and 8 agent checks, plus the monitor checks.
This is the existing synthetic project verification, not a hardware or installed
application test.


### Integration follow-up: empty cloud model

The first full integration review found one additional recovery state: a valid
HTTPS cloud endpoint with an empty model disabled Start while its settings remedy
remained inside the closed options disclosure. The visible remedy now covers
both invalid endpoints and empty models. Opening options still shows exactly one
settings action. German and English component regressions failed before the fix
and pass afterwards; they also assert disabled Start, no automatic request and
no implicit consent. All 248 frontend tests pass. These use synthetic IPC and do
not send text to a provider. The initial restricted test process failed before
running any cases and is retained separately from the two actual before failures.
The readiness test contracts also have explicit function documentation; the
80-percent documentation requirement remains unchanged.

TypeScript, the production build, UI formatting, Rust formatting and a fresh
required project verification pass after this follow-up as well. The 352-test
native result belongs to the same executable native code: only documentation
comments were added to the readiness test functions afterwards.
