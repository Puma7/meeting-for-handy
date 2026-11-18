# Optional meeting workflows

This records the user's complete 28 October 2026 feature input and the public-source comparison. It is a development backlog, not a claim that these features already exist. Recording, exact source evidence and preservation take priority. Additional processing/views start disabled and remain independent of capture.

The notepad and chat ideas, with ratings and proposed waves, are collected in the German [notepad and chat plan](NOTEPAD_CHAT_PLAN.md).

| Requested workflow | Current baseline | Next acceptance boundary |
| --- | --- | --- |
| Import audio/video from disk or USB, including MP3/MP4/WAV | Native legacy session.json import exists; general media import does not | Use an existing decoder with a bounded local work copy, source hash and track selection; never modify the external original. Check malformed files, multiple tracks, storage exhaustion, cancellation and restart. |
| Transcribe retained audio again with another local model | Verified compact audio copies can create a new meeting/model snapshot; general original-audio variants remain work | Keep original transcripts/analyses, show side-by-side variants and require explicit promotion. Bind annotations and derived results to their actual source revision. |
| Export transcript and each result tab, audio export and open audio folder | TXT/MD/JSON transcript exports and verified session-folder opening exist; whole analysis can be copied | Add individual tab export with source IDs and literal quotes, and explicit original/derived audio selection. Exports must preserve original evidence and neutralize active content. |
| Participants, confirmed names, word counts/shares and optional live statistics | Manual section labels and separate experimental local speaker jobs exist; section/track IDs are not stable people | Separate manual naming, post-meeting diarization and live experiments. A mentioned name does not identify the speaker. Preserve unknown/overlap buckets. Label counts as recognized words; real speaking time requires measured speech intervals and an explicit denominator, never nominal section times. |
| Templates and configurable result tabs | Revisioned templates and result tabs exist | Extend the existing library; changing a template must not change saved results. |
| Source-backed mindmap tab | Not implemented | First render validated structured nodes locally. Each node opens a source ID and verbatim quote; relationships must not invent causality. |
| Optional locally generated meeting overview image | Not implemented | Begin with a deterministic local overview card from supported results. A generative image model is a separate optional module with resource/quality checks and no capture ownership. |
| Exact search first; optional natural-language/semantic search later | Local SQLite full-text/prefix search exists | Keep exact results primary. An optional local semantic index must be revisioned/rebuildable; generated answers still need original quotes. A new vector database is not a prerequisite unless measured requirements justify it. |
| Selected-folder outputs, sync-folder integration, webhooks/n8n/Zapier or email | No dedicated outgoing integration workflow | Start with explicit folder export. Explain that independently synchronized folders may upload files. Later add an isolated opt-in text transport with preview, target, protected keys and idempotent retry. No automatic audio, local paths, provider switching or messages. External text transfer requires explicit approval for each operation. |

## Sequence

1. Reliable import and original-audio re-transcription variants with visible resumable jobs.
2. Per-tab/source-preserving exports, audio access, templates, exact search and source bookmarks.
3. Separately enabled speaker overview and mindmap. Stable person identity, speech intervals and live attribution need their own quality criteria.
4. Optional semantic search, generative imagery and outgoing integrations after separate user-facing review and resource/privacy checks.

## Research evidence and limits

These are qualitative examples, not a measured market-demand survey or tests of competitor products. Vendor features establish available workflows; public user requests establish individual needs.

- [Handy discussion 962](https://github.com/cjpais/Handy/discussions/962) requests processing a retained recording with a different model. [TypeWhisper releases](https://github.com/TypeWhisper/typewhisper-mac/releases) document original-preserving comparison of re-transcription variants.
- [Plaud Web file import](https://support.plaud.ai/hc/en-us/articles/63431016171545-File-import-on-Plaud-Web-for-v4-0-x-or-later) documents audio/video import. [Buzz issue 1386](https://github.com/chidiwilliams/buzz/issues/1386) illustrates why multiple audio tracks need explicit handling.
- [Plaud exports](https://support.plaud.ai/hc/en-us/articles/63429911054617-Export-transcripts-notes-and-audio-for-v4-0-x-or-later) cover transcript, notes and audio, but remove citation markers. Keeping literal source evidence in our exports is a deliberate requirement.
- [Fireflies analytics](https://guide.fireflies.ai/articles/2608597716-understand-fireflies-analytics-and-conversation-intelligence) offers conversation statistics. [Plaud speaker labeling](https://support.plaud.ai/hc/en-us/articles/63431016377113-Auto-speaker-labeling-on-Plaud-Web-for-v4-0-x-or-later) distinguishes neutral speakers from confirmed personal identity.
- [Plaud mindmap](https://support.plaud.ai/hc/en-us/articles/57430809095833-Mind-Map), [templates](https://support.plaud.ai/hc/en-us/articles/63431072621081-Summary-templates-on-Plaud-Web-for-v4-0-x-or-later) and [search](https://support.plaud.ai/hc/en-us/articles/63431032160409-Search-on-Plaud-Web-for-v4-0-x-or-later) demonstrate separate optional result/navigation workflows.
- [MacWhisper integrations](https://docs.macwhisper.com/article/53-integrating-macwhisper-with-other-services) and [watch folders](https://docs.macwhisper.com/article/35-automatically-transcribing-files-in-watch-folders) illustrate export automation. [Buzz discussion 1612](https://github.com/chidiwilliams/buzz/discussions/1612) illustrates input/output loops and the danger of deleting original audio.

No new import, identity, statistical accuracy, model-quality or integration capability is claimed by the audit-fix patch.
