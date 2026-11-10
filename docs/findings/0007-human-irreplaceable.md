# Findings 0007 — the human input is irreplaceable (judge kills)

Question: can model judges substitute for the reader/human on new turns,
so the 30 fires get labels with nobody doing them? Attempted twice,
killed twice. Scripts: `/tmp/calibrate_judge.py`,
`/tmp/calibrate_judge_ctx.py`. Evidence: `/tmp/judge_calib.json`,
`/tmp/judge_calib_ctx.json` (170 items: 113 divergence + 57 vetted).

## Calibration 1: bare turn (1500 chars)

| judge | class agreement (chance 0.125) | stuck agreement |
|---|---|---|
| laguna | 0.124 | 0.382 |
| mini | 0.176 | 0.400 |
| sol | 0.171 | 0.200 |

Stuck calls independent of truth (judge-Y 0.14–0.20 whether truth is Y
or N). Parse compliance fine; discrimination zero.

## Calibration 2: ±3-turn window (5000 chars)

| judge | class | stuck |
|---|---|---|
| laguna | 0.112 | 0.394 |
| mini | 0.212 | 0.365 |
| sol | 0.100 | 0.171 |

Context did not fix it. The maneuver taxonomy is session-depth-dependent:
reproducing reader labels needs the session, not a window. Model judges
will NOT be applied to the fires — uncalibrated labels with unknown
error would be manufactured evidence, the exact failure the gates exist
to prevent.

## Cadence dynamics (judge-free): null

Inter-turn gaps around 123 fires vs symmetric baseline (n≈350 each):
median before/after 653/715s (fire) vs 568/705s (base); sub-60s
fractions 0.164→0.138 vs 0.134→0.167. No flailing signature, no stall
signature. Script: `/tmp/study_cadence.py`.

## What this leaves

Measurable without Mo: exhausted for stuck?-ground-truth (internal
state, unobservable) and maneuver-class on new turns (needs session
depth no local instrument reproduces). The two open questions that need
a human are exactly the two that need THIS human — which is itself a
finding about what the taxonomy encodes. Remaining machine legs:
behavioral resolved? (reframe null, cadence null — nothing found),
τ=0.8 follow-up (needs frozen held-out labels = the packet answers),
restatement arm (needs live key).
