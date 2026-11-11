# Rust capture and storage migration

`meeting-core` is the first migrated capture/storage implementation. It does not call a speech recognizer. Audio writers and recognition workers have independent lifetimes. Existing C# v1 sessions are inspected through `read_legacy` without creating lock files, repairing WAV headers, replacing manifests or exporting files. They are not automatically migrated or removed.

## New sessions

New Rust sessions use schema version 2 and camelCase JSON. Each session has a UUID directory, a manifest, an exclusive writer lease, an append-only audio journal, an append-only transcript journal, and separate 16-kHz mono 16-bit PCM WAV tracks. Hound 3.5.1 handles WAV serialization. Samples are written in 30-second files with 64-KiB buffering. Routine checkpoints update the WAV header, force the file through `sync_all`, then append and sync an audio checkpoint before exposing the confirmed sample count.

Each audio checkpoint retains a SHA-256 digest of its confirmed PCM prefix. Recovery verifies that prefix and replays journals one bounded line at a time. The newest durable manifest or journal watermark is retained: an older journal entry cannot downgrade a newer confirmed manifest after a damaged tail. Bytes beyond confirmed watermarks are never promoted. Unconfirmed bytes and unfinished journal lines remain on disk. A corrupt confirmed audio prefix is reported instead of silently repaired. Transcript entries reference a known audio part and cannot claim progress beyond confirmed audio. Per-part transcript sequences protect newer manifest text against older replay entries. Prior entries remain when the model or text changes.

`SessionStore::open` opens one session directory, not the history root. It acquires an exclusive writer lease and reconstructs state without rewriting recovered originals. `list_sessions` provides a lightweight manifest list; open the selected Rust session to see journal-derived current state. A second concurrent writer is rejected. Original legacy sessions remain read-only; an old streaming journal cannot replace a newer completed batch transcript or mix another model's text into the selected result.

During ASR restart, a fresh native stream reconstructs its context from the start of the short audio window. Earlier hypotheses are not persisted over a later saved checkpoint. At the storage boundary, same-model processed samples cannot move backward and a saved committed prefix must remain a prefix of both new full and committed text. A divergent hypothesis stops recognition with a visible warning while preserving the previous confirmed text and independent audio capture. Live previews clone at most 16 recent parts; worker polling clones only the next pending part, including older work outside that preview.

## Capture

The Windows backend uses the existing MIT-licensed `wasapi` 0.26.0 crate. System audio uses the selected render endpoint in loopback capture mode; the optional microphone has its own capture thread and durable writer. OS audio conversion provides 16-kHz mono PCM. No ASR inference runs on those threads. Buffers and accepted packets are bounded. An oversized driver packet or write error stops the affected capture visibly and retains saved audio. A microphone startup failure does not stop a healthy system-audio track.

Digital silence on the system track is padded against a monotonic clock. Track starts and missing intervals have approximate recording timing; these are not exact word timestamps. Device disconnection/discontinuity is reported; automatic device substitution is not implemented. Current Rust implements pause/microphone mute with privacy epochs and Linux/macOS capture adapters. Implementation and automated checks do not establish physical-device or endurance readiness; the retained C# portable preview has separate behavior and evidence.

Capture diagnostics do not own the recording lifetime. A warning that cannot be saved remains visible in the opened session, while writable PCM continues. Such a warning is not crash-durable until storage recovers. Stopping attempts every owned writer join and final status save before returning collected diagnostic failures. Real PCM-write errors still stop the affected track and retain confirmed data.

## Encrypted session storage

### Create and open a protected profile

In **Settings → Data and maintenance → Encrypt session storage**, choose **Create encrypted profile**, then select a local parent folder. Finish active operations and close older app versions or other tools that can write into the source. Start the migration from the current window that has the source profile open.

The app prepares protected storage before writing session content. It copies native session files, app-managed archives and stored records such as annotations and retention history. It verifies each copied file's size and SHA-256 digest, then rechecks the source inventory before publishing the protected profile. The source files stay in the original profile. The copy creates its own session and operation leases. The app rebuilds the derived search index in the protected profile, where SQLite keeps the database, write-ahead log and shared-memory files. SQLite keeps temporary tables in memory.

Choose **Open encrypted profile** to launch the verified copy in a separate window. The original window stays open. Later changes apply to the profile where they are made. Use the same action and the folder picker to reopen a previously created profile. A missing marker or failed protection check stops the protected launch before it opens the session store. Browser-only drafts and preferences from the original window remain available in the original profile. Imported legacy originals and exports already saved outside the native profile stay in their original locations.

### Protected Windows profile

Select a local NTFS drive that supports Windows Encrypting File System (EFS). The app enables EFS on a new directory and stores new browser-only drafts and preferences in a dedicated WebView directory within the profile. For a direct launch, pass the absolute path of the verified profile, for example `HandyMeeting.exe --protected-profile "C:\MeetingStorage\HandyMeeting-Protected-example"`. [Windows file encryption](https://learn.microsoft.com/en-us/windows/win32/fileio/file-encryption)

EFS protects file contents at rest through Windows. File and directory names remain visible to users who can list the folder. Access on a different Windows installation depends on the EFS certificate and its private key or an applicable Windows recovery agent. Portable `.hmvault` backups use their own recovery credentials. [EFS files and directories](https://learn.microsoft.com/en-us/windows/win32/fileio/handling-encrypted-files-and-directories) · [Windows EFS certificate backup](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cipher)

### Protected macOS and Linux profiles

Use your regular desktop account. macOS uses the built-in disk image tools to create an AES-256-encrypted APFS sparse bundle. Its storage use grows as the image fills. Linux uses the system installation of `gocryptfs` and needs working FUSE, including `/dev/fuse` and the system unmount helper. [Apple disk images](https://support.apple.com/guide/disk-utility/create-a-disk-image-dskutl11888/mac) · [gocryptfs documentation](https://nuetzlich.net/gocryptfs/)

When creating a profile, choose a passphrase of at least 12 characters. Keep it separate from the profile. Enter that passphrase in the app each time you choose **Open encrypted profile**. The app checks the vault and its mounted storage before opening the session data. It holds a lock so a second app process cannot open the same vault, and unmounts the volume on normal exit. If an interruption leaves the volume mounted, close programs using it and eject or unmount it normally before reopening. Files are readable through the mounted profile while it is unlocked.

To make a backup of the full profile, close its protected window and wait for the volume to unmount. Copy the complete outer profile folder created by the app. Keep the sparse bundle or the `gocryptfs` files together with the profile marker. To recover, use **Open encrypted profile** from the original window, select the copied folder and enter the same passphrase on a compatible system. The independent recovery key described below belongs to the single-meeting backup format.

Linux stores new browser-only drafts and preferences in the protected WebView directory. On macOS the protected window uses a nonpersistent WebView, so its browser-only drafts and preferences are temporary. Saved native meeting data and shared settings retain their usual storage behavior.

### Encrypted session copies

The encrypted copy action reads one inactive native session with its local originals under the existing storage and publication leases. The portable `.hmvault` contains the session files, its app-managed exports and the associated annotation files and revisions. It excludes global settings, credentials, model files, other meetings, the derived search index and separate archives. Creating a copy preserves the original and requires a new file outside the active profile. Erasing an active meeting does not manage these external copies.

New backups use version 2. Each backup has a random content key and a random 32-byte salt. The passphrase wraps the content key with PBKDF2-HMAC-SHA-256 at 600,000 iterations. A separate randomly generated 32-byte recovery key wraps it independently using HKDF-SHA-256. The application shows that key when the backup succeeds, and the user stores it separately. Version 1 files still open with their original passphrase.

AES-256-GCM authenticates sequential 64 KiB frames with a monotonic nonce and binds the full header and frame length to each frame. The encrypted inventory includes paths and SHA-256 digests. An authenticated terminal frame and physical end-of-file are required. Before publishing, the application verifies the whole temporary file through the passphrase and independently authenticates the first frame through the recovery key. [Pinned encryption implementation](https://docs.rs/ring/0.17.14/ring/aead/) · [Passphrase derivation parameters](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html).

Recovery authenticates the complete backup before writing plaintext. It writes into an owned staging directory in the selected private destination, verifies the bytes on disk, reopens the native session and publishes a new independent profile. For an isolated application start, set `HANDYMEETING_DATA_DIR` to the returned `profile` directory. The search index can be rebuilt from recovered transcripts. Recovery intentionally produces readable files. A forced process stop or a filesystem error during cleanup can leave a directory named `.handy-recovery-*` with partially restored plaintext in the chosen target. Such a directory is never published as a recovered profile and should be inspected and removed by its owner.

## Evidence

On Windows, `cargo test -p meeting-core` passed fourteen targeted regression tests: forced termination of an owned synthetic child with two distinct audio streams and both transcript checkpoints; unfinished journal tails; preservation of unconfirmed audio without promoting it; confirmed-audio corruption; exclusive writer lease; unchanged legacy reads and rejected traversal; model-change journal-prefix retention; oversized-packet rejection; reopening a normally completed session after multiple checkpoints; newer durable manifest protection against damaged journal tails; same-model progress/prefix continuity; preservation of a twenty-second nonempty checkpoint after a further process kill during rejected earlier replay; legacy final batch results taking precedence over old/other-model streaming checkpoints; and a two-hour metadata fixture whose live preview stays bounded while old pending work remains selectable. The ignored test entry is the dedicated child process invoked by both hard-kill regressions, not a skipped recovery check. `cargo clippy -p meeting-core --all-targets --locked -- -D warnings` passed. These regressions exposed and fixed Windows backup synchronization permissions, old watermark comparisons, text rewind during recognition restart, and stale legacy journal precedence.

These tests are synthetic and short. They do not open real audio devices, establish two-hour stability, or simulate power loss. The existing two-hour C# result remains evidence for its original version only. Real device, disconnect, full-disk and Rust long-session tests are required before a release candidate.

The example `cargo run -p meeting-core --example synthetic_capture -- OUTPUT_ROOT 2` generates two accelerated synthetic tone tracks and explicit synthetic text checkpoints. It opens no microphone and is not a transcription-quality test.

Upstream audio references: [WASAPI crate](https://docs.rs/wasapi/0.26.0/wasapi/), [recording/loopback example](https://github.com/HEnquist/wasapi-rs/blob/master/examples/record.rs), [Hound](https://docs.rs/hound/3.5.1/hound/). Hound is Apache-2.0, WASAPI is MIT, and fs2 is MIT/Apache-2.0; other dependency licenses remain part of the repository-wide distribution audit.
