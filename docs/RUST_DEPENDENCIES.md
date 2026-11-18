# Dependency and license evidence for the Rust beta

Status: 25 October 2026, `1.0.0-beta.1`, Windows x64. This records supplied license texts and dependency checks; it is not a claim of general correctness, complete security or portable-device reliability.

## Scope and reproduction

Run from the repository root after installing the locked Rust dependencies and the frontend's `npm ci` dependencies:

```powershell
./scripts/collect-rust-notices.ps1
# Reproduce from already collected remote texts without network access:
./scripts/collect-rust-notices.ps1 -Offline
```

The script reads `cargo metadata --locked --filter-platform x86_64-pc-windows-msvc`. It selects package IDs from the **active resolve nodes**, rather than every package listed in cross-platform `Cargo.lock` metadata. The result contains **313 third-party Rust packages** and excludes the four workspace crates. It includes active build dependencies as well as application dependencies; 313 is not a count of libraries necessarily linked into the running app. Linux GLib/GTK and macOS Objective-C dependencies are absent from this Windows manifest.

Evidence for this collection:

- Cargo lock SHA-256: `bf189970c52cdf7a332ee9424f850112309bfd1bc4b2113664c086c4ea66e212`.
- [Windows Rust manifest](../licenses/rust/x86_64-pc-windows-msvc/manifest.json): package name, version, declared SPDX expression, repository/source archive, original license filename and SHA-256. All **558 referenced text files** were checked against their recorded SHA-256 values after collection.
- [Frontend manifest](../licenses/frontend/manifest.json): **seven production npm packages**, plus the separately attributed Handy palette; nine referenced license/notice texts. Development tools are excluded from this shipped-frontend list.
- Frontend lock SHA-256: `edee24f4b27f3df508ebabf577b35465eff65d583f7b9e1a36706938141bf769`.

All copied license/notice files use `.txt` names. Their contents are preserved, including the original copyright and disclaimer text. The manifests contain no absolute local paths or credentials. Historical slash-separated Cargo license expressions, such as `MIT/Apache-2.0`, are retained as declared metadata rather than silently rewritten.

Existing license files that differ from a newly collected copy cause the script to stop and preserve the existing file for review. No dependency versions are updated by collection. Remote fallback texts are pinned by source revision and by reviewed SHA-256. An offline run verifies the same hashes.

## Main components and original texts

| Component | Version | Declared license | Evidence |
| --- | --- | --- | --- |
| transcribe-cpp | 0.4.1 | MIT | Supplied crate `LICENSE`, copied verbatim |
| transcribe-cpp-sys | 0.4.1 | MIT | Supplied crate `LICENSE`, copied verbatim |
| wasapi | 0.26.0 | MIT | Supplied `LICENSE.txt`, copied verbatim |
| hound | 3.5.1 | Apache-2.0 | Supplied lowercase `license`, copied verbatim |
| tauri | 2.11.5 | Apache-2.0 OR MIT | Both supplied license files retained |
| @tauri-apps/api | 2.11.0 | Apache-2.0 OR MIT | Both supplied npm license files retained |
| React / React DOM | 18.3.1 | MIT | Supplied npm `LICENSE` files retained |
| Lucide React | 0.542.0 | ISC | Supplied npm `LICENSE` retained |
| Handy shared palette | commit 73c095857bc4eadff097f64767c40ad129cc372d | MIT | Original CJ Pais notice and license in [HANDY_STYLE_NOTICE](../apps/desktop/HANDY_STYLE_NOTICE.md), also copied into the frontend notices |

The full graph also includes BSD, Unicode, Boost, ISC, Zlib, CDLA and MPL declarations, including packages with combined license expressions. The project's MIT license does not replace their licenses. The manifest preserves every original expression and corresponding supplied texts rather than assuming the entire dependency graph is MIT.

Eleven published crates omit standalone license files. Their actual upstream license texts were retrieved from the revisions recorded in those crates' `.cargo_vcs_info.json`: alloc-stdlib, clipboard-win, five `unic-*` crates and three `webview2-com` crates. Their exact source URLs and hashes are recorded in the manifest. For `selectors`, its supplied source-header notice explicitly references Mozilla's MPL 2.0. That notice and the [canonical Mozilla text](https://www.mozilla.org/media/MPL/2.0/index.txt) were retained; no legal text was generated from an SPDX name.

MPL-declared source packages in this Windows build graph are available through the exact source-archive links in the manifest: `cssparser 0.36.0`, `cssparser-macros 0.6.1`, `dtoa-short 0.3.5`, `option-ext 0.2.0` and `selectors 0.36.1`. The collection does not modify their source files.

## Packaging boundaries

The beta package must include the applicable target's Rust notices, the frontend notices and [ADDITIONAL_NOTICES.txt](../licenses/rust/x86_64-pc-windows-msvc/ADDITIONAL_NOTICES.txt). Keep the reused Handy MIT notice with the frontend attribution.

The existing SDK at `.build/rust-native-0.4.1` and native transcribe.cpp runtime are unchanged by this collection. Preserve the runtime's existing `native/transcribe-0.4.1/licenses` tree in the distributed native directory, including its upstream transcribe.cpp, ggml and other component notices. These native licenses are additional to the Rust-wrapper licenses. Model weights are not bundled and have separate licenses.

Old .NET/Avalonia notices belong to the preserved prototypes. This collection is an additional beta notice set and does not rewrite the historical [THIRD_PARTY_NOTICES.txt](../THIRD_PARTY_NOTICES.txt). A Windows collection is not a completed Mac/Linux license or packaging review; regenerate and review the corresponding target graph before distributing those targets.

## Dependency security checks and unresolved warnings

The initially inherited dependency lock reported 12 known vulnerabilities. Compatible dependency updates were applied to the beta's own lock rather than treating upstream parity as sufficient security evidence.

An independent read-only `cargo-audit` run against the locally available RustSec database reported **zero vulnerability entries**, **six unmaintained-package warnings**, and **one unsoundness warning**. Its local raw report is `.build/rust-license-audit.json`; the database reported 1,352 advisories without a Git commit/time field. This is a check against that available database, not proof that every current vulnerability is known or that the code is safe.

| Category | Package | Version | Advisory |
| --- | --- | --- | --- |
| Unmaintained | proc-macro-error | 1.0.4 | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) |
| Unmaintained | unic-char-property | 0.9.0 | [RUSTSEC-2025-0081](https://rustsec.org/advisories/RUSTSEC-2025-0081.html) |
| Unmaintained | unic-char-range | 0.9.0 | [RUSTSEC-2025-0075](https://rustsec.org/advisories/RUSTSEC-2025-0075.html) |
| Unmaintained | unic-common | 0.9.0 | [RUSTSEC-2025-0080](https://rustsec.org/advisories/RUSTSEC-2025-0080.html) |
| Unmaintained | unic-ucd-ident | 0.9.0 | [RUSTSEC-2025-0100](https://rustsec.org/advisories/RUSTSEC-2025-0100.html) |
| Unmaintained | unic-ucd-version | 0.9.0 | [RUSTSEC-2025-0098](https://rustsec.org/advisories/RUSTSEC-2025-0098.html) |
| Unsoundness | glib | 0.18.5 | [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) |

The table above records the earlier registry lock. The current Linux GTK3/WebKit stack requires the 0.18 series. Its `glib` now resolves to the immutable [0.18 backport commit](https://github.com/gtk-rs/gtk-rs-core/commit/ea720152f28e293ef4362ee844ee5cc499f32d2a). The commit applies the [upstream iterator fix](https://github.com/gtk-rs/gtk-rs-core/pull/1343) to the 0.18.5 source. The backport was not released on crates.io. The Rust crate still identifies itself as 0.18.5, so inspect its resolved source as well as its version. The macros and three FFI crates use the same revision to preserve type identity. The four platform license manifests record the resulting lock. Windows and macOS do not resolve these five packages. Remove the Git overrides together when the supported Linux stack accepts a patched registry release.

The frontend's full npm audit initially found development-tool advisories. Exact Vite `6.4.5` and Vitest `4.1.12` updates removed those reports while preserving the existing React/Tauri API compatibility. The final **full audit, including development dependencies**, reported zero known npm advisories; local evidence is `.build/frontend-audit-patched.json`. The updated frontend build and twenty-one synthetic frontend regressions passed, including completed-history preservation during bounded polling, an asynchronous ending state, guarded focus-refresh of Handy models and explicit reselection when a chosen model disappears. Audit results do not replace device, recovery, privacy-boundary or long-session checks.

## Beta.2 Unix notice collection before credential persistence

The collections below were generated on 25 October 2026 from unchanged Cargo lock SHA-256 `d7b93ad3cb55700c3b0466adc0350fff5071c13e86155b1ed38a068c41f8ab5b`. These are target-filtered dependency/license records, produced on Windows using locked Cargo metadata. They do not establish that a Mac or Linux binary was built, signed, run or validated on hardware. The earlier beta.1 collection counts and lock hash above describe the earlier evidence, not this current lock.

| Target | Active third-party packages | Referenced license/notice texts | Manifest SHA-256 |
| --- | ---: | ---: | --- |
| [Linux x64](../licenses/rust/x86_64-unknown-linux-gnu/manifest.json) | 338 | 593 | `fe9d4897cf1d71bea68c4aba89bf41a21d9e27e1eb0a931c327a2846c2a8d1fb` |
| [Mac Apple Silicon](../licenses/rust/aarch64-apple-darwin/manifest.json) | 298 | 540 | `f511a1b2f0451d5eb63b22d946ef4ba3c13a1f3da3d32d022d00a36c1c55f54a` |
| [Mac Intel](../licenses/rust/x86_64-apple-darwin/manifest.json) | 299 | 542 | `f42369cf1c49407dce5532af4d4622466932c42294640d0facc39626e019d576` |

Every referenced file was rehashed against its manifest after collection. This includes active build dependencies, not only linked runtime libraries. The manifests retain declared SPDX alternatives; the original texts and notices, not the SPDX labels, are distributed. Reproduction:

```powershell
./scripts/collect-rust-notices.ps1 -Target x86_64-unknown-linux-gnu
./scripts/collect-rust-notices.ps1 -Target aarch64-apple-darwin
./scripts/collect-rust-notices.ps1 -Target x86_64-apple-darwin
# Add -Offline to use and verify already collected remote texts.
```

Additional exact upstream sources were reviewed as follows:

- `libappindicator-sys 0.9.0` records VCS `eafd1e3682a1247f595410266091e9684021cb6f`, subdirectory `sys`. Its published archive omits standalone licenses. The [same upstream revision](https://github.com/tauri-apps/libappindicator-rs/tree/eafd1e3682a1247f595410266091e9684021cb6f/sys) contains the original root [MIT](https://raw.githubusercontent.com/tauri-apps/libappindicator-rs/eafd1e3682a1247f595410266091e9684021cb6f/LICENSE-MIT) and [Apache](https://raw.githubusercontent.com/tauri-apps/libappindicator-rs/eafd1e3682a1247f595410266091e9684021cb6f/LICENSE-APACHE) texts, which are preserved unchanged.
- `unic-ucd-ident 0.9.0` records VCS `8a6ce83063d90b91ae2ce59eddb803edd393fca9`, unlike its sibling `unic-*` crates at `5878605364af97a3358368a6eaef02104af2e016`. Its [MIT](https://raw.githubusercontent.com/open-i18n/rust-unic/8a6ce83063d90b91ae2ce59eddb803edd393fca9/LICENSE-MIT), [Apache](https://raw.githubusercontent.com/open-i18n/rust-unic/8a6ce83063d90b91ae2ce59eddb803edd393fca9/LICENSE-APACHE), and [copyright](https://raw.githubusercontent.com/open-i18n/rust-unic/8a6ce83063d90b91ae2ce59eddb803edd393fca9/COPYRIGHT.md) files were checked at its actual revision and are byte-identical to the previously collected texts. The source URLs now identify that actual revision.
- The Mac dependency set omits standalone license texts from the published `block2`, `dispatch2`, `objc2`, `objc2-encode`, `objc2-exception-helper` and six `objc2-*` framework archives. Each manifest includes the complete upstream `LICENSE.md` from its own recorded VCS revision: [block2](https://raw.githubusercontent.com/madsmtm/objc2/b4167b582b2f75f9a1be75495c41b765344fd03c/LICENSE.md), [dispatch2](https://raw.githubusercontent.com/madsmtm/objc2/e68410e22a2d29d5f37a0b4c1b7e8e194808eade/LICENSE.md), [objc2](https://raw.githubusercontent.com/madsmtm/objc2/8852b424193ca41602281b3d7540d7c8ed51e49a/LICENSE.md), [encode/exception helper](https://raw.githubusercontent.com/madsmtm/objc2/8d214f5477365ffcbcbb7de058c86ed9a518efb7/LICENSE.md), and [framework crates](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md). Those notices expressly allow MIT for every affected crate, either alone or as an alternative. This collection uses that MIT option and preserves the full current notice, including its Apple SDK discussion.
- Upstream's [licensing clarification commit](https://github.com/madsmtm/objc2/commit/cfb199226661ec6e4939fcd7cd7531b3c5453076) replaced the prior `LICENSE.txt` with `LICENSE.md`. The unmodified [historical MIT original](https://raw.githubusercontent.com/madsmtm/objc2/757cd841d0293341dbefd9b99d7548dec244ffde/LICENSE.txt), including `Copyright (c) Steven Sheldon`, is additionally retained with SHA-256 `e353f37b12aefbb9f9b29490e837cfee05d9bda70804b3562839a3285c1df1e5`. Every such manifest entry explicitly identifies this as a historical notice; it is not presented as a file supplied by the current crate archive. No copyright holder or legal text was invented.

All new remote sources are fixed by source revision and reviewed SHA-256 in the collector. The five current objc2 `LICENSE.md` copies are identical, SHA-256 `7f976f7e9cb2d87df7230606feb932c3f21ac0e664045a775b600046ff850c54`. No Apple SDK files, frameworks, model weights or user recordings are included in this collection. The upstream Apple SDK/Xcode boundary remains visible; collecting these notices is not a blanket legal assurance or a completed Mac packaging/signing review.

## Native credential dependency expansion

The explicitly approved native key-store addition pins `keyring = 4.2.0` with only `v1`, reuses existing `zeroize = 1.8.2` and `fs2 = 0.4.3`, and adds 63 package versions to the cross-platform lock. Every package name/version from the preceding lock remains present; existing pins were not upgraded. New package metadata declares no minimum Rust version above the pinned compiler 1.91.1. The earlier Unix collection table remains historical evidence for its recorded lock; this table records the expanded graph.

Current Cargo lock SHA-256: `e6a7aa95fc3e550f62a0154a882e2eec1afab4cc86fa1ec131009c7336114fbb`.

| Target | Active third-party packages | Referenced original texts | Manifest SHA-256 |
| --- | ---: | ---: | --- |
| [Windows x64](../licenses/rust/x86_64-pc-windows-msvc/manifest.json) | 316 | 563 | `f4cff4ac219a405f4d2a72f866482f3be8ab408b9c9f32ca9b332d73c955d3ed` |
| [Linux x64](../licenses/rust/x86_64-unknown-linux-gnu/manifest.json) | 399 | 707 | `246bbb4c882ec55b2efbf2c676034abb40f6d3a2ba7ef53518b18955621cb97e` |
| [Mac Apple Silicon](../licenses/rust/aarch64-apple-darwin/manifest.json) | 304 | 551 | `9545f690e6aa3a715f524a26869e6b7db831bb10af6c8e260c8a3fc477da49bd` |
| [Mac Intel](../licenses/rust/x86_64-apple-darwin/manifest.json) | 305 | 553 | `965c00309008bad80efb3b0ad5a0b00b3672bb2fc5f7c35e4d8fb0c96edbe1e4` |

All 2,374 referenced files were rehashed after regeneration. Newly active keyring, native-store, Secret Service, asynchronous-runtime and cryptographic dependencies retain their supplied license texts; no new remote fallback was needed. The credential architecture, native Windows evidence and pending Mac/Linux execution are described in [RUST_CREDENTIALS.md](RUST_CREDENTIALS.md). Library versions and licenses do not establish that a native keychain test or target build has actually run.
