# Findings 0006 — cross-lab origination ladder (keyed runs, 2026-09-23)

Same 113 prefixes + weak-eval prompt shape throughout; mini judge
unless noted. Runners: `/tmp/div_eval_cross.py` (Laguna),
`/tmp/div_eval_g6.py` (Sol/Luna). Keys in env only, results in
`/tmp/div_eval_{laguna,g6sol,g6luna}.json`.

| proposer | judge | matches | per-class MATCH (hk/premise/abort/transplant/scale/domain/other) |
|---|---|---|---|
| gpt-4o-mini | mini | 9/113 = 0.080 | 0/0/3/4/0/1/1 |
| gpt-4o (8-turn ctx) | mini | 20/113 = 0.177 | 0/4/6/5/2/1/2 |
| laguna-s-2.1 | mini | 42/113 = 0.372 | 4/6/10/7/4/3/8 |
| laguna-s-2.1 | laguna | 21 + ~4 lean /104 = ~0.24 | (stricter instrument; 9 empty, 12 analytical) |
| gpt-6-sol | mini | 41/111 = 0.369 | 5/7/12/6/2/5/4 |
| gpt-6-luna | mini | 40/112 = 0.357 | 4/4/10/7/3/4/8 |

Three independent labs converge at 36–37% unprompted. Judge instruments:
mini/laguna agree on 64% of jointly scored items; verbosity cannot
explain it (75 vs 63–67 mean words; matched-vs-unmatched 81 vs 72
within Laguna). gpt-6 needed `max_completion_tokens` (`max_tokens`
400s); Luna dropped 1 item on output truncation.

Revisions forced by this round:

1. The 8% was a small-model artifact. The unprompted ceiling for this
   task class is mid-30s, cross-lab replicated.
2. The kill-class zeros are scale-dependent, not absolute: hypothesis-kill
   goes 0/19 (mini) → 4–5/19 (frontier); premise 0/25 → 4–7/25. The
   exoskeleton conclusion (stopping authority stays human) now rests on
   consequence-arguments, not on origination zeros — update any text
   that cites 0-for-44 as permanent.
3. The gap survives intact in relative terms: best unprompted 0.37 vs
   recognition 0.92 vs instructed 0.32 (mini). Specification still
   dominates within a lab; priors differ across labs.
4. No GPT-5 on the project key at run time (model list tops at 4o);
   the scale rung above 6 remains untested.
