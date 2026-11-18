# Standard recording and source-backed updates

## Can someone record a meeting without setting up a text model?

The German and English automated UI tests start with two available devices and an installed speech recognizer. They start and stop a meeting, remount the application, reopen the saved meeting from history and copy both tracks. The desktop commands use synthetic responses. When only computer audio is available, the UI warns about the missing microphone if the user wanted it. Recording can still start. Reconnecting a microphone respects the user's preference. The start button, readiness label, device selection and capture path retain the behavior on the base branch. The additional microphone preference is used only to display the notice.

The separate native speech test ran at commit `1e3ab40` on Windows 11, Core i7-13700, with the freshly built debug app reporting version `1.1.0-beta.1` and Nemotron 3.5 ASR 0.6B Q8 on CPU. Two synthetic English WAVs provided 31 reference words each. With a forced restart of the recognition worker, the microphone and computer tracks each scored **0 word errors out of 31**. Both PCM hashes, the journal and the reopened transcripts passed the verifier. The accelerated probe took **56.51 seconds** under concurrent test load. This measures the complete probe, including restart, rather than per-word latency. Hardware capture, two-hour operation, novice usability and process/GPU memory were outside this run.

## Can a task change without overwriting the decision or deadline?

Four controlled conversations cover the room choice and the two-device pilot in both German and English. Each introduces a commitment with an owner and deadline, then a separate decision. Later windows contain a negation, an unrelated report and a literal completion by the responsible person. Each window is tested with an explicit model proposal to complete the task and with an empty model change list. Sending a draft invitation or preparing a checklist template keeps the actual task open. The checks require the same task ID, owner and deadline, the unchanged decision, and the exact final quote from the completion window.

The source dialog marks the cited words inside the complete original section. The UI tests exercise source navigation, original audio preview and stale evidence handling. A browser run with synthetic desktop responses covered 60 view states across German and English. It recorded zero page errors and zero horizontal overflow in the measured views and source dialog.

## Model comparison

| Model | Local inference | Quality and latency |
| --- | --- | --- |
| Gemma 4 12B | Memory admission declined before inference. | Unmeasured |
| Gemma 4 E4B | Memory admission declined before inference. | Unmeasured |

Available memory was about 8.5 GiB. The existing admission checks required about 10.7 GiB for 12B and 9.7 GiB for E4B. The controlled tests exercise the application's handling of updates. They cannot rank the two models.

For a future comparison, the opt-in runner uses LM Studio on loopback port 1235 and a fresh synthetic directory per case. For each locally installed model, run both languages (`de`, `en`) and both scenarios (`room`, `pilot`) sequentially. Example from the repository root, with an existing parent directory:

```sh
cargo run --locked -p meeting-notes --example update_quality -- .build/quality-12b-en-room google/gemma-4-12b-qat en room
```

Each case exercises three updates through the local workspace, saves the per-update time, retry count and state, and compares the reopened workspace with the final saved result. The built-in checks cover one expected decision and task per fixture. Inspect the recorded states for extra facts, misleading paraphrases and missed implications before drawing conclusions about model quality.
