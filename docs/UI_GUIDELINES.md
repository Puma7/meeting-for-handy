# Interface conventions

Use the existing Handy theme, fonts, colors and spacing tokens. These conventions
apply to recording, history, live summaries, post-meeting analysis and settings.

- Put the current state and the next useful action first. Keep one primary action
  per task. Use secondary buttons for alternatives and text buttons for setup.
- Keep frequent controls visible. Put explanations, optional automation, template
  editing and maintenance in named native disclosures. They must work by keyboard.
- Keep running maintenance progress/cancellation outside collapsed settings.
- Keep an actionable error, its remedy and its completion acknowledgement in the
  same visible status region. A visible error must not depend on hidden controls
  to resolve it, even when an optional section closes while work is running.
- Keep errors, capture warnings, unconfirmed results and per-run cloud consent
  visible at their decision point. A collapsed help section must never grant consent.
- Use short source names (System audio / Microphone). Give both source cards the
  same heading, selector and meter structure. Signal wording and values each have
  a stable row; missing or quiet signal must not move one bar relative to the other.
- A digital level is neither calibrated loudness nor proof that audio was saved.
  Preserve clipping and unavailable/stale states. Never turn unknown readings into zero.
- Show whole-computer CPU/RAM percentages, with used/total memory in GiB available
  in keyboard-accessible computer details. GPU usage is currently unmeasured; do not invent a percentage.
  Positive transcription backlog remains pending even below the high-load threshold.
- A driver discontinuity is a capture warning. It does not prove an audible gap or
  an uninterrupted recording. Preserve the original warning and direct the user to
  listen to the affected range; missing audio cannot be restored by changing wording.
- Keep model setup separate from starting an analysis. An unavailable local server
  needs a short next action and explicit setup help, without automatically expanding
  installation instructions or starting/downloading services.
- Preserve transcript sorting, section-time qualification and access to evidence,
  original results and saved template instructions. Advanced controls remain available.

Check rendered examples with synthetic data in both themes and at narrow/wide
desktop widths. Unit tests cannot prove equal visual geometry or absence of overflow.
Do not use private meeting contents in screenshots or public review examples.

See [the synthetic interface validation notes](validation/interface-clarity-20261030.md)
for the measured layout checks and their limits.
