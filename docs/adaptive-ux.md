# Adaptive UX Blueprint

Tecla's adaptive layer should feel like a keyboard getting easier to use, not
like firmware second-guessing the user. The core UX rule is:

> Observe quietly, adapt conservatively, explain clearly, and always let the
> user win.

This document turns the adaptive ideas in `ROADMAP.md` into user-facing product
rules and implementation contracts.

## Global Principles

1. **User intent beats model confidence.** If the user corrects Tecla, Tecla
   backs off immediately for that word, chord, key, or timing pattern.
2. **Adaptation starts as suggestions.** New adaptive behavior runs in shadow
   mode first: it records what it would have done, but does not change output
   until it has enough evidence.
3. **No surprise mode switches.** Tecla can tune thresholds silently, but it
   must not enable major behavior classes such as autocorrect, sticky keys, or
   dictionary mutation without an explicit profile setting.
4. **Recent overrides matter most.** If a user rejects a correction or timing
   decision, suppress the same decision for a short session-local window.
5. **Everything is reversible.** Runtime actions must exist for undo last
   correction, disable adaptation, clear learned entry, reset typing profile,
   and export diagnostics.
6. **Telemetry is local and inspectable.** The keyboard can stream telemetry,
   but the useful state also needs on-device counters and a human-readable
   host view.
7. **Accessibility is not a label.** The system adapts to observed timing and
   correction behavior. It should not infer disability, medication cycles, or
   health state.

## UX Surfaces

### On-Device State

The firmware should maintain three kinds of state:

- **Session state:** short-lived suppression lists, recent corrections, recent
  steno misses, recent timing decisions. Cleared on reboot or profile reset.
- **Adaptive profile:** stable timing baselines, per-key confidence, per-feature
  thresholds, user-approved dictionary entries.
- **Diagnostics:** counters and ring buffers exported through telemetry or a
  doctor console.

### User Controls

Minimum runtime actions:

- `AdaptiveToggle`
- `AdaptiveResetSession`
- `AdaptiveResetProfile`
- `AutocorrectUndoLast`
- `AutocorrectRejectLast`
- `AutocorrectLearnLast`
- `DictionaryAddLastWord`
- `DictionaryRemoveLastWord`
- `StenoRawProtocolToggle`
- `TelemetryToggle`

Host-side commands should mirror those actions through a `tecla doctor` or
`tecla console` surface.

## Hostless Autocorrect

Autocorrect is the easiest feature to make hostile. It needs a state machine,
not just edit distance.

### Desired Behavior

1. User types `teh `.
2. Tecla emits backspaces and `the `.
3. If the user immediately backspaces/retypes `teh`, Tecla must not correct it
   again in the same context.
4. If the user repeats that rejection, Tecla should treat `teh` as intentional
   for this session and offer or record a user-dictionary entry depending on
   profile policy.

### State Machine

Each correction produces a `CorrectionRecord`:

- original word
- replacement word
- cursor-visible span length
- timestamp
- context hash: previous word, next boundary, active layer/profile
- confidence
- source: dictionary, transposition, proximity, suffix
- undoable emitted HID sequence

Outcomes:

- **Accepted:** user continues typing without undoing/backspacing into the
  corrected span.
- **Rejected:** user uses backspace/delete/cursor movement within a short
  window after the correction, or re-enters the original word.
- **Ambiguous:** user edits nearby after the timeout; do not learn either way.

Suppression:

- Reject once: suppress exact original -> replacement in the same context for
  the session.
- Reject twice: suppress exact original globally for the session.
- Reject three times across sessions: suggest or add original to user dictionary
  if the profile allows learning.
- Accept repeatedly: raise confidence, but never make an edit irreversible.

### Correction Policy

Autocorrect must only fire when all are true:

- word is complete at a boundary
- replacement confidence is above profile threshold
- word is not in session suppression
- word is not in user dictionary
- replacement is not a high-risk class

High-risk classes require stricter thresholds:

- short words, especially 3-4 letters
- names and capitalized words
- code-like tokens, paths, emails, URLs
- repeated characters
- words after explicit undo/reject
- steno output text

### User-Friendly Learning

Learning should be tiered:

- **Session accept list:** do not correct this token again until reboot.
- **Pending dictionary candidate:** observed rejections over multiple sessions.
- **User dictionary entry:** explicit action or profile-enabled auto-add after
  repeated evidence.

Default: auto-add off, candidate tracking on.

## Adaptive Tap-Hold

Tap-hold adaptation should reduce misfires without making the keyboard feel
inconsistent.

### Signals

- hold time EMA/variance per key
- backspace shortly after a mod-tap decision
- repeated same-key correction pattern
- inter-key timing and rollover ratio
- layer/action context

### Policy

- Start in shadow mode per key.
- Adapt only after minimum observations.
- Change tapping term in small steps.
- Floor at authored value unless user enables aggressive adaptation.
- If user toggles or manually edits a key's behavior, freeze that key.

### UX Rules

- If adaptation would change a decision, record it before applying.
- If the user immediately undoes/backspaces after a decision, mark that decision
  as likely wrong.
- Repeated likely-wrong holds should lengthen tapping term for that key.
- Repeated likely-wrong taps should shorten tapping term for that key.
- Never adapt all keys because one key is noisy.

## Adaptive Combos

Combos should tolerate the user's real finger spread without swallowing fast
rolling text.

### Signals

- co-press spread for key pairs
- combo hit/miss attempts
- near-miss: same combo keys pressed just outside combo term
- false positive: combo output followed by undo/backspace

### Policy

- Adapt combo windows per combo, not globally.
- Use authored combo term as the baseline.
- Widen only for observed near-misses.
- Tighten only after stable, clean activations.
- Suppress adaptation for combos the user manually overrides.

## Adaptive Steno

Steno adaptation should make rolled chords work without delaying clean chords.

### Signals

- chord press spread
- all-release timing
- dictionary hit/miss
- undo/retry after a stroke
- inter-chord gap
- repeated miss that becomes hit after adding one late key

### Policy

- Per-user settle time starts at zero or a conservative default.
- Increase settle time only when late-key evidence is strong.
- Decrease settle time when clean chords dominate.
- Keep settle time bounded so expert writers are not slowed down.
- Treat steno dictionary misses as useful diagnostics, not failures.

### Steno Doctor View

For each stroke:

```text
keys:    S- T- P- H-
outline: STPH
timing:  spread 18ms, release 63ms, gap 112ms
lookup:  hit "something"
output:  "something "
notes:   clean
```

Miss view:

```text
outline: SKWR
lookup:  miss
nearby:  SKWR-F -> cap next word, SKWR-R -> lower next char
action:  no output
```

## Accessibility Layer

Accessibility features should be presented as input comfort and reliability,
not as labels.

### Slow Keys

- Good when accidental brushes are frequent.
- Bad when it hides intentional fast taps.
- Adaptation should suggest, not auto-enable by default.
- If enabled, threshold should adapt to the user's held-time distribution.

### Bounce Keys

- Good when repeated unintended presses happen.
- Dangerous if it blocks intentional double-taps.
- Auto-enable only in explicit adaptive-comfort profile.
- Prefer per-key bounce windows over a global window.

### Sticky Keys

- Should have clear feedback through telemetry/display/LED when active.
- Must be easy to cancel.
- Should not be silently enabled.

## Telemetry UX

Raw telemetry is for firmware. User-facing telemetry should answer:

- What did the keyboard see?
- What did it decide?
- What did it output?
- What would adaptation change?
- How confident is it?
- How do I undo or freeze that behavior?

### Views

1. **Live Input View**
   - matrix events
   - resolved actions
   - HID reports
   - dropped/backpressured reports

2. **Adaptation View**
   - current per-key tapping terms
   - combo windows
   - steno settle time
   - autocorrect threshold
   - shadow-mode recommendations

3. **Corrections View**
   - last corrections
   - accepted/rejected state
   - suppressed pairs
   - dictionary candidates

4. **Steno View**
   - paper tape
   - dictionary hit/miss
   - translation
   - suggestions/reverse lookup

## Implementation Order

1. Autocorrect correction records, undo/reject suppression, and tests.
2. Telemetry event ring for recent decisions and emitted HID reports.
3. `tecla doctor` text UI for live input and steno strokes.
4. Tap-hold shadow adaptation with no behavior changes.
5. User-visible adaptation recommendations.
6. Opt-in adaptive profile that applies conservative changes.
7. Persistent adaptive profile and user dictionary.

## Non-Goals

- No cloud learning.
- No medical inference.
- No irreversible autocorrect.
- No hidden dictionary mutation by default.
- No global timing changes from a small amount of evidence.
