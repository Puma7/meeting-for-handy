# 30 October 2026 – first real portable-ASR CI outcomes and runtime correction

On PR #35 head `b4c6494787d74ccaf846aac369ce35968f47167e`, the complete
[Mac ARM job](https://github.com/Puma7/meeting-for-handy/actions/runs/41020929728/job/123947869618)
passed the real Handy 0.9.8/Q8 CPU test: 45.133471083 seconds, 31 reference words per
track and zero word errors on either, owned worker restart, audio hashes, confirmed
journal prefix and reopened text preserved. This is short generated speech, not
physical capture, GPU, real-time performance or a two-hour result.

On head `bfce18849590fd3328a2d880eb20dbc20ced9666`, the
[Linux preflight](https://github.com/Puma7/meeting-for-handy/actions/runs/41024330019/job/123958786715)
proved a GLIBC mismatch in the upstream Ubuntu 24.04-built AppImage on our Ubuntu 22.04 host.
The official Ubuntu 22.04-built DEB is now pinned instead: 41,941,410 bytes,
SHA256 `4efe1fdcc2b68e95612e866810ccb23e06bbdf004a4e857de0862b0c6927caa8`.
Its downloaded bytes and regular `usr/bin/handy` were independently inspected,
without execution or installation. A regression requires private extraction and
rejects AppImage execution or package installation.

The same head's [Intel job](https://github.com/Puma7/meeting-for-handy/actions/runs/41024330019/job/123958786818)
exceeded the 45-second real Handy catalog limit. No Intel ASR success is claimed.
The next CPU-only attempt disables child Metal enumeration, as supported by the
verified source of Handy's transcribe-cpp-sys 0.2.4 (crate SHA256
`1c6946c7bf90046fc51e5bc25373a4f6270ca10fdcf4f35b1174075e43c535b5`).
This removes an unnecessary GPU initialization boundary; the timeout itself does
not prove where Handy stalled. New Linux/Intel success still requires actual CI.
No deadlines, content, preservation or completeness criteria were relaxed.

The extracted DEB executable uses `$ORIGIN/../lib/Handy:$ORIGIN/../lib`
for its shipped transcribe/ONNX libraries. Its additional `libgtk-layer-shell.so.0`
system dependency is explicitly installed in the disposable Linux job.
All 110 local Python guards ran (109 passed; one Unix-link case skipped on
Windows). The required project check passed. The Rust/frontend product code
retains its previously checked bytes apart from the documented diagnostic input.
These new notes contain only public CI references and synthetic checks. The
previously published full PR #35 report is unchanged.


## Linux runtime dependency follow-up

The [Linux job on 25aad](https://github.com/Puma7/meeting-for-handy/actions/runs/41031813793/job/123983123795)
reached the real catalog preflight but failed with a missing shared-library flag.
Reading the actual pinned DEB control file and ELF dependencies showed the remaining
external requirements: libopenblas0 and libappindicator3-1, in addition to the
already installed gtk-layer-shell/GTK/WebKit dependencies. The CI installation
list initially attempted these declared requirements. The private preflight can expose
only explicitly allowlisted missing-library names, never arbitrary paths/errors.
The new Linux run still has to pass; no deadline or success criterion was changed.

Both macOS jobs on historical head `25aad484` completed successfully: [ARM](https://github.com/Puma7/meeting-for-handy/actions/runs/41031813793/job/123983123654) and [Intel](https://github.com/Puma7/meeting-for-handy/actions/runs/41031813793/job/123983123681). The real pinned CPU recognizer took 30.663633958 seconds on ARM and 24.841663909 seconds on Intel. Each generated track had 31 reference words and zero word errors; both checks preserved audio hashes and journals through the owned-worker restart. These are accelerated generated-speech checks, without physical capture, GPU or endurance claims. They do not prove the next head.

After the dependency/diagnostic correction, all 111 local Python guards ran (110 passed; one Unix-only case skipped on Windows), and `./verify.ps1` passed. The missing-library diagnostic regression failed before the fix.

The [Linux job on 23fda92](https://github.com/Puma7/meeting-for-handy/actions/runs/41035965054/job/123996783935)
then exposed an APT conflict: legacy libappindicator3-1 cannot coexist with the
already required libayatana-appindicator3-1. The runner retains its existing
Ayatana development/runtime packages and adds OpenBLAS, the actual missing hard
ELF dependency. The extracted catalog executable has no hard libappindicator3
dependency; the DEB's package-level dependency list also serves its tray GUI.
This avoids installing the legacy conflicting package. A subsequent real Linux
catalog/inference run remains required; the dependency fix is not a passing run.

## Current macOS minimum and completed target CI

The macOS package built on head `6a541d0e06fc22588b463a0369b181bcb69ba8c0`
requires **macOS 15.0 or later** on both Intel and Apple Silicon. The packager and
the current [portable-installation guide](../RUST_PORTABLE.md) declare 15.0.
The actual Intel DMG from [the completed current CI job](https://github.com/Puma7/meeting-for-handy/actions/runs/41036718033/job/123999442517)
was also inspected without mounting, installing or executing it. Its
`Meeting for Handy.app/Contents/Info.plist` reports `LSMinimumSystemVersion=15.0`
and application version `1.0.7`. The downloaded artifact (ID `12969339081`) matched
its published SHA256
`d8a3a24b073e67c6e92494a43cfae9f03bbe6e639983851c4262b7c447986dd8`.
This is package-metadata evidence; no macOS 13 or 14 runtime test was performed.

The macOS 13 statement in the historical `TEST_REPORT.md` belongs to its explicitly
labelled Beta 2 CI/package record. It does not describe the current 1.0.7 package
or extend current support to macOS 13/14. That historical report remains unchanged.

On the same `6a541d0` head, current Linux, Windows, Intel and
[Apple Silicon](https://github.com/Puma7/meeting-for-handy/actions/runs/41036718033/job/123999442411)
checks completed successfully. Both downloaded model-free macOS proof artifacts
matched their published digests and the checked source fingerprint. They include
the real generated-speech CPU recognition, preservation/restart checks and native
credential-store checks. These are CI results for that exact head, not physical
microphone/loopback capture, permission-dialog interaction, GPU or a combined
two-hour meeting. Subsequent commits still require their own current CI result.


## Entire-track virtual tone verification

Full review on 2fedecfb reproduced a silent final two seconds passing the former
middle-half-only signal check despite matching PCM duration and hashes. The
verifier now checks overlapping 250 ms windows from the start through the final
sample. Only a source-specific declared zero prefix is excluded and reported
separately; it must still leave eight seconds of tone. Startup gaps retain the
incomplete verdict and never excuse silence in the other track or later audio.
The existing RMS >1000, 440 Hz ±40, 1000 Hz ±60, PCM/hash and 500 ms track-difference
criteria are unchanged.

Three meaningful pre-fix regressions failed: trailing silence, unexcused leading
silence and a declared prefix replacing required tone. All 115 Python guards now
run with 114 passes and one Unix-only skip on Windows. A positive fixture retains
a declared 500 ms prefix and checks the rest of the track. These are synthetic
verifier fixtures; fresh real Linux virtual-device CI remains required on the
new head. No physical, GPU or long-duration claim follows from them.


## Candidate 14: enough fixture tone after startup

Candidate 13's stricter whole-track signal verifier correctly rejected current
Linux job 124164329203: the declared startup prefix left less than eight seconds
of actual tone. The separate generated-speech ASR check in that same job passed
with zero/31 and one/31 word errors, worker restart and unchanged journals/audio;
this does not make the failed virtual-device step or overall CI successful.
No raw PCM artifact was uploaded, so playback buffering versus capture-helper
startup cannot be separated conclusively from that log.

Before the next actual Linux run, the private fixture now requests 50 ms playback
latency, warms its own sources for three seconds and requests nine seconds of
recording. Recorder readiness can precede receipt of real PCM. This changes only
the supplied fixture within the existing 8–10 second receipt window. The verifier
still requires at least eight seconds of tone after a source-specific declared
zero prefix, all overlapping 250 ms signal windows, the same RMS/frequency limits,
confirmed hashes/counts and at most 500 ms between tracks. A new boundary test
accepts one second of declared prefix plus eight seconds of tone in a nine-second
receipt, and rejects 1.25 seconds prefix at that same length. Startup gaps remain
explicitly incomplete audio. The actual new Linux CI is required; no physical
microphone, performance, GPU or endurance result is implied.

## Candidate 15: numeric virtual-capture failure evidence

The actual candidate 14 Linux run also failed the existing full-track signal
check with "Silent or mixed-up virtual source window". Its final capture flag
was complete and its only warning concerned approximate receipt timing. The
failure did not identify which source/window failed, and no raw PCM was uploaded.
The cause remains unproven; neither playback latency nor a recorder defect is
claimed from that message.

The verifier now adds a bounded JSON diagnostic to that same failure: fixed
source name, window start/end sample, track length, sample rate, measured RMS
and zero-crossing frequency, required frequency/tolerance, unchanged minimum
RMS, and whether this source declared startup padding. This contains no PCM,
audio filenames, storage paths, meeting identities or transcript content.
Synthetic early-microphone and late-system failures verify the reported source
and position; the privacy check verifies that only the fixed source label and
numeric/boolean fields are exposed. The former generic message failed both
diagnostic regressions.

All 118 Python checks ran with 117 passes and one regular Unix-only skip under
Windows. This changes diagnostics only: the 250 ms overlapping windows through
the final sample, eight seconds of actual tone, source-specific zero-prefix
exception, RMS/frequency, 8–10 second track and 500 ms difference limits are
unchanged. The recording fixture, native/frontend sources and the complete
previously approved historical TEST_REPORT.md remain byte-identical to candidate
14. A fresh actual Linux run is required to obtain evidence for the remaining
failure; local synthetic success is not a successful device/target CI result.

## Candidate 16: tone duration independent of window averages

An additional synthetic regression reproduces a real verifier gap: an 8.5 second
track with 10 ms of silence every 125 ms passes every existing 250 ms window,
yet contains only about 7.82 seconds of tone. The old checker accepted it. The
checker now separately excludes runs of exact-zero samples from these known
synthetic sine fixtures, while retaining isolated phase-zero samples. It still
requires eight seconds of qualified tone after any source-specific declared
prefix. The existing single phase-zero boundary allowance for that excluded
prefix remains one sample. Complete exact-eight-second sine tracks pass; repeated
short silences cannot substitute for the required duration. All original RMS,
frequency, overlapping-window, receipt-length, hash and track-difference limits
remain in force, and numeric failure diagnostics remain available.

The per-operation download, synthesis and ASR deadlines remain hard upper bounds.
The workflow's independent 12 minute total ASR-step budget is also deliberately
retained. The sum of every sequential maximum can exceed that total; it does not
grant a slow sequence extra runtime. Hitting the total budget must fail the CI
step, never turn incomplete evidence into success. The workflow now states this
policy explicitly instead of implying that each maximum can be consumed in full.
No check or runtime limit was relaxed to satisfy a review suggestion.

All 120 Python checks ran with 119 passes and one regular Unix-only skip under
Windows. The recording fixture, native/frontend source bytes and complete
candidate 15 TEST_REPORT.md remain unchanged. The real current target CI must
still run; the unexplained Linux signal failure from candidate 14 is not claimed
fixed by these synthetic checker regressions.

Candidate 15's actual Linux job identified the first microphone window
(samples 0–4000): 890 Hz against 1000 Hz ±60, RMS 5342.167 against >1000,
144833 samples in the track, no declared startup gap. This narrows the observed
failure but does not identify its cause. The next diagnostic additionally
counts initial exact-zero samples independently of the startup flag, so a
possible unreported initial padding can be distinguished from other signal
problems. The count never excuses undeclared silence or changes pass criteria.

## Candidate 17: distinguish inserted startup padding from helper input

Candidate 16's actual Linux job failed at microphone samples 0–4000 with
888 Hz against 1000 Hz ±60, RMS 5324.608 and 455 leading zero samples, with
no source-specific startup warning. Those 28.4375 ms explain the window's
lower zero-crossing estimate numerically, but do not establish whether the
zeros were inserted by native receipt-time alignment or arrived from the
helper. No root-cause fix is claimed.

The portable capture path now retains one passive first-packet observation
per source in memory: samples already written, samples actually inserted by
the existing alignment, first packet length and that packet's leading zero
count. Restarts cannot overwrite that first observation. The opt-in device
diagnostic includes these fixed-source integer counters in its result. Windows
and tracks without a received portable packet have no observation, rather than
an invented zero. Only the first packet is measured; a completely silent first
packet does not establish how long silence in later input packets lasts.

The Linux verifier includes only a validated numeric allowlist of these counters
in its signal failure. Arbitrary strings, duplicate source records and inconsistent
counts are rejected without reflecting their contents. Neither counters nor
estimated padding excuse undeclared silence. Every original tone-duration,
window, RMS, frequency, hash, receipt-length and track-difference requirement
remains unchanged. The recording fixture, workflows, locked dependencies and
complete candidate 16 TEST_REPORT.md remain byte-identical. The alignment target,
PCM writes, readiness, cancellation and warning threshold are unchanged.

On Windows x64 with Rust 1.91.1 and warnings denied, all 350 Rust all-target
tests passed, with the same two special credential-provider/crash-writer entries
regularly ignored. The two new synthetic tests compare the entire saved sample
sequence for separate native/helper zeros and exercise the actual portable pump
with an owned synthetic helper. All 123 Python checks ran: 122 passed and the
same Unix-only case was regularly skipped on Windows. New cases prove that
diagnostics preserve rejection of undeclared signal loss and cannot expose
arbitrary result text. The unchanged frontend retains its earlier 225-test
evidence; it was not rerun for these native diagnostic changes.

An actual new Linux CI run is required to distinguish the observed zero prefix's
origin. These synthetic tests provide no physical microphone, local text-model,
GPU, performance or combined two-hour evidence, and do not make the failing
candidate 16 target run successful.

## Candidate 18: declare every estimated initial prefix truthfully

The actual [candidate 17 Linux job](https://github.com/Puma7/meeting-for-handy/actions/runs/41110229620/job/124250502899)
identified the zero prefix's origin. The first microphone window failed at
870 Hz with 523 leading zero samples. Its passive native counters recorded
522 inserted alignment samples, one leading zero in the first 200-sample helper
packet and no previously written samples. The native alignment therefore
accounts for the estimated prefix; the incoming helper did not supply those
522 samples. The previous >8000-sample initial-warning threshold left this short
estimated prefix undeclared and reported completeCapture=true.

The native initial path now declares any nonzero estimated alignment prefix
with its existing source-specific startup warning. This deliberately makes the
capture-integrity flag more conservative, including short prefixes; it does not
prove that audible speech was lost. Intentional privacy-control resumes retain
their existing notice policy. Alignment targets, saved PCM, timing origin,
readiness, cancellation, privacy gates and the passive counters are unchanged.
Incoming zeros without native padding receive no estimated-start declaration.

The verifier is byte-identical to candidate 17: it gets no undeclared-padding
allowance, and counters alone never excuse silence. Its existing declared-prefix
path still requires eight seconds of qualified tone and every overlapping
250 ms window through the final sample, with unchanged RMS/frequency, receipt
length, hashes and 500 ms track-difference limits. A source with declared startup
silence remains passed-with-startup-gaps and completeCapture=false. A new Python
case reproduces the observed short-prefix shape and verifies that removing the
warning/flag fails even when the same native counters remain present.

The deterministic native before case failed because a known short estimated
alignment was not declared, while the incoming-zero negative case passed. After
correction all 351 Rust workspace/all-target tests pass with warnings denied;
the same two special entry points are regularly ignored. All 124 Python checks
ran: 123 passed and the same Unix-only case was regularly skipped on Windows.
Those Python fixture checks overlapped native compilation, not a performance or
endurance measurement. The unchanged frontend retains earlier 225-test evidence
and was not rerun. The complete candidate 17 TEST_REPORT.md, locked dependencies,
recording fixture, workflows and verifier remain byte-identical. The CI guide
now precisely distinguishes nine-second requests, 8–10-second saved tracks and
the eight-second qualified-tone minimum; evidence prose spacing is corrected.

The failed candidate 17 run remains evidence of the diagnosed issue, not a
passing run. Fresh target CI on this correction is required. Physical recording,
permissions, GPU, text-model and combined two-hour reference limits remain open;
no gap-free device or delivery-readiness claim follows from these synthetic
tests or the new warning.
