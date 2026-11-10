#!/usr/bin/env python3
"""Oracle for the P9.1 stuck trigger: fire indices per session, same rule as
pact-analysis::stuck::StuckDetector. Vendored next to the fixture (not in the
skills oracle dir) so the gate is hermetic: fixture + oracle + test travel
together and the gate survives the /tmp prototype being wiped.

Usage: python3 stuck_oracle.py trajectory-moves.json
Output lines: <session>=<i0>,<i1>,... (no line when nothing fires)
"""
import json
import sys

seqs = json.load(open(sys.argv[1]))
for sid in sorted(seqs):
    streak, fires = 0, []
    for i, mv in enumerate(seqs[sid]):
        streak = streak + 1 if mv == "REJECT_CORRECT" else 0
        if streak >= 2:
            fires.append(str(i))
    if fires:
        print(f"{sid}={','.join(fires)}")
