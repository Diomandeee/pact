# P9.3 — Live-gate trigger A/B: protocol

Two candidate triggers for the live stuck gate. Both run under this
protocol; neither is tuned on these items.

## Arms

- **T (two-reject):** fire iff the rule-based classifier (`track.py`,
  local, no API) labels the current turn AND the previous turn
  `REJECT_CORRECT`. This is the `StuckDetector` rule ported in P9.1.
- **R (ranker):** embed the current turn with `text-embedding-3-small`,
  score with `ranker_v2.pkl` (sklearn LogisticRegression, 1536-d,
  pre-trained by the divergence pilot, untouched). Fire iff P ≥ τ, τ = 0.5.

## Items (frozen before the run)

- 113 should-fire: divergence turns with a maneuver class
  (`labels_human_443.jsonl`, the block-point eval set).
- 111 should-stay-silent: depth-matched unblocked turns
  (`control_items_113.json`: 42 vetted human-attributed + 69 screened,
  drift mean 0.1 turns).

## Metric and win condition (declared before the first run)

Primary: balanced accuracy = (fire rate on block + silence rate on
control) / 2. An arm wins iff it leads by **≥ 5pp**. A smaller gap is a
tie, and ties go to **T** — the arm with no API dependency, no per-turn
cost, and no embedding round-trip in the hot path. The tiebreak is
declared here, not discovered after the scores.

Secondary (reported, not decisive): fire rates per arm on each set,
τ-sensitivity for R (no τ tuning on these items — the sweep is
diagnostic only).

## What this round does NOT decide

Restatement content (measured already: 8→32% with the repertoire in
hand). Live adoption (did the human take the restated frame?) — that is
the defined follow-up: a watcher on the live stream logging
trigger-fires against post-hoc outcome labels, scored the same way.

## Provenance

Embeddings for R are cached to `ranker-embed-cache.json` on first run so
the round is re-runnable without API. Every turn text sent to the
embedding endpoint is logged by count (not content) in the results file.
