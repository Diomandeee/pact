# Findings 0004 — trigger utility bounds (overnight enhancement)

Two local-only studies, 2026-09-23. One null with a corrected bias, one
validation of the control set.

## A. Post-fire thread fate: null

For the 123 backfilled fires, reframe-within-k vs a symmetric
position-matched baseline (nearest non-fire idx ±5, same session):

| window | fire | baseline |
|---|---|---|
| 5 turns | 0.138 | 0.130 |
| 10 turns | 0.309 | 0.293 |
| 15 turns | 0.341 | 0.341 |

Identical. A first run with an asymmetric baseline (random idx in
[i−10, n)) falsely showed fires suppressing reframing (0.138 vs 0.244)
and lengthening threads — pure position bias, caught and fixed, recorded
here so nobody re-runs the broken version. Under the fair measure,
fires carry no forward signal about imminent reframing. Consistent with
the turn-level independence in §8 of the program record: the trigger
marks stuck neighborhoods, and nothing in the thread's next 15 turns
distinguishes a fire point from its neighbors.

## F. Control-set validation + proposer false alarms (n=59)

Parsing rule (stated): verdict = last MATCH/NOMATCH line; human-turn
class from continuation phrasing (continu|no maneuver|no redirect|same
line|simply to continue|lack of specific action) vs maneuver-class
names. Class names appear in 2/59 judgments (free-form judge); the
continuation reading covers 31 explicitly, and all 27 remaining are
topic-mismatch NOMATCHes with ordinary continuations on both sides —
verified by reading, not by regex alone.

- MATCH 1/59 = 0.017: the single MATCH is continuation-agreement (both
  sides merely continue). The 2% floor is therefore a continuation floor.
- Model-side redirect-ish proposals (restart/reframe/redirect/
  troubleshoot language about the model's own proposal): 8/59 ≈ 14%.
  Mostly troubleshooting steps on unblocked turns — legitimate help,
  not frame redirects, but the live gate should expect roughly one
  unsolicited interventionist proposal in seven quiet turns.
- Control set validated: effectively 59/59 human continuations. The
  vetting (57 mo-attributed) + screening (track.py non-redirect) pipeline
  admitted zero actual redirects by the judge's reading.
