# Findings 0003 — what predicts unblocking (overnight enhancement)

Three local-only studies on frozen artifacts, 2026-09-23. No API, no new
labels. Scripts: `/tmp/study_postfire.py` (study A); inline python
(B/C/F/G) — promote to fixtures if re-run is needed.

## B. Reframe class × outcome (dataset-v3, n=429)

| reframe_class | n | P(unblocked) | Fisher vs scope_change |
|---|---|---|---|
| scope_change | 145 | 0.931 | — |
| definition_change | 21 | 0.857 | (n.s., same direction) |
| none | 237 | 0.696 | p ≈ 2e-8 |
| constraint_add | 13 | 0.462 | p ≈ 5e-5 |
| medium_change | 9 | 0.778 | — |
| others (analogy/audience/role/constraint_remove) | 1 each | — | too thin |

Move level: REFRAME 160/173 = 0.925 vs REJECT_CORRECT 154/230 = 0.670,
Fisher p ≈ 2e-10. Surface: codex 0.803 vs muse 0.713 (n=314/115;
confounded with task mix, reported not claimed).

This answers the adapted join question neither program could answer
alone: scope-changing reframes unblock far more reliably than
constraint-adding ones (p ≈ 5e-5). The maneuver that changes what the
work *is* beats the maneuver that adds rules to the failing frame —
wrong-frame over wrong-facts, now with an outcome attached.

## C. Move bigrams predicting the current row's outcome

Preceding (prev2, prev1) → P(row blocked), base 0.226 on labeled rows
(selected sample — describes v3, not random turns):

| bigram | n | P(blocked) | lift |
|---|---|---|---|
| (REFRAME, OTHER) | 16 | 0.000 | −0.226 |
| (QUESTION, QUESTION) | 23 | 0.043 | −0.182 |
| (OTHER, QUESTION) | 35 | 0.086 | −0.140 |
| (REJECT, REJECT) | 12 | 0.333 | +0.108 |
| rest | — | ≈ base | — |

A reframe two turns back perfectly predicts unblocking in-slice; question
runs predict it nearly as well. RR elevates but n=12.

## G. Embedding geometry per class (in-sample structure, not CV)

Centroid confusion: small formulaic classes perfectly self-cohesive
(rec 1.00); REFRAME rec 0.86 (19 lost to REJECT); REJECT rec 0.80
(40 lost to REFRAME). Mean-cosine gaps: REFRAME +0.065, all small
classes +0.03…+0.08, **REJECT_CORRECT −0.001**.

Rejection has no content signature: it is a discourse relation to the
prior frame, invisible to content embeddings. This derives E4's honest
negative (route signal not in proposal text) from the geometry side,
and bounds every embedding-only stuck detector including Task B's
0.669-at-baseline: the signal it would need is not in the vectors.
