# Findings 0005 — parametric policy + second-corpus scale (overnight)

## Parametric restatement policy (PACT P9.5, built + gated)

Measured unblock rates as selection weights (`pact-analysis/src/policy.rs`):
scope 0.931 (145), definition 0.857 (21), medium 0.778 (9),
constraint_add 0.462 (13); REFRAME 0.925 (173), REJECT 0.670 (230).
`select()` = argmax with MIN_N=5 (thin classes reported, never selected)
and TIEBAND=0.01 (near-ties go to more data). The gate pins every weight
to findings-0003 fractions — re-measurement changes both in one commit or
the build stays red. Break-proof: 0.99 mutation failed the pin test and
the selection test. Deliberate boundary: `Maneuver` variants unweighted
(join came back n=12).

Live-gate restatement order falls out: scope first, definition second,
medium third, constraint-adding last, thin classes never.

## Scale: trigger on the labeled thread store (exploratory, not pre-registered)

5,349 Codex user turns classified, piped through the `pact stuck`
binary: 528 fires. Scored against machine-labeled turn status:

- failed/interrupted turns near a fire (±2): 45/358 = 0.126
- completed turns near a fire: 769/4824 = 0.159
- threads with any failed turn that also fire: 21/69 = 0.304 vs
  30/185 = 0.162 of threads firing overall

Turn-level: null (slightly inverted, noise). Session-level: weak
enrichment — the same shape as prompt-logs (right neighborhoods, wrong
addresses), now on a second corpus with independent labels. The trigger
generalizes as a session finder, not a turn finder, twice over.
Script: `/tmp/study_thread_scale.py` (read-only on the sqlite store).
