# Local text model residency — 2026-10-30

The behavior and provider boundaries below describe the combined implementation. Candidate and reference sections retain historical results and the draft, integration and review gates that applied to their named revisions; those statements do not describe the latest PR status. Current status: [PR37](https://github.com/Puma7/meeting-for-handy/pull/37) and [PR38](https://github.com/Puma7/meeting-for-handy/pull/38).

Scope: HandyMeeting 1.0.7 development sources, based on `f2ccb8a8a268323da5ea0ad8ed1eb69102694da6`. This change does not publish or install a new application version.

The current runtime contract is in Behavior and Provider boundaries below, with
the later Conservative Ollama final logical release section supplying details.
Evidence sections retain the actual historical candidate states and measurements.
Their earlier draft/unavailable-provider or eager-cleanup statements describe
those snapshots, not the current runtime policy or current GitHub merge status.
The latest completed selected-provider reference and final-head CI/reviews govern
validation status; references 01–03 predate the concrete-origin correction below
and are not reassigned to that changed runtime.

## Behavior

Previously, keeping a text client alive did not prevent the provider's idle policy from unloading its model between live updates. Known local Ollama and LM Studio connections now reserve their selected model explicitly. Generic local OpenAI-compatible servers retain their existing transport behavior; residency is not claimed for them.

The live worker loads before waiting for transcript updates. The reservation survives capture ending, recognition catching up, final windows and final overview persistence. Failed or partial finalization retains the reservation for retry. A successful post-meeting analysis can release an inactive live reservation only when its source hash covers the current completed transcript. An older resumed snapshot cannot release newer work. Completion in another meeting cannot release this meeting's unfinished work.

Live notes and post-meeting analysis share per-model ownership. Releasing an anchor does not unload a model while an in-flight client or another job still uses it. Explicit cancellation also releases its anchor; recording and recognition remain independent.

Cleanup is also deferred across separate loopback spellings of the same provider/model. Their actual routes, credentials and instance IDs remain separate. Conservatively grouping distinct local servers may keep one warm longer; it does not redirect requests between servers.

## Provider boundaries

- Ollama: app-owned native `/api/generate` loads and `/api/chat` inference use a finite 300-second lifetime. Each finite slot has one weakly owned worker that renews it every 60 seconds, including idle intervals without inference and failed/partial work awaiting retry. Renewals use per-slot state; final cleanup and new reservations serialize within a provider/model family. The global reservation registry is released before management or a slot-state wait. Same-family startup can still wait for a pending bounded load or renewal state transition. Messages, JSON schema and output budget are unchanged. Final logical release stops this app's renewal and sends no unload or TTL-restore request. The existing finite server lifetime (normally up to 300 seconds after the last app request) expires naturally unless another client extends it; an initially present indefinite model stays loaded. See Conservative Ollama final logical release below for the tested ownership limits.
- LM Studio: native REST v1 explicitly loads the chosen catalog key. Inference addresses the returned instance ID. Only a newly returned instance absent from the pre-load inventory is eligible for app cleanup. A reused existing instance is rejected with a clear error because the app cannot establish its pin or ownership. Cleanup must acknowledge the exact owned instance ID. Native REST v1 support is required; no silent provider or API fallback occurs.
- Native loading is bounded to 300 seconds. Once submitted, its acknowledgement is read even after cancellation so a confirmed LM Studio instance remains tracked for exact cleanup. Ollama logical release instead ends renewal without changing its existing server lifetime. User cancellation is still recorded immediately; a replacement live worker waits for its predecessor to finish. Inference requests remain cancellable.
- Native management and leased inference use the same concrete origin throughout the lifecycle. The existing localhost canonical identity selects 127.0.0.1; an IPv6-only provider requires an explicit [::1] endpoint. Unleased legacy text and discovery retain their dual-loopback behavior; stored credential and resume identities are unchanged. Distinct same-port IPv4/IPv6 listeners cannot substitute for one another during native inventory, loading, inference or cleanup. This binds the network origin, not a process that later replaces a server at the same address and port.
- An already loaded selected Ollama model must have a readable RFC3339 expires_at before mutation. Missing, null, numeric, empty or invalid values cause a compatibility refusal. No indefinite lifetime is guessed from unreadable inventory. Valid readable finite/indefinite policies and the existing 300-second / 60-second bounds are unchanged; real Ollama compatibility remains untested.
- Management uses the existing loopback-only, no-proxy, no-redirect transport. It receives provider/model metadata and credentials only, never session objects or audio-file access. Management errors do not expose server response bodies. Cloud consent and source validation remain unchanged.

These are application-owned reservations, not exclusive locks against another program or a user changing the model server. Server shutdown, manual unload, app termination, a management timeout without an acknowledgement or concurrent external instance changes can invalidate residency or leave an unconfirmed server-side load. No foreign instance is guessed and unloaded to repair such uncertainty. An unconfirmed LM Studio load or exact-instance cleanup prevents unconfirmed reuse and requires checking the local service. A fresh idle Ollama reservation must re-read native inventory and confirm its load before inference; final logical release itself sends no provider mutation.

## Evidence and remaining gates

The new component tests use synthetic loopback HTTP services. They cover virtual idle expiry, load-once behavior, shared users, foreign instance preservation, native/OpenAI request contracts, truncated responses, failure/retry, cancellation during loading, acknowledgement validation, credential/route boundaries, and retained live windows through overview retry. Coordinator tests cover separate meetings, independent live/post owners, stale completion and retained in-flight clients. The virtual clock is not a real-duration, hardware or performance measurement.

The idle regression fails with the previous unreserved client; the cancellation regression fails when a submitted native load is abandoned before its acknowledgement. Both pass with the corrected implementation.

For published candidate 02, the prescribed `verify.ps1` passed: 47 PCM, 35 notes, 28 template and 8 agent checks, plus monitor checks. The unchanged frontend passed 225 tests, TypeScript and production build. All-target Rust tests passed: 365 passed, zero failed, two existing dedicated entry points normally ignored (native credential-provider roundtrip and crash-writer child). Workspace/all-target Clippy passed with `-D warnings`, and formatting and diff checks passed. The final local run used four test threads and two build jobs; no test deadline changed.

A concurrent repeat of the legacy checks timed out during an agent exchange; the sequential repeat passed with the same check and unchanged deadline. The initial full Rust build exhausted local disk space; only this task's fresh build cache was removed, then the checks were rebuilt with debug symbols and incremental compilation disabled. Prior logs remain retained. No product test, warning policy or CI threshold was weakened.

For candidates 01–05, actual Ollama and the configured LM Studio server were unreachable during read-only probes. Those historical probes and synthetic fixtures remain retained. The selected LM Studio provider now has the real positive/nonbinding, idle and release reference documented below. Actual Ollama compatibility remains untested. Existing capture interruption, signing/notarization and platform limitations remain open. The full historical `TEST_REPORT.md` is unchanged.

Official protocol sources: [Ollama keep-alive](https://docs.ollama.com/faq), [native chat](https://docs.ollama.com/api/chat), [LM Studio idle TTL and auto-evict](https://lmstudio.ai/docs/developer/core/ttl-and-auto-evict), [load](https://lmstudio.ai/docs/developer/rest/load), [list](https://lmstudio.ai/docs/developer/rest/list), [unload](https://lmstudio.ai/docs/developer/rest/unload).


## Crash and idle-renewal follow-up

An app crash formerly abandoned an app-owned `keep_alive: -1` load. On restart,
its far-future expiry could be mistaken for an unrelated indefinite model and
conservatively retained forever. App-owned loads and inference now set 300 seconds
and renew while an actual owner remains. This avoids creating that indefinite
post-crash state without guessing ownership of any existing foreign pin. A
preexisting indefinite model remains indefinite. A finite borrowed model retains
its prior longer finite grace. Each slot serializes renewal with final release,
so a late refresh cannot reload a released slot. Renewal failure is checked before
inference; a successful retry must receive a fresh provider acknowledgement.

A new test exits only its own synthetic child process without destructors. It
genuinely failed before the correction with two indefinite request lifetimes;
the corrected load and inference have bounded lifetimes, and a subsequent
synthetic app can own and release a fresh load after simulated provider expiry.
Two additional synthetic HTTP tests wait through the production 60-second timer:
an owner renews without inference, and failed renewal blocks text until retry.
All 16 provider protocol tests passed in 60.09 seconds. These are not real-model
residency, memory, performance, GPU, device or long-meeting measurements.

Actual Ollama and configured LM Studio remained unreachable in probe 05. The real
positive/nonbinding local reference is still required; the PR remains a draft.
Unexpected app suspension longer than the finite lease, server shutdown/manual
unload, unacknowledged management and external server changes remain boundaries.
Existing foreign indefinite models are deliberately not unloaded to hide them.

The follow-up also passed 368 Rust workspace/all-target tests, with the same two
existing dedicated entry points normally ignored. Strict workspace/all-target
Clippy passed with warnings as errors. Frontend source bytes are unchanged from
the prior 225-test proof; no new frontend run is claimed. The final required
project check and publication fingerprint are recorded with the exact candidate.


## Candidate 04: provider retry and conservative LM Studio admission

A transient preload failure no longer terminates the live worker. Cancellation
still releases its owner and stops it; other failures stay visible while the
existing update loop can prepare again through its normal retry/backoff. A
synthetic 503 provider refusal made the former worker end with failed; the same
actual worker now stays waiting with its owner retained and ends only after an
explicit stop. This does not show that an unavailable real server becomes ready.

An idle Ollama route can recover from an unconfirmed finite release. A new
reservation discards only the idle cleanup-failure latch; prepare must re-read
/api/ps and obtain a fresh load acknowledgement before inference. LM Studio's
unconfirmed exact-instance unload remains blocked. The synthetic Ollama retry
failed with the former permanent latch and now demonstrates the new provider
state query; foreign indefinite/finite policies remain preserved.

Before native LM Studio loading, the selected catalog entry must have a readable
empty loaded_instances list. A pre-existing selected-key instance causes a clear
failure before a load, unload or inference request. It is not borrowed, claimed,
forcibly evicted or duplicated, and no residency guarantee is made for it. The
user must resolve that selected model's use in LM Studio before a dedicated app
reservation is possible. Other catalog keys and foreign instances remain intact.
The synthetic already-loaded-key regression failed before this preflight check.
This policy deliberately avoids depending on unverified REST duplicate-load
semantics; actual provider compatibility remains a draft gate.

A submitted native LM Studio load with no validated new instance acknowledgement
is now an uncertain outcome, retained on that route. Automatic retry never submits
a second load and never guesses an instance to unload. A synthetic unacknowledged
load previously produced two load submissions; it now produces only one. The
existing bounded 300 s acknowledgement wait, cancellation handling and exact unload
identity verification are unchanged. An HTTP error is conservatively uncertain,
not proof that the provider created nothing.

### Manual recovery of an uncertain LM Studio load

1. Stop initiating further app analyses on that route. Inspect the provider's
   actual loading status; a client timeout does not prove server-side cancellation.
2. Wait until LM Studio confirms that pending loading finished or was aborted.
   Check its instance list and its own operation records. Do not unload an instance
   merely because it appeared near the failed request or has the same model key.
3. Remove an instance only when those provider records establish that it belongs
   to this app's failed operation and no parallel user needs it. If ownership
   cannot be established, leave it untouched; recovery is not proven safe.
4. Retry app-owned loading only after the selected key has no loaded instances
   and the provider confirms no pending load. Restart the app to reset that
   uncertain route, then prepare and validate a fresh acknowledgement.

This is explicit manual recovery, not an automatic ownership reconstruction or a
crash-cleanup guarantee. Native provider versions, real idle residency, foreign
instance preservation, positive/nonbinding content and durable final cleanup
still need the real local reference test. The P1 remains open and the PR remains
a draft while both configured services are unreachable. No service, model or
private meeting was started/used by these protocol regressions.

The corrected candidate passed 371 workspace/all-target Rust tests, with the two
existing special entry points normally ignored. All 18 provider protocol cases
passed, including the actual 60-second renewal timers and finite-TTL own-child
termination test. The exact native preload regression passed. Frontend bytes
remain identical to the earlier 225-test/TypeScript/build proof; no new frontend
run or real-provider result is claimed. Full lint/format/verify outcomes are
recorded with the immutable candidate proof.

## Candidate 06: actual selected LM Studio reference — 2026-10-31

The maintainer explicitly authorized starting the existing local service and using
the already installed selected model, with no downloads or user meetings. The
existing LM Studio service was started on loopback port 1235. No application was
installed or replaced. All 2,979 published source files still matched candidate
05 during the reference; candidate 06 changes only this validation note.

Environment: Windows 11 Pro build 26200, Intel Core i7-13700 (16 cores / 24 logical
processors), 66,838,500 KiB visible physical memory, NVIDIA RTX 4070 with 12,282 MiB
device memory and driver 621.01. Provider: LM Studio 0.4.26+4, CLI commit dcefd5a.
Selected installed model: `google/gemma-4-e4b`, Gemma 4 E4B, GGUF Q4_K_M,
6,326,936,720 catalog bytes. Native REST v1 reported an 8,192-token context,
Flash Attention and GPU KV-cache offload. These settings were observed, not
changed to fit an outcome. Notes used `handymeeting-rust-notes-3`; live notes used
`handymeeting-live-notes-2`.

The private executable used the unchanged production `meeting-notes` crate and
the unchanged desktop `model_sessions` coordinator module. Its own build was
offline and locked, with warnings as errors; every dependency name, version,
source and checksum matched the production Cargo.lock. It used new private
output directories and artificial conversation text only.

The actual run passed in 461.270 seconds:

- Native REST v1 acknowledged loading the selected existing model and returned an
  instance ID. A separate test owner first loaded that same selected model. The
  production pool refused to adopt or duplicate this already loaded instance;
  the complete native inventory remained equal. Only the instance established
  by that test owner's acknowledgement was then unloaded, with an exact-ID
  acknowledgement. No unrelated model was loaded for this check.
- The application pool then loaded its own selected instance. The preregistered
  idle interval was 330 seconds without inference or repeated load requests.
  Eleven read-only inventory samples at approximately 30-second intervals kept
  the same single instance; measured idle time was 330.180 seconds. Dropping
  transient work handles retained the actual coordinator's retry anchor. A fresh
  handle reused the same instance after the idle interval.
- Post-meeting and live inference both produced one explicit decision and one
  explicit task from the positive text, with the literal named owner and due
  expression. Both produced zero decisions and zero tasks from the nonbinding
  text. Production validators checked existing source IDs and literal quotes.
  The positive live run stored two windows and a final overview of five items;
  the nonbinding live run stored two windows and a final overview of two items.
  These are checked outputs for these inputs, not a general semantic guarantee.
- Each post-meeting case was interrupted after its first durable step and
  resumed from disk. Both original snapshots and first checkpoints remained
  byte-identical, and the two confirmed steps were not requested again. Finished
  results were read back through the production result validator.
- Completing the post owner preserved the live owner. Live windows and the
  final overview were read from disk before the harness invoked the production
  coordinator's completion method. Remaining client leases delayed unloading;
  dropping the last client released the owned instance. Fresh native inventory
  confirmed no loaded instances and the complete original catalog unchanged.

Working Set and Private Bytes were sampled separately per process. The observed
`llama-server` peaks were 6,803,124,224 Working Set bytes and 6,855,098,368 Private
Bytes; these distinct measures are not added together. NVIDIA's whole-device
memory-used counter peaked at 8,286 MiB and read 3,599 MiB after release. That GPU
counter includes the desktop and other applications; it is not model-exclusive
VRAM or proof of complete RAM/VRAM reclamation. Sampling targeted five seconds,
with actual timestamp gaps retained in private evidence. Existing desktop
applications remained running. No capture or speech-recognition load was started.

The reference establishes this installed LM Studio/model combination and the
observed idle interval. There were no unrelated pre-existing loaded instances,
so actual foreign preservation here covers a separately test-owned instance of
the selected key; nonempty unrelated-key preservation remains synthetic protocol
coverage. It was one private harness process using the real provider and original
core/coordinator modules, not an installed-app UI test, independent application
process test, automatic full-worker finalization test, crash test, physical
capture test or combined two-hour endurance test. Actual Ollama and generic
OpenAI-compatible residency are not established by this LM Studio run. External
server changes, suspension and unacknowledged management remain the boundaries
described above. The PR remains a draft and this evidence does not authorize a
merge, release, version bump or installed update.

Raw artificial conversations, local paths, instance records and process samples
remain private. Only this sanitized note is published; TEST_REPORT.md remains
byte-identical. Fresh final-head reviews and CI are tracked separately from the
older candidate results.

## Candidate 07: isolated final cleanup and repeated actual reference

The prior complete review's retained cleanup-lock concern was validated against
candidate 06. Its final lease drop kept the global pool registry locked while
waiting for slot state and native provider cleanup. Thus unrelated model
reservations could wait for another model's bounded release. The earlier broad
statement that management never held that registry was inaccurate for cleanup.

Two new synthetic wire regressions hold an actual Ollama release or native LM
Studio unload response pending. An unrelated model must reserve, infer and
finish before that response is released. Both failed on candidate 06 at the
intended isolation assertion and passed after correction. An initial run lacked
permission to reach the fixture's loopback sockets; it is retained as a setup
failure and is not the before-regression evidence.

Slots now share a transition gate for their provider/model cleanup family.
Registry lookup/insertion stays brief. Final cleanup snapshots the same eligible
slots, releases the registry, and retains only the family gate during slot-state
waits and provider acknowledgement. A concurrent new loopback alias receives the
same gate before it becomes usable. A separate regression checks that it waits
for the old cleanup, then loads and retains its own lease until its last user.
Existing alias and failed-cleanup recovery cases still pass. Routes, credential
digests, exact unload eligibility, instance acknowledgements, retry policies,
300/60-second Ollama timing and 10-second management deadlines are unchanged.
This isolates different model families; it does not promise immediate startup
for the same family or independently running applications.

Fresh final product checks passed: 374 Rust workspace/all-target tests, with the
same two special entries normally ignored; all 21 provider protocol cases;
workspace/all-target Clippy with warnings as errors; and Rust format check.
Dependencies remained locked and offline. The first workspace attempt omitted
the existing native SDK environment and stopped before tests when a fallback
source build could not find CMake. The successful repetition used the existing
pinned 0.4.1 SDK read-only, without downloading or changing it. Frontend bytes
remain identical to the prior 225-test/TypeScript/build proof; no new UI run is
claimed.

The actual selected-provider reference was repeated on the corrected production
core with the same private harness, unchanged desktop coordinator, installed LM
Studio 0.4.26+4 and existing Gemma 4 E4B Q4_K_M model. The hardware, model size,
context and native API scope are those recorded for candidate 06. No source file
changed during the repeated run; only this sanitized note was updated afterward.

The run passed its preregistered lifecycle and positive/nonbinding checks in
464.763 seconds. Idle time was 330.328 seconds with eleven read-only samples of
the same single instance, without inference or repeated loading. The separately
test-owned selected-instance refusal/preservation, native owned load,
post-completion/live-owner isolation, two preserved interrupted/resumed post
checkpoints, durable live window/overview readback and final exact-instance
unload all passed again. Both live and post outputs retained one explicit
decision and one explicit task for positive text, versus zero of each for
nonbinding text. The valid task retained the literal owner/due expression; an
unanswered question retained unknown owner and due date.

The positive final live overview stored five accepted items. One model-produced
overview item was rejected by the unchanged `ownerOrDueOutsideTasks` rule and
omitted; the valid explicit task remained in the separate task list. The
nonbinding overview stored two items. This rejection is retained and reported,
not hidden by repeating the run or relaxing the validator. Source-ID/literal
quote validation and these checked cases do not establish general semantic
correctness, complete overview coverage or a zero-rejection model guarantee.

Ninety-four memory samples targeted five-second intervals, with actual gaps
retained. The highest observed individual `llama-server` peaks were
6,803,718,144 Working Set bytes and 6,854,184,960 Private Bytes, reported separately
rather than added. Whole-device GPU memory used read 4,014 MiB before inference,
peaked at 9,240 MiB and read 4,312 MiB after release. It includes other desktop
activity and is not model-exclusive memory or proof of complete reclamation.
Owned Rust tests/Clippy overlapped part of this run, using two build jobs and
four test threads; existing desktop applications remained running. No capture
or ASR load was started, and these observations are not a performance benchmark.

Fresh native inventory again matched the complete original catalog, with no
loaded instances. The same limits apply: one private harness process, no
installed UI/full-worker or independent application-process proof, no actual
Ollama reference, no user meetings or downloads, and no physical capture or
combined two-hour endurance test. Raw synthetic content, paths, instance records
and process samples remain private. TEST_REPORT.md remains byte-identical.
Required final verification and fresh final-head review/CI receipts are tracked
separately; the PR stays a draft without a merge, release or installed update.

## Candidate 08: preserve Ollama alias cleanup policy

The first complete candidate-07 review round was not clean. Codex identified a
valid Ollama ownership-policy failure: a second loopback spelling could observe
the first slot's app-owned load as pre-existing finite residency. Arbitrary
HashMap cleanup order could then unload through the original slot and reload
through the later finite restoration, leaving the model warm after all users
finished. The old alias fixture always reported an empty inventory and could
not expose this stateful interaction.

A new stateful native-protocol fixture changes its actual inventory after load
and unload requests. Both load orders run on the same fixed two-slot registry,
so the test covers both cleanup orders independently of hash randomization.
Before correction, one initially absent case ended at 300 seconds rather than
unloaded; the opposite load order ended unloaded. Originally finite and
indefinite policies and an unrelated model are also checked. A separate fixture
uses equal selected names on two distinct local servers and verifies each
server's own original policy, including no unload request to the foreign one.

Cleanup now restores finite policies before performing app-owned unloads. Finite
restorations run in ascending lifetime order, so a longer observed foreign
policy is restored last. The existing family transition keeps these policies
stable; the shared registry remains released throughout state waits and network
requests. Every request still uses its original route, credential and model
identity. No server identity is guessed, no reservation authority is transferred
between servers, and no unload eligibility or provider deadline is broadened.
Both new stateful tests pass after correction; all prior regressions remain.

CodeRabbit's complete base-to-head round also identified missing first-load
guidance in the German README and an initially unverified matching English
documentation suggestion. The omission was independently confirmed against the
native preflight code and real selected-instance refusal. Both READMEs now say
that the selected LM Studio key must have no loaded instance for the first
app-owned load during preparation. They also state that an already confirmed
own reservation is reused and foreign instances are not adopted, duplicated or
unloaded. This is a preparation requirement, not a rule that the model must be
unloaded before starting the whole application. Link labels now describe the
actual reference evidence and remaining limits. No behavior was changed for this
documentation correction.

Fresh candidate-08 product checks passed: 376 Rust workspace/all-target tests,
the same two special entries normally ignored, all 23 provider protocol cases,
Clippy with warnings as errors, and Rust format check. The existing pinned SDK
and locked dependencies were used unchanged. Frontend bytes remain equal to the
prior 225-test/TypeScript/build proof; no new frontend run is claimed.

The same preregistered actual selected LM Studio/Gemma reference was repeated on
this corrected production core and unchanged desktop coordinator. All 2,979
source files were unchanged during the run; only the READMEs and this sanitized
note were updated afterward. Provider, model, hardware, prompt versions and
native API/configuration scope are those documented above. The original raw
records and all earlier outcomes remain private and preserved.

The repeat passed in 449.759 seconds, including 330.162 seconds idle without
inference or reload and eleven samples of the same single instance. Actual
selected-instance refusal/preservation, owned native load, positive/nonbinding
live and post outputs, two preserved resumed checkpoints, durable live final
readback, other-owner/client preservation and exact final owned unload passed.
Positive live/post cases retained one decision and one task; nonbinding cases
retained zero of each. The valid task kept its literal owner/due expression,
and the unanswered question retained unknown values. The positive final overview
again stored five items and rejected one `ownerOrDueOutsideTasks` item; the task
remained in its separate list. The nonbinding overview stored two items. The
unchanged validator and this semantic coverage limit are retained explicitly.

Ninety-one memory samples targeted five seconds, with actual gaps retained.
Highest individual `llama-server` observed peaks were 6,801,133,568 Working Set
bytes and 6,852,231,168 Private Bytes, reported separately. Whole-device GPU
memory used was 4,466 MiB before inference, peaked at 9,431 MiB, and was 4,365 MiB
after release. These device counters include other desktop activity; they are
not model-exclusive VRAM or a complete memory-reclamation claim. Own Rust tests,
Clippy and the harness build had finished before this repeat; existing desktop
applications remained running. No capture or ASR load was started.

Final fresh native inventory matched the original catalog and contained no
loaded instances. The limits remain: actual evidence for this installed LM
Studio/model combination and observed interval, synthetic Ollama behavior, no
installed UI/full-worker/independent app-process or physical capture proof, and
no combined two-hour endurance result. No downloads or user meetings were used.
TEST_REPORT.md, dependencies and protection/reviewer rules remain unchanged.
Final verification, immutable publication and two fresh final-head review rounds
per provider are tracked separately. The PR remains a draft; no merge, release,
version bump or installed update is authorized.


## Combined source validation before sequential merge (31 October 2026)

The PR37 model coordinator, provider transport and prompts retain their reviewed
candidate 08 code. The source is combined with the PR35/36 integration, including
the scoped readiness fixture and the cloud empty-model UI recovery correction.
Fresh native workspace/all-target checks pass 380 tests with two existing special
entries ignored; Clippy denies warnings, and Rust formatting and the required
project verification pass. The first native attempt stopped at the missing Tauri
frontend build prerequisite; that build was then produced from the locked local
package cache before the successful suite. This was not a runtime test failure.
The exact shared UI bytes pass the 248-case PR36 suite, TypeScript, production
build and formatting. Dependency locks, protection rules and version 1.0.7 remain
unchanged. TEST_REPORT.md inherits only the already approved public main report;
no private PR37 full report is added.

A new real local reference uses only the existing selected google/gemma-4-e4b
Q4_K_M model with the already running LM Studio 0.4.26+4 installation (CLI commit
dcefd5a rechecked), native REST v1 instance management and synthetic conversations.
The existing Windows 11/i7-13700/RTX4070 host and locked model dependencies are
recorded privately; GPU capacity is 12,282 MiB and the driver is 621.01. No model was
downloaded, and no user meeting or capture was used.

The native-instance idle interval is 330.2569121 seconds with 11 inventory samples,
no inference and no reload during idle. The full harness takes 619.6541797 seconds
(623.14 seconds for the measuring wrapper). The independently preloaded selected
fixture instance is refused before app load and preserved. Durable post-step
cancellation/resume preserves checkpoints and reuses confirmed steps. Live window
and overview files are read back before completion. Other coordinator owners and
in-flight clients delay final cleanup; the exact owned instance is released only
after the final client ends. Final inventory equals the original empty loaded
state, and the installed model catalog is unchanged. No unrelated model was
loaded just to produce a foreign-model demonstration; that case remains protocol
fixture evidence.

Actual positive live and post outputs each contain one decision and one task with
literal “Anna” and “Freitag” attribution from the German originals. Nonbinding live and
post outputs each contain zero decisions and zero tasks. Accepted source IDs and
literal quotations were checked against the synthetic originals; the unanswered
licensing question retains unknown owner and date. The final positive overview
accepts five items and rejects one; its persisted diagnostic retains the count,
not the rejection reason; the separate overview JSON records
ownerOrDueOutsideTasks: 1. The explicit task remains available separately. The
nonbinding post summary is empty. These observations are disclosed, with no
claim of complete semantics or zero rejection and no validator relaxation.

Memory is sampled 125 times at a nominal five-second interval. Maximum individual
llama-server Working Set is 6,802,272,256 bytes and maximum individual Private Bytes
is 6,851,588,096 bytes; these are separate per-process peaks, not simultaneous sums.
Whole-device GPU memory is 6,158 MiB before inference, peaks at 11,053 MiB and is 6,399 MiB
after release. GPU readings include other desktop load and do not prove exclusive
model usage or complete memory reclamation. Own Rust/Clippy/project checks and the
harness build ended before this run. Existing desktop applications stayed running;
private Git/source/ZIP comparisons and preparation of the separate PR38 source
also overlapped. No concurrent capture, ASR or model download was started.

This reference exercises actual production core/coordinator modules. It is not
an installed GUI, full automatic worker, physical capture, independent application
process, Ollama server, GPU endurance or combined two-hour test. Generic OpenAI
routes receive no residency guarantee. Fresh CI and reviews on the eventual
published integration head are still required before merge.


### Legacy notes crash-fixture correction after integration candidate 09

The candidate 09 CI notes check failed after its own crash child had exited:
the immediate single resume encountered a still-held exclusive processing.lock.
The saved diagnostic establishes contention, not the identity of the remaining
holder. The notes fixture now reuses the already tested template fixture's
bounded lease-release probe after confirming its own child has exited. The same
existing five-second test bound is used. Production GenerateAsync/FileShare.None,
the 30-second checkpoint-ready limit, consent, content validation and persisted
prefix comparison are unchanged. The probe never deletes the lock, catches only
actual sharing/lock violations and does not retry GenerateAsync. Its established
held-lease expiry and other-I/O countercases remain part of the template checks.

Fresh required verification passes on both integrated sources: 47 PCM, 35 notes,
28 templates, eight agent checks and the monitor checks. Ten further independent
notes runs each pass all 35 cases; each records confirmed own-child exit, the same
5000 ms bound and no lock deletion. The failed CI artifact and log are retained.
This is a crash-test harness correction, not a production model/recording change.

The 380 native and 248 shared UI test results and the real local reference above
belong to their actual candidate 09 sources. The follow-up changes only the two
legacy verification source files and this sanitized note. Production native,
model/coordinator/prompt, UI and dependency bytes remain exact. The complete
source fingerprint therefore changes, while the referenced production modules
remain byte-identical; a separate equivalence proof records that distinction.
No old reference is represented as a new run on the later head. Fresh current CI
and two new review rounds remain required on the corrected published head.


### Configured native routes retain their loopback host identity

The first full review of candidate 10 found that configured-provider matching
checked loopback eligibility but discarded the host from its comparison key.
A different loopback bind with the same port and path could therefore acquire
the wrong native provider route. Two actual before regressions fail: an
unconfigured bind receives Ollama management and a same-port LM Studio bind is
misclassified as Ollama. Both are real production matching errors.

Matching now includes the normalized host together with scheme, port and path.
Only the existing localhost alias maps to pinned 127.0.0.1; distinct IPv4 loopback
addresses and IPv6 ::1 stay distinct. Credentials, HTTPS, remote hosts, query,
fragment, and different ports or paths remain ineligible. A pure matching helper
uses the same production path without changing process environment in parallel
tests. Three regressions cover wrong-bind rejection, cross-provider same-port
routing, accepted same-bind aliases and the existing unsafe-URL exclusions.
All 383 workspace/all-target Rust tests pass, with two existing special entries
regularly ignored. Format, Clippy with warnings denied and required project
verification pass. Shared UI and dependency bytes remain unchanged.

A fresh real selected-model reference is required after this production binding
change. Its private harness now imports the actual local_models.rs selector as
well as the coordinator and core modules; every real reservation must use the
provider returned for the configured endpoint. An otherwise matching
127.0.0.2:1235 endpoint must remain unclassified. This extends the previous
reference coverage, which selected the native provider explicitly. The same
330-second idle, positive/nonbinding content, ownership, persistence and exact
owned-release requirements apply. No model download or user meeting is used.


The fresh host-corrected real reference completed successfully on the same
HandyMeeting 1.0.7 / Windows 11 Pro 26200 / i7-13700 / RTX 4070 environment,
LM Studio 0.4.26+4 (CLI dcefd5a), existing google/gemma-4-e4b Q4_K_M model,
8192 context and 6,326,936,720-byte installed model. Its actual selector returns
LM Studio for the configured endpoint and rejects the different same-port bind
before any native mutation. Idle residency lasts 330.475444 seconds
with the same instance at all eleven samples; the harness lasts
450.304480 seconds (456.204000 seconds including
the sampling wrapper). All preregistered ownership, positive/nonbinding, durable
checkpoint/resume, final readback and exact owned-unload checks pass. Accepted
quotes/source IDs were manually read. The positive final overview retains five
items and rejects one; the stored overview records ownerOrDueOutsideTasks: 1.
This reason is read from this run, not inferred from an earlier run. The task remains separately present, unknown owner/due remain unknown,
and the nonbinding post summary is empty. This is no semantic completeness or
zero-rejection claim, and no validation rule was relaxed.

Memory is sampled 92 times at a nominal five-second interval.
Maximum individual llama-server Working Set is 6,803,189,760 bytes and
maximum individual Private Bytes is 6,853,861,376 bytes, reported separately.
Whole-device GPU usage is 3978 MiB before inference,
8838 MiB peak and 3901 MiB after release.
These readings include other desktop load; per-process peaks are not simultaneous
sums and do not prove exclusive attribution or complete reclamation. Own builds
and local verification ended before this run; existing desktop applications and
light source/GitHub inspection remained active. No capture, ASR or download ran.

All 2,987 tracked source files stayed byte-identical during the reference. Its
private fingerprint precedes only this final sanitized results paragraph. The
actual provider selector, coordinator, core/prompt and dependency bytes stay
exact for publication. Earlier reference runs retain their original fingerprints
and are not reassigned. The installed GUI, full automatic worker, independent
app-process, actual Ollama, hardware capture and endurance limitations above
remain. Fresh current CI and two clean review rounds are still required.


### Conservative Ollama final logical release after the candidate 11 architecture review

The complete candidate 11 review had no inline comments and Minimal merge risk,
but retained a Low architecture concern. It is not counted as a clean round.
The concern is valid: final Ollama cleanup used the initial global model policy
snapshot to send keep_alive=0 or restore an older finite lifetime. An independent
client could have adopted/extended the same model or deliberately removed it
since preparation. App-local owner synchronization cannot establish server-wide
ownership of Ollama's shared model key.

A new stateful six-case regression confirms the issue before correction: all
six final releases send a global management request, and five change the later
independently set state. Both initially absent and initially finite models are
covered after late indefinite adoption, longer finite adoption and removal.
The old finite restore even reloads a model deliberately removed by that other
fixture client. These are actual assertion failures against synthetic HTTP
provider state, not a real Ollama server or process-coordination proof.

Final Ollama release now removes the local loaded/owner state and stops our
renewal without any provider request. It sends neither an unload nor a cached
TTL restore. The existing app-owned server lease remains finite: normally up
to 300 seconds after the last app request, unless another client extends it.
Finalization/erasure therefore ends our logical reservation, not a promise of
immediate Ollama eviction or physical memory reclamation. Initial observed
foreign finite/indefinite retention and the existing 300/60-second active
load/renewal behavior are retained. Active independent server changes still
have no cross-application CAS/ownership guarantee; no new general coordination
or exact shared-server unload authority is claimed. LM Studio remains different:
its actual acknowledged own instance ID is still unloaded and checked exactly;
foreign, uncertain or in-flight instances retain the existing protections.

The six-case regression passes after the correction. Alias/client-owner and
unrelated-work tests now assert no global final Ollama write, preserved later
foreign state and fresh provider discovery for the next reservation. The native
LM Studio delayed-cleanup/alias/acknowledgement countercases remain strict.
A virtual idle fixture still uses the same 300-second finite bound: 240 seconds
of virtual quiet leaves 60 seconds, logical release does not rewrite it, and the
remaining virtual grace then expires naturally. No timeout or validation bar is
weakened. The failed intermediate full-run log retaining the old immediate-zero
expectation is preserved separately. The actual 60-second renewal fixture also
continues for 65 seconds after final logical release and requires no further
request while the registry remains alive. Fresh full-source verification and
selected-provider reference results follow below when completed.

The corrected full workspace/all-target run passes all 385 Rust tests with two
existing special entries regularly ignored, including all 25 provider fixtures.
The actual 60-second renewal plus 65-second no-renewal interval passes. Rust
format, Clippy with warnings denied and required verification (47 PCM, 35 notes,
28 templates, eight agents and monitor checks) pass. Shared UI/dependency bytes
are unchanged; no UI results are represented as a new run. The fresh selected
real-provider reference 03 remains to be completed before publication.

The [documented Ollama generate request](https://docs.ollama.com/api/generate)
identifies a model and a retention duration. The [Ollama lifetime FAQ](https://docs.ollama.com/faq#how-do-i-keep-a-model-loaded-in-memory-or-make-it-unload-immediately)
explains immediate removal with zero, finite durations and indefinite negative
values. From this used interface we infer no client-specific compare-and-swap
ownership for final eviction; the conservative application policy therefore
avoids that final global mutation. This is an integration design inference,
not a claim that documentation proves arbitrary external-client coordination.


The fresh selected-provider reference 03 after the conservative Ollama correction
passes all preregistered real checks through the actual production selector,
coordinator and core modules. The existing google/gemma-4-e4b Q4_K_M model is
used with LM Studio 0.4.26+4 / CLI dcefd5a, 8192 context and
6,326,936,720 installed model bytes, HandyMeeting 1.0.7, Windows 11 Pro 26200,
i7-13700 and RTX 4070. No downloads or user meetings are used. The configured
provider is recognized and a different same-port loopback bind is rejected.
Actual same-instance idle residency lasts 331.076433 seconds at eleven
samples. Harness duration is 457.311727 seconds, with sampling
wrapper 461.407000 seconds. Positive live/post decisions and tasks,
nonbinding rejection of commitments, original literal sources, durable checkpoint
resume, final readback, other-owner/client deferral and exact own LM Studio
instance release all pass. Baseline native catalog/loaded state is preserved.

All accepted output items and original quotes/source IDs were manually read.
The positive final overview accepts 5 items and rejects
1; the stored overview records ownerOrDueOutsideTasks: 1.
This reason is read from this run, not assigned from an earlier run. The positive task remains separately present, unknown
owner/due stay unknown, and the nonbinding post summary has
0 items. No semantic completeness, zero-rejection
or validator-relaxation claim follows.

Memory has 93 samples at nominal five-second intervals. Maximum
individual model-server Working Set is 6,802,620,416 bytes and maximum individual
Private Bytes is 6,851,530,752 bytes. Whole-device GPU usage is
3995 MiB before inference / 8799 MiB peak /
4525 MiB after release. These are model-server process peaks; the
renamed private harness is excluded by the sampler's process-name filter, so
no harness or installed-application memory peak is claimed. GPU readings include
other desktop load; no simultaneous process-peak sum, exclusive attribution or
complete reclamation is established. Own builds/local checks ended before the
reference; existing desktop apps and light source/GitHub inspection remained.
No capture, ASR or model download overlapped.

All 2,987 tracked source files stayed unchanged during this
actual run. Only the final sanitized result note is subsequently appended;
production code and dependencies remain exact. Historical references retain
their original source fingerprints and are not reassigned. This real selected
LM Studio reference is not a real Ollama-server, full automatic worker, installed
GUI, independent app-process, erasure-against-real-provider, physical capture,
GPU endurance, combined two-hour or forensic-memory proof. Conservative Ollama
final release is separately proved by stateful protocol fixtures and the actual
60-second renewal plus 65-second post-release silence, with virtual natural
expiry at the unchanged 300-second bound. Current final-head CI and two clean
rounds per reviewer are still required before merge.


## Concrete native origin and unreadable Ollama expiry — integration candidate 14

The full candidate 13 review had no inline finding and minimal merge risk, but
retained a Medium architecture concern: a localhost route could gain native
authority from the configured IPv4 provider and contact a different IPv6
listener on the same port. Legacy unleased text fallback was preexisting;
new native load/renew/unload authority and leased inference must use a single
concrete origin. Four new wire-level regressions genuinely failed before this
correction and pass afterwards. Separate owned IPv4/IPv6 services prove refusal
of IPv6-only substitution, no fallback after the original IPv4 service stops,
and no switch between inventory and native loading. Explicit IPv6 remains a
positive control. These synthetic services do not establish actual credential
disclosure, foreign eviction or elevated privileges.

The additional expiry proposal was also validated: five missing/unreadable
Ollama expiry representations formerly caused a guessed indefinite request.
Preparation now refuses all five before mutation. Existing readable finite
and indefinite cases remain covered. No real Ollama server/version compatibility
is inferred from these protocol fixtures. Existing limits for external changes
at the same concrete origin and shared active Ollama TTLs remain explicit.
No proxy, redirect, local/cloud, credential, cancellation, response-size,
deadline or reviewer threshold was relaxed. TEST_REPORT.md remains byte-identical
to the approved public main report. The fresh full checks and selected-provider reference 04 below apply to this
changed runtime; neither is inherited from candidate 13.


The corrected candidate 14 passes 389 Rust workspace/all-target tests, with two
existing special child/credential entry points regularly ignored, including
all 29 provider protocol tests and the real 60-second renewal / 65-second
post-release silence. Strict locked/offline Clippy with -D warnings, Rust format
and verify.ps1 (47 PCM, 35 notes, 28 template, 8 agent and monitor checks) pass.
Frontend files and dependency locks are unchanged from the existing 248-test,
TypeScript and production-build integration proof; no new frontend run is
claimed. The prepared combined deletion source separately passes 432 native
tests with the same two regular ignores, strict Clippy, format and verify.
No deadline or threshold was weakened. Actual model references follow below.


The fresh selected-provider reference 04 after the concrete-origin and unreadable-expiry corrections
passes all preregistered real checks through the actual production selector,
coordinator and core modules. The existing google/gemma-4-e4b Q4_K_M model is
used with LM Studio 0.4.26+4 / CLI dcefd5a, 8192 context and
6,326,936,720 installed model bytes, HandyMeeting 1.0.7, Windows 11 Pro 26200,
i7-13700 and RTX 4070. No downloads or user meetings are used. The configured
provider is recognized through localhost pinned to 127.0.0.1, while a different
same-port loopback bind is rejected.
Actual same-instance idle residency lasts 330.202894 seconds at eleven
samples. Harness duration is 473.170428 seconds, with sampling
wrapper 476.594000 seconds. Positive live/post decisions and tasks,
nonbinding rejection of commitments, original literal sources, durable checkpoint
resume, final readback, other-owner/client deferral and exact own LM Studio
instance release all pass. Baseline native catalog/loaded state is preserved.

All accepted output items and original quotes/source IDs were manually read.
The positive final overview accepts 5 items and rejects
1; the actual persisted rejection count is
ownerOrDueOutsideTasks: 1. This item is excluded from the accepted overview;
no reason is assigned from an earlier run. The positive task remains separately present, unknown
owner/due stay unknown, and the nonbinding post summary has
0 items. No semantic completeness, zero-rejection
or validator-relaxation claim follows.

Memory has 96 samples at nominal five-second intervals. Maximum
individual model-server Working Set is 6,802,763,776 bytes and maximum individual
Private Bytes is 6,851,641,344 bytes. Whole-device GPU usage is
3935 MiB before inference / 8789 MiB peak /
3894 MiB after release. These are model-server process peaks; the
renamed private harness is excluded by the sampler's process-name filter, so
no harness or installed-application memory peak is claimed. GPU readings include
other desktop load; no simultaneous process-peak sum, exclusive attribution or
complete reclamation is established. Own builds/local checks ended before the
reference; existing desktop apps and light source/GitHub inspection remained.
No capture, ASR or model download overlapped.

All 2,987 tracked source files stayed unchanged during this
actual run. Only the final sanitized result note is subsequently appended;
production code and dependencies remain exact. Historical references retain
their original source fingerprints and are not reassigned. This real selected
LM Studio reference is not a real Ollama-server, full automatic worker, installed
GUI, independent app-process, erasure-against-real-provider, physical capture,
GPU endurance, combined two-hour or forensic-memory proof. Conservative Ollama
final release is separately proved by stateful protocol fixtures and the actual
60-second renewal plus 65-second post-release silence, with virtual natural
expiry at the unchanged 300-second bound. Current final-head CI and two clean
rounds per reviewer are still required before merge.


## Integration candidate 15: real paired-port collision in test fixtures

The remaining candidate 14 review finding concerns fixture setup, not a proved
production residency defect. A real independently held IPv6 port reproduced
Windows AddrInUse 10048 in the old one-shot paired bind. The fixture now retries
only AddrInUse with fresh ephemeral IPv4/IPv6 pairs, at most sixteen attempts.
The new regression proves a different real connected pair is obtained while the
original occupied listener remains held. All four native-origin/expiry
regressions still execute and pass with the same refusal and positive controls.

Actual IPv6 loopback remains a mandatory environmental precondition for these
native authority regressions. Unsupported IPv6, permissions errors and exhausted
retries still fail the check. No return-as-success skip is added, and no missing
IPv6 coverage is called passed. The older optional generic transport discovery
checks retain their existing convention; they are not substituted for the new
native authority proof. No network setting, product source, dependency, deadline
or protection configuration changes.

Only this integration-test fixture and sanitized validation notes change from
candidate 14. Runtime, selector/coordinator/core, prompts, UI and dependency bytes
remain exactly those of the genuine selected-provider reference 04 above. Its
actual fingerprint and measurements remain assigned to the original run; they
are not relabeled as a new model run on candidate 15. Fresh full local checks and
current final-head review/CI results are recorded separately.


Fresh candidate 15 checks pass: 390 Rust workspace/all-target tests, including
all 30 provider fixtures, with two existing special entry points regularly
ignored. Strict locked/offline warnings-as-errors Clippy, Rust formatting and
verify.ps1 pass (47 PCM, 35 notes, 28 template, 8 agent and monitor checks).
The prepared deletion integration separately passes 433 native tests with the
same two regular ignores, strict Clippy, format and verify. No new frontend or
model execution is claimed: all runtime/UI/dependency/report bytes are exact to
their previously executed proofs. Only the paired-port fixture and sanitized
validation notes differ from the actual reference 04 source inventories.

Parallel-load statements in the model-reference sections describe test-owned
activity: no capture, ASR or model download was started by these references.
Existing external applications were preserved and not exhaustively profiled;
these observations establish no exclusive or uncontended performance result.
Separate unrelated Rust processes observed later were left untouched. Their
activity is not attributed to the model-server process measurements.


## Historical reference-result erratum — 2026-10-31, integration candidate 20

Earlier sanitized notes for references 01–03 confused the diagnostic rejected-item
count with the separate persisted overview result. All six original positive
final overview JSON files, for both separate integration sources and all three
references, contain five accepted items and rejected ownerOrDueOutsideTasks: 1.
The broad earlier statement that no rejection reason was persisted was wrong;
the rolling note now distinguishes the diagnostic counter from the overview.
Original private evidence, model outputs, fingerprints and measurements remain
unchanged. Reference 04 was already reported with its actual persisted reason.
No rejected item is promoted to accepted, no semantic-completeness claim is
added and no validator changes. This is a documentation-only erratum; candidate 19
runtime, tests, UI, dependency locks and approved public TEST_REPORT bytes remain
exact. The executed 433-test / 30-provider and 272-frontend results retain their
original runtime provenance; no new model, frontend or Rust execution is claimed.
