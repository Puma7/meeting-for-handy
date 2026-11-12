# Local meeting features for 1.2.0

| Area | Implementation and remaining validation |
| --- | --- |
| Processing status | Separate capture, recognition, and local analysis status with actionable errors. |
| Resource scheduling | Capture and recognition take priority. Local analysis can wait for available system RAM, and slow updates back off under memory pressure. GPU memory scheduling and low-memory device validation remain open. |
| Setup check | An explicit test tone checks audio storage and the transcription worker's processing protocol. The optional local text-model check uses sample text. Word recognition, microphone permissions and hardware require separate tests. |
| Meeting overview | Compact current and historical views preserve access to the complete stored results. |
| Result provenance | The UI and JSON export distinguish an AI draft, a manual status change, and confirmation of a specific result revision. Confirmation requires current original evidence. |
| Original audio | Workspace citations and history search can open the verified recording on the correct track. The preview may join adjacent verified chunks and is capped at 30 seconds. Extended playback across chunks remains open. |
| Local meeting questions | Query a meeting or up to twelve selected meetings with date and VIP filters. The app checks quotations against the original passages and can abstain. Named folders and optional semantic reranking remain open. |
| Quiet and long sessions | Synthetic quiet speech, two-track continuation, and accelerated four-hour and twenty-four-hour index fixtures were exercised. Real-time long-session and device tests remain open. |
| Encrypted session storage | Opt-in profiles store active session files and app-managed archives and rebuild the search index in protected storage. Windows uses EFS on supported NTFS drives, macOS uses an encrypted APFS sparse bundle, and Linux uses `gocryptfs` with FUSE. A portable single-meeting backup can be opened in an isolated profile with its passphrase or separate recovery key. |
| Calendar preparation | An explicitly chosen local calendar file can suggest a title and template, and offers the agenda for copying. Automatic meeting detection and calendar account integration remain open. |
| Diagnostics | An opt-in local preview contains allowlisted technical fields and can be copied manually. |
| Delivery | Source and synthetic checks cover the development build. Tests of the final installer, update, and signing remain separate release gates. |

The model reference used Gemma 4 12B QAT through LM Studio on Windows with synthetic meetings. It exercised a cross-meeting comparison, a selected-meeting scope and an unsupported question that required abstention. The test-tone setup check exercised the Nemotron Streaming worker protocol through Handy 0.10.0 on CPU. Physical capture and long-session reliability require separate tests.

The Windows development candidate before protected profiles passed 640 Rust tests and 500 UI tests, a production frontend build, Rust formatting, Clippy with warnings treated as errors, `verify.ps1`, and eight native executable checks in an isolated profile. The regular Rust suite excluded four explicitly invoked diagnostics and fixtures. The earlier local model reference applies to the history question path. No fresh model run was recorded for the backup and UI follow-up.

Encrypted-copy tests use synthetic data. They verify both audio tracks, transcript state, analysis and export bytes, and metadata after recovery into a separate profile. An independently encoded version-1 fixture verifies backward compatibility. The tests reject wrong credentials, altered key slots and frames, truncation, duplicate frames and trailing bytes. They also cover cancellation and cleanup of a failed restoration while preserving an unrelated destination file. The [storage documentation](../RUST_STORAGE.md#encrypted-session-copies) describes the readable restore output and the limits after a forced interruption.

The protected-profile migration tests use synthetic copies and a simulated protection provider. They cover byte-for-byte verification of the copied profile, rebuilding the search index, a missing marker, failure to confirm protection, a held session lease, and a source change during copying. The Windows pull-request workflow includes a native EFS test for new audio and SQLite files on its temporary runner. [Protected profiles and recovery](../RUST_STORAGE.md#encrypted-session-storage)

The native macOS and Linux test uses a generated two-track session and a disposable encrypted profile. It checks source preservation, wrong-passphrase and duplicate-open rejection, fresh SQLite database, WAL and shared-memory files, new audio in the mounted profile, and recovery of the session and searchable text from a detached ciphertext copy. Physical capture and long-session validation remain open.

The protected-profile update passed 647 Rust tests across the workspace and the final desktop rerun, 506 UI tests, the production frontend build, `verify.ps1`, and 57 release-script tests. It also covers accent-insensitive history questions, duplicate legacy history entries, and ambiguous local times in calendar imports.
