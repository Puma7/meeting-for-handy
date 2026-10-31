# Building Meeting for Handy with Rust/Tauri

Builds use the version in the workspace Cargo manifest, desktop package/lock and Tauri configuration. Release builds take it from the release tag (`scripts/set-version.py`, see [RELEASING.md](RELEASING.md)); a local build uses whatever version the files carry. Windows produces a checked ZIP and an NSIS Setup EXE; Linux and Mac packages are built on their target hosts. Actual installer execution checks run in the disposable CI account, never against a developer's existing installation. Existing programs, recordings and settings are preserved. Local build commands do not publish GitHub releases.

## Separate tools and filesystem bookkeeping

Verification uses separate standard tool invocations. `scripts/rust-artifacts.mjs` uses only filesystem, path and cryptographic operations: it never starts processes, runs shell commands, downloads packages or changes security settings. Its receipts record exit codes reported by those separate tools; actual CI logs remain evidence that the commands ran.

Native SDK preparation, installer-tool preparation and package-manager setup are separate from source-bound verification. The tooling uses normal developer commands; it does not modify machine security settings or global package-manager versions.

## Prerequisites and preparation

Use PowerShell 7.4+, Python 3.11+, Rust 1.91.1 with rustfmt/clippy, Node 26.7.0 and Visual Studio C++ Build Tools. The preserved C# regression suite also needs the .NET SDK pinned by `global.json`. Running the packaged Rust application does not need .NET or Python.

Prepare the pinned native SDK separately:

```powershell
./scripts/prepare-rust-native.ps1
./scripts/prepare-installer.ps1
```

Use npm **12.2.1** through its local `bin/npm-cli.js`, without changing global npm. Set `HANDYMEETING_NPM_CLI` to a working exact-version CLI. Existing reviewed project-local tools can be reused. For a clean machine, the Windows CI preparation step shows ordinary `npm pack npm@12.2.1`, a fixed archive SHA-512 comparison, and `npm install` of that verified local archive under `.build/tools/npm-bootstrap`, with lifecycle scripts disabled.

The reviewed integrity is:

```text
sha512-bKCmFRVT/BB+YPEYbZwQ1XUI3N5t4xKSpU1MxOjo7z3iWzKBJ0ZXRC1xFdEN+TC7N5TdywaBzf2dMXGJq/e2PQ==
```

Application dependencies use the exact `package-lock.json`:

```powershell
node $env:HANDYMEETING_NPM_CLI --prefix apps/desktop ci --ignore-scripts --no-audit --no-fund
```

Keep the command's actual failure visible. Do not record a successful check after a failed tool invocation.

## Independent verification

Start the source-bound check run, then execute each standard tool separately and immediately record its actual exit code. CI implements these as separately named steps.

```powershell
node scripts/rust-artifacts.mjs begin-check

python -m unittest discover -s scripts -p test_release_packages.py
node scripts/rust-artifacts.mjs record-check distribution-guards $LASTEXITCODE

./verify.ps1
if (-not $?) { throw 'Legacy verification failed.' }
node scripts/rust-artifacts.mjs record-check legacy 0

node scripts/rust-artifacts.mjs self-test
node scripts/rust-artifacts.mjs record-check artifact-guards $LASTEXITCODE

cargo fmt --all -- --check
node scripts/rust-artifacts.mjs record-check cargo-format $LASTEXITCODE

node $env:HANDYMEETING_NPM_CLI --prefix apps/desktop test
node scripts/rust-artifacts.mjs record-check frontend-tests $LASTEXITCODE

node $env:HANDYMEETING_NPM_CLI --prefix apps/desktop run build
node scripts/rust-artifacts.mjs record-check frontend-build $LASTEXITCODE
```

Frontend assets must exist before Rust compilation because Tauri's production protocol embeds them. Set the native SDK for the current build process, then run the Rust checks:

```powershell
$env:TRANSCRIBE_DIR = (Resolve-Path .build/rust-native-0.4.1).Path
$env:PATH = (Join-Path $env:TRANSCRIBE_DIR 'bin') + ';' + $env:PATH

cargo test --workspace --all-targets --locked
node scripts/rust-artifacts.mjs record-check rust-tests $LASTEXITCODE

cargo clippy --workspace --all-targets --locked -- -D warnings
node scripts/rust-artifacts.mjs record-check rust-clippy $LASTEXITCODE

node scripts/rust-artifacts.mjs finish-check
```

A nonzero reported exit code is retained as a failed event and makes the recording command fail. Finish requires every fixed named check to succeed, the same source fingerprint, reviewed native SDK identity, and matching Cargo/npm license manifests and retained notice hashes. Dependency/license regeneration is a separate explicit action using `scripts/collect-rust-notices.ps1`. Locks and licenses are never silently rewritten by artifact bookkeeping.

`begin-check --skip-legacy` is available for isolated development checks; such a receipt cannot stage a distributable candidate.

## Exact release executable and staging

The minimal build script requires an already finished matching check receipt. It runs only the fixed release build with compiler warnings rejected and restores its process-local native environment:

```powershell
./scripts/build-rust.ps1 -UseExistingVerification
```

The equivalent fixed Cargo build can also be run directly. It targets `x86_64-pc-windows-msvc` under `.build/rust-target`. Native SDK preparation is never hidden inside verification or artifact staging.

Bind that binary before running its isolated model-free checks:

```powershell
$pending = node scripts/rust-artifacts.mjs begin-release-check | ConvertFrom-Json
& ./.build/rust-target/x86_64-pc-windows-msvc/release/HandyMeeting.exe --release-checks $pending.outputDirectory | Out-Host
# Stop if the executable failed.
node scripts/rust-artifacts.mjs stage
./scripts/package-rust-windows.ps1
./scripts/package-rust-installer.ps1
```

Keep the native SDK's `bin` directory in the current process PATH for the direct release check. The release-check mode opens no GUI, microphone, Handy installation or model. Staging requires its fresh successful versioned JSON, the exact unchanged executable SHA-256, and the original checked source fingerprint.

The versioned program destination is `dist-rust-<application-version>`. Its payload contains the checked executable, known native DLLs and original native licenses, Rust/frontend/installer notices, project license, and Handy palette attribution. A manifest hashes every retained file. Existing different bytes are rejected before publishing any new payload. The separate launcher is `Meeting for Handy <application-version> starten.cmd`. Earlier launchers and programs remain intact. A build from later source with the same application version is not permission to overwrite an older candidate or its receipts; use a separate source/build folder until the next version is assigned.

Rust user data lives outside this directory under local `HandyMeeting/RustBeta` application data. No model weights, recordings, settings or API keys are staged.

## Package correspondence

The filesystem-only package script independently recomputes the same source fingerprint, checks the release receipt, executable SHA-256, payload manifest and launcher hash, and copies only those exact files. It never starts the application or any tools. System.IO.Compression creates the ZIP with a SHA-256 sidecar and verification JSON.

The source fingerprint includes all Rust files in the three crates and desktop app, the preserved C# source/project/XAML files and their root verification/SDK/NuGet configuration, frontend source/configuration, Cargo/npm locks, build scripts including the bookkeeping module, bundled assets and Rust/frontend license records. Dependency trees, build output, TypeScript caches, and generated Tauri schemas are excluded consistently. Added Rust source files change the fingerprint. If source changes between checks and staging/package creation, a new check run is required. The prepared SDK is checked against its complete reviewed DLL manifest, including rejection of additional DLLs; staged files are checked again against their payload hashes.

An existing ZIP is preserved. Use a new version or a fresh `-OutputDirectory` instead of overwriting an earlier candidate. Receipts and individual check events remain under `.build`; they are not private meeting data and contain no model weights.

## CI scope and manual evidence

`.github/workflows/rust.yml` runs the independent legacy, frontend, Rust and artifact checks on Windows. Draft PRs and main pushes run these source checks without a distributable package. Ready PRs, explicit manual requests and the reusable release caller additionally build/check the exact release executable and retain ZIP/Setup artifacts. Its disposable CI account installs the actual Setup, checks every installed payload hash, executes the installed program's headless checks, and verifies uninstall preserves an unknown user file. These installation checks are deliberately unavailable in a normal developer account. Third-party Actions are pinned to immutable commits. A prepared workflow is not a successful CI run.

Source-only runs additionally exercise the portable crates without ASR on Linux and both Macs. Full candidates omit those duplicate core jobs because the complete native jobs execute both the no-ASR configuration and full workspace in their existing checkout. Separately, `.github/workflows/rust-portable.yml` builds the complete native desktop application and DEB/DMGs on Linux and both Mac architectures for ready PRs or explicit manual/reusable requests. It does not build packages for draft PR updates or main pushes. See [CI scope](CI.md). `.github/workflows/rust-release.yml` calls both workflows, requires matching source fingerprints and actual credential/installer checks, and assembles the five downloads. See [release instructions](RELEASING.md). These jobs do not capture physical audio or validate a visible desktop session.

Real local text-model references, actual audio devices/disconnects, and a new two-hour Rust memory/recovery run remain separate release gates. Complete portable candidate jobs additionally require the native two-track generated-speech/restart check described in [CI scope](CI.md); its target-host success must be read from the actual run, and it does not replace device or endurance checks. Private local test sessions and raw recording reports stay outside public source export; synthetic regression fixtures and the explicitly labelled screenshot demos are public. The earlier C# two-hour result does not certify this Rust release. Process termination is not a power-loss test. See [TEST_REPORT.md](../TEST_REPORT.md) and [RUST_STORAGE.md](RUST_STORAGE.md) for actual measurements and outstanding checks.

## Cross-platform byte identity

The first Windows CI run detected that Git checkout expanded LF lockfiles to CRLF; its Cargo.lock SHA-256 exactly matched an independently reproduced CRLF expansion. The repository now disables checkout text conversion in .gitattributes. Lock and original-license checks remain byte-exact; the expected hashes were not relaxed. Portable desktop packaging is a separate target-host workflow documented in [RUST_PORTABLE.md](RUST_PORTABLE.md).
