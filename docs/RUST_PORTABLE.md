# Rust portable packages

The Rust/Tauri desktop now has target-specific capture adapters and package builders for
Linux x86-64 and macOS Apple Silicon/Intel. Package versions follow the release tag. A source
implementation, cross-target compilation, a successful CI package build, and an actual
device recording are different evidence. See `TEST_REPORT.md` for completed checks.

## Supported package targets

| Target | Build baseline | Intended runtime | Package |
| --- | --- | --- | --- |
| Linux x86-64 | Ubuntu22.04, Rust1.91.1, WebKitGTK4.1 | Ubuntu22.04/24.04; Debian12/13 | `HandyMeeting-<version>-linux-amd64.deb` |
| Apple Silicon | macOS runner, Rust1.91.1, deployment target13.0 | macOS15 or newer, M-series | `HandyMeeting-<version>-osx-arm64.dmg` |
| Intel Mac | macOS runner, Rust1.91.1, deployment target13.0 | macOS15 or newer, Intel x86-64 | `HandyMeeting-<version>-osx-x64.dmg` |

These are intended targets, subject to each build and hardware test. Linux ARM, ALSA-only
systems without a PulseAudio-compatible service, macOS12 and universal Mac binaries are
not provided. A Wayland/X11 desktop and the listed GTK/WebKit dependencies
are required for the Linux GUI. The release package is named `handymeeting`, with program files in
`/opt/handymeeting/<version>`; recordings remain outside the package. Earlier Rust beta
packages retain their separate `handymeeting-rust-beta` name and directories. Installing
the release does not delete their data. The application bundle ID and existing RustBeta
data location remain stable to preserve user settings and permission continuity.

## Capture and timing

Linux reuses `pactl` and `parec` from `pulseaudio-utils`, with PulseAudio or PipeWire-Pulse.
The system track uses the server's actual output monitor source, rather than guessing
a `.monitor` suffix. Microphones are separate selectable sources. No audio server is
started, stopped, or reconfigured. A missing utility or unavailable service produces an
actionable error. Device query output and process deadlines are bounded.

macOS reuses `platform/macos/HandyMeetingCapture.swift`: ScreenCaptureKit captures system
audio and AVAudioEngine captures the microphone. Only audio stream outputs are registered;
screen frames are not retained. The helper acknowledges startup after permission and
capture startup. System audio always uses the default output. Besides the default
microphone, the helper's `--list-microphones` mode lists Core Audio input devices by their
stable device UID (no permission prompt), and a chosen one is passed as
`--track microphone --device UID`; an unavailable device fails visibly instead of silently
recording another microphone. This selection compiles in CI but was added without Mac
hardware and still needs a real-device test. Capture needs the system's Screen & System
Audio Recording/Screen Recording permission, and the microphone permission when that track
is requested. Exact settings names vary by macOS.
An ad-hoc CI signature is not a Developer ID signature or notarization; unsigned candidate
distribution still needs an explicit trust/distribution plan before a public release.
On first start, macOS blocks such a candidate; testers open it once via System Settings >
Privacy & Security > "Open Anyway" (Gatekeeper stays enabled). The bundle identifier
`org.handymeeting.beta` is kept: Tauri derives the WebView storage (saved preferences and
the template library in `localStorage`) from it, so a new identifier would hide that data.
Because ad-hoc signatures change with every build, macOS can ask for recording
permissions again after an update regardless of the identifier; only a stable Developer ID
signature avoids that.

Each source has its own child process and disk writer. A bounded two-packet queue carries
16kHz mono16-bit PCM. Stop/cleanup targets only the child started for that track. Received
PCM is drained and the writer is finalized after a helper exits or fails. macOS helpers
also exit on parent-pipe EOF. Linux `parec` stops when the parent's pipe closes; it has no
independent file or network destination. ASR workers remain separate from capture.

These helpers return raw PCM without common device timestamps. Track alignment uses
approximate receipt timing. Leading gaps and pauses without packets can be represented
by estimated silence; late real packets remain retained and timing can drift. A warning
states this limitation. Such silence does not prove the source was silent. This beta
does not claim sample-accurate alignment, complete capture across sleep/device changes,
or reliable two-hour operation on untested target hardware. Actual device gap detection,
stop tails, drift and permission handling require target-specific tests.

## Native runtime and packages

`scripts/prepare-rust-portable.py` consumes `scripts/native-assets.json`. The three official
transcribe.cpp0.4.1 archives have fixed SHA-256 values. Only expected regular native
libraries, the ABI contract and licenses are materialized. Existing different SDK files
are rejected. The separate `.build/rust-native-beta2-0.4.1-<rid>` namespace preserves the
Windows SDK and earlier distributions. SDK preparation downloads no recognition model;
the separate CI speech check downloads a pinned model into its private cache. No model is bundled.

The prebuilt SDK uses the maintained `transcribe-cpp` safe Rust bindings. No local FFI or
recognition algorithm is introduced. Native modules sit beside the executable in `native/`.
Linux uses `$ORIGIN/native`, macOS `@executable_path/native` as the executable's loader path;
upstream dylibs already refer to their siblings via `@loader_path`.

The SDK's actual Linux requirements are checked with `readelf --version-info` on the Linux
build host. All18 official libraries were also inspected from their verified archive on
Windows: the highest observed tags were GLIBC2.27, GLIBCXX3.4.22 and CXXABI1.3.9;
all version-requirement records were confirmed with LLVM. All nine Mac dylibs declare
minimum macOS11.0 in their actual build commands. The capture helper builds for13.0,
while the distributed app declares macOS15.0 as its minimum supported runtime.
Ubuntu22 builds
must still pass real Debian12/13 package installation and headless release checks. The
Mac SDK's minimum build versions are checked with `otool -l` against macOS13.0.

`scripts/rust-portable-artifacts.py` performs filesystem/hash bookkeeping only. It checks
source receipts, current lockfile license manifests, all native library hashes and the
exact executable's model-free release-check result. Staging uses a fresh private directory
and explicit payload lists. `package-rust-portable.sh` builds the native Mac helper, seals
the complete app with an ad-hoc signature, verifies it, and re-runs the relocated binary's
checks without SDK loader environment variables. Final hashes are recorded after signing;
the manifest stays outside the sealed app to avoid circular signature/hash changes.
DMGs/DEBs and manifests have accompanying package hashes and existing candidates are never
overwritten. Models, session histories, settings, API keys and private recordings are not
package inputs. No installer hooks or automatic cloud calls are included.

After final DEB/DMG creation and verification, `bind-package` writes the exact archive
filename, SHA-256 and byte length into its adjacent portable manifest.
The DEB's control file, desktop entry and `/usr/bin/handymeeting` command link (to
`/opt/handymeeting/<version>/HandyMeeting`) are generated by `rust-portable-artifacts.py
deb-metadata`; it recommends `xdg-utils` and `gnome-keyring` (a Secret Service for stored
API keys) without requiring them. Handy discovery on Linux also finds versioned AppImages
such as `~/Applications/Handy_0.9.8_amd64.AppImage` (newest matching version first, after
an unversioned `Handy.AppImage`). The earlier sealed
payload proof remains unchanged. Release assembly requires that binding as well as the
matching source and native/relocated checks. Replacing an archive and recomputing only its
checksum sidecar cannot reuse a different package's manifest.

## CI and required target tests

`.github/workflows/rust-portable.yml` builds the actual Tauri GUI for all three targets,
runs locked workspace tests and warning-rejecting Clippy, executes synthetic bounded-pipe
stop/crash/reopen checks, verifies native runtime linkage, and creates candidate packages.
It also requires real CPU transcription of two generated speech fixtures with pinned
Handy/model assets, an owned ASR restart and source/content checks. Only a sanitized verdict
is uploaded; [CI scope](CI.md) describes the downloads, thresholds and remaining limits.
The Linux job also installs the DEB in Debian Bookworm and Trixie containers and runs its
headless release check through `/usr/bin/handymeeting`. This tests installation and dynamic
linkage, not desktop rendering or physical device access. On Ubuntu 22.04,
`scripts/check-linux-virtual-audio.sh` starts a private PulseAudio server with a null sink and
a remapped virtual microphone, runs the real `pactl` device listing and the
`device_capture` example, and requires two separate tracks carrying their own test tones.
These are virtual devices, not physical hardware. `scripts/check-gui-smoke.sh` launches the
exact release executable (under `xvfb-run` on Linux, directly on the macOS runners) with
empty private data for 15 seconds, requires a live process and a window, then stops it. The workflow uploads artifacts; it does not publish a release.

Before calling a target ready, record an actual simultaneous system/microphone session on
that target; verify speech separation, permission denial, default device changes, pause,
sleep/resume, stop tails, helper/ASR/parent crashes, confirmed hashes after reopening, and
a full two-hour capture with separate host/ASR memory measurements. Local model inference
must use a compatible streaming model already installed by Handy. No such target hardware
or inference result follows merely from successful package construction.
