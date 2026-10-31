# Native desktop validation: macOS, Ubuntu and Debian

Related: [Mac issue2](https://github.com/Puma7/handy-meeting/issues/2) and
[Linux issue3](https://github.com/Puma7/handy-meeting/issues/3). A CI-built package,
an isolated keychain test, a synthetic pipe test and a physical-device meeting are
separate evidence. These issues stay open until the requested target evidence
exists. No physical Mac/Linux recording is claimed by this document.

## Test matrix and evidence

Test Apple Silicon and Intel separately on macOS 15+, Ubuntu 22.04/24.04 and
Debian 12/13 on x64 desktops. Linux needs PulseAudio or PipeWire-Pulse. Record the
package/executable SHA-256, version, OS/build, chip/CPU, RAM, GPU/driver, audio
devices, selected model/revision/runtime, permissions and test start/end times.
Do not publish meeting audio, keys, local paths, device serials or private text.
Keep local originals and every failed run. Use new output directories and an
immutable copy of the application for long tests.

Before each test, write the expected outcome and numerical limits. Reuse the
existing predeclared memory limits:128MiB worker Private Bytes growth and128MiB/h
linear slope after two-minute warmup;128MiB Working Set growth. For the app host,
128MiB Private Bytes growth and64MiB/h after five-minute warmup. These Windows
metrics are not portable synonyms for `ps` VSZ, RSS or physical footprint. Report
native resident/private/footprint counters under their actual names, and mark a
nonavailable metric/criterion as not evaluated. GPU dedicated/shared memory stays
separate. Record parallel model loads and compilation as external activity.

## Guided application checks

1. Install the complete architecture-specific candidate and Handy with an
   installed compatible streaming model. macOS candidates are ad-hoc signed,
   not notarized. Do not disable system protection as part of this checklist.
2. Open the native window. Check DE/EN, light/dark/system appearance, keyboard
   navigation, focus, template editing, original citations, history and local
   text-model discovery. Browser screenshots do not establish native rendering.
3. Deny microphone and system-audio permissions separately, then grant each
   explicitly in OS settings. Verify an actionable error and retained data;
   do not silently start the microphone after a denied request.
4. Make a short simultaneous system+microphone recording. Play a known synthetic
   system speech fixture and speak a different microphone marker. Listen to
   both saved tracks and compare reference markers. Estimated silence and
   `hasSignal` alone do not establish a successful source recording. A suggested
   preregistered short-track length difference is500ms, as in the diagnostic.
5. Test source selection, default-device changes, unplug/reconnect, audio-server
   interruption, sleep/resume and ordinary stop tails. Retain warnings and
   measure actual gaps; do not overwrite the recording to conceal them.
6. Stop only an owned ASR child while recording continues. Verify audio advances,
   recognition resumes and confirmed text remains. Then kill only the owned
   test application after an explicit confirmed checkpoint. Reopen and compare
   audio PCM prefixes and text-journal prefixes against the recorded hashes.
   Never kill Handy or another person's/user's recording process.
7. Repeat for two real hours with both tracks and dense per-process memory
   measurements. Distinguish worker restarts/backends and preserve the original
   limits. The entire saved audio must match the source timeline/markers and
   confirmed text must retain source assignment. A process kill is not a power
   failure, and an unconfirmed tail can be missing.
8. Test disk-full/write-denied in a small isolated test volume or owned test
   directory. Never fill the user's system disk or change a whole device's
   permissions. Check that the failure is visible and earlier confirmed data
   remains recoverable. Restore only the owned test fixture.

## Explicit audio-only diagnostic

The `meeting-core` example uses the actual production capture adapters and no
ASR/network. It is never started by ordinary verification. Short run:

```text
cargo run -p meeting-core --example device_capture -- NEW_OUTPUT 30
```

Long device access requires an additional explicit opt-in:

```text
cargo run -p meeting-core --example device_capture -- NEW_OUTPUT 7200 --allow-long-recording
```

On macOS build the existing Swift helper as documented in `RUST_PORTABLE.md` and
place the matching `HandyMeetingCapture` beside the diagnostic executable. The
ordinary packaged app already has that helper. Ensure permissions apply to the
executable actually being tested. Linux uses its existing audio service; the
diagnostic does not start or reconfigure that service.

`plan.json` precedes capture. `session-pointer.json` identifies its owned test
session; immutable numbered snapshots are written every ten seconds. The tool
prints its own PID. A tester may explicitly terminate that process to test a
crash, then run:

```text
cargo run -p meeting-core --example device_capture -- --recover OWNED_OUTPUT
```

Recovery refuses an active writer and compares every earlier confirmed PCM
prefix in the last diagnostic checkpoint. A longer recovered prefix is allowed
only when its older prefix still hashes identically. Missing/changed confirmed
data fails. New recovery reports never replace old evidence. This diagnostic
has no transcript or ASR; completing it does not complete the full-app cases
above. Source clocks on portable adapters remain approximate.

## Result record

For each case, record pass/fail/not-run, actual duration, steps, warnings,
expected/observed markers, metric definition, numerical calculation, frozen
thresholds and local evidence paths. Keep failed initial attempts. Explicitly
list missing permissions/hardware or unavailable counters. Send a sanitized
report for review before closing either issue. Developer ID signing and Apple
notarization remain a separate distribution requirement.
