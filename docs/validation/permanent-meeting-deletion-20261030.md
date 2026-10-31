# Explicit permanent meeting deletion — 2026-10-30

The behavior and limits below describe the combined implementation. Candidate and reference sections retain historical results and the draft, integration and review gates that applied to their named revisions; those statements do not describe the latest PR status. Current status: [PR37](https://github.com/Puma7/meeting-for-handy/pull/37) and [PR38](https://github.com/Puma7/meeting-for-handy/pull/38).

A newly recorded native meeting, including a zero/two-second or failed meeting, can be explicitly removed from History or its detail view without first archiving it. A preview names exactly one meeting and counts its attributed files. The user must type the displayed confirmation word. This is irreversible, has no trash copy, and is separate from automatic retention. Automatic age, VIP and complete-archive rules remain unchanged. Manual confirmation can remove a pinned meeting.

## Scope and safeguards

The operation removes both tracks, PCM/journals/transcript revisions/backups, live and post-analysis snapshots, speaker experiments, converted audio, exports stored inside the meeting, annotation revisions, attributed whole/audio-only ZIPs and retained sources/restoration staging, audio-copy catalog entries and matching content-bearing maintenance plans/logs. The complete rebuildable SQLite/FTS cache, WAL/SHM and retained unreadable cache copies are removed; the index is rebuilt from remaining meetings afterward. Other original meetings/settings/models/keys are not deleted. A failed index rebuild is reported separately from successful file removal.

An exclusive native maintenance reservation refuses any active capture, recognition, analysis, live summary, export, indexing or mutation, including the reservation-to-worker gap. Cross-process source relocation, annotation, archive and index locks protect participating app processes. Bounded ordinary-file inventories reject links/reparse points, foreign identities, unknown cache entries and changed bytes. Each remaining file is rechecked before unlinking, and only empty directories are removed. A changed inventory requires a new preview; late files survive and produce an explicit partial failure.

Cancellation, I/O failure or app exit can leave an irreversible partial removal. A minimal local continuation marker retains only meeting/archive identities and confirmation time, never titles, quotes, paths or an inventory copy. A new preview and confirmation remove remaining bytes even when manifests/receipts were already removed. The marker and its backup disappear after verified completion. Empty stable coordination lock files can remain; they contain no meeting content and must not be unlinked while other processes may refer to them. This is removal of app-managed content, not guaranteed erasure of every filesystem identifier.

Native and React state drop the selected inactive meeting's cached view/results/retry references. Request generations discard late replies, so an old snapshot cannot restore a removed meeting to the interface. Initial focus is on the safe close action; keyboard focus remains within the confirmation.

## Explicit limits

Imported legacy originals are read in place outside app-owned storage; this action does not delete those originals and is not offered on legacy rows. External exports, hardlink copies, clipboard, external backups, cloud/provider histories and OS/device forensic traces are outside this operation. App-managed transcript backups are included in removal. No overwrite/free-space wiping or forensic secure-erasure guarantee is made. No real user meeting was deleted during development. No capture, ASR algorithm, model endpoint or prompt was changed. PR37 model-residency coordination is integrated: after maintenance admission and explicit confirmation, erasure releases only the named meeting's inactive Live and Analysis anchors outside the app and coordinator locks. Other meetings' owners and in-flight clients retain their reservations and delay provider cleanup. This does not clear provider history or guarantee erasure of model-server memory.

## Validation

Fixtures are synthetic local files/PCM, not a hardware recording or real-model test. Tests cover direct short/empty/failed/corrupt native meetings, both tracks, all attributed content classes, whole/audio-only archives, every file interruption boundary, stale/wrong confirmation, late writes, source/index leases, hardlink isolation, Windows directory junction rejection, conflicting source/archive identities, unaffected other originals and FTS content removal. Unix symlink coverage is compiled/executed by target CI; a Windows run does not prove that target result.

Frontend interaction tests cover exact typed confirmation, no Enter bypass, cancellation before a native job ID arrives, foreign preview rejection, partial failure, safe focus/Escape, restart continuation and a delayed pre-erasure app-state reply. The rendered dialog was inspected with synthetic IPC in dark/light and wide/narrow layouts; it is not an installed native end-to-end deletion test.

The original published candidate `1c58782be6786727c8454e43712c5658ee6fd19f` passed local Windows validation: 363 Rust workspace/all-target tests, with two existing explicit entry points normally ignored (native credential provider and crash-writer child); 234 frontend tests; TypeScript/production build; Rust formatting; Clippy with warnings as errors; frontend formatting; and verify.ps1 (47 PCM, 35 notes, 28 templates, 8 agent checks, monitor). Thirteen new core erasure tests, a native preview-capability test and a native admission regression are included. The first verify run hit the unchanged monitor fixture replacement deadline under concurrent test load; a sequential repeat passed the same deadline. Earlier fixture-only compile/encoding corrections and failed checks remain in private evidence; they are not represented as product regressions. Current target CI and review links are tracked in the PR, not inferred from local runs. No release, installer, merge, physical storage erasure, Linux/macOS runtime success or two-hour capture claim follows from local synthetic checks. The historical TEST_REPORT.md remains byte-unchanged; this sanitized note is the current feature-specific evidence boundary.


## First full-review corrections

The first CodeRabbit full review (5712541593) and second Codex code review
(5712644774) found six distinct issues, including a shared orphan-marker issue.
Their full risk/architecture/extra comments were read; neither is recorded as a
clean final-head review. The PR was returned to draft while these were corrected.

- Storage maintenance now has its own reservation flag. Capture start/stop and
  microphone reconnect transitions keep saved text readable; maintenance still
  blocks reads from reservation through worker completion, including stale cache
  insertion. The synthetic transition regression failed with the former guard.
- Native metadata and speaker edits hold the same process-shared archive lease
  across source reading, annotation commit and index refresh. Two isolated service
  instances with real filesystem locks verify the whole write scope; the former
  command had no such fence. This protects cooperating current app instances,
  not arbitrary older binaries or programs that ignore the storage protocol.
- Both primary and backup manifest identities are checked before adding restore
  staging to the selected meeting's inventory. Attributed and orphan staging
  cannot grant permission to remove an intact foreign recording. Missing or
  unreadable staging manifests alone do not prevent removal of an incomplete
  owned tree. The foreign-staging regression failed before this correction.
- Orphaned atomic-write staging files are retained as inert entries, never used
  as deletion authority and never allowed to hide valid continuation markers.
  Actual JSON marker identities remain strict. Recovery discovery and completion
  were tested with a leftover temporary file; discovery failed before the fix.
- The erasure dialog participates in application-wide background inertness and
  keyboard trapping. History navigation and replacement of the submit button by
  progress no longer move focus outside a running destructive dialog. The full
  App regression starts from meeting details, checks focus/inertness through that
  navigation and also rejects a delayed old snapshot. It failed before the fix.
- Search/index contention now gives neutral retry guidance to ordinary readers,
  without suggesting that a search user permanently delete anything.

Function-level erasure capability, inventory and synthetic-fixture intent was
also documented. The documentation coverage requirement remains 80%; only a
fresh provider result can establish the new coverage and clean-review status.
The preceding published candidate 02 passed all 234 UI tests plus TypeScript/production build.
Its targeted core suite passed 15 erasure cases; native annotation-fence and transition
regressions passed. That source passed 367 full workspace/all-target Rust tests
(two existing special entry points ignored), all 234 frontend tests,
TypeScript/production build, formatting, Clippy with warnings as errors and a
fresh sequential verify.ps1 (47 PCM, 35 notes, 28 templates, 8 agent checks and
monitor). This result does not replace fresh target CI or clean external reviews.
No real meeting was used.

## Further complete-review corrections

Maintenance can hold a source snapshot after its session lease ends. Every native
search-index update now revalidates the source identity while holding shared index
admission through the write. Explicit erasure holds exclusive admission across
source removal and cache purging. A delayed restore/audio-archive snapshot cannot
recreate erased transcript content. The regression failed before this correction
for the actual native and archived snapshot forms.

A canceled archive writer can leave a ZIP without its final archive manifest.
Validated pending attribution now permits its explicit removal. Available ZIP,
session and backup identities must still agree; readable foreign identities and
unattributed ZIPs remain blocked. Real cancellation of the synthetic archive
writer failed to reach erasure preview before the correction.

Interrupted atomic metadata is parsed by bounded JSON structure, including a
complete meetingId field in an otherwise EOF-truncated record. IDs embedded in
titles, paths or quoted JSON are never deletion authority. Foreign records remain
byte-identical. Malformed ordinary non-temporary records still fail closed.
Unassignable temporary remnants are preserved and their count is displayed before
confirmation; they are explicitly outside the claimed removal of attributed data.
A changed count invalidates the preview. Their contents cannot be claimed erased.

After three consecutive failed/missing/unknown status replies, the erasure dialog
stops automatic retries and requests cancellation. It retains the exact native
job and blocks a new deletion until a known terminal status is actually observed.
An explicit status retry remains available; cancellation acknowledgment alone
never calls the stopped/completed callbacks. Synthetic failure/recovery and safe
focus tests cover this path. The former observer continued polling indefinitely.

The annotation-admission fixture now acquires the real erasure OS lease before
invoking a second Service's metadata/speaker command. Both must refuse with source
bytes intact; both succeed after release. This removes the competing lock probe
and scheduler-dependent two-second fixture deadline. It remains synthetic
two-Service/OS-lock coverage, without a separate-process runtime claim.

The preceding candidate 03 passed 371 Rust workspace/all-target tests (the two
existing explicit special entry points normally ignored), 236 frontend tests,
TypeScript/production build, Rust/frontend formatting and Clippy with warnings
as errors. Eighteen synthetic core erasure tests passed. A sequential verify.ps1
passed all 47 PCM, 35 notes, 28 template and 8 agent checks plus monitor.
The real rendered dialog was checked with synthetic IPC at 1280px and 390px,
including light/dark warning disclosure, bounded status failure and a later known
cancellation. Neither viewport overflowed. No real meeting or native application
was erased; these are not hardware, separate-process or endurance results.


## Candidate 04 review corrections

Retention preview construction and the atomic publication of its content-bearing
plan now share erasure's process-shared archive lease. A delayed cooperating app
cannot write an old title-bearing plan after successful erasure. A deterministic
two-Service fixture holds the real OS lease, checks refusal without publication,
then verifies normal publication and removal after release. This is not a
separate-process runtime test. The former publisher failed that exclusion check.

Archive attribution now reconciles readable ZIP, primary and backup source
identities before a foreign receipt or earlier orphan can filter a copy out.
Contradictory attribution blocks removal before any source bytes change. Synthetic
ZIP/source fixtures with a foreign receipt failed before the correction and
preserve all original bytes after refusal. Incomplete unreadable ZIP bytes still
require explicit pending attribution; no guessed foreign ownership is introduced.

Shared archive receipt staging files use the same bounded exact meetingId parser.
Unassignable metadata/speaker staging files have no trustworthy meeting ID, so
they remain untouched and contribute to the disclosed remnant count. The final
success notice also retains that warning: such files can still contain meeting
data and need manual inspection. Titles, quotes and embedded IDs never authorize
their deletion. Stable archives/.leases coordination remains, accepting only
ordinary empty SHA-256-named lock files. Unexpected or nonempty entries block the
operation. Existing archive recovery tests caught a temporary misclassification
of that directory; the corrected full recovery suite passes without weaker rules.

A rejected cancellation request after the native destructive job ID arrives no
longer ends observation or clears destructive admission. The dialog continues
watching that exact job until its terminal status is established; it never offers
a new erasure merely because cancellation failed. The delayed-ID/cancel-failure
regression failed with the former handler. A separate full-App test verifies both
normal completion and the surviving-remnant warning after clearing stale replies.

This candidate passed 375 Rust workspace/all-target tests, with the two existing
special entry points normally ignored; 21 synthetic core erasure cases; 238
frontend tests; TypeScript and production build. Rust/frontend formatting and Clippy with warnings as errors also passed. A fresh
verify.ps1 passed 47 PCM, 35 notes, 28 template and 8 agent checks plus monitor.
Its build briefly overlapped the tail of an additional filtered native test
invocation; the complete earlier Rust run and Clippy had already finished. That
filtered invocation selected no case because its module prefix was omitted; it
is retained as such, and a corrected exact-name rerun is recorded separately. No real user meeting, provider service, audio device or
external export was deleted. Hardware, separate-process, Linux/macOS runtime,
GPU, physical/forensic erasure and endurance claims remain outside this proof.

## Candidate 05: embedded identity, repeated interruption and publication exclusion

A successful archive inventory/hash verification now also checks each readable
embedded primary and backup session identity before the ZIP can be selected for
erasure. The same check applies to orphan ZIPs without a receipt. Four synthetic
fixtures keep the outer manifest and all content hashes valid while introducing
a foreign primary or backup identity, with and without a receipt. All refuse
erasure and preserve the existing files. The former code accepted the first such
ZIP despite its foreign embedded identity.

An already confirmed interruption marker retains the checked archive identity
when a later interruption has removed the original creation pending file but
has not yet removed its incomplete ZIP. A new preview and typed confirmation
remain necessary. Readable contradictory ZIP identities still block removal;
an unrelated unverified ZIP gains no authority. A regression uses an actual
cancelled archive creation and stops erasure between pending-file and ZIP removal.
The former retry failed; the corrected retry removes the remaining attributed
bytes and clears its minimal marker.

Analysis, including resumed analysis of an archived transcript, now holds a
process-shared publication lease from before reading sources or snapshots until
all checkpoint/result writes end. Live-summary and optional speaker workers
retain the same shared lease through their worker lifetime, and transcript
exports retain it through saving. Explicit erasure and the existing archived
permanent-deletion paths require exclusive publication admission before removal.
This adds exclusion, without changing automatic retention age or VIP rules.

The stable coordination lock is outside meeting and cache namespaces. Publishers
can coexist and do not reacquire the archive/annotation lease: no capture handle
or recording transition is held by this new lease. A core fixture proves that
two concurrent publishers are admitted, and erasure remains blocked until both
end. Native fixtures pause new and resumed archived analyses at their own
loopback HTTP stub after persisted snapshot creation. Erasure is refused until
the request and writes end, then succeeds. The old analysis failed this exclusion
check. These are synthetic same-process threads/Services exercising OS locks,
not a separate-process, real model-provider or physical-device test. External
programs and older binaries that ignore these locks are outside this guarantee.

The review's suspected maintenance-guard lifetime problem was not reproduced.
The actual deletion worker is now directly testable through a small extracted
start helper, with its guard scope unchanged. Its native regression performs a
synthetic erasure, observes the exact job's complete status, then immediately
acquires indexing admission and successfully rebuilds the derived index. The
guard ends before terminal status publication; no admission rule was relaxed.

The full Rust workspace/all-target run passed 380 tests; the two existing special
entry points remain normally ignored. All 24 synthetic core erasure cases pass.
The frontend, dependencies, review rules and complete historical TEST_REPORT.md
remain byte-identical to published candidate 04. Its 238 frontend tests and
TypeScript/production build are previous evidence for those unchanged bytes,
not newly executed frontend suites. No real meeting, external export, model
service or user program was changed or erased.

Rust formatting and Clippy with warnings as errors passed. The sequential
verify.ps1 run also passed 47 PCM, 35 notes, 28 template and 8 agent checks plus
monitor. Earlier restricted temporary-folder access and an invalid status in a
new completion fixture are retained as infrastructure/fixture failures; neither
is reported as a failing application regression or a successful verification.

## Candidate 06: bounded exclusive index admission

Exclusive erasure admission now uses the existing metadata lock helper instead
of a single try-lock attempt. It retains that helper's 25 retries with 10 ms
delays; no wait or test limit is increased. Shared index readers still fail
immediately while erasure owns the lock. A sustained reader still prevents
erasure admission, before any destructive operation. The branch results remain
io::Result values before the existing user-facing error mapping; no error is
converted to an uninformative unit value.

The new synthetic regression holds an actual shared OS file lease and releases
it only after the exclusive path has observed contention. A scoped, one-shot,
thread-local test observer makes that ordering deterministic without sleeping
or competing probe loops. The old single-attempt path failed this test; the
corrected path acquires admission after the reader ends. The same fixture checks
that new shared admission remains blocked until the exclusive lease is dropped.
A second case keeps its reader throughout the bounded attempts: erasure remains
blocked and its synthetic content file is byte-identical. These are two file
handles in one process, not a separate-process erasure or physical-device test.
The observer is compiled only for tests and restores prior thread state even
during unwinding.

All 382 Rust workspace/all-target tests pass with warnings denied and the same
two existing special entry points regularly ignored. The 24 core erasure cases
are retained alongside the two new index-admission cases. Frontend sources,
dependencies, protection/review rules and the entire historical TEST_REPORT.md
remain byte-identical to published candidate 05. The earlier 238 frontend tests
and TypeScript/production build apply to those unchanged bytes and were not
rerun here. No real user meeting, external export, provider or installed program
was changed. All previously documented erasure and model-residency integration
limits remain in force.

## Candidate 07: history completeness after erasure

The erasure worker now invalidates completed search progress before removing
the derived cache, while maintenance admission is still held. A retained
meeting's later metadata update may recreate a partial cache, but that cache
must not hide other original meetings. Completed scans are also bound to a
content-free identity stored inside their actual SQLite database. History
checks that identity and holds the same shared OS index lease through every
result page. A missing, invalid or replaced cache falls back to the original
session folders, including in another Service that remembers an older scan.

This adds compatible derived-cache metadata; the primary session format,
existing search schema version and application version are unchanged. A
recreated cache receives a new identity. Ordinary reopening preserves it. An
explicit rebuild retains an invalid cache aside and creates a new identity;
original session files are neither repaired nor removed by that operation.
The identity is part of the SQLite cache already covered by erasure, with no
new content-bearing file or erasure inventory location.

The native regression uses three synthetic meetings and two independent
Services. After both complete their scans, one erases the selected meeting
through the actual deletion worker. Updating only one retained meeting then
recreates a partial index. Both histories must still show both retained
meetings and exclude the erased one; the retained manifests remain byte-equal.
The old code failed that assertion in both Services. The corrected code passes,
also proving that an unchanged fully scanned cache still uses the indexed path.
This is a same-process two-Service test using real OS file leases, not an
actual two-process, physical-device or provider test. An initial zero-test
invocation and a fixture compilation error are retained separately and are not
application regression evidence.

All 384 Rust workspace/all-target tests pass with warnings denied; the same
two existing special entry points remain normally ignored. The 24 core erasure
cases remain covered. Frontend sources, locked dependencies, protection and
review rules, and the entire historical TEST_REPORT.md remain byte-identical to
published candidate 06. The earlier 238 frontend tests and TypeScript/production
build are evidence for those unchanged bytes, not newly executed suites. No real
user meeting, model, external export or installed program was changed. All
previous erasure boundaries and the pending integration with model residency
remain in force.

## Candidate 08: desktop history and orphan-source reconciliation

The desktop's unfiltered History now uses the authoritative native meeting
summaries, including when the search cache is empty or only partially rebuilt.
Transcript queries remain separate and use the index with their original
section identifiers. Clearing the query restores all retained meetings. The
native summaries are paginated in groups of 25, hidden erased identities are
excluded before pagination, and removal of the final page clamps the view to
the remaining rows. Refresh also reloads the authoritative summaries. Legacy
imports retain their separate non-searchable section.

Two full-App regressions exercise confirmed synthetic deletion followed by a
failed or cancelled rebuild with empty or partial search results. Both old
desktop paths hid retained meetings after a query was cleared. Both now pass,
and filtered source results remain distinct from the complete unfiltered list.
A further case verifies 26-row pagination, removal of the last visible page,
refresh and no empty-query index request. The existing explicit-source scroll
test now enters an actual transcript query before selecting its hit; all its
highlight, scroll-position, page-reset and console assertions are retained.

Orphan retained .source folders now reconcile every available readable primary
and backup identity before an identity can filter the folder out of erasure.
Contradictory identities block the preview and preserve the whole tree. The new
regression failed with a foreign primary manifest hiding the selected meeting's
backup. Both primary/backup directions now block, with original bytes unchanged.
A paired case proves that agreeing selected-meeting copies are removed and
agreeing foreign copies remain byte-equal. These tests use synthetic temporary
meetings only; no real meeting or externally exported copy was erased.

All 386 Rust workspace/all-target tests pass, with the same two existing special
entry points normally ignored, including 26 core erasure cases. All 241 frontend
tests, TypeScript and the production frontend build pass. The first frontend
run retained a failing old empty-query source-selection fixture; another type
check identified summary metadata field names, corrected through the existing
userMetadata object. These failed checks remain recorded, without removing any
source-anchor assertion or weakening a deadline. The final browser component
fixture shows both retained meetings after clearing a query, including at
390 pixels with no horizontal overflow or console errors. It does not perform
native IPC, real erasure, hardware capture or provider calls.

Locked dependencies, protection and reviewer rules, the primary session format,
application version and entire historical TEST_REPORT.md remain unchanged.
Coordination covers participating app processes. Hash checks followed by path
removal are not atomic object binding against a hostile filesystem writer;
older or external programs that ignore the shared leases remain outside the
guarantee. No physical or forensic erasure guarantee is added. All earlier
external-copy boundaries and pending model-residency integration still apply.

## Candidate 09: retain VIP ordering in authoritative history

The unfiltered native History list preserves the previous SQLite ordering before
pagination: pinned meetings first, then descending stored start date and ascending
native identity for equal dates. The comparator uses ordinary string ordering,
matching the index without depending on the browser locale. It sorts a filtered
copy and never mutates the summaries supplied by the parent. Transcript queries
keep their existing indexed ordering; the native list is not sorted during them.

Both new regressions failed on candidate 08: an older VIP behind 26 recent
ordinary meetings was missing from the first page, and mixed pinned groups with
equal dates retained the wrong order. Both now pass. The cases also verify the
VIP indicator, the later page, exact identity tie order and unchanged frozen
input arrays. All 243 frontend tests, TypeScript and production build pass.
The synthetic browser component shows an older pinned meeting before newer
ordinary meetings, including after Refresh, a transcript query and clearing that
query. Desktop and 390-pixel views have no horizontal overflow or console errors.
This is component/IPC simulation, not native E2E, real erasure or device evidence.

All native source files are byte-identical to published candidate 08. Its 386
Rust tests (two existing special entry points normally ignored), 26 core erasure
cases, formatting and Clippy results remain prior evidence for unchanged native
bytes and were not rerun for this frontend correction. Locked dependencies,
protection and review rules, application version, primary session format and the
entire historical TEST_REPORT.md remain unchanged. All existing deletion limits
and the model-residency integration requirement still apply.

## Candidate 10: readiness-fixture isolation and bounded archive work

The Intel CI run for candidate 09 failed the existing synthetic startup-progress
test before packaging. Its 150 ms fixture deadline depended on the host scheduling
a separate recorder thread in time. Ten unchanged local baseline runs passed;
inserting a private, deterministic 250 ms recorder stall reproduced the same
deadline error in all ten runs. This establishes the fixture's scheduling
dependence, not an observed trace of the Intel scheduler or a hardware fault.

The positive test now uses a thread-local scoped clock driven by the exact
startup-progress snapshot already inspected by the controller. Recorder gates
separate eight progress steps from a final idle turn. All ten deliberately
stalled runs pass with the corrected fixture. A new negative case proves that
progress does not waive the eventual idle deadline: a late microphone is stopped
while the healthy system track remains owned. Removing deadline renewal as a
private fault injection was missed in all ten old-test runs and detected in all
ten corrected-test runs. Temporary fault and stall edits were restored exactly.

The test deadline remains 150 ms, the existing real harness guard remains five
seconds, and the readiness polling interval remains 100 ms. Release builds alias
the clock directly to std::time::Instant at the same three read sites; after
resolving that alias and removing cfg(test) observations, the readiness-policy
body is token-identical. Its macOS 60-second and other-supported-platform
10-second idle deadlines, cancellation and late-worker stop policy are unchanged.
This is controlled-clock/thread fixture coverage, not a new device, endurance or
actual Intel CI result. The new target CI must independently pass.

A later local full run exposed a separate race in the synthetic archived-analysis
HTTP gate: Windows accepted sockets inherited the listener's nonblocking mode.
A read timeout alone did not make the accepted stream blocking. A private loopback
probe delayed the first client bytes by 250 ms: the original read returned
WouldBlock immediately, whereas explicitly selecting blocking mode read all nine
synthetic bytes after 250 ms. The test server now selects that mode before its
unchanged five-second read timeout. Its fresh/resumed analysis, concurrent erasure
exclusion, final worker join and subsequent confirmed erasure assertions remain
unchanged. No production HTTP transport is modified; the failed full-run evidence
is retained separately.

The non-blocking archive-performance observation from the complete candidate 09
review is also addressed. Source-lease preparation returns its checked archive
targets for the pre-removal inventory while the exclusive archive/publication
leases remain held. Attributed ZIP contents are no longer decompressed and
manifest-hashed twice in that unchanged preparation phase. Each ordinary file
still receives its inventory hash and is rechecked before unlinking. Attribution
after removal is computed fresh; interrupted ZIP recovery and every readable
primary/backup identity check remain in place. This is bounded duplicate-work
removal, without a measured large-archive latency claim or a new safety exception.

The complete Rust workspace/all-target run passes 387 tests, including the 26
core erasure cases; the same two existing special entry points are normally
ignored. Rust formatting, Clippy with warnings denied and verify.ps1 pass. The
243 frontend tests and TypeScript/production build remain prior candidate 09
evidence for byte-identical UI files, not newly executed suites. Locked
dependencies, protection and review rules, application version, primary session
format and the entire historical TEST_REPORT.md remain unchanged. No real
meeting, device, provider service or installed program was modified. All prior
erasure boundaries and pending model-residency integration still apply.

## Candidate 11: restore focus to the erasure opener

The complete candidate 10 target CI passed on Windows, Linux, macOS ARM and
macOS Intel, including the corrected synthetic readiness fixture. Its clean
Codex code review did not make the round complete: the full CodeRabbit review
identified one remaining keyboard-focus issue after closing the erasure dialog.

Both new full-App regressions failed on candidate 10: closing by button or Escape
left focus on the History heading instead of the original erasure button. The
visible synthetic browser reproduced the same result. The dialog previously
captured its supposed opener after its layout effect had already moved focus
inside, and synchronous restoration also preceded the parent's modal cleanup.

The actual opener is now captured before the first layout focus transfer.
Restoration runs on the next animation frame after both cleanups, only while
this dialog remains unmounted, the opener is still connected and no replacement
dialog is present. This retains StrictMode cleanup safety and avoids stealing
focus from another modal. The two regressions pass with StrictMode enabled.
Existing full-App deletion cases additionally verify that a removed opener
retains the safe History-heading fallback, without attempting to focus a detached
control. Cancellation, confirmation and native job admission are unchanged.

All 245 frontend tests, TypeScript, production build, formatting and verify.ps1
pass. An initial restricted Temp rename failure ran no tests; a later test-only
unsupported locator option and its subsequent format check were corrected.
Those failed checks are retained separately and are not application-regression
evidence. The genuine old-code focus failures remain recorded independently.

The full synthetic App browser flow returns focus to the opener at 1280 pixels
after Close and at 390 pixels after Escape, with no horizontal overflow or final
console errors. This is simulated preview/cancellation IPC only; real deletion
is disabled in the browser fixture. No installed native IPC, screen-reader
speech, physical recording or provider test is inferred from it.

All native files remain byte-identical to candidate 10. Its 387 Rust all-target
tests, 26 core erasure cases, two normally ignored existing special entry points
and Clippy results are prior evidence for those unchanged bytes, not new runs.
The new candidate still requires its own target CI and fresh final-head reviews.
Locked dependencies, protection and review rules, primary session format,
application version and the whole historical TEST_REPORT.md are unchanged.
All existing erasure limits, separate-process assurance limits and pending
model-residency integration remain explicit; no real meeting was modified.

## Candidate 12: pending-start recovery and precise retention coordination

The second complete candidate 11 CodeRabbit review found three additional
correctness issues despite the clean first round and successful target CI.
Those results remain attached to candidate 11; they are not a clean final review.

If Stop was selected before a preview/erase command returned its job identity,
a subsequent rejected command left the Close button disabled. Both new old-code
regressions failed on that exact state. Both rejection handlers now clear the
cancelling state when entering the existing failed stage. The tests verify that
Close is enabled and works, no unknown job is cancelled, no deletion is reported
complete and only the existing rejected erase-start path releases its callback.
Known running jobs keep their existing bounded status/cancellation observation.

Read-only retention previews now hold the shared content-publication lease from
before the scan through final plan publication. Ordinary metadata/VIP and speaker
edits may proceed; erasure still requires exclusive publication admission.
A parked-scan regression failed on the old broad archive-mutation lock. It now
performs both native edits while indexing admission and the scan remain active,
then confirms erasure is blocked without relying on a cached session writer.
The complementary test parks an actual confirmed synthetic erasure before its
first removal and verifies a second Service cannot publish a retention plan.
After cancellation, publication succeeds; a freshly confirmed final erasure
removes the title-bearing plan. These are synchronized threads/Services using
OS file leases in one process, not independent-process or hardware validation.

Mixed identities in maintenance records now block erasure before any removal,
for both ordinary complete JSON and interrupted atomic JSON, including readable
identity fields before EOF truncation. Simply preserving a known selected-meeting
record while reporting complete erasure would hide sensitive remaining content;
removing it would also remove foreign evidence. The conservative error preserves
all original bytes and requires the conflict to be reviewed before another
preview. Old-code cases proved both ordinary and temporary mixed records were
previously accepted. All four complete/truncated and ordinary/temporary cases
now refuse safely. Rebuildable multi-meeting retention plans remain removable,
and the paired checks preserve every foreign original byte. Existing unknown
and unrelated remnants retain their previous handling and disclosure.

All 389 Rust workspace/all-target tests pass, including 27 core erasure cases;
the same two existing special entry points remain normally ignored. All 247
frontend tests, TypeScript, production build, Rust/frontend formatting, Clippy
with warnings denied and verify.ps1 pass. Test-setup failures (an unsupported
matcher, missing test import and an independently held source-writer fixture)
remain recorded separately from genuine before-code failures. No test deadline,
assertion purpose, dependency lock or protection/reviewer rule was relaxed.

Visible synthetic browser checks exercise pending preview and erase starts,
Stop, deterministic simulated admission rejection, enabled Close and actual
dialog closure, at desktop and 390-pixel width with no overflow or console
errors. The fixture has no real IPC, filesystem or erasure implementation.
No real meeting, device, model server, external copy or installed app is used.
Application version, primary session format and whole historical TEST_REPORT.md
remain unchanged. Fresh candidate 12 target CI and final-head reviews are still
required; all earlier distribution, independent-process, hardware, provider,
external-copy and model-residency integration limits remain in force.

## Candidate 13: distinguish transcript backups from external backups

The second full candidate 12 review found ambiguous scope wording: the README
and both dialog languages said that backups remain outside removal, although
app-managed transcript backups are included in the existing file inventory.
They now explicitly say those transcript backups are also removed and identify
only external backups as outside the operation. The scope and limits above use
the same distinction. This corrects the information given before confirmation;
it changes no inventory, permission, confirmation, cancellation or removal logic.
The original wording is preserved privately as evidence, without inventing a
runtime failure for this text-only correction.

Fresh frontend validation on Windows, with application version 1.0.7, passes all
247 existing tests, TypeScript, the production build and the changed component's
format check. Required repository verification is performed on the final source
before export. No new test merely matching the replacement sentence is added.
All native code is byte-identical to candidate 12: its 389 Rust workspace and
all-target tests, 27 core erasure cases, two regularly ignored special entrypoints,
Rust format and Clippy with warnings denied remain earlier evidence and were not
rerun for candidate 13. Dependencies, protection and reviewer rules, the primary
session format, version and whole historical TEST_REPORT.md remain unchanged.
New candidate 13 target CI and final-head review rounds are still required.
No real meeting or external backup was accessed or removed; independent-process,
installed-app, hardware, provider and combined model-residency limits remain open.

## Candidate 14: confirmation ownership, readable contention and foreign evidence

The complete candidate 13 review found that opening erasure from meeting details
put the full runtime Meeting into summary-typed confirmation state. TypeScript
did not remove its transcript parts, recording warnings or diagnostic fields.
All erasure entry points now construct a new object containing only the identity,
title, start time, duration, status and model identifier needed by confirmation.
Two full-App regressions reach failed and cancelled erasure with synthetic
transcript/warning data and inspect the actual dialog component boundary. They
failed before the correction because the full object was still owned by that
state, and now pass with only the six confirmation fields. The original frozen
meeting remains unchanged. This verifies object ownership, not forensic clearing
of the renderer heap, physical memory or another process.

Exclusive publication contention now reports a bilingual wait-and-retry action
after the same bounded lock retries. The platform's actual lock-contended error
and WouldBlock are mapped; unrelated I/O errors keep their original cause.
Shared publication behavior and retry limits are unchanged. A held pair of real
shared filesystem leases reproduced the former raw Windows error 33 through
the public preview API. The strengthened test now checks readable rejection of
both preview and confirmed erasure, preserved source bytes and successful erasure
only after both publishers release their leases. This uses one process with
independent file handles, not two installed application processes.

The architecture text's additional identity proposal was also validated against
the code. Foreign identity evidence had been filtered by eligibility to authorize
native deletion. Six synthetic orphan-source and ordinary/EOF maintenance-log
variants therefore accepted a conflicting legacy or otherwise ineligible string
identity; a separate hash-valid ZIP regression also failed before correction.
Every readable competing string identity now blocks source/ZIP attribution, and
complete structured meetingId fields remain evidence in ordinary and temporary
records even when they cannot authorize native deletion themselves. Target
eligibility stays strict. All six variants now block before removal and preserve
every pre-existing byte. Verified and orphan ZIPs, primary and backup generations,
and the earlier eligible-identity controls remain covered. Rebuildable shared
retention plans retain their separate policy. This prevents silent filtering of
foreign evidence; it grants no authority to erase imported or invalid identities.

Fresh validation on Windows at unchanged application version 1.0.7 passes 249
frontend tests, TypeScript, production build and frontend formatting. The final
Rust workspace/all-target run passes 390 tests, including all 28 core erasure
cases; the same two separate special entrypoints remain regularly ignored.
Rust formatting and Clippy with warnings denied are checked on that final code,
and repository verification is required before export. Earlier intermediate
checks and genuine before-failure logs remain preserved privately. No existing
assertion, retry budget, confirmation rule or protection threshold is relaxed.
Dependencies, primary session format, version and whole historical TEST_REPORT.md
remain unchanged. No new browser, installed E2E, independent-process, hardware,
provider or physical-memory proof is claimed. Fresh candidate 14 target CI and
final-head review rounds are still required.


## Integration after the separately reviewed model-residency work

The combined source retains PR35's passive startup-gap diagnostics, the final
PR36 UI remedies and deterministic readiness fixture, and PR37's actual native
provider model ownership. Shared stylesheet and capture merge conflicts were
resolved against the reviewed components; startup/privacy/PCM thresholds,
manual erasure admission, identity/hash checks and all protection rules remain.
The existing approved public TEST_REPORT is inherited byte-for-byte from main;
no private full report or raw synthetic conversation evidence is published.

Confirmed erasure now detaches only its meeting's inactive Live and Analysis
model-owner anchors after successful maintenance admission and cache-forget,
outside application and owner-map mutexes and before filesystem removal. Other
meetings' anchors and in-flight client references retain their leases. Native
cleanup still follows the established exact-owned-instance acknowledgement and
fail-closed rules; no guessed foreign unload or destructive auto-resume is added.

Two worker-level synthetic regressions reproduce the missing-anchor release
before the fix and pass afterward for completed erasure and stale-preview
failure. They also check refused admission preserves anchors, unrelated meeting
files/owners survive, and an outstanding client lease stays usable. The initial
pre-test compilation error was separately caused by an existing Worker test
initializer missing the newly inherited real source-publication lease; it is
not counted as a reproduced runtime failure. The actual before run contains
two failed assertions; the after run contains two passes. These loopback-port-9
fixtures do not contact a real provider, and do not prove installed GUI or
independent application-process erasure.

The combined source also inherits the separately validated bounded C# crash-test
lease probe and the configured-provider host-identity correction. Different
loopback binds with the same port/path do not inherit native model management;
localhost retains its pinned IPv4 alias. No provider consent, URL exclusion,
production deadline, dependency pin or 80-percent review documentation threshold
is weakened. Fresh real provider and final combined-source verification results
are recorded separately below when completed.

Fresh combined-source verification after the host correction passes 426 Rust
workspace/all-target tests, with two existing special entries regularly ignored,
plus Rust format, Clippy with warnings denied, and required verification
(47 PCM, 35 notes, 28 templates, eight agents and monitor checks). The earlier
272 frontend tests, TypeScript and production build still cover byte-identical
UI sources; those UI tests were not relabelled as a new run. One attempted Rust
rerun failed during compilation because the shared own target cache still held
the other source's compiled core API. The scoped own package artifacts were
rebuilt after all own compiler processes ended; no source correction or test
assertion relaxation was required. The failed compilation log is retained.


The fresh real reference on this combined source imports its actual production
local_models.rs selector, model_sessions.rs coordinator and meeting-notes core.
The selector recognizes the configured LM Studio endpoint and rejects a different
loopback bind on the same port before native model mutation. Existing selected
google/gemma-4-e4b Q4_K_M is used, without downloads or user meetings. Versions
are HandyMeeting 1.0.7, Windows 11 Pro 26200, i7-13700, RTX 4070, LM Studio
0.4.26+4 / CLI dcefd5a, 8192 context and 6,326,936,720 installed model bytes.
Same-instance actual idle residency lasts 330.190348 seconds with eleven
samples; the harness lasts 459.551586 seconds and its sampling
wrapper 461.547000 seconds. Actual selected-model foreign admission
preserves an independently loaded fixture-owned instance; only that exact test
instance is then unloaded. No unrelated foreign model is loaded just for testing.

Positive live and post analysis each retain one explicit decision and one task
with literal Anna/Freitag; nonbinding cases retain zero decisions/tasks. Quotes
and existing source IDs were manually read. Durable cancellation checkpoints,
byte-preserved originals, resume without repeating confirmed steps, final live
disk readback, other-owner and in-flight client deferral, and exact final owned
instance release all pass; the native catalog and loaded state equal baseline.
The positive final overview accepts five items and rejects one. Its rejection
reason is persisted separately in the overview as ownerOrDueOutsideTasks: 1;
it is read from this run rather than borrowed from old runs. The task remains
separately present. The nonbinding post summary is empty. These are disclosed
limits, not semantic completeness or zero-rejection proof. Validators stay exact.

There are 93 memory samples at a nominal five-second interval. Maximum
individual llama-server Working Set is 6,803,685,376 bytes; maximum individual
Private Bytes is 6,852,263,936 bytes. GPU usage for the entire device is
3987 MiB before inference, 8741 MiB peak and
3964 MiB after release. Other desktop load is included; per-process peaks
are not simultaneous sums, exclusive attribution or complete reclamation proof.
Own builds/local checks ended before this run; light source/GitHub inspection
overlapped, existing desktop apps remained, and no capture/ASR/download ran.
All 2,996 tracked source files stayed unchanged during the reference; only this
final sanitized results paragraph is subsequently added. Production code and
dependency bytes remain exact for publication.

The real reference exercises native model lifetime through core/coordinator
components. The erasure-to-model-owner bridge is separately covered by the two
synthetic worker regressions above, not by installed GUI erasure against a real
provider. No installed application, independent app-process recovery, actual
Ollama, physical capture, GPU endurance, combined two-hour or forensic-memory
claim follows. All existing deletion scope and partial-failure boundaries remain.
Fresh current CI and two clean rounds per reviewer are required before merge.


The later candidate 11 model architecture review exposed a shared Ollama policy
overwrite on final release. The combined source now carries the conservative
correction: confirmed erasure detaches only its own inactive model anchors;
Ollama stops our renewal without native unload/cached-TTL rewrite and may remain
warm until its existing finite server grace expires. LM Studio still releases
only the exact acknowledged own instance after other clients release. This does
not add provider-history, forensic-memory or physical erasure promises. Earlier
reference runs and prepared candidate 15 retain their actual historical bytes;
they are not final evidence for this later source. Fresh validation follows.

The conservative-release combined source passes all 428 Rust workspace/all-target
tests (two existing special entries regularly ignored), including all 25 provider
fixtures and the real 60-second renewal plus 65-second no-renewal interval. Rust
format, Clippy with warnings denied and required verification (47 PCM, 35 notes,
28 templates, eight agents and monitor checks) pass. The UI remains byte-identical
to the separate 272-test/TypeScript/build proof. A new actual selected-provider
reference 03 is required; references 01/02 are retained as historical evidence.


The fresh selected-provider reference 03 after the conservative Ollama correction
passes all preregistered real checks through the actual production selector,
coordinator and core modules. The existing google/gemma-4-e4b Q4_K_M model is
used with LM Studio 0.4.26+4 / CLI dcefd5a, 8192 context and
6,326,936,720 installed model bytes, HandyMeeting 1.0.7, Windows 11 Pro 26200,
i7-13700 and RTX 4070. No downloads or user meetings are used. The configured
provider is recognized and a different same-port loopback bind is rejected.
Actual same-instance idle residency lasts 330.161955 seconds at eleven
samples. Harness duration is 468.295515 seconds, with sampling
wrapper 474.187000 seconds. Positive live/post decisions and tasks,
nonbinding rejection of commitments, original literal sources, durable checkpoint
resume, final readback, other-owner/client deferral and exact own LM Studio
instance release all pass. Baseline native catalog/loaded state is preserved.

All accepted output items and original quotes/source IDs were manually read.
The positive final overview accepts 5 items and rejects
1; its persisted overview records ownerOrDueOutsideTasks: 1.
The reason is read from this run rather than assigned from old runs. The positive task remains separately present, unknown
owner/due stay unknown, and the nonbinding post summary has
0 items. No semantic completeness, zero-rejection
or validator-relaxation claim follows.

Memory has 94 samples at nominal five-second intervals. Maximum
individual model-server Working Set is 6,802,632,704 bytes and maximum individual
Private Bytes is 6,854,340,608 bytes. Whole-device GPU usage is
3754 MiB before inference / 8374 MiB peak /
3747 MiB after release. These are model-server process peaks; the
renamed private harness is excluded by the sampler's process-name filter, so
no harness or installed-application memory peak is claimed. GPU readings include
other desktop load; no simultaneous process-peak sum, exclusive attribution or
complete reclamation is established. Own builds/local checks ended before the
reference; existing desktop apps and light source/GitHub inspection remained.
No capture, ASR or model download overlapped.

All 2,996 tracked source files stayed unchanged during this
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


## Combined integration candidate 18: native origin and expiry refusal

This prepared source inherits the concrete native-origin and unreadable Ollama
expiry corrections described in the shared local-model validation note. All
432 Rust workspace/all-target tests pass, with two existing special entry
points regularly ignored; the 29 provider fixtures include all four genuine
before/after regressions. Locked/offline warnings-as-errors Clippy, Rust format
and verify.ps1 pass. All frontend files are unchanged from the existing 272-test
TypeScript/build/format integration proof; no new frontend run is claimed.
The full approved public TEST_REPORT.md remains byte-identical. A fresh real
selected-provider reference 04 follows; current final-head CI and two clean
review rounds remain required before this combined source can merge.


The fresh selected-provider reference 04 after the concrete-origin and unreadable-expiry corrections
passes all preregistered real checks through the actual production selector,
coordinator and core modules. The existing google/gemma-4-e4b Q4_K_M model is
used with LM Studio 0.4.26+4 / CLI dcefd5a, 8192 context and
6,326,936,720 installed model bytes, HandyMeeting 1.0.7, Windows 11 Pro 26200,
i7-13700 and RTX 4070. No downloads or user meetings are used. The configured
provider is recognized through localhost pinned to 127.0.0.1, while a different
same-port loopback bind is rejected.
Actual same-instance idle residency lasts 331.072850 seconds at eleven
samples. Harness duration is 468.476504 seconds, with sampling
wrapper 471.593000 seconds. Positive live/post decisions and tasks,
nonbinding rejection of commitments, original literal sources, durable checkpoint
resume, final readback, other-owner/client deferral and exact own LM Studio
instance release all pass. Baseline native catalog/loaded state is preserved.

All accepted output items and original quotes/source IDs were manually read.
The positive final overview accepts 5 items and rejects
1; recorded rejection counts are
{"ownerOrDueOutsideTasks": 1}. No rejected item is counted as accepted or assigned from old runs. The positive task remains separately present, unknown
owner/due stay unknown, and the nonbinding post summary has
0 items. No semantic completeness, zero-rejection
or validator-relaxation claim follows.

Memory has 95 samples at nominal five-second intervals. Maximum
individual model-server Working Set is 6,802,550,784 bytes and maximum individual
Private Bytes is 6,852,988,928 bytes. Whole-device GPU usage is
3930 MiB before inference / 8745 MiB peak /
3835 MiB after release. These are model-server process peaks; the
renamed private harness is excluded by the sampler's process-name filter, so
no harness or installed-application memory peak is claimed. GPU readings include
other desktop load; no simultaneous process-peak sum, exclusive attribution or
complete reclamation is established. Own builds/local checks ended before the
reference; existing desktop apps and light source/GitHub inspection remained.
No capture, ASR or model download overlapped.

All 2,996 tracked source files stayed unchanged during this
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


Prepared integration candidate 19 inherits the bounded paired-port test-fixture
correction described in the shared model note. Actual IPv6 native-origin
coverage remains mandatory; no skip, runtime, UI, dependency or protection change
is introduced. The real selected-provider reference 04 retains its original
source fingerprint; all runtime bytes are identical and only the fixture plus
sanitized notes differ. Fresh final native checks are recorded separately.


The fresh candidate 19 local checks pass all 433 Rust workspace/all-target tests,
including 30 provider fixtures, with two existing special entry points regularly
ignored, plus strict locked/offline Clippy, Rust format and verify.ps1. The
existing 272-test frontend/TypeScript/build proof retains identical UI bytes.
No new model run is claimed: all runtime modules remain exact to actual
reference 04; only test-fixture and sanitized validation-note bytes differ.


Integration candidate 20 corrects only the two sanitized validation notes: original
overview JSON results from references 01–03 do persist ownerOrDueOutsideTasks: 1, while
the separate diagnostic exposes only the rejected count. Five accepted overview
items, the separate explicit task, all original measurements and private evidence
remain unchanged. Reference 04 was already correct. All runtime/test/UI/dependency
and full approved public TEST_REPORT bytes are exact to prepared 19; no additional
model, frontend or Rust run is claimed for this documentation-only correction.

## Integration candidate 26: distinguish a read failure from unfinished erasure

When History cannot read the interrupted-erasure list and no pending entry is
known, the notice now reports that read failure without claiming an unfinished
deletion or offering an inspect/continue action. A later successful empty read
removes the notice. If a previously read pending entry exists and a refresh
fails, its warning and inspect action remain available; continuation still
requires the existing fresh native preview and confirmation.

Four targeted component cases exercise German and English error-only states,
empty retry, and preservation of a known entry after a failed refresh. The two
error-only cases failed against the former component, while both known-entry
controls passed. All four pass after correction. An initial managed temporary
file-rename failure ran no tests and is not counted as a reproduced product
failure. The final full frontend run passes 276 tests, TypeScript and production
build; the changed component, tests and styles pass the existing formatter.

Actual browser rendering with the production component and stylesheet exposed
a second issue: the shared horizontal notice layout squeezed the heading,
details and pending row beside one another. A scoped block layout and wrapping
pending rows now keep them readable without changing other notices. A browser
geometry check failed before that style correction and passes afterward.

Synthetic browser checks use existing Playwright 1.57.0 and installed Microsoft
Edge 156.0.4314.50 at 1280 by 900 and 390 by 844 pixels. Both languages, empty
retry, retained known-entry state after a failed refresh and its inspect callback
pass. Page identity, nonblank content, absent framework overlay, zero relevant
console errors/warnings, stacked notice geometry and no horizontal overflow are
checked; screenshots were inspected. The callback receives only a synthetic
identity. This is component validation without native IPC, actual erasure, user
meetings, models, independent installed-app processes or screenreader speech.

The npm tool matches the existing workflow's pinned 12.2.1 integrity; package
and lockfile bytes are unchanged. Backend, model transport, reservation logic,
native tests, dependency locks and full approved public TEST_REPORT bytes remain
exact to the previous candidate. Native workspace checks and the actual selected
LM Studio reference 04 are retained with their original source identities; this
UI correction is not a new native or model-reference run. Fresh verify.ps1 is
required before publication, followed by fresh final-head reviews and CI. No
deadline, protection, privacy boundary or documentation threshold is reduced.
