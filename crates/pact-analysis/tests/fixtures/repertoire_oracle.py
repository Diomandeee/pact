#!/usr/bin/env python3
"""Oracle for the P9.2 repertoire: independent derivation of each fixture
row's class from its RAW fields (verdict + maneuver), never from the baked
`expected`. Vendored next to the fixture so the gate is hermetic.

Usage: python3 repertoire_oracle.py repertoire-fixture.json
Output lines: <index>|<verdict>|<maneuver or ->|<derived class>
"""
import json
import sys

MAP = {
    "hypothesis-kill-by-directional-evidence": "HypothesisKill",
    "scale-reframe": "ScaleReframe",
    "domain-structure-import": "DomainImport",
    "premise-rejection-with-immutability": "PremiseRejection",
    "thread-transplant": "ThreadTransplant",
    "abort-restart": "AbortRestart",
    "other-maneuver": "OtherManeuver",
}

rows = json.load(open(sys.argv[1]))
for i, r in enumerate(rows):
    if r["verdict"] == "divergence" and r.get("maneuver") in MAP:
        derived = MAP[r["maneuver"]]
    elif r["verdict"] == "divergence":
        derived = "CORRUPT"
    else:
        derived = "Continuation"
    print(f"{i}|{r['verdict']}|{r.get('maneuver') or '-'}|{derived}")
