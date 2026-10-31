<p align="center"><img src="assets/handymeeting.svg" width="76" alt="Meeting for Handy logo"></p>
<h1 align="center">Meeting for Handy</h1>
<p align="center"><strong>Your meeting. Your computer. Your transcript.</strong></p>
<p align="center">Local recording, live transcripts and meeting notes with original quotes.</p>

<p align="center">
<a href="https://github.com/Puma7/meeting-for-handy/releases/latest">Download</a> ·
<a href="README.de.md">Deutsch</a> ·
<a href="docs/USER-GUIDE.md">User guide</a> ·
<a href="https://github.com/Puma7/meeting-for-handy/releases">Changelog</a> ·
<a href="https://github.com/Puma7/meeting-for-handy/issues">Feedback</a>
</p>

[![Rust checks](https://github.com/Puma7/meeting-for-handy/actions/workflows/rust.yml/badge.svg)](https://github.com/Puma7/meeting-for-handy/actions/workflows/rust.yml)
[![Linux and Mac packages](https://github.com/Puma7/meeting-for-handy/actions/workflows/rust-portable.yml/badge.svg)](https://github.com/Puma7/meeting-for-handy/actions/workflows/rust-portable.yml)
[![Release packages](https://github.com/Puma7/meeting-for-handy/actions/workflows/rust-release.yml/badge.svg)](https://github.com/Puma7/meeting-for-handy/actions/workflows/rust-release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

An independent **Rust + Tauri companion for [Handy](https://github.com/cjpais/Handy)**. Record the audio of Zoom, Teams or any other desktop meeting, optionally your microphone as a separate track, and transcribe it locally while the meeting runs. A live summary keeps up during the meeting; afterwards you turn the transcript into summaries, decisions and tasks — without sending audio to a cloud service.

## Download

**[Latest release](https://github.com/Puma7/meeting-for-handy/releases/latest)** — open *Assets* and pick the package for your computer. What changed in each version is listed on the [releases page](https://github.com/Puma7/meeting-for-handy/releases).

| Your computer | Package | Requirements |
| --- | --- | --- |
| Windows 10/11 · x64 | Setup EXE or portable ZIP | WebView2 and the Microsoft Visual C++ x64 runtime |
| Ubuntu 22.04+ / Debian 12–13 · x64 | DEB | PulseAudio or PipeWire-Pulse |
| Mac · Apple silicon | `osx-arm64` DMG | macOS 15+, recording permissions |
| Mac · Intel | `osx-x64` DMG | macOS 15+, recording permissions |

On Windows the setup installs into `%LOCALAPPDATA%\Programs\Meeting for Handy` and adds a Start menu entry. Running a newer setup updates the app in place; recordings and settings are kept. The app can also check for updates itself.

The packages are not code-signed (macOS: ad-hoc signed, not notarized), so the operating system may warn before the first start. Do not switch off system protection to run them. Mac/Linux device capture and a full two-hour run of the Rust app still need real-hardware validation; test with a non-critical meeting first. [Measured evidence and open limits](TEST_REPORT.md).

## What it does

| Capture | Understand | Keep control |
| --- | --- | --- |
| Separate system-audio and optional microphone tracks with live level meters | Local streaming speech recognition with compatible Handy models | Audio capture keeps running even if recognition fails |
| Live transcript with search, pause/resume and microphone mute | **Live summary** during the meeting: overview, decisions, tasks, questions | Local history with search, names, VIP flags, archives and audio copies |
| TXT, Markdown or JSON export and one-click session folder | Post-meeting analysis with editable templates and tabs | Original transcripts and every analysis version are retained |

Every statement in a summary links to an existing transcript section and a verbatim quote; unknown owners and deadlines stay unknown. Results remain AI drafts to review.

![Meeting for Handy transcript and local file controls](assets/screenshots/transcript-light.png)
*Real app components with synthetic demo data. English and German interfaces are included.*

![Meeting for Handy source-linked analysis](assets/screenshots/analysis-light.png)

## First meeting

1. Install **Handy** and download a compatible **streaming speech model** there. Meeting for Handy reads Handy's model list; model weights are not part of our downloads.
2. Install the package for your computer (Windows setup or ZIP, Linux DEB through the package manager, Mac app from the DMG into Applications).
3. Open **Meeting → Recording setup**, name the meeting, choose a speech model and the audio output used by the meeting app. Enable your microphone for a separate track.
4. Start a short test and check both level meters. The output can also contain other apps' sounds.
5. End the meeting, read the transcript and open **Analyse this meeting**. **Open session folder** shows the audio, journals, exports and analyses.

For summaries you need a local text model as well: **Ollama** or **LM Studio**. The app finds both on this computer, suggests Gemma 4 E2B/E4B according to your memory and can download the suggestion through Ollama. Speech models and text models are different things; a speech model alone writes no notes.

Live summaries keep the selected local model reserved until the final summary is saved; failed finalization keeps it available for retry. LM Studio requires native REST v1 support and a confirmed app-owned instance.
For the first app-owned load when preparing analysis or live summaries, the selected LM Studio model key must have no loaded instance. An already loaded foreign instance is neither adopted nor unloaded, and the app does not load a second copy. An existing confirmed app-owned reservation is reused.
Ollama uses a shared server TTL rather than per-client instance ownership. Finishing the last local owner stops our renewal without sending an unload or restoring a cached TTL. An app-owned model may therefore remain warm for its existing finite server grace (normally up to 300 seconds after the last request); another client may extend it. LM Studio still releases only its exact acknowledged owned instance.
Native model reservations use one concrete loopback address throughout inventory, loading, inference and cleanup. For this lifecycle, localhost means 127.0.0.1; configure an explicit [::1] endpoint for an IPv6-only provider. An unreadable expiry on an already loaded Ollama model stops preparation before any model mutation; its retention policy is not guessed.
See the [provider requirements, reference evidence and limits](docs/validation/local-model-residency-20261030.md).

## Privacy and limits

- **Audio stays local.** Speech recognition runs in the app's own local worker processes and reuses Handy's model files.
- **Local text models are the default.** A cloud provider is optional and needs your explicit approval for every run; it receives text, source IDs and instructions, never audio or file paths. There is no automatic cloud fallback, and no system proxy is used.
- **Keys are kept for the session only** unless you store them in Windows Credential Manager, macOS Keychain or Linux Secret Service.
- **Recovery:** saved checkpoints survive tested process crashes; the last unconfirmed seconds can be missing. This is no power-loss guarantee. Recordings, transcripts and analyses are never deleted automatically.
- **Two tracks are not speaker identification.** Names are assigned only when you confirm them; optional speaker experiments give neutral per-section IDs.
- **No hidden recording.** Local agents need visible approval for each recording and separate microphone consent ([agent protocol](docs/RUST_AGENT_PROTOCOL.md)).

Data lives in the local application-data folder `HandyMeeting/RustBeta` (name kept for compatibility). Files are not encrypted; a sync service may upload them.

## Built alongside Handy

The [original discussion](https://github.com/cjpais/Handy/discussions/1923) places meeting features outside Handy's own scope; Meeting for Handy explores that companion direction. It reads installed model metadata and uses the transcribe.cpp bindings with a pinned native runtime; it never changes Handy's settings, credentials or models. This is **an independent project**, not an official Handy product. [Integration boundary](docs/HANDY_UPSTREAM_PROPOSAL.md).

## Development

[Build and verification](docs/RUST_BUILD.md) · [Releasing](docs/RELEASING.md) · [CI](docs/CI.md) · [Local files](docs/MEETING_FILES.md) · [Device checklist](docs/NATIVE_VALIDATION.md) · [Roadmap](docs/OPTIONAL_FEATURE_ROADMAP.md) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)

| Folder | Content |
| --- | --- |
| `crates/` | Rust libraries: capture and sessions (`meeting-core`), notes and live summary (`meeting-notes`), Handy detection (`handy-bridge`) |
| `apps/desktop/` | Tauri app: Rust backend (`src-tauri/`) and React interface (`src/`) |
| `scripts/` | Build, packaging, installer and release scripts with their tests |
| `docs/` | Documentation; `docs/evidence/` holds measurement records, `docs/legacy/` the C# prototype documents |
| `licenses/` | Original license texts of all bundled dependencies |

**Releases:** create a GitHub release with a new tag (`v1.2.3`); version, packages and release notes follow automatically ([details](docs/RELEASING.md)).

**Legacy C# prototype:** `src/`, `verify.ps1`, `build.ps1`, `setup-native.ps1`, `global.json`, `NuGet.config`, `System.Numerics.Tensors.*`, `licenses/nuget/`, `licenses/nuget-packages.json`, `licenses/Avalonia-MIT.txt` and `licenses/Inter-OFL.txt` belong to the earlier C# prototypes (0.4/0.5). They are still checked by CI but no longer shipped. **They will be removed with version 2.0**; from then on the repository contains only the Rust project.

MIT licensed. Thanks to Handy, transcribe.cpp, Tauri and the libraries this project builds on; their notices are in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) and `licenses/`.
